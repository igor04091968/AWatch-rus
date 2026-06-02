use std::fmt;

use anyhow::{Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum StatusLevel {
    Ok,
    Warn,
    Fail,
    Unknown,
}

impl StatusLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::Warn => "WARN",
            Self::Fail => "FAIL",
            Self::Unknown => "UNKNOWN",
        }
    }

    pub fn exit_code(self) -> i32 {
        match self {
            Self::Ok => exit_codes::OK,
            Self::Warn | Self::Fail | Self::Unknown => exit_codes::CHECK_FAILED,
        }
    }
}

impl fmt::Display for StatusLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<&str> for StatusLevel {
    fn from(value: &str) -> Self {
        match value.trim().to_ascii_uppercase().as_str() {
            "OK" => Self::Ok,
            "WARN" | "WARNING" => Self::Warn,
            "FAIL" | "FAILED" | "ERROR" => Self::Fail,
            _ => Self::Unknown,
        }
    }
}

pub mod exit_codes {
    pub const OK: i32 = 0;
    pub const ERROR: i32 = 1;
    pub const CHECK_FAILED: i32 = 2;
    pub const POLICY_DENIED: i32 = 3;
}

pub fn now_utc_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn parse_utc_rfc3339(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .with_context(|| format!("invalid RFC3339 timestamp: {value}"))
        .map(|ts| ts.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_level_maps_known_values() {
        assert_eq!(StatusLevel::from("OK"), StatusLevel::Ok);
        assert_eq!(StatusLevel::from("warn"), StatusLevel::Warn);
        assert_eq!(StatusLevel::from("ERROR"), StatusLevel::Fail);
        assert_eq!(StatusLevel::from("other"), StatusLevel::Unknown);
    }

    #[test]
    fn parses_zulu_timestamp() {
        let ts = parse_utc_rfc3339("2026-05-31T10:20:30Z").unwrap();
        assert_eq!(
            ts.to_rfc3339_opts(SecondsFormat::Secs, true),
            "2026-05-31T10:20:30Z"
        );
    }
}
