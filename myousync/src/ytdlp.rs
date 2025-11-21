use std::path::PathBuf;

use log::{error, info};
use serde::Deserialize;
use serde_json::Value;
use tokio::process::Command;

use crate::{
    dbdata::{self},
    util::limiter::Limiter,
    MsState,
};

static LIMITER: Limiter = Limiter::new(std::time::Duration::from_secs(10));

#[derive(thiserror::Error, Debug)]
pub enum YtDlpError {
    #[error("")]
    IOError(#[from] std::io::Error),
    #[error("")]
    JsonEncodingErr(#[from] std::string::FromUtf8Error),
    #[error("")]
    JsonDeserializationErr(#[from] serde_json::Error),
    #[error("YT-dlp returned an error: {0}")]
    CommandError(String),
}

pub async fn get(s: &MsState, video_id: &str) -> Result<YtDlpResponse, YtDlpError> {
    if let Some(file) = try_get_metadata(video_id) {
        return Ok(file);
    }

    info!("Getting yt-dlp for: {}", video_id);
    LIMITER
        .wait_for_next_fetch_of_time(s.config.scrape.yt_dlp_rate)
        .await;

    let dlp_output = Command::new(&s.config.scrape.yt_dlp)
        .current_dir(s.config.paths.temp.as_path())
        .arg("--quiet")
        .arg("--dump-json")
        .arg("--no-simulate")
        .arg("--extract-audio")
        .arg("--embed-thumbnail")
        .args(["--format", "ba"])
        .args(["--sponsorblock-remove", "music_offtopic"])
        .args(["--use-extractors", "youtube"])
        .args(["--output", "%(id)s.%(ext)s"])
        .arg(format!("https://www.youtube.com/watch?v={video_id}"))
        .output()
        .await?;

    let mut json = match serde_json::from_slice::<Value>(&dlp_output.stdout) {
        Ok(json) => json,
        Err(json_err) => {
            let dlp_stderr = String::from_utf8(dlp_output.stderr)?.trim().to_string();
            error!("Got ERROR yt-dlp: {} | {}", json_err, dlp_stderr);
            return Err(YtDlpError::CommandError(dlp_stderr));
        }
    };

    if let Some(obj) = json.as_object_mut() {
        obj.remove("formats");
        obj.remove("heatmap");
        obj.remove("requested_formats");
        obj.remove("automatic_captions");
    }
    let dlp_res = serde_json::to_string(&json)?;

    dbdata::DB.set_yt_dlp(video_id, &dlp_res);

    let dlp_res: YtDlpResponse = serde_json::from_str(&dlp_res)?;

    Ok(dlp_res)
}

pub fn try_get_metadata(video_id: &str) -> Option<YtDlpResponse> {
    if let Some(dlp_res) = dbdata::DB.try_get_yt_dlp(video_id) {
        let ytdlp_data = serde_json::from_str(&dlp_res).unwrap();
        return Some(ytdlp_data);
    }
    None
}

pub fn find_local_file(s: &MsState, video_id: &str) -> Option<PathBuf> {
    let mut path = s.config.paths.temp.clone();
    path.push(format!("{}.*", video_id));
    glob::glob(path.to_str().unwrap())
        .unwrap()
        .next()
        .and_then(|r| r.ok())
}

#[derive(Debug, Deserialize)]
pub struct YtDlpResponse {
    #[expect(dead_code)]
    pub id: String,

    pub title: String,
    #[expect(dead_code)]
    pub channel: String,
    #[expect(dead_code)]
    pub duration: u32,

    pub album: Option<String>,
    pub artist: Option<String>,
    pub track: Option<String>,
}
