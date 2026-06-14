#![deny(unsafe_op_in_unsafe_fn)]

//! Shared production primitives for AWatch-rus.
//!
//! This crate intentionally stays small and dependency-light. It contains the
//! status, exit-code and runtime-configuration guardrails that are reused by
//! operational binaries and health/check tooling. Keep business-specific portal,
//! DLP or workforce logic out of this crate.

use std::fmt;

use anyhow::{Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

/// Normalized health/check status used by CLI tools, probes and JSON payloads.
///
/// CONTRACT: serialized values are uppercase and must remain stable because
/// deployment scripts, smoke checks and dashboards can key off these strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum StatusLevel {
    /// Component is healthy and the check passed.
    Ok,
    /// Component works, but a risk or degraded condition needs attention.
    Warn,
    /// Component check failed or a required dependency is unavailable.
    Fail,
    /// Component did not provide enough information for a reliable status.
    Unknown,
}

impl StatusLevel {
    /// Return the stable uppercase representation used in human and JSON output.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::Warn => "WARN",
            Self::Fail => "FAIL",
            Self::Unknown => "UNKNOWN",
        }
    }

    /// Map status to the process exit code expected by operational checks.
    ///
    /// CONTRACT: `WARN` exits as a failed check rather than success so that
    /// automation does not silently ignore degraded production state.
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

/// Stable process exit codes for AWatch-rus operational binaries.
///
/// CONTRACT: keep these numeric values stable. Shell scripts, systemd units,
/// smoke tests and runbooks can depend on them.
pub mod exit_codes {
    /// Successful execution.
    pub const OK: i32 = 0;
    /// Unexpected runtime or IO error.
    pub const ERROR: i32 = 1;
    /// Health/check policy failed or returned a degraded status.
    pub const CHECK_FAILED: i32 = 2;
    /// A safety policy denied a requested action.
    pub const POLICY_DENIED: i32 = 3;
}

/// Return the current UTC timestamp in compact RFC3339/Zulu format.
pub fn now_utc_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Parse an RFC3339 timestamp and normalize it to UTC.
pub fn parse_utc_rfc3339(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .with_context(|| format!("invalid RFC3339 timestamp: {value}"))
        .map(|ts| ts.with_timezone(&Utc))
}

/// Runtime configuration guardrails.
///
/// SECURITY: these helpers are deliberately conservative. They reject empty,
/// documentation, TEST-NET and common placeholder values before a component is
/// allowed to run in production mode. This prevents demo-safe examples from
/// accidentally becoming live runtime configuration.
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

    /// Return true when a value looks like a public/demo placeholder.
    ///
    /// RATIONALE: AWatch-rus documentation intentionally uses TEST-NET ranges
    /// and HOST-EXAMPLE markers. Production binaries should fail closed when
    /// such values reach runtime configuration.
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

    /// Return true when a value is unsafe for a secret-like configuration field.
    pub fn is_secret_placeholder(value: &str) -> bool {
        is_runtime_placeholder(value)
            || matches!(
                value.trim().to_ascii_uppercase().as_str(),
                "API_KEY" | "INFLUX_TOKEN" | "WRITE_TOKEN" | "BEARER_TOKEN"
            )
    }

    /// Ensure a required runtime value is not empty or demo-only.
    ///
    /// SECURITY: callers should invoke this before opening network connections,
    /// starting ingestion or enabling exporters in production mode.
    pub fn ensure_runtime_value(name: &str, value: &str, context: &str) -> Result<()> {
        if is_runtime_placeholder(value) {
            bail!("{name} contains an empty/example/TEST-NET value while {context}");
        }
        Ok(())
    }

    /// Ensure a required secret is not empty or an obvious placeholder.
    pub fn ensure_secret_value(name: &str, value: &str, context: &str) -> Result<()> {
        if is_secret_placeholder(value) {
            bail!("{name} contains an empty/example secret value while {context}");
        }
        Ok(())
    }

    /// Ensure an iterator of runtime values is non-empty and production-safe.
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

    /// Validate a complete InfluxDB exporter configuration block.
    ///
    /// CONTRACT: when an exporter is enabled, URL, org, bucket, token and host
    /// list must all be real runtime values. A partial/demo exporter config is
    /// more dangerous than a disabled exporter because it creates false
    /// confidence in monitoring readiness.
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
