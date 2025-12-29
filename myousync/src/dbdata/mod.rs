mod models;
mod sql_system_time;

use std::{
    fmt::Debug,
    sync::{LazyLock, Mutex},
    time::SystemTime,
};

use log::info;
use rusqlite::{Connection, Params};
use serde_rusqlite::from_rows;

pub use models::*;
pub use sql_system_time::SqlSystemTime;

pub static DB: LazyLock<DbState> = LazyLock::new(|| DbState::new());
const DB_VERSION: u32 = 2;

pub struct DbState {
    conn: Mutex<Connection>,
}

impl DbState {
    pub fn new() -> Self {
        Self::new_at("ytdata.db")
    }

    pub fn new_at(dbpath: &str) -> Self {
        let conn = Connection::open(dbpath).unwrap();

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
            CREATE TABLE IF NOT EXISTS playlist_config (
                playlist_id TEXT PRIMARY KEY NOT NULL,
                jelly_playlist_id TEXT DEFAULT NULL,
                enabled INTEGER NOT NULL DEFAULT 0
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
                position INTEGER NOT NULL,
                jelly_status INTEGER NOT NULL DEFAULT 0,
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
                override_result TEXT DEFAULT NULL,
                last_error TEXT DEFAULT NULL,
                jelly_id TEXT DEFAULT NULL
            );
            CREATE TABLE IF NOT EXISTS users (
                username TEXT PRIMARY KEY NOT NULL,
                password TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS kvp (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL,
                last_update INTEGER NOT NULL
            );
            COMMIT;",
        )
        .unwrap();

        let state = Self {
            conn: Mutex::new(conn),
        };

        Self::migrate(&state);

        state
    }

    fn migrate(state: &Self) {
        let cur_ver: u32 = state
            .get_key("version")
            .map_or(DB_VERSION, |v| v.parse().expect("Invalid version"));

        if cur_ver >= DB_VERSION {
            return;
        }

        info!("Upgrading database from version {cur_ver} to {DB_VERSION}",);

        let mut new_ver = cur_ver;
        if new_ver == 0 {
            new_ver = 1;
            let con = &state.conn.lock().unwrap();
            con.run("ALTER TABLE status ADD COLUMN last_error TEXT DEFAULT NULL");
            Self::set_key_with_con(con, "version", &new_ver.to_string());
        }
        if new_ver == 1 {
            new_ver = 2;
            let con = &state.conn.lock().unwrap();

            con.run_all(&[
                "ALTER TABLE status ADD COLUMN jelly_id TEXT DEFAULT NULL",
                "ALTER TABLE playlist_items ADD COLUMN position INTEGER DEFAULT 0",
                "ALTER TABLE playlist_items ADD COLUMN jelly_status INTEGER NOT NULL DEFAULT 0",
                "DELETE FROM users",
                "ALTER TABLE users DROP COLUMN password",
                "ALTER TABLE users ADD COLUMN password TEXT NOT NULL DEFAULT ''",
            ]);
            Self::set_key_with_con(con, "version", &new_ver.to_string());
        }

        info!("Database upgrade complete");
    }

    // YT_API

    pub fn set_yt_dlp(&self, video_id: &YoutubeVideoId, dlp: &str) {
        self.set_ytdata(video_id, dlp, "ytdlp");
    }

    pub fn delete_yt_data(&self, video_id: &YoutubeVideoId) {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM ytdata WHERE video_id = ?1", [video_id])
            .unwrap();
    }

    fn set_ytdata(&self, video_id: &YoutubeVideoId, data: &str, col: &str) {
        let conn = self.conn.lock().unwrap();
        let query = format!(
            "INSERT INTO ytdata (video_id, {col}) VALUES (?1, ?2) ON CONFLICT(video_id) DO UPDATE SET {col} = ?2"
        );
        conn.execute(&query, (&video_id, &data)).unwrap();
    }

    pub fn try_get_yt_dlp(&self, video_id: &YoutubeVideoId) -> Option<String> {
        self.try_get_ytdata(video_id, "ytdlp")
    }

    fn try_get_ytdata(&self, video_id: &YoutubeVideoId, col: &str) -> Option<String> {
        let conn = self.conn.lock().unwrap();
        let query = format!("SELECT {col} FROM ytdata WHERE video_id = ?1");
        conn.query_row(&query, [video_id], |row| row.get::<_, Option<String>>(0))
            .get_single_row()?
    }

    // PLAYLIST Config

    pub fn get_playlist_config(&self) -> Vec<PlaylistConfig> {
        self.all(
            "SELECT playlist_id, jelly_playlist_id, enabled FROM playlist_config",
            (),
        )
    }

