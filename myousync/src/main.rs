mod auth;
mod brainz;
mod dbdata;
mod musicfiles;
mod net;
mod util;
mod yt_api;
mod ytdlp;

use anyhow::anyhow;
use axum::{
    extract::{
        ws::{Message, WebSocketUpgrade},
        Path,
    },
    middleware,
    response::IntoResponse,
};
use axum::{Json, Router};
use brainz::{BrainzMetadata, BrainzMultiSearch};
use chrono::Utc;
use dbdata::{FetchStatus, VideoStatus};
use duration_str::deserialize_duration;
use log::{debug, error, info, warn};
use musicfiles::MetadataTags;
use reqwest::Method;
use serde::Deserialize;
use std::{collections::HashSet, future::Future, path::PathBuf, sync::LazyLock, time::Duration};
use tokio::sync::broadcast::Sender;
use tower_http::{cors::CorsLayer, services::ServeDir};
use ytdlp::YtDlpResponse;

static NOTIFY_MUSIC_UPDATE: LazyLock<Sender<String>> =
    LazyLock::new(|| tokio::sync::broadcast::channel::<String>(100).0);
static TRIGGER_MUSIC_TAG: LazyLock<Sender<()>> =
    LazyLock::new(|| tokio::sync::broadcast::channel::<()>(1).0);
static TRIGGER_PLAYLIST_SYNC: LazyLock<Sender<()>> =
    LazyLock::new(|| tokio::sync::broadcast::channel::<()>(1).0);

#[tokio::main]
async fn main() {
    colog::init();

    let s = MSState::new();
    tokio::select! {
        _ = run_server() => {},
        _ = playlist_sync_loop(&s) => {},
        _ = music_tag_loop(&s) => {},
    }
}

