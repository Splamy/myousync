use crate::{
    brainz::{BrainzMetadata, BrainzMultiSearch},
    dbdata::sql_system_time::SqlSystemTime,
};
use rusqlite::{
    ToSql,
    types::{FromSql, FromSqlResult, ToSqlOutput},
};
use serde::{Deserialize, Serialize};
use std::{
    borrow::Borrow,
    fmt::{Debug, Display},
};

// == Helper ==

macro_rules! ValueId {
    ($type_name:ident) => {
        #[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone)]
        #[serde(transparent)]
        pub struct $type_name(String);

        impl $type_name {
            pub const fn new(value: String) -> Self {
                Self(value)
            }
        }

        impl Display for $type_name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                std::fmt::Display::fmt(&self.0, f)
            }
        }

        impl ToSql for $type_name {
            fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
                Ok(rusqlite::types::ToSqlOutput::from(self.0.clone()))
            }
        }

        impl FromSql for $type_name {
            fn column_result(
                value: rusqlite::types::ValueRef<'_>,
            ) -> rusqlite::types::FromSqlResult<Self> {
                value.as_str().map(Into::into)
            }
        }

        impl From<String> for $type_name {
            fn from(value: String) -> Self {
                Self::new(value)
            }
        }

        impl From<&str> for $type_name {
            fn from(value: &str) -> Self {
                Self::new(value.to_string())
            }
        }

        impl Borrow<str> for $type_name {
            fn borrow(&self) -> &str {
                self.0.borrow()
            }
        }

        impl AsRef<str> for $type_name {
            fn as_ref(&self) -> &str {
                self.0.as_ref()
            }
        }

        impl std::fmt::Debug for $type_name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                std::fmt::Debug::fmt(&self.0, f)
            }
        }
    };
}

macro_rules! SqlEnum {
    ($type_name:ident) => {
        impl ToSql for $type_name {
            fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
                Ok(ToSqlOutput::from(*self as i64))
            }
        }

        impl FromSql for $type_name {
            fn column_result(value: rusqlite::types::ValueRef) -> FromSqlResult<Self> {
                i64::column_result(value).and_then(|num| {
                    num.try_into()
                        .map_err(|_| rusqlite::types::FromSqlError::OutOfRange(num))
                })
            }
        }
    };
}

// Models

ValueId!(YoutubePlaylistId);
ValueId!(YoutubeVideoId);
ValueId!(JellyPlaylistId);
ValueId!(JellyItemId);

#[derive(Debug, Deserialize, Serialize)]
pub struct UserData {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct AuthData {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: SqlSystemTime,
}

#[derive(Deserialize)]
pub struct PlaylistConfig {
    pub playlist_id: YoutubePlaylistId,
    pub jelly_playlist_id: Option<JellyPlaylistId>,
    pub enabled: bool,
}

impl PlaylistConfig {
    pub const fn new(playlist_id: YoutubePlaylistId) -> Self {
        Self {
            playlist_id,
            jelly_playlist_id: None,
            enabled: true,
        }
    }
}

pub struct Playlist {
    pub playlist_id: YoutubePlaylistId,
    pub etag: String,
    pub total_results: u32,
    pub fetch_time: SqlSystemTime,
    pub items: Vec<PlaylistItem>,
}

#[derive(Debug)]
pub struct PlaylistItem {
    pub video_id: YoutubeVideoId,
    pub title: String,
    pub artist: String,
    pub position: u32,
    pub jelly_status: JellyStatus,
}

pub struct JellySyncStatus {
    pub playlist_id: YoutubePlaylistId,
    pub video_id: YoutubeVideoId,
    pub fetch_status: FetchStatus,
    pub jelly_status: JellyStatus,
    pub jelly_id: Option<JellyItemId>,
}

#[derive(Deserialize, Serialize)]
pub struct VideoStatus {
    pub video_id: YoutubeVideoId,
    pub fetch_status: FetchStatus,
    pub fetch_time: Option<SqlSystemTime>,
    pub last_update: Option<SqlSystemTime>,
    pub last_query: Option<BrainzMultiSearch>,
    pub last_result: Option<BrainzMetadata>,
    pub last_error: Option<String>,
    pub override_query: Option<BrainzMultiSearch>,
    pub override_result: Option<BrainzMetadata>,
    pub jelly_id: Option<JellyItemId>,
}

impl VideoStatus {
    pub const fn new(video_id: YoutubeVideoId) -> Self {
        Self {
            video_id,
            fetch_status: FetchStatus::NotFetched,
            fetch_time: None,
            last_update: None,
            last_query: None,
            last_result: None,
            last_error: None,
            override_query: None,
            override_result: None,
            jelly_id: None,
        }
    }

    pub fn update_now(&mut self) {
        self.last_update = Some(SqlSystemTime::now());
    }

    pub fn is_downloaded(&self) -> bool {
        self.fetch_status != FetchStatus::NotFetched
            && self.fetch_status != FetchStatus::FetchError
            && self.fetch_status != FetchStatus::Disabled
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
pub enum FetchStatus {
    #[default]
    NotFetched = 0,
    Fetched,
    FetchError,
    BrainzError,
    Categorized,
    Disabled,
}

SqlEnum!(FetchStatus);
impl TryFrom<i64> for FetchStatus {
    type Error = ();

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::NotFetched),
            1 => Ok(Self::Fetched),
            2 => Ok(Self::FetchError),
            3 => Ok(Self::BrainzError),
            4 => Ok(Self::Categorized),
            5 => Ok(Self::Disabled),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
pub enum JellyStatus {
    #[default]
    NotSynced = 0,
    Synced,
}

SqlEnum!(JellyStatus);
impl TryFrom<i64> for JellyStatus {
    type Error = ();

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::NotSynced),
            1 => Ok(Self::Synced),
            _ => Err(()),
        }
    }
}
