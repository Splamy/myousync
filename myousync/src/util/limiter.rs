use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

pub struct Limiter {
    wait_time: Duration,
    times: Mutex<LimiterTimes>,
}

struct LimiterTimes {
    next_allowed_time: Option<Instant>,
    claimed_next_time: Option<Instant>,
}

impl Limiter {
    const CHECK_TOLERANCE: Duration = Duration::from_millis(15);

    pub const fn new(time: Duration) -> Self {
        Self {
            wait_time: time,
            times: Mutex::new(LimiterTimes {
                next_allowed_time: None,
                claimed_next_time: None,
            }),
        }
    }

    pub async fn wait_for_next_fetch(&self) {
        loop {
            match self.try_claim_next() {
                None => return,
                Some(wait_time) => tokio::time::sleep(wait_time).await,
            }
        }
    }

    fn try_claim_next(&self) -> Option<Duration> {
        let mut times = self.times.lock().unwrap();

        let now = Instant::now();

        let Some(next_allowed_time) = times.next_allowed_time else {
            times.next_allowed_time = Some(now + self.wait_time);
            return None;
        };

        if now
            > next_allowed_time
                .checked_sub(Self::CHECK_TOLERANCE)
                .unwrap_or(next_allowed_time)
        {
            times.next_allowed_time = Some(now + self.wait_time);
            return None;
        }

        let new_claimed_time = if let Some(claimed_next_time) = times.claimed_next_time
            && claimed_next_time > next_allowed_time
        {
            claimed_next_time + self.wait_time
        } else {
            next_allowed_time
        };

        times.claimed_next_time = Some(new_claimed_time);
        Some(now - new_claimed_time)
    }

    pub fn allow_next_fetch_in(&self, duration: Duration) {
        let mut times = self.times.lock().unwrap();

        let now = Instant::now();

        times.next_allowed_time = Some(now + duration);
        times.claimed_next_time = None;
    }
}
