use std::sync::Mutex;

use chrono::{DateTime, Utc};

pub struct Limiter {
    wait_time: std::time::Duration,
    last_fetch: Mutex<DateTime<Utc>>,
}

impl Limiter {
    pub const fn new(time: std::time::Duration) -> Self {
        Self {
            wait_time: time,
            last_fetch: Mutex::new(DateTime::<Utc>::MIN_UTC),
        }
    }

    pub async fn wait_for_next_fetch(&self) {
        self.wait_for_next_fetch_of_time(self.wait_time).await
    }

    pub async fn wait_for_next_fetch_of_time(&self, wait_time: std::time::Duration) {
        let wait_time = chrono::Duration::from_std(wait_time).unwrap();
        let mut last_fetch = self.last_fetch.lock().unwrap();
        let elapsed = Utc::now() - *last_fetch;
        if elapsed < wait_time {
            let wait_time = wait_time - elapsed;
            tokio::time::sleep(wait_time.to_std().unwrap()).await;
        }
        *last_fetch = Utc::now();
    }

    pub fn set_last_fetch_now(&self) {
        *self.last_fetch.lock().unwrap() = Utc::now();
    }
}
