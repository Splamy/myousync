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
    body::Body,
    extract::{
        ws::{Message, WebSocketUpgrade},
        Path,
    },
    http::{Request, StatusCode},
    middleware,
    response::IntoResponse,
    Json, Router,
};
use brainz::{BrainzMetadata, BrainzMultiSearch};
use chrono::Utc;
use dbdata::{FetchStatus, VideoStatus};
use duration_str::deserialize_duration;
use log::{debug, error, info, warn};
use musicfiles::MetadataTags;
use reqwest::Method;
use serde::Deserialize;
use std::{
    collections::HashSet,
    env,
    ffi::OsStr,
    ffi::OsString,
    future::Future,
    path::PathBuf,
    sync::{Arc, LazyLock, Mutex},
    time::Duration,
};
use tokio::sync::broadcast::Sender;
use tower_http::{
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
};
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

    let config_path = PathBuf::from(
        std::env::args()
            .nth(1)
            .or(env::var("MYOUSYNC_CONFIG_FILE").ok())
            .unwrap_or("myousync.toml".into()),
    );
    let s = MsState::new(&config_path);
    tokio::select! {
        _ = run_server(&s) => {},
        _ = playlist_sync_loop(&s) => {},
        _ = music_tag_loop(&s) => {},
    }
}

async fn run_server(s: &MsState) {
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
            "/trigger_sync",
            axum::routing::post({
                async move || {
                    MsState::trigger_sync();
                }
            })
            .layer(cors_layer.clone())
            .layer(middleware::from_fn(auth::auth)),
        )
        .route(
            "/reindex",
            axum::routing::post({
                async move |Json(video_ids): Json<Vec<String>>| {
                    dbdata::DB.set_videos_reindex(&video_ids);
                    MsState::trigger_tagger();
                }
            })
            .layer(cors_layer.clone())
            .layer(middleware::from_fn(auth::auth)),
        )
        .route(
            "/video/{video}/retry_fetch",
            axum::routing::post({
                async move |Path(video_id): Path<String>| {
                    MsState::push_override(&video_id, |v| {
                        if v.is_downloaded() {
                            return false;
                        }
                        v.fetch_status = FetchStatus::NotFetched;
                        true
                    });
                }
            })
            .layer(cors_layer.clone())
            .layer(middleware::from_fn(auth::auth)),
        )
        .route(
            "/video/{video}/query",
            axum::routing::post({
                async move |Path(video_id): Path<String>,
                            Json(query): Json<Option<BrainzMultiSearch>>| {
                    MsState::push_override(&video_id, |v| {
                        if !v.is_downloaded() {
                            return false;
                        }
                        let cleaned_query = query.as_ref().map(|q| BrainzMultiSearch {
                            trackid: norm_string(q.trackid.as_deref()),
                            title: q.title.trim().to_owned(),
                            artist: norm_string(q.artist.as_deref()),
                            album: norm_string(q.album.as_deref()),
                        });
                        v.override_query = cleaned_query;
                        v.fetch_status = FetchStatus::Fetched;
                        true
                    });
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
                    MsState::push_override(&video_id, |v| {
                        if !v.is_downloaded() {
                            return false;
                        }
                        let cleaned_result = result.as_ref().map(|r| BrainzMetadata {
                            title: r.title.trim().to_owned(),
                            artist: r.artist.iter().map(|s| s.trim().to_owned()).collect(),
                            album: norm_string(r.album.as_deref()),
                            brainz_recording_id: norm_string(r.brainz_recording_id.as_deref()),
                        });
                        v.override_result = cleaned_result;
                        v.fetch_status = FetchStatus::Fetched;
                        true
                    });
                }
            })
            .layer(cors_layer.clone())
            .layer(middleware::from_fn(auth::auth)),
        )
        .route(
            "/video/{video}/delete",
            axum::routing::post({
                let s = s.clone();
                async move |Path(video_id): Path<String>| {
                    MsState::push_override(&video_id, |v| {
                        dbdata::DB.delete_yt_data(&video_id);
                        if let Some(file) = find_file(&s, &video_id) {
                            if let Err(err) = musicfiles::delete_file(&s.config.paths, &file) {
                                let err = err.to_string();
                                error!("Error deleting file: {:?}", err);
                                v.last_error = Some(err);
                                return false;
                            }
                        }

                        v.fetch_status = FetchStatus::Disabled;
                        true
                    });
                }
            })
            .layer(cors_layer.clone())
            .layer(middleware::from_fn(auth::auth)),
        )
        .route(
            "/video/{video}/preview",
            axum::routing::get({
                let s = s.clone();
                async move |headers: axum::http::HeaderMap, Path(video_id): Path<String>| {
                    if let Some(path) = find_file(&s, &video_id) {
                        let mut req = Request::new(Body::empty());
                        *req.headers_mut() = headers;
                        return ServeFile::new(path).try_call(req).await.map_err(|e| {
                            error!("Error serving file: {:?}", e);
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "Error serving file".to_string(),
                            )
                        });
                    }

                    Err((StatusCode::NOT_FOUND, "File not found".to_string()))
                }
            })
            .layer(cors_layer.clone()), //.layer(middleware::from_fn(auth::auth)),
        )
        .route("/ws", axum::routing::get(ws_handler))
        .fallback_service(ServeDir::new("web"));

    let endpoint = format!("0.0.0.0:{}", s.config.web.port);
    let listener = tokio::net::TcpListener::bind(endpoint).await.unwrap();
    info!(
        "Listening on: http://{}",
        listener
            .local_addr()
            .unwrap()
            .to_string()
            .replace("0.0.0.0", "localhost")
    );
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

