use std::time::{Duration, SystemTime};

pub fn from_timestamp(timestamp: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH
        .checked_add(Duration::from_secs(timestamp))
        .unwrap()
}

pub fn to_timestamp(time: SystemTime) -> u64 {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