    pub fn add_playlist_config(&self, playlist_config: &PlaylistConfig) {
        let conn = self.conn.lock().unwrap();
        let query = "INSERT INTO playlist_config (playlist_id, jelly_playlist_id, enabled) 
               VALUES (?1, ?2, ?3) 
               ON CONFLICT(playlist_id) DO UPDATE SET jelly_playlist_id = ?2, enabled = ?3";
        conn.execute(
            query,
            (
                &playlist_config.playlist_id,
                &playlist_config.jelly_playlist_id,
                playlist_config.enabled,
            ),
        )
        .unwrap();
    }

    pub fn delete_playlist_config(&self, playlist_id: &YoutubePlaylistId) {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM playlist_config WHERE playlist_id = ?1",
            (playlist_id,),
        )
        .unwrap();
    }

    // PLAYLISTS

    pub fn try_get_playlist(&self, playlist_id: &YoutubePlaylistId) -> Option<Playlist> {
        let conn = self.conn.lock().unwrap();
        let mut playlist = conn
            .query_row(
                "SELECT playlist_id, etag, total_results, fetch_time FROM playlists WHERE playlist_id = ?1",
                [playlist_id],
                |row| {
                    Ok(Playlist {
                        playlist_id: row.get(0)?,
                        etag: row.get(1)?,
                        total_results: row.get(2)?,
                        fetch_time: row.get(3)?,
                        items: vec![],
                    })
                },
            )
            .get_single_row()?;

        let mut stmt = conn
            .prepare(
                "SELECT video_id, title, artist, position, jelly_status FROM playlist_items WHERE playlist_id = ?1",
            )
            .unwrap();

        let rows = stmt
            .query_map([playlist_id], |row| {
                Ok(PlaylistItem {
                    video_id: row.get("video_id")?,
                    title: row.get("title")?,
                    artist: row.get("artist")?,
                    position: row.get("position")?,
                    jelly_status: row.get("jelly_status")?,
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
                    playlist.fetch_time,
                ),
            )
            .unwrap();

        let mut stmt = conn.prepare(
            "INSERT INTO playlist_items (playlist_id, video_id, title, artist, position) VALUES (?1, ?2, ?3, ?4, ?5)").unwrap();

        for item in &playlist.items {
            stmt.execute((
                &playlist.playlist_id,
                &item.video_id,
                &item.title,
                &item.artist,
                &item.position,
            ))
            .unwrap();
        }

        tx.commit().unwrap();
    }

    pub fn update_playlist_fetch_time(
        &self,
        playlist_id: &YoutubePlaylistId,
        fetch_time: SystemTime,
    ) {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE playlists SET fetch_time = ?1 WHERE playlist_id = ?2",
            (SqlSystemTime(fetch_time), playlist_id),
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

    pub fn get_track_query_override(&self, video_id: &YoutubeVideoId) -> Option<String> {
        self.single::<Option<String>, _>(
            "SELECT override_query FROM status WHERE video_id = ?1",
            (video_id,),
        )
        .flatten()
    }

    pub fn get_track_result_override(&self, video_id: &YoutubeVideoId) -> Option<String> {
        self.single::<Option<String>, _>(
            "SELECT override_result FROM status WHERE video_id = ?1",
            (video_id,),
        )
        .flatten()
    }

    pub fn modify_video_status<F: Fn(&mut VideoStatus) -> bool>(
        &self,
        video_id: &YoutubeVideoId,
        modify: F,
    ) -> Option<VideoStatus> {
        if let Some(mut video) = Self::get_video(self, video_id) {
            let save = modify(&mut video);
            if !save {
                return None;
            }
            video.update_now();
            Self::set_full_track_status(self, &video);
            Some(video)
        } else {
            None
        }
    }

    pub fn get_all_videos(&self) -> Vec<VideoStatus> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT * FROM status").unwrap();
        let rows = stmt
            .query_map([], Self::map_video_status)
            .unwrap()
            .map(|r| r.unwrap());

        rows.collect()
    }

    pub fn get_all_ids(&self) -> Vec<YoutubeVideoId> {
        self.all("SELECT video_id FROM status", [])
    }

    pub fn get_video_fetch_status(&self, video_id: &YoutubeVideoId) -> Option<FetchStatus> {
        self.single::<i64, _>(
            "SELECT fetch_status FROM status WHERE video_id = ?1",
            [video_id],
        )
        .and_then(|s| FetchStatus::try_from(s).ok())
    }

    pub fn get_all_unprocessed_ids(&self) -> Vec<YoutubeVideoId> {
        self.all(
            "SELECT video_id FROM status WHERE fetch_status IN (0, 1)",
            [],
        )
    }

    pub fn get_video(&self, video_id: &YoutubeVideoId) -> Option<VideoStatus> {
        let conn = self.conn.lock().unwrap();
        Self::get_video_internal(&conn, video_id)
    }

    fn get_video_internal(conn: &Connection, video_id: &YoutubeVideoId) -> Option<VideoStatus> {
        conn.query_row_and_then(
            "SELECT * FROM status WHERE video_id = ?1",
            [video_id],
            Self::map_video_status,
        )
        .get_single_row()
    }

    fn map_video_status(row: &rusqlite::Row) -> rusqlite::Result<VideoStatus> {
        Ok(VideoStatus {
            video_id: row.get("video_id")?,
            fetch_time: row.get("fetch_time")?,
            fetch_status: row.get("fetch_status")?,
            last_update: row.get("last_update")?,
            last_query: row
                .get::<_, Option<String>>("last_query")?
                .map(|s| serde_json::from_str(&s).unwrap()),
            last_result: row
                .get::<_, Option<String>>("last_result")?
                .map(|s| serde_json::from_str(&s).unwrap()),
            last_error: row.get("last_error")?,
            override_query: row
                .get::<_, Option<String>>("override_query")?
                .map(|s| serde_json::from_str(&s).unwrap()),
            override_result: row
                .get::<_, Option<String>>("override_result")?
                .map(|s| serde_json::from_str(&s).unwrap()),
            jelly_id: row.get("jelly_id")?,
        })
    }

    pub fn set_full_track_status(&self, status: &VideoStatus) {
        let conn = self.conn.lock().unwrap();
        Self::set_full_track_status_internal(&conn, status);
    }

    fn set_full_track_status_internal(conn: &Connection, status: &VideoStatus) {
        conn
            .execute(
                "INSERT INTO status (video_id, last_update, fetch_time, fetch_status, last_query, last_result, override_query, override_result, last_error, jelly_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(video_id)
                 DO UPDATE SET last_update = ?2, fetch_time = ?3, fetch_status = ?4, last_query = ?5, last_result = ?6, override_query = ?7, override_result = ?8, last_error = ?9, jelly_id = ?10",
                (
                    &status.video_id,
                    status.last_update,
                    status.fetch_time,
                    status.fetch_status,
                    status.last_query.as_ref().map(|q| serde_json::to_string(q).unwrap()),
                    status.last_result.as_ref().map(|r| serde_json::to_string(r).unwrap()),
                    status.override_query.as_ref().map(|q| serde_json::to_string(q).unwrap()),
                    status.override_result.as_ref().map(|r| serde_json::to_string(r).unwrap()),
                    status.last_error.as_ref(),
                    status.jelly_id.as_ref()
                )
            )
            .unwrap();
    }

    pub fn set_videos_reindex<T: AsRef<str>>(&self, video_ids: &[T]) {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction().unwrap();

        for video_id in video_ids {
            conn.execute(
                "UPDATE status SET fetch_status = 1 WHERE video_id = ?1 AND fetch_status = 4",
                (video_id.as_ref(),),
            )
            .unwrap();
        }

        tx.commit().unwrap();
    }

    // BRAINZ

    pub fn try_get_brainz(&self, query: &str) -> Option<String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row("SELECT data FROM brainz WHERE query = ?1", [query], |row| {
            row.get::<_, Option<String>>(0)
        })
        .get_single_row()?
    }

    pub fn set_brainz(&self, query: &str, data: &str) {
        let conn = self.conn.lock().unwrap();
        conn
            .execute(
                "INSERT INTO brainz (query, fetch_time, data) VALUES (?1, ?2, ?3) ON CONFLICT(query) DO UPDATE SET fetch_time = ?2, data = ?3",
                (&query, SqlSystemTime::now(), &data))
            .unwrap();
    }

    // Jellyfin

    pub fn get_jellyfin_unsynced(&self, has_jid: Option<bool>) -> Vec<JellySyncStatus> {
        let conn = self.conn.lock().unwrap();

        let mut query: String = "
            SELECT i.playlist_id, i.jelly_status, i.video_id, s.fetch_status, s.jelly_id
            FROM playlist_config p
            LEFT JOIN playlist_items i on p.playlist_id  = i.playlist_id 
            LEFT JOIN status s on s.video_id = i.video_id
            WHERE p.enabled <> 0 
            AND p.jelly_playlist_id IS NOT NULL
            AND i.jelly_status <> 1
            AND s.fetch_status = 4
            "
        .into();

        if let Some(has_jid) = has_jid {
            query.push_str(if has_jid {
                " AND s.jelly_id IS NOT NULL"
            } else {
                " AND s.jelly_id IS NULL"
            });
        }

        let mut stmt = conn.prepare(&query).unwrap();

        let rows = stmt
            .query_map([], |row| {
                Ok(JellySyncStatus {
                    playlist_id: row.get("playlist_id")?,
                    video_id: row.get("video_id")?,
                    fetch_status: row.get("fetch_status")?,
                    jelly_status: row.get("jelly_status")?,
                    jelly_id: row.get("jelly_id")?,
                })
            })
            .unwrap()
            .map(|r| r.unwrap());

        rows.collect()
    }

    pub fn get_jellyfin_playlist_item_ids(
        &self,
        youtube_playlist_id: &YoutubePlaylistId,
    ) -> Vec<String> {
        self.all(
            "
            SELECT s.jelly_id
            FROM playlist_items i
            LEFT JOIN status s on s.video_id = i.video_id
            WHERE i.playlist_id = ?1
            AND s.jelly_id IS NOT NULL
            ORDER BY i.position ASC
            ",
            (youtube_playlist_id,),
        )
    }

    pub fn set_jellyfin_items_to_synced(&self, youtube_playlist_id: &YoutubePlaylistId) {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "
            UPDATE playlist_items
            SET jelly_status = 1
            FROM status WHERE status.video_id = playlist_items.video_id
            AND playlist_id = ?1
            AND jelly_id IS NOT NULL
            ",
            (youtube_playlist_id,),
        )
        .unwrap();
    }

    pub fn set_jellyfin_id(&self, video_id: &YoutubeVideoId, jelly_id: &JellyItemId) -> bool {
        let conn = self.conn.lock().unwrap();
        let count = conn
            .execute(
                "UPDATE status SET jelly_id = ?1 WHERE video_id = ?2",
                (jelly_id, video_id),
            )
            .unwrap();
        count > 0
    }

    // User

    pub fn get_user(&self, username: &str) -> Option<UserData> {
        self.single(
            "SELECT username, password FROM users WHERE username = ?1",
            (username,),
        )
    }

    pub fn add_user(&self, username: &str, hashed_password: &str) {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO users (username, password) VALUES (?1, ?2)",
            (username, hashed_password),
        )
        .unwrap();
    }

    pub fn delete_user(&self, username: &str) -> usize {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM users WHERE username = ?1", (username,))
            .unwrap()
    }

    pub fn get_key(&self, key: &str) -> Option<String> {
        self.single("SELECT value FROM kvp WHERE key = ?1", [key])
    }

    pub fn set_key(&self, key: &str, value: &str) {
        let conn = self.conn.lock().unwrap();
        Self::set_key_with_con(&conn, key, value);
    }

    pub fn delete_key(&self, key: &str) -> Option<String> {
        self.single("DELETE FROM kvp WHERE key = ?1", [key])
    }

    pub fn set_key_with_con(conn: &std::sync::MutexGuard<'_, Connection>, key: &str, value: &str) {
        conn
            .execute(
                "INSERT INTO kvp (key, value, last_update) VALUES (?1, ?2, ?3) ON CONFLICT(key) DO UPDATE SET value = ?2, last_update = ?3",
                (&key, &value, SqlSystemTime::now()))
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

    fn single<T: serde::de::DeserializeOwned + Debug, P: Params>(
        &self,
        query: &str,
        params: P,
    ) -> Option<T> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(query).unwrap();
        let res = stmt.query(params).get_single_row()?;
        let mut rows = from_rows::<T>(res);
        rows.next().map(|row| row.unwrap())
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

trait ConnectionExt {
    fn run(&self, query: &str);
    fn run_all(&self, queries: &[&str]);
}

impl ConnectionExt for Connection {
    fn run(&self, query: &str) {
        self.execute(query, ()).unwrap();
    }

    fn run_all(&self, queries: &[&str]) {
        let tx = self.unchecked_transaction().unwrap();
        for query in queries {
            self.execute(query, ()).unwrap();
        }
        tx.commit().unwrap();
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::dbdata::sql_system_time::SqlSystemTime;

    #[test]
    fn test_read_write_kvp() {
        let db = DbState::new_at(":memory:");
        db.set_key("hi", "ho");
        let res = db.get_key("hi");
        assert_eq!(res, Some("ho".to_string()));
    }

    #[test]
    fn test_read_write_auth_data() {
        let db = DbState::new_at(":memory:");
        let now = SqlSystemTime::now_rounded();

        let data = AuthData {
            access_token: "a".to_string(),
            refresh_token: "b".to_string(),
            expires_at: now,
        };

        db.set_auth(&data);

        let read_data = db.try_get_auth().expect("Auth got missing");
        assert_eq!(*data.expires_at, *read_data.expires_at);
    }
}
