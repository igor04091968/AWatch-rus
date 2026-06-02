use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use detmir_core::StatusLevel;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const DEFAULT_STATE_FILE: &str = "/var/lib/detmir-ai/latest-state.json";

#[derive(Debug, Deserialize)]
pub struct DetmirState {
    pub severity: Option<String>,
    pub check_ok: Option<bool>,
    pub dlp_ok: Option<bool>,
    pub needs_heal: Option<bool>,
    pub reasons: Option<Vec<String>>,
    pub detmir_summary: Option<DetmirSummary>,
    pub dlp_counts: Option<DlpCounts>,
    pub check: Option<Value>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct DetmirSummary {
    pub bucket_ok: Option<u64>,
    pub bucket_stale: Option<u64>,
    pub bucket_dead: Option<u64>,
    pub service_failures: Option<u64>,
    pub service_warnings: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct DlpCounts {
    pub ok: Option<u64>,
    pub warn: Option<u64>,
    pub fail: Option<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct NormalizedStatus {
    pub severity: String,
    pub check_ok: bool,
    pub dlp_ok: bool,
    pub needs_heal: bool,
    pub reasons: Vec<String>,
    pub detmir_summary: DetmirSummary,
    pub dlp_counts: DlpCounts,
    pub ok_for_operator: bool,
}

impl NormalizedStatus {
    pub fn level(&self) -> StatusLevel {
        StatusLevel::from(self.severity.as_str())
    }

    pub fn exit_code(&self) -> i32 {
        if self.ok_for_operator {
            StatusLevel::Ok.exit_code()
        } else {
            StatusLevel::Fail.exit_code()
        }
    }
}

impl DetmirState {
    pub fn normalize(self) -> NormalizedStatus {
        let summary = self.detmir_summary.or_else(|| {
            self.check
                .as_ref()
                .and_then(|check| check.get("summary"))
                .and_then(|summary| serde_json::from_value(summary.clone()).ok())
        });
        let summary = summary.unwrap_or_default();
        let dlp_counts = self.dlp_counts.unwrap_or_default();

        let severity = self.severity.unwrap_or_else(|| "UNKNOWN".to_string());
        let check_ok = self
            .check_ok
            .or_else(|| {
                self.check
                    .as_ref()
                    .and_then(|check| check.get("ok"))
                    .and_then(Value::as_bool)
            })
            .unwrap_or(false);
        let dlp_ok = self.dlp_ok.unwrap_or(false);
        let needs_heal = self.needs_heal.unwrap_or(false);
        let reasons = self.reasons.unwrap_or_default();

        let ok_for_operator = severity == "OK"
            && check_ok
            && dlp_ok
            && !needs_heal
            && summary.bucket_stale.unwrap_or(0) == 0
            && summary.bucket_dead.unwrap_or(0) == 0
            && summary.service_failures.unwrap_or(0) == 0
            && dlp_counts.warn.unwrap_or(0) == 0
            && dlp_counts.fail.unwrap_or(0) == 0;

        NormalizedStatus {
            severity,
            check_ok,
            dlp_ok,
            needs_heal,
            reasons,
            detmir_summary: summary,
            dlp_counts,
            ok_for_operator,
        }
    }
}

pub fn read_state(path: impl AsRef<Path>) -> Result<NormalizedStatus> {
    let path = path.as_ref();
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read DetMir state file {}", path.display()))?;
    let state: DetmirState = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse DetMir state JSON {}", path.display()))?;
    Ok(state.normalize())
}

pub fn write_json_atomic<T: Serialize>(path: impl AsRef<Path>, value: &T) -> Result<()> {
    let path = path.as_ref();
    let parent = path
        .parent()
        .with_context(|| format!("path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create state directory {}", parent.display()))?;

    let tmp_path = temp_path(path);
    let mut payload = serde_json::to_vec_pretty(value)?;
    payload.push(b'\n');
    fs::write(&tmp_path, payload)
        .with_context(|| format!("failed to write temporary state {}", tmp_path.display()))?;
    fs::rename(&tmp_path, path)
        .with_context(|| format!("failed to atomically replace {}", path.display()))?;
    Ok(())
}

fn temp_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("state.json");
    path.with_file_name(format!(".{file_name}.{}.tmp", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_current_state_shape() {
        let raw = r#"{
          "severity": "OK",
          "check_ok": true,
          "dlp_ok": true,
          "needs_heal": false,
          "reasons": [],
          "detmir_summary": {
            "bucket_ok": 8,
            "bucket_stale": 0,
            "bucket_dead": 0,
            "service_failures": 0,
            "service_warnings": 0
          },
          "dlp_counts": {"ok": 22, "warn": 0, "fail": 0}
        }"#;
        let state: DetmirState = serde_json::from_str(raw).unwrap();
        let normalized = state.normalize();
        assert!(normalized.ok_for_operator);
        assert_eq!(normalized.exit_code(), 0);
    }

    #[test]
    fn reads_legacy_check_summary_shape() {
        let raw = r#"{
          "severity": "OK",
          "dlp_ok": true,
          "check": {
            "ok": true,
            "summary": {
              "bucket_ok": 8,
              "bucket_stale": 0,
              "bucket_dead": 0,
              "service_failures": 0,
              "service_warnings": 0
            }
          },
          "dlp_counts": {"ok": 1, "warn": 0, "fail": 0}
        }"#;
        let state: DetmirState = serde_json::from_str(raw).unwrap();
        assert!(state.normalize().ok_for_operator);
    }

    #[test]
    fn writes_json_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let payload = DetmirSummary {
            bucket_ok: Some(1),
            ..Default::default()
        };
        write_json_atomic(&path, &payload).unwrap();
        let stored: DetmirSummary =
            serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(stored.bucket_ok, Some(1));
    }
}
