use std::sync::{LazyLock, Mutex};

use chrono::{DateTime, Utc};
use rusqlite::{Connection, Params};
use serde::{Deserialize, Serialize};
use serde_rusqlite::from_rows;

use crate::brainz::{BrainzMetadata, BrainzMultiSearch};

pub static DB: LazyLock<DbState> = LazyLock::new(|| DbState::new());

pub struct DbState {
    conn: Mutex<Connection>,
}

impl DbState {
    pub fn new() -> Self {
        let conn = Connection::open("ytdata.db").unwrap();

        conn.execute_batch(
            "
            BEGIN;
            CREATE TABLE IF NOT EXISTS ytdata (
                video_id TEXT PRIMARY KEY NOT NULL,
                snippet TEXT DEFAULT NULL,
                ytdlp TEXT DEFAULT NULL
            );
            CREATE TABLE IF NOT EXISTS authdata (
                access_token TEXT NOT NULL,
                refresh_token TEXT NOT NULL,
                expires_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS playlists (
                playlist_id TEXT PRIMARY KEY NOT NULL,
                etag TEXT NOT NULL,
                total_results INTEGER NOT NULL,
                fetch_time INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS playlist_items (
                playlist_id TEXT NOT NULL,
                video_id TEXT NOT NULL,
                title TEXT NOT NULL,
                artist TEXT NOT NULL,
                PRIMARY KEY (playlist_id, video_id),
                FOREIGN KEY (playlist_id) REFERENCES playlists(playlist_id) ON DELETE CASCADE
            );
            CREATE TABLE IF NOT EXISTS brainz (
                query TEXT PRIMARY KEY NOT NULL,
                fetch_time INTEGER NOT NULL,
                data TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS status (
                video_id TEXT PRIMARY KEY NOT NULL,
                last_update INTEGER NOT NULL,
                fetch_time INTEGER NOT NULL,
                fetch_status INTEGER NOT NULL,
                last_query TEXT DEFAULT NULL,
                last_result TEXT DEFAULT NULL,
                override_query TEXT DEFAULT NULL,
                override_result TEXT DEFAULT NULL
            );
            CREATE TABLE IF NOT EXISTS users (
                username TEXT PRIMARY KEY NOT NULL,
                password BLOB NOT NULL
            );
            CREATE TABLE IF NOT EXISTS kvp (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL,
                last_update INTEGER NOT NULL
            );
            COMMIT;",
        )
        .unwrap();

        Self {
            conn: Mutex::new(conn),
        }
    }

    // YT_API

    pub fn set_yt_dlp(&self, video_id: &str, dlp: &str) {
        self.set_ytdata(video_id, dlp, "ytdlp");
    }

    fn set_ytdata(&self, video_id: &str, data: &str, col: &str) {
        let conn = self.conn.lock().unwrap();
        let query = format!(
            "INSERT INTO ytdata (video_id, {col}) VALUES (?1, ?2) ON CONFLICT(video_id) DO UPDATE SET {col} = ?2");
        conn.execute(&query, (&video_id, &data)).unwrap();
    }

    pub fn try_get_yt_dlp(&self, video_id: &str) -> Option<String> {
        self.try_get_ytdata(video_id, "ytdlp")
    }

    fn try_get_ytdata(&self, video_id: &str, col: &str) -> Option<String> {
        let conn = self.conn.lock().unwrap();
        let query = format!("SELECT {col} FROM ytdata WHERE video_id = ?1");
        conn.query_row(&query, &[video_id], |row| row.get::<_, Option<String>>(0))
            .get_single_row()?
    }

    // PLAYLISTS

    pub fn try_get_playlist(&self, playlist_id: &str) -> Option<Playlist> {
        let conn = self.conn.lock().unwrap();
        let mut playlist = conn
            .query_row(
                "SELECT playlist_id, etag, total_results, fetch_time FROM playlists WHERE playlist_id = ?1",
                &[playlist_id],
                |row| {
                    Ok(Playlist {
                        playlist_id: row.get(0)?,
                        etag: row.get(1)?,
                        total_results: row.get(2)?,
                        fetch_time: DateTime::from_timestamp(row.get(3)?, 0).unwrap(),
                        items: vec![],
                    })
                },
            )
            .get_single_row()?;

        let mut stmt = conn
            .prepare("SELECT video_id, title, artist FROM playlist_items WHERE playlist_id = ?1")
            .unwrap();

        let rows = stmt
            .query_map(&[playlist_id], |row| {
                Ok(PlaylistItem {
                    video_id: row.get(0)?,
                    title: row.get(1)?,
                    artist: row.get(2)?,
                })
            })
            .unwrap()
            .map(|r| r.unwrap());

        playlist.items = rows.collect();

        Some(playlist)
    }

    pub fn set_playlist(&self, playlist: &Playlist) {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction().unwrap();

        conn.execute(
            "DELETE FROM playlists WHERE playlist_id = ?1",
            (&playlist.playlist_id,),
        )
        .unwrap();

        conn
            .execute(
                "INSERT INTO playlists (playlist_id, etag, total_results, fetch_time) VALUES (?1, ?2, ?3, ?4)",
                (
                    &playlist.playlist_id,
                    &playlist.etag,
                    playlist.total_results,
                    playlist.fetch_time.timestamp(),
                ),
            )
            .unwrap();

        let mut stmt = conn.prepare(
            "INSERT INTO playlist_items (playlist_id, video_id, title, artist) VALUES (?1, ?2, ?3, ?4)").unwrap();

        for item in &playlist.items {
            stmt.execute((
                &playlist.playlist_id,
                &item.video_id,
                &item.title,
                &item.artist,
            ))
            .unwrap();
        }

        tx.commit().unwrap();
    }

    pub fn update_playlist_fetch_time(&self, playlist_id: &str, fetch_time: DateTime<Utc>) {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE playlists SET fetch_time = ?1 WHERE playlist_id = ?2",
            (fetch_time.timestamp(), playlist_id),
        )
        .unwrap();
    }

    // YT AUTH

    pub fn try_get_auth(&self) -> Option<AuthData> {
        self.single(
            "SELECT access_token, refresh_token, expires_at FROM authdata",
            [],
        )
    }

    pub fn set_auth(&self, auth: &AuthData) {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM authdata", ()).unwrap();

        conn.execute(
            "INSERT INTO authdata (access_token, refresh_token, expires_at) VALUES (?1, ?2, ?3)",
            (&auth.access_token, &auth.refresh_token, auth.expires_at),
        )
        .unwrap();
    }

    // FILESYSTEM

    pub fn get_track_query_override(&self, video_id: &str) -> Option<String> {
        self.single(
            "SELECT override_query FROM status WHERE video_id = ?1",
            &[video_id],
        )
    }

    pub fn get_track_result_override(&self, video_id: &str) -> Option<String> {
        self.single(
            "SELECT override_result FROM status WHERE video_id = ?1",
            &[video_id],
        )
    }

    pub fn modify_video_status<F: Fn(&mut VideoStatus) -> bool>(
        &self,
        video_id: &str,
        modify: F,
    ) -> Option<VideoStatus> {
        let conn = self.conn.lock().unwrap();
        if let Some(mut video) = Self::get_video_internal(&conn, video_id) {
            let save = modify(&mut video);
            if !save {
                return None;
            }
            video.update_now();
            Self::set_full_track_status_internal(&conn, &video);
            Some(video)
        } else {
            None
        }
    }

    pub fn get_all_videos(&self) -> Vec<VideoStatus> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT video_id, last_update, fetch_time, fetch_status, last_query, last_result, override_query, override_result FROM status")
            .unwrap();
        let rows = stmt
            .query_map([], Self::map_video_status)
            .unwrap()
            .map(|r| r.unwrap());

        rows.collect()
    }

    pub fn get_all_ids(&self) -> Vec<String> {
        self.all("SELECT video_id FROM status", [])
    }

    pub fn get_all_unprocessed_ids(&self) -> Vec<String> {
        self.all(
            "SELECT video_id FROM status WHERE fetch_status IN (0, 1)",
            [],
        )
    }

    pub fn get_video(&self, video_id: &str) -> Option<VideoStatus> {
        let conn = self.conn.lock().unwrap();
        Self::get_video_internal(&conn, video_id)
    }

    fn get_video_internal(conn: &Connection, video_id: &str) -> Option<VideoStatus> {
        conn.query_row_and_then("SELECT video_id, last_update, fetch_time, fetch_status, last_query, last_result, override_query, override_result FROM status WHERE video_id = ?1",
            &[video_id],
            Self::map_video_status)
            .get_single_row()
    }

    fn map_video_status(row: &rusqlite::Row) -> rusqlite::Result<VideoStatus> {
        Ok(VideoStatus {
            video_id: row.get(0)?,
            last_update: row.get(1)?,
            fetch_time: row.get(2)?,
            fetch_status: FetchStatus::try_from(row.get::<_, i64>(3)?).unwrap(),
            last_query: row
                .get::<_, Option<String>>(4)?
                .map(|s| serde_json::from_str(&s).unwrap()),
            last_result: row
                .get::<_, Option<String>>(5)?
                .map(|s| serde_json::from_str(&s).unwrap()),
            override_query: row
                .get::<_, Option<String>>(6)?
                .map(|s| serde_json::from_str(&s).unwrap()),
            override_result: row
                .get::<_, Option<String>>(7)?
                .map(|s| serde_json::from_str(&s).unwrap()),
        })
    }

    pub fn set_full_track_status(&self, status: &VideoStatus) {
        let conn = self.conn.lock().unwrap();
        Self::set_full_track_status_internal(&conn, status)
    }

    fn set_full_track_status_internal(conn: &Connection, status: &VideoStatus) {
        conn
            .execute(
                "INSERT INTO status (video_id, last_update, fetch_time, fetch_status, last_query, last_result, override_query, override_result)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(video_id)
                 DO UPDATE SET last_update = ?2, fetch_time = ?3, fetch_status = ?4, last_query = ?5, last_result = ?6, override_query = ?7, override_result = ?8",
                (
                    &status.video_id,
                    status.last_update,
                    status.fetch_time,
                    status.fetch_status as i64,
                    status.last_query.as_ref().map(|q| serde_json::to_string(q).unwrap()),
                    status.last_result.as_ref().map(|r| serde_json::to_string(r).unwrap()),
                    status.override_query.as_ref().map(|q| serde_json::to_string(q).unwrap()),
                    status.override_result.as_ref().map(|r| serde_json::to_string(r).unwrap()),
                )
            )
            .unwrap();
    }

    // BRAINZ

    pub fn try_get_brainz(&self, query: &str) -> Option<String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT data FROM brainz WHERE query = ?1",
            &[query],
            |row| row.get::<_, Option<String>>(0),
        )
        .get_single_row()?
    }

    pub fn set_brainz(&self, query: &str, data: &str) {
        let conn = self.conn.lock().unwrap();
        conn
            .execute(
                "INSERT INTO brainz (query, fetch_time, data) VALUES (?1, ?2, ?3) ON CONFLICT(query) DO UPDATE SET fetch_time = ?2, data = ?3",
                (&query, Utc::now().timestamp(), &data))
            .unwrap();
    }

    // User

    pub fn get_user(&self, username: &str) -> Option<UserData> {
        self.single(
            "SELECT username, password FROM users WHERE username = ?1",
            &[username],
        )
    }

    pub fn get_key(&self, key: &str) -> Option<String> {
        self.single("SELECT value FROM kvp WHERE key = ?1", &[key])
    }

    pub fn set_key(&self, key: &str, value: &str) {
        let conn = self.conn.lock().unwrap();
        conn
            .execute(
                "INSERT INTO kvp (key, value, last_update) VALUES (?1, ?2, ?3) ON CONFLICT(key) DO UPDATE SET value = ?2, last_update = ?3",
                (&key, &value, Utc::now().timestamp()))
            .unwrap();
    }

    // Helper

    fn all<T: serde::de::DeserializeOwned, P: Params>(&self, query: &str, params: P) -> Vec<T> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(query).unwrap();
        let res = stmt.query(params);
        match res {
            Ok(res) => from_rows::<T>(res).map(|r| r.unwrap()).collect(),
            Err(rusqlite::Error::QueryReturnedNoRows) => vec![],
            Err(e) => panic!("{}", e),
        }
    }

    fn single<T: serde::de::DeserializeOwned, P: Params>(
        &self,
        query: &str,
        params: P,
    ) -> Option<T> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(query).unwrap();
        let res = stmt.query(params).get_single_row()?;
        let mut rows = from_rows::<T>(res);
        rows.next()?.ok()
    }
}

