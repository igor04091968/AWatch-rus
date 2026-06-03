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

pub mod runtime_guard {
    use anyhow::{Result, bail};

    const TEST_NET_MARKERS: [&str; 3] = ["192.0.2.", "198.51.100.", "203.0.113."];
    const CONTAINS_PLACEHOLDERS: [&str; 2] = ["HOST-EXAMPLE", "WINDOWS_USER_EXAMPLE"];
    const EXACT_PLACEHOLDERS: [&str; 9] = [
        "CHANGE_ME",
        "CHANGEME",
        "REPLACE_ME",
        "REPLACE-ME",
        "YOUR_TOKEN",
        "YOUR_API_KEY",
        "TOKEN",
        "SECRET",
        "PASSWORD",
    ];

    pub fn is_runtime_placeholder(value: &str) -> bool {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return true;
        }
        let normalized = trimmed.to_ascii_uppercase();
        TEST_NET_MARKERS
            .iter()
            .any(|marker| normalized.contains(marker))
            || CONTAINS_PLACEHOLDERS
                .iter()
                .any(|placeholder| normalized.contains(placeholder))
            || EXACT_PLACEHOLDERS
                .iter()
                .any(|placeholder| normalized == *placeholder)
            || normalized == "EXAMPLE"
            || normalized.contains("-EXAMPLE")
            || normalized.contains("_EXAMPLE")
            || normalized.contains("EXAMPLE.")
            || normalized.contains(".EXAMPLE")
            || normalized.starts_with("YOUR_")
            || (normalized.starts_with('<') && normalized.ends_with('>'))
    }

    pub fn is_secret_placeholder(value: &str) -> bool {
        is_runtime_placeholder(value)
            || matches!(
                value.trim().to_ascii_uppercase().as_str(),
                "API_KEY" | "INFLUX_TOKEN" | "WRITE_TOKEN" | "BEARER_TOKEN"
            )
    }

    pub fn ensure_runtime_value(name: &str, value: &str, context: &str) -> Result<()> {
        if is_runtime_placeholder(value) {
            bail!("{name} contains an empty/example/TEST-NET value while {context}");
        }
        Ok(())
    }

    pub fn ensure_secret_value(name: &str, value: &str, context: &str) -> Result<()> {
        if is_secret_placeholder(value) {
            bail!("{name} contains an empty/example secret value while {context}");
        }
        Ok(())
    }

    pub fn ensure_runtime_values<'a>(
        name: &str,
        values: impl IntoIterator<Item = &'a String>,
        context: &str,
    ) -> Result<()> {
        let mut empty = true;
        for value in values {
            empty = false;
            ensure_runtime_value(name, value, context)?;
        }
        if empty {
            bail!("{name} contains an empty/example value while {context}");
        }
        Ok(())
    }

    pub fn ensure_influx_runtime_config(
        prefix: &str,
        url: &str,
        org: &str,
        bucket: &str,
        token: &str,
        hosts: &[String],
    ) -> Result<()> {
        let context = format!("{prefix}_ENABLED=true");
        ensure_runtime_value(&format!("{prefix}_URL"), url, &context)?;
        ensure_runtime_value(&format!("{prefix}_ORG"), org, &context)?;
        ensure_runtime_value(&format!("{prefix}_BUCKET"), bucket, &context)?;
        ensure_secret_value(&format!("{prefix}_TOKEN"), token, &context)?;
        ensure_runtime_values(&format!("{prefix}_HOSTS"), hosts, &context)?;
        Ok(())
    }
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

    #[test]
    fn runtime_guard_detects_public_placeholders() {
        assert!(runtime_guard::is_runtime_placeholder("HOST-EXAMPLE"));
        assert!(runtime_guard::is_runtime_placeholder(
            "http://192.0.2.10:8086"
        ));
        assert!(runtime_guard::is_runtime_placeholder("<TOKEN>"));
        assert!(runtime_guard::is_secret_placeholder("CHANGE_ME"));
        assert!(!runtime_guard::is_runtime_placeholder("aw_metrics"));
        assert!(!runtime_guard::is_runtime_placeholder("proxmox"));
        assert!(!runtime_guard::is_secret_placeholder(
            "prod-write-token-value"
        ));
    }

    #[test]
    fn runtime_guard_validates_full_influx_config() {
        let hosts = vec!["WINDOWS-HOST".to_string()];
        runtime_guard::ensure_influx_runtime_config(
            "AW_WORKTIME_INFLUX",
            "http://influxdb.internal:8086",
            "proxmox",
            "aw_metrics",
            "prod-write-token-value",
            &hosts,
        )
        .unwrap();

        let err = runtime_guard::ensure_influx_runtime_config(
            "AW_WORKTIME_INFLUX",
            "http://influxdb.internal:8086",
            "proxmox",
            "aw_metrics",
            "CHANGE_ME",
            &hosts,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("AW_WORKTIME_INFLUX_TOKEN"));
    }
}
