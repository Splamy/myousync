use std::mem;
use std::sync::LazyLock;

use crate::net::CLIENT;
use crate::{dbdata, util::limiter::Limiter};
use log::{debug, error, info};
use regex::Regex;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use thiserror::Error;

static LIMITER: Limiter = Limiter::new(std::time::Duration::from_millis(1500));
const RATE_LIMIT_WAIT: std::time::Duration = std::time::Duration::from_secs(10);
static SPLIT_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bft\.?|\bfeat\.?|;|&").unwrap());

#[derive(Error, Debug)]
pub enum BrainzError {
    #[error("")]
    ConnectionError(#[from] reqwest::Error),
    #[error("No query parameters provided")]
    EmptyQuery,
    #[error("Failed to parse response")]
    JsonError(#[from] serde_json::Error),
    #[error("No results found")]
    EmptyResult,
}

pub async fn fetch_recordings(search: &RecordingSearch) -> Result<BrainzMetadata, BrainzError> {
    let mut parts = Vec::new();
    if let Some(part) = search.title.to_query_part("recording") {
        parts.push(part);
    }
    for part in search
        .artist
        .iter()
        .filter_map(|a| a.to_query_part("artist"))
    {
        parts.push(part);
    }
    if let Some(part) = search.album.to_query_part("release") {
        parts.push(part);
    }
    if parts.is_empty() {
        return Err(BrainzError::EmptyQuery);
    }

    let query = parts.join(" AND ");
    self::fetch_recordings_url(&query).await
}

async fn fetch_recordings_by_id(id: &str) -> Result<BrainzMetadata, BrainzError> {
    let query = format!("rid:{id}");
    fetch_recordings_url(&query).await
}

async fn fetch_recordings_url(query: &str) -> Result<BrainzMetadata, BrainzError> {
    let url = format!("http://musicbrainz.org/ws/2/recording/?limit=3&query={query}");

    let response = if let Some(cached_response) = dbdata::DB.try_get_brainz(&url) {
        cached_response
    } else {
        debug!("Fetching brainz data from {url}");
        LIMITER.wait_for_next_fetch().await;

        let response = loop {
            let response = CLIENT
                .get(&url)
                .header("User-Agent", "splamy_music_sync/0.1 ( splamyn@gmail.com )")
                .header("Accept", "application/json")
                .send()
                .await?;

            if response.status() == StatusCode::SERVICE_UNAVAILABLE {
                tokio::time::sleep(RATE_LIMIT_WAIT).await;
                LIMITER.set_last_fetch_now();
                continue;
            }

            break response;
        };

        let text = response.text().await?;
        dbdata::DB.set_brainz(&url, &text);

        text
    };

    let mut data: RecordingResponse = serde_json::from_str(&response)?;

    if let Some(recording) = data.recordings.get_mut(0) {
        let metadata = BrainzMetadata {
            title: mem::take(&mut recording.title),
            artist: recording
                .artist_credit
                .iter_mut()
                .map(|a| mem::take(&mut a.name))
                .collect(),
            album: recording
                .releases
                .get_mut(0)
                .map(|r| mem::take(&mut r.title)),
            brainz_recording_id: Some(mem::take(&mut recording.id)),
        };
        Ok(metadata)
    } else {
        Err(BrainzError::EmptyResult)
    }
}

pub async fn analyze_brainz(dlp: &BrainzMultiSearch) -> Result<BrainzMetadata, BrainzError> {
    if let Some(trackid) = &dlp.trackid {
        return fetch_recordings_by_id(trackid).await;
    }

    let mut search: Vec<RecordingSearch> = vec![];

    if dlp.album.is_some() || dlp.artist.is_some() {
        debug!("Searching by native music info");
        let artist_vec: Vec<QTerm> = dlp
            .artist
            .iter()
            .flat_map(|a| a.split(',').map(|a| QTerm::Exact(a.trim().into())))
            .collect();

        search.push(RecordingSearch {
            title: QTerm::Exact(dlp.title.clone()),
            artist: artist_vec.clone(),
            album: QTerm::exact_option(&dlp.album),
        });
        search.push(RecordingSearch {
            title: QTerm::Exact(dlp.title.clone()),
            artist: artist_vec,
            album: QTerm::None,
        });
    }

    if dlp.title.contains(" - ") {
        let parts: Vec<&str> = dlp.title.split(" - ").collect();

        search.push(RecordingSearch {
            title: QTerm::Exact(parts[1].to_string()),
            artist: split_artists(parts[0]).map(QTerm::Exact).collect(),
            album: QTerm::None,
        });

        search.push(RecordingSearch {
            title: QTerm::Exact(parts[0].to_string()),
            artist: split_artists(parts[1]).map(QTerm::Exact).collect(),
            album: QTerm::None,
        });
    }

    let mut brainz_res: Option<BrainzMetadata> = None;

    if let Some(nc_match) = search.iter().find(|rec_search| {
        rec_search.artist.iter().any(|ff| {
            ff.get_text()
                .is_some_and(|a| a.to_uppercase().contains("NIGHTCORE"))
        })
    }) {
        brainz_res = Some(BrainzMetadata {
            brainz_recording_id: None,
            title: nc_match.title.get_text().unwrap_or(&dlp.title).to_owned(),
            artist: vec!["Nightcore".to_string()],
            album: Some("Nightcore".to_string()),
        });
    }

    if brainz_res.is_none() {
        for search_opt in search {
            info!("Searching brainz by {search_opt:?}");

            match self::fetch_recordings(&search_opt).await {
                Ok(result) => {
                    debug!("Got result with {result:?}");
                    brainz_res = Some(result);
                    break;
                }
                Err(e) => {
                    error!("Error: {e:?}");
                }
            }
        }
    }

    let brainz_res = brainz_res.ok_or(BrainzError::EmptyResult);
    info!("Got brainz res: {brainz_res:?}");

    brainz_res
}

fn split_artists(artist: &str) -> impl Iterator<Item = String> + use<'_> {
    SPLIT_REGEX.split(artist).map(|s| {
        s.trim()
            .to_string()
            .replace(['(', ')', '[', ']', '【', '】'], "")
    })
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BrainzMultiSearch {
    pub trackid: Option<String>,

    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BrainzMetadata {
    pub brainz_recording_id: Option<String>,
    pub title: String,
    pub artist: Vec<String>,
    pub album: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub enum QTerm {
    #[default]
    None,
    Exact(String),
    Fuzzy(String),
}

impl QTerm {
    pub fn exact_option<T: ToString>(text: &Option<T>) -> Self {
        text.as_ref()
            .map(|s| QTerm::Exact(s.to_string()))
            .unwrap_or(QTerm::None)
    }

    #[expect(dead_code)]
    pub fn fuzzy_option<T: ToString>(text: &Option<T>) -> Self {
        text.as_ref()
            .map(|s| QTerm::Fuzzy(s.to_string()))
            .unwrap_or(QTerm::None)
    }

    pub fn to_query_part(&self, name: &str) -> Option<String> {
        match self {
            QTerm::None => None,
            QTerm::Exact(s) => Some(format!("{}:\"{}\"", name, urlencoding::encode(s))),
            QTerm::Fuzzy(s) => Some(format!("{}:{}", name, urlencoding::encode(s))),
        }
    }

    pub fn get_text(&self) -> Option<&str> {
        match self {
            QTerm::None => None,
            QTerm::Exact(s) => Some(s),
            QTerm::Fuzzy(s) => Some(s),
        }
    }
}

#[derive(Debug, Default)]
pub struct RecordingSearch {
    pub title: QTerm,
    pub artist: Vec<QTerm>,
    pub album: QTerm,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all(deserialize = "kebab-case"))]
struct RecordingResponse {
    #[expect(dead_code)]
    pub count: i32,
    #[expect(dead_code)]
    pub offset: i32,
    pub recordings: Vec<Recording>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all(deserialize = "kebab-case"))]
struct Recording {
    pub id: String,
    pub title: String,
    #[expect(dead_code)]
    pub length: Option<i32>,
    pub artist_credit: Vec<ArtistCredit>,
    #[expect(dead_code)]
    pub first_release_date: Option<String>,
    #[serde(default)]
    pub releases: Vec<Release>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all(deserialize = "kebab-case"))]
struct ArtistCredit {
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all(deserialize = "kebab-case"))]
struct Release {
    #[expect(dead_code)]
    pub id: String,
    pub title: String,
    #[expect(dead_code)]
    pub date: Option<String>,
    //media: Vec<Media>,
}