// extension method for query Result<T>
trait MyExtension<T> {
    fn get_single_row(self) -> Option<T>;
}

impl<T> MyExtension<T> for rusqlite::Result<T> {
    fn get_single_row(self) -> Option<T> {
        match self {
            Ok(s) => Some(s),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => panic!("{}", e),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct AuthData {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
}

// #[derive(Queryable, Identifiable, Selectable, Debug, PartialEq, Clone)]
// #[diesel(table_name = playlists)]
pub struct Playlist {
    pub playlist_id: String,
    pub etag: String,
    pub total_results: u32,
    pub fetch_time: DateTime<Utc>,
    pub items: Vec<PlaylistItem>,
}

#[derive(Debug)]
pub struct PlaylistItem {
    pub video_id: String,
    pub title: String,
    pub artist: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize, Default)]
pub enum FetchStatus {
    #[default]
    NotFetched = 0,
    Fetched,
    FetchError,
    BrainzError,
    Categorized,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct VideoStatus {
    pub video_id: String,
    pub fetch_time: u64,
    pub last_update: u64,
    pub fetch_status: FetchStatus,
    pub last_query: Option<BrainzMultiSearch>,
    pub last_result: Option<BrainzMetadata>,
    pub override_query: Option<BrainzMultiSearch>,
    pub override_result: Option<BrainzMetadata>,
}

impl VideoStatus {
    pub fn update_now(&mut self) {
        self.last_update = Utc::now().timestamp() as u64;
    }

    pub fn is_downloaded(&self) -> bool {
        self.fetch_status != FetchStatus::NotFetched && self.fetch_status != FetchStatus::FetchError
    }
}

impl TryFrom<i64> for FetchStatus {
    type Error = ();

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(FetchStatus::NotFetched),
            1 => Ok(FetchStatus::Fetched),
            2 => Ok(FetchStatus::FetchError),
            3 => Ok(FetchStatus::BrainzError),
            4 => Ok(FetchStatus::Categorized),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UserData {
    pub username: String,
    pub password: String,
}
