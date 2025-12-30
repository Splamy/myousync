use std::{
    ops::Deref,
    time::{Duration, SystemTime},
};

use rusqlite::{
    ToSql,
    types::{FromSql, FromSqlResult, ToSqlOutput, ValueRef},
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::util::time::{from_timestamp, to_timestamp};

#[derive(Debug, Copy, Clone)]
pub struct SqlSystemTime(pub SystemTime);

impl SqlSystemTime {
    pub fn now() -> Self {
        Self(SystemTime::now())
    }

    pub fn now_rounded() -> Self {
        Self::now().round()
    }

    pub fn round(&self) -> Self {
        let duration = self.0.duration_since(SystemTime::UNIX_EPOCH).unwrap();
        let duration = Duration::from_secs(duration.as_secs());
        Self(SystemTime::UNIX_EPOCH + duration)
    }
}

impl Deref for SqlSystemTime {
    type Target = SystemTime;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<SystemTime> for SqlSystemTime {
    fn from(value: SystemTime) -> Self {
        Self(value)
    }
}

// Sql

impl ToSql for SqlSystemTime {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(
            i64::try_from(to_timestamp(self.0))
                .map_err(|err| rusqlite::Error::ToSqlConversionFailure(err.into()))?,
        ))
    }
}

impl FromSql for SqlSystemTime {
    fn column_result(value: ValueRef) -> FromSqlResult<Self> {
        u64::column_result(value).map(|num| Self(from_timestamp(num)))
    }
}

// Serde

impl Serialize for SqlSystemTime {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(to_timestamp(self.0))
    }
}

impl<'de> Deserialize<'de> for SqlSystemTime {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: u64 = Deserialize::deserialize(deserializer)?;
        Ok(Self(from_timestamp(s)))
    }
}
