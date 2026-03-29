use std::path::{Path, PathBuf};

use crate::{
    MsJellyfin, MsState,
    dbdata::{DB, JellyItemId, JellyPlaylistId, YoutubePlaylistId},
    musicfiles,
    net::CLIENT,
};
use gethostname::gethostname;
use log::{debug, error, info, warn};
use rand::distr::{Alphanumeric, SampleString};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum JellyError {
    #[error("request error: {0}")]
    ConnectionError(#[from] reqwest::Error),
    #[error("Auth failed: {0}")]
    AuthFailure(String),
    #[error("unknown jellyfin error")]
    Unknown,
}

const JELLY_AUTH_KEY: &str = "jelly_auth";
const JELLY_DEVICE_ID: &str = "jelly_device";

pub async fn sync_all(s: &MsState) {
    let Some(jelly_config) = &s.config.jellyfin else {
        return;
    };

    let jelly_ctx = match login_jellyfin(jelly_config).await {
        Ok(jelly_ctx) => jelly_ctx,
        Err(err) => {
            error!("Failed to login to jellyfin: {err}");
            return;
        }
    };

    let unsynced = DB.get_jellyfin_unsynced(None);
    if unsynced.is_empty() {
        debug!("Nothing to sync with jellyfin");
        return;
    }

    if unsynced.iter().any(|item| item.jelly_id.is_none()) {
        debug!("Found unsynce jelly items");

        let sync_data = match get_jellyfin_full_data(&jelly_ctx, jelly_config).await {
            Ok(res) => res,
            Err(err) => {
                warn!("Failed to fetch full data: {err}");
                return;
            }
        };
        let sync_map: HashMap<&Path, &JellyItemId> = sync_data
            .iter()
            .map(|j| (Path::new(&j.path), &j.id))
            .collect();

        let mut cache = s.file_cache.lock().unwrap();
        musicfiles::rebuild_cache(s, &mut cache);

        for item in unsynced.iter().filter(|item| item.jelly_id.is_none()) {
            let Some(mut file_path) = cache.lookup.get(&item.video_id) else {
                warn!("Could not find {} locally, but should exist", item.video_id);
                continue;
            };

            let mut tmp_path = PathBuf::new();
            if let Some(rewrite) = &jelly_config.rewrite_path {
                if let Ok(p) = file_path.strip_prefix(&rewrite.from) {
                    tmp_path.push(&rewrite.to);
                    tmp_path.push(p);
                    file_path = &tmp_path;
                }
            }

            if let Some(jelly_id) = sync_map.get(file_path.as_path()) {
                DB.set_jellyfin_id(&item.video_id, jelly_id);
            } else {
                debug!(
                    "Didn't find {} at {} yet",
                    &item.video_id,
                    file_path.display()
                );
            }
        }
        drop(cache);
    }

    let check_playlists: HashSet<&YoutubePlaylistId> =
        unsynced.iter().map(|i| &i.playlist_id).collect();

    debug!("Affected playlists: {check_playlists:?}");

    let lists = DB.get_playlist_config();
    for list in lists {
        if !list.enabled {
            debug!("Playlist {} not enabled", &list.playlist_id);
            continue;
        }

        let Some(jelly_playlist_id) = list.jelly_playlist_id else {
            debug!(
                "Playlist {} has no jellyfin playlist associated",
                &list.playlist_id
            );
            continue;
        };

        if !check_playlists.contains(&list.playlist_id) {
            debug!("Playlist {} has no new items", &list.playlist_id);
            continue;
        }

        debug!(
            "Updating playlist {} to jellyfin {}",
            &list.playlist_id, &jelly_playlist_id
        );

        let ordered_jelly_ids = DB.get_jellyfin_playlist_item_ids(&list.playlist_id);

        let res = jellyfin_update_playlist(
            &jelly_ctx,
            jelly_config,
            jelly_playlist_id,
            JellyfinUpdatePlaylistRequest {
                ids: Some(ordered_jelly_ids),
                ..Default::default()
            },
        )
        .await;

        if let Err(jelly_err) = res {
            error!("Error while updating playlist: {jelly_err}");
            continue;
        }

        DB.set_jellyfin_items_to_synced(&list.playlist_id);
    }
}

async fn login_jellyfin(jelly_config: &MsJellyfin) -> Result<JellyfinContext, JellyError> {
    if let Some(existing_auth) = login_jellyfin_wit_existing_data(jelly_config).await {
        return Ok(existing_auth);
    }

    debug!("No stored jelly auth data, trying to login");

    let auth_data = JellyfinAuthRequest {
        username: jelly_config.user.to_string(),
        pw: jelly_config.password.to_string(),
    };

    let url = format!("{}/Users/AuthenticateByName", jelly_config.server);
    let auth_header = get_auth_header(None);
    let request = CLIENT
        .post(&url)
        .header("Authorization", auth_header)
        .json(&auth_data)
        .send()
        .await?;
    if !request.status().is_success() {
        let response_text = request.text().await?;
        return Err(JellyError::AuthFailure(response_text));
    }
    let response = request.json::<JellyfinAuthResponse>().await?;

    DB.set_key(JELLY_AUTH_KEY, &serde_json::to_string(&response).unwrap());

    let auth_header = get_auth_header(Some(&response));

    Ok(JellyfinContext { auth_header })
}

async fn login_jellyfin_wit_existing_data(jelly_config: &MsJellyfin) -> Option<JellyfinContext> {
    let existing_auth = DB.get_key(JELLY_AUTH_KEY)?;
    let Ok(existing_auth) = serde_json::from_str::<JellyfinAuthResponse>(&existing_auth) else {
        debug!("Could not deserialize old jelly auth data");
        DB.delete_key(JELLY_AUTH_KEY);
        return None;
    };
    let auth_header = get_auth_header(Some(&existing_auth));

    let url = format!("{}/Users/Me", jelly_config.server);
    let request = CLIENT
        .get(&url)
        .header("Authorization", &auth_header)
        .send()
        .await;

    let Ok(request) = request else {
        return None;
    };

    if request.status() == StatusCode::UNAUTHORIZED {
        debug!("Old auth seems to have been invalidated, clearing cached data");
        DB.delete_key(JELLY_AUTH_KEY);
        return None;
    } else if !request.status().is_success() {
        let response = request.text().await.ok()?;
        debug!("Failed to get auth status: {response}");
        return None;
    }

    let _response = match request.json::<JellyfinAuthUser>().await {
        Ok(response) => response,
        Err(err) => {
            error!("Failed to parse auth me response: {err}");
            return None;
        }
    };

    debug!("Found valid jelly login data");
    Some(JellyfinContext { auth_header })
}

async fn get_jellyfin_full_data(
    ctx: &JellyfinContext,
    jelly_config: &MsJellyfin,
) -> Result<Vec<JellyfinItem>, JellyError> {
    let url = format!("{}/Items", jelly_config.server);

    let request = CLIENT
        .get(&url)
        .query(&[
            ("includeItemTypes", "Audio"),
            ("fields", "Path"),
            ("parentId", &jelly_config.collection),
            ("recursive", "true"),
            ("enableImages", "false"),
            ("filters", "IsNotFolder"),
            ("locationType", "FileSystem"),
        ])
        .header("Authorization", &ctx.auth_header)
        .send()
        .await?;

    let status = request.status();
    if !status.is_success() {
        let response = request.text().await?;
        info!("Failed to get full sync. Status: {status}, Response: {response}");
        return Err(JellyError::Unknown);
    }

    let response = request.json::<JellyfinItemResponse>().await?;

    Ok(response.items)
}

async fn jellyfin_update_playlist(
    ctx: &JellyfinContext,
    jelly_config: &MsJellyfin,
    jelly_playlist_id: JellyPlaylistId,
    jelly_update: JellyfinUpdatePlaylistRequest,
) -> Result<(), JellyError> {
    let url = format!("{}/Playlists/{}", jelly_config.server, jelly_playlist_id);

    let request = CLIENT
        .post(&url)
        .json(&jelly_update)
        .header("Authorization", &ctx.auth_header)
        .send()
        .await?;

    if !request.status().is_success() {
        let response = request.text().await?;
        error!("Failed to update playlist {jelly_playlist_id}: {response}");
        return Err(JellyError::Unknown);
    }

    Ok(())
}

fn get_auth_header(auth_data: Option<&JellyfinAuthResponse>) -> String {
    let hostname = gethostname()
        .into_string()
        .unwrap_or_else(|_| "GenericMyousyncDevice".to_string());

    let device_id = DB.get_key(JELLY_DEVICE_ID).unwrap_or_else(|| {
        let device_id = Alphanumeric.sample_string(&mut rand::rng(), 32);
        DB.set_key(JELLY_DEVICE_ID, &device_id);
        device_id
    });

    let mut params = vec![
        ("Client", "myousync"),
        ("Device", &hostname),
        ("Version", "1.0.0"),
        ("DeviceId", &device_id),
    ];

    if let Some(auth_data) = auth_data {
        params.push(("Token", &auth_data.access_token));
    }

    build_auth_header(&params)
}

fn build_auth_header(params: &[(&str, &str)]) -> String {
    let mut auth_builder: String = r"MediaBrowser ".to_string();
    let mut has_one = false;
    for param in params {
        if has_one {
            auth_builder.push_str(", ");
        }
        has_one = true;
        auth_builder.push_str(param.0);
        auth_builder.push_str("=\"");
        auth_builder.push_str(param.1);
        auth_builder.push('"');
    }
    auth_builder
}

struct JellyfinContext {
    pub auth_header: String,
}

// /Items

#[derive(Deserialize)]
#[serde(rename_all(deserialize = "PascalCase"))]
struct JellyfinItemResponse {
    pub items: Vec<JellyfinItem>,
}

#[derive(Deserialize)]
#[serde(rename_all(deserialize = "PascalCase"))]
struct JellyfinItem {
    pub id: JellyItemId,
    pub path: String,
}

// Update Playlist

#[derive(Serialize, Default)]
#[serde(rename_all(deserialize = "PascalCase"))]
struct JellyfinUpdatePlaylistRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ids: Option<Vec<String>>,
}

// /AuthenticateByName

#[derive(Serialize)]
#[serde(rename_all(serialize = "PascalCase"))]
struct JellyfinAuthRequest {
    pub username: String,
    pub pw: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all(serialize = "PascalCase", deserialize = "PascalCase"))]
struct JellyfinAuthResponse {
    pub user: JellyfinAuthUser,
    pub access_token: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all(serialize = "PascalCase", deserialize = "PascalCase"))]
struct JellyfinAuthUser {
    pub id: String,
}
