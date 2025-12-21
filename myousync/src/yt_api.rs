use std::{io, mem};

use crate::{MsConfig, dbdata::YoutubePlaylistId, net::CLIENT};
use chrono::TimeDelta;
use log::{debug, info};
use serde::Deserialize;
use thiserror::Error;

use crate::dbdata::{self, AuthData, Playlist, PlaylistItem};

const PLAYLISTS_QUICK_CACHE_TIME: TimeDelta = chrono::Duration::minutes(1);

#[derive(Error, Debug)]
pub enum YTError {
    #[error("")]
    ConnectionError(#[from] reqwest::Error),
    #[error("Maximum auth time exceeded")]
    AuthTimeExceeded,
    #[error("Auth rejected")]
    AuthRejected,
    #[error("Missing refresh token")]
    MissingRefreshToken,
    #[error("")]
    IOError(#[from] io::Error),
    #[error("")]
    JsonEncodingErr(#[from] std::string::FromUtf8Error),
    #[error("")]
    JsonDeserializationErr(#[from] serde_json::Error),
    #[error("unknown data store error")]
    Unknown,
}

pub async fn get_auth(config: &MsConfig) -> Result<AuthData, YTError> {
    if let Some(data) = dbdata::DB.try_get_auth() {
        debug!("Found YT Auth");

        if chrono::Utc::now().timestamp() < data.expires_at {
            debug!("YT Auth is still valid");
            return Ok(data);
        }

        debug!("YT Auth is expired, refetching");

        let mut form_data = String::new();
        form_data.push_str("client_id=");
        form_data.push_str(&urlencoding::encode(&config.youtube.client_id));
        form_data.push_str("&client_secret=");
        form_data.push_str(&urlencoding::encode(&config.youtube.client_secret));
        form_data.push_str("&refresh_token=");
        form_data.push_str(&urlencoding::encode(&data.refresh_token));
        form_data.push_str("&grant_type=refresh_token");

        let response = CLIENT
            .post("https://oauth2.googleapis.com/token")
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(form_data)
            .send()
            .await?
            .json::<YtTokenResponse>()
            .await?;

        match response {
            YtTokenResponse::Success(token_data) => {
                let new_data = AuthData {
                    access_token: token_data.access_token,
                    expires_at: chrono::Utc::now().timestamp() + token_data.expires_in,
                    refresh_token: data.refresh_token,
                };

                dbdata::DB.set_auth(&new_data);

                return Ok(new_data);
            }
            YtTokenResponse::Error(_error) => {
                return Err(YTError::Unknown);
            }
        }
    }

    info!("No YT Auth found, fetching");

    let mut form_data = String::new();
    form_data.push_str("client_id=");
    form_data.push_str(&urlencoding::encode(&config.youtube.client_id));
    form_data.push_str("&scope=");
    form_data.push_str(&urlencoding::encode(
        "https://www.googleapis.com/auth/youtube",
    ));

    debug!("form_data: {form_data}");

    let code_response = CLIENT
        .post("https://oauth2.googleapis.com/device/code")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(form_data)
        .send()
        .await?
        .json::<YtDeviceCodeResponse>()
        .await?;

    info!("Please go to: {}", code_response.verification_url);
    info!("Enter code: {}", code_response.user_code);

    let mut form_data = String::new();
    form_data.push_str("client_id=");
    form_data.push_str(&urlencoding::encode(&config.youtube.client_id));
    form_data.push_str("&client_secret=");
    form_data.push_str(&urlencoding::encode(&config.youtube.client_secret));
    form_data.push_str("&device_code=");
    form_data.push_str(&urlencoding::encode(&code_response.device_code));
    form_data.push_str("&grant_type=urn:ietf:params:oauth:grant-type:device_code");

    let timeout = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::seconds(code_response.expires_in))
        .unwrap();

    while chrono::Utc::now() < timeout {
        info!("Waiting for user to authorize");
        tokio::time::sleep(tokio::time::Duration::from_secs(
            code_response.interval as u64,
        ))
        .await;

        let token_response = CLIENT
            .post("https://oauth2.googleapis.com/token")
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(form_data.clone())
            .send()
            .await?
            .json::<YtTokenResponse>()
            .await?;

        match token_response {
            YtTokenResponse::Error(error) => {
                if error.error == "authorization_pending" {
                    continue;
                } else if error.error == "slow_down" {
                    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                    continue;
                } else if error.error == "expired_token" {
                    return Err(YTError::AuthTimeExceeded);
                } else if error.error == "access_denied" {
                    return Err(YTError::AuthRejected);
                }
            }
            YtTokenResponse::Success(token_data) => {
                let new_data = AuthData {
                    access_token: token_data.access_token,
                    expires_at: chrono::Utc::now().timestamp() + token_data.expires_in,
                    refresh_token: token_data
                        .refresh_token
                        .ok_or(YTError::MissingRefreshToken)?,
                };

                dbdata::DB.set_auth(&new_data);

                return Ok(new_data);
            }
        }
    }

    Err(YTError::AuthTimeExceeded)
}

pub async fn get_playlist(
    config: &MsConfig,
    playlist_id: &YoutubePlaylistId,
) -> Result<Playlist, YTError> {
    let maybe_cached_playlist = dbdata::DB.try_get_playlist(playlist_id);

    if maybe_cached_playlist
        .as_ref()
        .is_some_and(|p| chrono::Utc::now() - p.fetch_time < PLAYLISTS_QUICK_CACHE_TIME)
    {
        debug!("Found cached playlist in last 5 minutes");
        return maybe_cached_playlist.ok_or(YTError::Unknown);
    }

    let auth = get_auth(config).await?;

    debug!("Getting playlist: {playlist_id}");
    let mut response = get_playlist_reponse(&auth, playlist_id, None).await?;
    let mut next_page = response.next_page_token.take();
    let page_info = response.page_info.clone();

    debug!("Got page info: {page_info:?}");

    if let Some(cached_playlist) = maybe_cached_playlist {
        if cached_playlist.etag == response.etag
            && cached_playlist.total_results == page_info.total_results
            && cached_playlist.items.len() == page_info.total_results as usize
        {
            debug!("Found cached playlist by etag");
            dbdata::DB.update_playlist_fetch_time(playlist_id, chrono::Utc::now());
            return Ok(cached_playlist);
        }
    }

    debug!("Creating new playlist");

    let mut playlist = Playlist {
        playlist_id: playlist_id.clone(),
        fetch_time: chrono::Utc::now(),
        etag: mem::take(&mut response.etag),
        total_results: page_info.total_results,
        items: Vec::with_capacity(page_info.total_results as usize),
    };

    drain_to(&mut playlist.items, response);

    while let Some(next_page_key) = next_page {
        debug!("Getting next page: {next_page_key}");

        let mut response = get_playlist_reponse(&auth, playlist_id, Some(&next_page_key)).await?;
        next_page = response.next_page_token.take();

        drain_to(&mut playlist.items, response);
    }

    debug!("Saving playlist to db cache");

    dbdata::DB.set_playlist(&playlist);

    Ok(playlist)
}

async fn get_playlist_reponse(
    auth: &AuthData,
    playlist_id: &YoutubePlaylistId,
    page: Option<&str>,
) -> Result<YtPlaylistItemsResponse, YTError> {
    let mut req = CLIENT
        .get("https://www.googleapis.com/youtube/v3/playlistItems")
        .query(&[
            ("part", "snippet"),
            ("playlistId", playlist_id.as_ref()),
            ("maxResults", "50"),
        ]);
    if let Some(page) = page {
        req = req.query(&[("pageToken", page)]);
    }
    let response = req
        .header("Authorization", format!("Bearer {}", auth.access_token))
        .send()
        .await?
        .text()
        .await?;

    Ok(serde_json::from_str(&response)?)
}

fn drain_to(items: &mut Vec<PlaylistItem>, response: YtPlaylistItemsResponse) {
    for (index, mut item) in response.items.into_iter().enumerate() {
        let artist = if let Some(mut artist) = item.snippet.video_owner_channel_title.take() {
            const STRIP_SUFFIX: &str = " - Topic";
            if artist.ends_with(STRIP_SUFFIX) {
                artist.truncate(artist.len() - STRIP_SUFFIX.len());
            }
            artist
        } else {
            mem::take(&mut item.snippet.channel_title)
        };

        items.push(PlaylistItem {
            video_id: mem::take(&mut item.snippet.resource_id.video_id).into(),
            title: mem::take(&mut item.snippet.title),
            artist,
            position: index as u32,
            jelly_status: dbdata::JellyStatus::NotSynced,
        });
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
pub struct YtPlaylistItemsResponse {
    pub etag: String,
    pub next_page_token: Option<String>,
    pub page_info: PageInfo,
    pub items: Vec<YtPlaylistItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
pub struct YtPlaylistItem {
    pub snippet: YtSnippet,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
pub struct YtSnippet {
    pub title: String,
    pub channel_title: String,
    pub video_owner_channel_title: Option<String>,
    pub resource_id: YtResourceId,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
pub struct YtResourceId {
    pub video_id: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
pub struct PageInfo {
    pub total_results: u32,
    #[expect(dead_code)]
    pub results_per_page: u32,
}

// Auth Stuff

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum YtTokenResponse {
    Success(YtTokenResponseSuccess),
    Error(YtTokenResponseError),
}

#[derive(Debug, Deserialize)]
struct YtTokenResponseSuccess {
    pub access_token: String,
    pub expires_in: i64,
    pub refresh_token: Option<String>,
    #[expect(dead_code)]
    pub scope: String,
    #[expect(dead_code)]
    pub token_type: String,
}

#[derive(Debug, Deserialize)]
struct YtTokenResponseError {
    pub error: String,
    #[expect(dead_code)]
    pub error_description: String,
}

#[derive(Debug, Deserialize)]
struct YtDeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub expires_in: i64,
    pub interval: i64,
    pub verification_url: String,
}