async fn run_server() {
    let cors_layer = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_headers(vec!["Authorization".parse().unwrap(), "*".parse().unwrap()])
        .allow_methods(vec![Method::GET, Method::POST]);

    // build our application with a single route
    let app = Router::new()
        .route(
            "/login",
            axum::routing::post(auth::sign_in).layer(cors_layer.clone()),
        )
        .route(
            "/login/check",
            axum::routing::post(async || "Ok")
                .layer(cors_layer.clone())
                .layer(middleware::from_fn(auth::auth)),
        )
        .route(
            "/video/{video}/query",
            axum::routing::post({
                async move |Path(video_id): Path<String>,
                            Json(query): Json<Option<BrainzMultiSearch>>| {
                    let cleaned_query = query.map(|q| BrainzMultiSearch {
                        trackid: norm_string(q.trackid.as_deref()),
                        title: q.title.trim().to_owned(),
                        artist: norm_string(q.artist.as_deref()),
                        album: norm_string(q.album.as_deref()),
                    });
                    dbdata::DB.set_track_query_override(&video_id, cleaned_query.as_ref());
                    _ = TRIGGER_MUSIC_TAG.send(());
                }
            })
            .layer(cors_layer.clone())
            .layer(middleware::from_fn(auth::auth)),
        )
        .route(
            "/video/{video}/result",
            axum::routing::post({
                async move |Path(video_id): Path<String>,
                            Json(result): Json<Option<BrainzMetadata>>| {
                    let result = result.map(|r| BrainzMetadata {
                        title: r.title.trim().to_owned(),
                        artist: r.artist.into_iter().map(|s| s.trim().to_owned()).collect(),
                        album: norm_string(r.album.as_deref()),
                        brainz_recording_id: norm_string(r.brainz_recording_id.as_deref()),
                    });
                    dbdata::DB.set_track_result_override(&video_id, result.as_ref());
                    _ = TRIGGER_MUSIC_TAG.send(());
                }
            })
            .layer(cors_layer.clone())
            .layer(middleware::from_fn(auth::auth)),
        )
        .route("/ws", axum::routing::get(ws_handler))
        .fallback_service(ServeDir::new("web"));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3001").await.unwrap();
    info!("Listening on: http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

fn norm_string(s: Option<&str>) -> Option<String> {
    s.and_then(|s| {
        let s = s.trim();
        if s.is_empty() {
            None
        } else {
            Some(s.to_owned())
        }
    })
}

async fn playlist_sync_loop(s: &MSState) {
    trigger_loop(
        s.config.playlist_sync_rate,
        TRIGGER_PLAYLIST_SYNC.clone(),
        async || {
            sync_all(&s).await;
        },
        "Playlist sync",
    )
    .await
}

async fn music_tag_loop(s: &MSState) {
    trigger_loop(
        s.config.cleanup_tag_rate,
        TRIGGER_MUSIC_TAG.clone(),
        async || {
            let all_ids = dbdata::DB.get_all_unprocessed_ids();
            for video_id in all_ids {
                if let Err(err) = sync_playlist_item(&s, &video_id).await {
                    error!("Error processing song: {:?}", err);
                }
            }
        },
        "Music tagger",
    )
    .await
}

async fn trigger_loop<
    B: Fn() -> BRet,
    BRet: Future<Output = ()>,
    D: Into<tokio::time::Duration>,
>(
    time: D,
    trigger: Sender<()>,
    loop_body: B,
    display: &str,
) {
    let mut interval = tokio::time::interval(time.into());
    let mut trigger = trigger.subscribe();

    debug!("Starting loop: {}", display);

    loop {
        tokio::select! {
            _ = interval.tick() => {
            },
            res = trigger.recv() => {
                debug!("Triggered: {:?}", res);
            }
        }
        info!("Entering loop: {}", display);
        loop_body().await;
        debug!("Exiting loop: {}", display);
    }
}

async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(async |mut socket| {
        let mut auth_ok = false;
        if let Some(Ok(init)) = socket.recv().await {
            if let Ok(auth) = init.to_text() {
                auth_ok = auth::decode_jwt(auth).is_ok();
            }
        }

        if !auth_ok {
            _ = socket.send(Message::Text("Unauthorized".into())).await;
            return;
        }

        let sub = NOTIFY_MUSIC_UPDATE.clone();
        let mut rx = sub.subscribe();
        {
            let init_list = dbdata::DB.get_all_videos();
            if let Err(err) = socket
                .send(Message::Text(
                    serde_json::to_string(&init_list).unwrap().into(),
                ))
                .await
            {
                debug!("Error sending init message: {:?}", err);
                return;
            }
        }

        while let Some(msg) = rx
            .recv()
            .await
            .inspect_err(|e| warn!("Error receiving message: {:?}", e))
            .ok()
        {
            if let Err(err) = socket.send(Message::Text(msg.into())).await {
                debug!("Error sending message: {:?}", err);
                break;
            }
        }

        debug!("Client disconnected");
    })
}

async fn sync_all(s: &MSState) {
    let all_ids = dbdata::DB.get_all_ids().into_iter().collect::<HashSet<_>>();

    for playlist_id in s.config.playlists.iter() {
        info!("Syncing {}", playlist_id);
        match yt_api::get_playlist(&s.config, playlist_id).await {
            Ok(playlist) => {
                for item in playlist.items.iter() {
                    if all_ids.contains(&item.video_id) {
                        continue;
                    }

                    s.push_update(&mut VideoStatus {
                        video_id: item.video_id.to_owned(),
                        fetch_status: FetchStatus::NotFetched,
                        ..Default::default()
                    });

                    s.trigger_tagger();
                }
            }
            Err(e) => {
                error!("Error with playlist sync: {:?}", e);
            }
        }
    }
}

async fn sync_playlist_item(s: &MSState, video_id: &str) -> anyhow::Result<()> {
    let mut status = dbdata::DB
        .get_video(&video_id)
        .ok_or_else(|| anyhow!("Video not found"))?;

    if status.fetch_status == FetchStatus::Categorized {
        info!("Video {} already categorized", video_id);
        return Ok(());
    }
    info!("checking vid {}", video_id);

    let dlp_file: YtDlpResponse = match status.fetch_status {
        FetchStatus::NotFetched => {
            if let Ok(dlp_file) = ytdlp::get(&s, video_id).await {
                status.fetch_time = Utc::now().timestamp() as u64;
                s.push_update_state(&mut status, FetchStatus::Fetched);
                dlp_file
            } else {
                s.push_update_state(&mut status, FetchStatus::FetchError);
                return Err(anyhow!("Fetch error"));
            }
        }
        FetchStatus::FetchError => {
            error!("Error fetching video"); // TODO add retry logic
            return Err(anyhow!("Fetch error"));
        }
        _ => {
            if let Some(dlp_file) = ytdlp::try_get_metadata(video_id) {
                dlp_file
            } else {
                s.push_update_state(&mut status, FetchStatus::FetchError);
                return Err(anyhow!("No metadata found"));
            }
        }
    };

    let brainz_res = if let Some(override_result) = dbdata::DB.get_track_result_override(video_id) {
        serde_json::from_str::<BrainzMetadata>(&override_result).unwrap()
    } else {
        let brainz_query =
            if let Some(override_query) = dbdata::DB.get_track_query_override(video_id) {
                serde_json::from_str::<BrainzMultiSearch>(&override_query).unwrap()
            } else {
                let query = BrainzMultiSearch {
                    trackid: None,
                    title: dlp_file.track.unwrap_or(dlp_file.title),
                    artist: dlp_file.artist,
                    album: dlp_file.album,
                };
                status.last_query = Some(query.clone());
                query
            };

        match brainz::analyze_brainz(&brainz_query).await {
            Ok(res) => {
                status.last_result = Some(res.clone());
                s.push_update(&mut status);
                res
            }
            Err(err) => {
                status.last_result = None;
                s.push_update_state(&mut status, FetchStatus::BrainzError);
                return Err(err.into());
            }
        }
    };
    s.push_update(&mut status);

    let file = ytdlp::find_local_file(&s, video_id)
        .or_else(|| musicfiles::find_local_file(&s, video_id))
        .ok_or_else(|| anyhow!("No file found"))?;

    let tags = MetadataTags {
        youtube_id: video_id.to_owned(),
        brainz: brainz_res,
    };

    // apply metadata to file
    musicfiles::apply_metadata_to_file(&file, &tags)?;

    musicfiles::move_file_to_library(s, &file, &tags)?;

    s.push_update_state(&mut status, FetchStatus::Categorized);

    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
pub struct MSConfig {
    pub playlists: Vec<String>,
    pub music: PathBuf,
    pub temp: PathBuf,
    pub yt_client_id: String,
    pub yt_client_secret: String,
    /// Min wait between requests to youtube-dl
    #[serde(deserialize_with = "deserialize_duration")]
    pub yt_dlp_rate: Duration,
    #[serde(deserialize_with = "deserialize_duration")]
    pub cleanup_tag_rate: Duration,
    #[serde(deserialize_with = "deserialize_duration")]
    pub playlist_sync_rate: Duration,
}

impl MSConfig {
    fn read() -> Result<Self, anyhow::Error> {
        let config = std::fs::read_to_string("msync.toml")?;
        Ok(toml::from_str::<MSConfig>(&config)?)
    }
}

#[derive(Debug, Clone)]
pub struct MSState {
    pub config: MSConfig,
}

impl MSState {
    pub fn new() -> Self {
        MSState {
            config: MSConfig::read().expect("Failed to read config"),
        }
    }

    pub fn push_update_state(&self, state: &mut VideoStatus, new_status: FetchStatus) {
        state.fetch_status = new_status;
        self.push_update(state);
    }

    pub fn push_update(&self, status: &mut VideoStatus) {
        status.update_now();
        dbdata::DB.set_full_track_status(status);
        _ = NOTIFY_MUSIC_UPDATE.send(serde_json::to_string(&vec![status]).unwrap());
    }

    pub fn trigger_tagger(&self) {
        _ = TRIGGER_MUSIC_TAG.send(());
    }
}