async fn playlist_sync_loop(s: &MsState) {
    trigger_loop(
        s.config.scrape.playlist_sync_rate,
        TRIGGER_PLAYLIST_SYNC.clone(),
        async || {
            sync_all(s).await;
        },
        "Playlist sync",
    )
    .await
}

async fn music_tag_loop(s: &MsState) {
    trigger_loop(
        s.config.scrape.cleanup_tag_rate,
        TRIGGER_MUSIC_TAG.clone(),
        async || {
            let all_ids = dbdata::DB.get_all_unprocessed_ids();
            for video_id in all_ids {
                if let Err(err) = sync_playlist_item(s, &video_id).await {
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

        while let Ok(msg) = rx
            .recv()
            .await
            .inspect_err(|e| warn!("Error receiving message: {:?}", e))
        {
            if let Err(err) = socket.send(Message::Text(msg.into())).await {
                debug!("Error sending message: {:?}", err);
                break;
            }
        }

        debug!("Client disconnected");
    })
}

async fn sync_all(s: &MsState) {
    let all_ids = dbdata::DB.get_all_ids().into_iter().collect::<HashSet<_>>();

    for playlist_id in s.config.scrape.playlists.iter() {
        info!("Syncing {}", playlist_id);
        match yt_api::get_playlist(&s.config, playlist_id).await {
            Ok(playlist) => {
                for item in playlist.items.iter() {
                    if all_ids.contains(&item.video_id) {
                        continue;
                    }

                    MsState::push_update(&mut VideoStatus {
                        video_id: item.video_id.to_owned(),
                        fetch_status: FetchStatus::NotFetched,
                        last_query: Some(BrainzMultiSearch {
                            trackid: None,
                            title: item.title.clone(),
                            artist: Some(item.artist.clone()),
                            album: None,
                        }),
                        ..Default::default()
                    });

                    MsState::trigger_tagger();
                }
            }
            Err(e) => {
                error!("Error with playlist sync: {:?}", e);
            }
        }
    }
}

async fn sync_playlist_item(s: &MsState, video_id: &str) -> anyhow::Result<()> {
    let mut status = dbdata::DB
        .get_video(video_id)
        .ok_or_else(|| anyhow!("Video not found"))?;

    info!("checking vid {}", status.video_id);

    let dlp_file: YtDlpResponse = match status.fetch_status {
        FetchStatus::NotFetched => match ytdlp::get(s, &status.video_id).await {
            Ok(dlp_file) => {
                status.fetch_time = Utc::now().timestamp() as u64;
                MsState::push_update_state(&mut status, FetchStatus::Fetched);
                dlp_file
            }
            Err(err) => {
                status.last_error = Some(err.to_string());
                MsState::push_update_state(&mut status, FetchStatus::FetchError);
                return Err(anyhow!("Fetch error: {}", err));
            }
        },
        FetchStatus::FetchError => {
            info!("Video {} fetch error", status.video_id);
            return Ok(());
        }
        FetchStatus::Categorized => {
            info!("Video {} already categorized", status.video_id);
            return Ok(());
        }
        FetchStatus::Disabled => {
            info!("Video {} disabled", status.video_id);
            return Ok(());
        }
        _ => {
            if let Some(dlp_file) = ytdlp::try_get_metadata(&status.video_id) {
                dlp_file
            } else {
                MsState::push_update_state(&mut status, FetchStatus::FetchError);
                return Err(anyhow!("No metadata found"));
            }
        }
    };

    let brainz_res = if let Some(override_result) =
        dbdata::DB.get_track_result_override(&status.video_id)
    {
        serde_json::from_str::<BrainzMetadata>(&override_result).unwrap()
    } else {
        let brainz_query =
            if let Some(override_query) = dbdata::DB.get_track_query_override(&status.video_id) {
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
                MsState::push_update(&mut status);
                res
            }
            Err(err) => {
                status.last_result = None;
                status.last_error = Some(err.to_string());
                MsState::push_update_state(&mut status, FetchStatus::BrainzError);
                return Err(err.into());
            }
        }
    };
    MsState::push_update(&mut status);

    let file = find_file(s, &status.video_id).ok_or_else(|| anyhow!("No file found"))?;

    let tags = MetadataTags {
        youtube_id: status.video_id.clone(),
        brainz: brainz_res,
    };

    // apply metadata to file
    musicfiles::apply_metadata_to_file(&file, &tags)?;

    musicfiles::move_file_to_library(s, &file, &tags)?;

    status.last_error = None;
    MsState::push_update_state(&mut status, FetchStatus::Categorized);

    Ok(())
}

fn find_file(s: &MsState, video_id: &str) -> Option<PathBuf> {
    ytdlp::find_local_file(s, video_id).or_else(|| musicfiles::find_local_file(s, video_id))
}

#[derive(Debug, Clone, Deserialize)]
pub struct MsConfig {
    pub paths: MsPaths,
    pub youtube: MsYoutube,
    pub web: MsWeb,
    pub scrape: MsScrape,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MsPaths {
    pub music: PathBuf,
    pub temp: PathBuf,
    pub migrate: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MsYoutube {
    #[serde(default = "MsConfig::get_youtube_client_id_from_env")]
    pub client_id: String,
    #[serde(default = "MsConfig::get_youtube_client_secret_from_env")]
    pub client_secret: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MsWeb {
    #[serde(default = "MsConfig::default_port")]
    pub port: u16,
    #[serde(default = "MsConfig::default_web_path")]
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MsScrape {
    pub playlists: Vec<String>,

    /// Min wait between requests to youtube-dl
    #[serde(deserialize_with = "deserialize_duration")]
    #[serde(default = "MsConfig::default_yt_dlp_rate")]
    pub yt_dlp_rate: Duration,
    #[serde(deserialize_with = "deserialize_duration")]
    #[serde(default = "MsConfig::default_cleanup_tag_rate")]
    pub cleanup_tag_rate: Duration,
    #[serde(deserialize_with = "deserialize_duration")]
    #[serde(default = "MsConfig::default_playlist_sync_rate")]
    pub playlist_sync_rate: Duration,
    #[serde(default = "MsConfig::default_yt_dlp")]
    pub yt_dlp: OsString,
}

impl MsConfig {
    fn read(config_path: &std::path::Path) -> Result<Self, anyhow::Error> {
        let config = std::fs::read_to_string(config_path)?;
        Ok(toml::from_str::<MsConfig>(&config)?)
    }

    const fn default_port() -> u16 {
        3001
    }

    fn default_web_path() -> String {
        "web".to_string()
    }

    const fn default_yt_dlp_rate() -> Duration {
        Duration::from_secs(10)
    }

    const fn default_cleanup_tag_rate() -> Duration {
        Duration::from_secs(60 * 60)
    }

    const fn default_playlist_sync_rate() -> Duration {
        Duration::from_secs(60 * 5)
    }

    fn get_youtube_client_id_from_env() -> String {
        env::var("YOUTUBE_CLIENT_ID").expect("youtube client id is not set")
    }

    fn get_youtube_client_secret_from_env() -> String {
        env::var("YOUTUBE_CLIENT_SECRET").expect("youtube client secret is not set")
    }

    fn default_yt_dlp() -> OsString {
        "yt-dlp".into()
    }
}

impl MsPaths {
    pub fn get_base_paths(&self) -> Vec<&std::path::Path> {
        let mut paths = vec![self.music.as_path(), self.temp.as_path()];
        if let Some(migrate) = &self.migrate {
            paths.push(migrate.as_path());
        }
        paths
    }

    pub fn is_sub_file(&self, path: &std::path::Path) -> bool {
        self.get_base_paths()
            .iter()
            .any(|p| path.starts_with(p) && path != *p)
    }
}

#[derive(Debug, Clone)]
pub struct MsState {
    pub config: MsConfig,
    pub file_cache: Arc<Mutex<std::collections::HashMap<String, PathBuf>>>,
}

impl MsState {
    pub fn new(config_path: &std::path::Path) -> Self {
        MsState {
            config: MsConfig::read(config_path).expect("Failed to read config"),
            file_cache: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }

    pub fn push_override<F: Fn(&mut VideoStatus) -> bool>(video_id: &str, modify: F) {
        if let Some(v) = dbdata::DB.modify_video_status(video_id, modify) {
            Self::trigger_tagger();
            Self::push_update_notification(&v);
        }
    }

    pub fn push_update_state(state: &mut VideoStatus, new_status: FetchStatus) {
        state.fetch_status = new_status;
        Self::push_update(state);
    }

    pub fn push_update(status: &mut VideoStatus) {
        status.update_now();
        dbdata::DB.set_full_track_status(status);
        Self::push_update_notification(status);
    }

    fn push_update_notification(status: &VideoStatus) {
        _ = NOTIFY_MUSIC_UPDATE.send(serde_json::to_string(&vec![status]).unwrap());
    }

    pub fn trigger_tagger() {
        _ = TRIGGER_MUSIC_TAG.send(());
    }

    pub fn trigger_sync() {
        _ = TRIGGER_PLAYLIST_SYNC.send(());
    }
}
