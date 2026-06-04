use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue};

use crate::config::AgentConfig;
use crate::telemetry::TelemetryRecord;

#[derive(Debug, Clone)]
pub struct TelemetryTransport {
    server_url: String,
    api_key: String,
    spool_dir: PathBuf,
    timeout: Duration,
    retry_attempts: u32,
}

impl TelemetryTransport {
    pub fn new(config: &AgentConfig) -> Self {
        Self {
            server_url: config.server_url.clone(),
            api_key: config.api_key.clone(),
            spool_dir: config.spool_dir.clone(),
            timeout: Duration::from_secs(config.timeout_seconds),
            retry_attempts: config.retry_attempts,
        }
    }

    pub fn send_or_spool(&self, record: &TelemetryRecord) -> Result<()> {
        match self.send(record) {
            Ok(()) => Ok(()),
            Err(err) => {
                self.spool(record)?;
                Err(err)
            }
        }
    }

    pub fn send(&self, record: &TelemetryRecord) -> Result<()> {
        let client = Client::builder()
            .timeout(self.timeout)
            .build()
            .context("build telemetry HTTP client")?;
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(&self.api_key).context("invalid api key header")?,
        );
        let mut last_error = None;
        for attempt in 0..self.retry_attempts.max(1) {
            let result = client
                .post(&self.server_url)
                .headers(headers.clone())
                .json(record)
                .send()
                .and_then(|response| response.error_for_status())
                .map(|_| ());
            match result {
                Ok(()) => return Ok(()),
                Err(err) => {
                    last_error = Some(err);
                    let backoff = Duration::from_millis(250 * u64::from(attempt + 1));
                    thread::sleep(backoff);
                }
            }
        }
        Err(anyhow!(
            "telemetry POST failed: {}",
            last_error
                .map(|err| err.to_string())
                .unwrap_or_else(|| "unknown error".to_string())
        ))
    }

    pub fn spool(&self, record: &TelemetryRecord) -> Result<PathBuf> {
        fs::create_dir_all(&self.spool_dir)
            .with_context(|| format!("create spool {}", self.spool_dir.display()))?;
        let file_name = format!(
            "{}-{}.json",
            record.timestamp.format("%Y%m%dT%H%M%S%.3fZ"),
            sanitize_file_part(&record.agent_id)
        );
        let path = self.spool_dir.join(file_name);
        fs::write(&path, serde_json::to_vec(record)?)
            .with_context(|| format!("write spool {}", path.display()))?;
        Ok(path)
    }

    pub fn flush_spool(&self) -> Result<usize> {
        flush_spool_dir(&self.spool_dir, |record| self.send(record))
    }
}

pub fn flush_spool_dir<F>(spool_dir: &Path, mut sender: F) -> Result<usize>
where
    F: FnMut(&TelemetryRecord) -> Result<()>,
{
    if !spool_dir.exists() {
        return Ok(0);
    }
    let mut sent = 0;
    let mut entries = fs::read_dir(spool_dir)
        .with_context(|| format!("read spool {}", spool_dir.display()))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        let data = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
        let record: TelemetryRecord =
            serde_json::from_slice(&data).with_context(|| format!("parse {}", path.display()))?;
        sender(&record)?;
        fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
        sent += 1;
    }
    Ok(sent)
}

fn sanitize_file_part(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

pub fn spool_health(spool_dir: &Path) -> serde_json::Value {
    let queued = fs::read_dir(spool_dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(|entry| entry.ok()))
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
        .count();
    serde_json::json!({
        "generated_at_utc": Utc::now(),
        "spool_dir": spool_dir.display().to_string(),
        "queued": queued,
    })
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::telemetry::{TelemetryRecord, empty_workforce_activity};

    fn record() -> TelemetryRecord {
        TelemetryRecord {
            agent_id: "agent/1".to_string(),
            hostname: "HOST-EXAMPLE".to_string(),
            os_name: "Linux".to_string(),
            os_version: "test".to_string(),
            platform: "linux".to_string(),
            username: "user".to_string(),
            domain: "".to_string(),
            timestamp: Utc::now(),
            uptime_seconds: 1,
            cpu_usage_percent: 0.0,
            memory_total: 1,
            memory_used: 1,
            active_sessions: Vec::new(),
            rdp_sessions: Vec::new(),
            ssh_sessions: Vec::new(),
            processes: Vec::new(),
            network_interfaces: Vec::new(),
            network_connections: Vec::new(),
            workforce_activity: empty_workforce_activity(),
            security_events: Vec::new(),
            collector_version: "test".to_string(),
        }
    }

    #[test]
    fn spools_and_flushes_records() {
        let dir = tempdir().unwrap();
        let config = AgentConfig {
            spool_dir: dir.path().to_path_buf(),
            ..AgentConfig::default()
        };
        let transport = TelemetryTransport::new(&config);
        let path = transport.spool(&record()).unwrap();
        assert!(path.is_file());
        let mut seen = 0;
        let flushed = flush_spool_dir(dir.path(), |_| {
            seen += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(flushed, 1);
        assert_eq!(seen, 1);
        assert!(!path.exists());
    }
}
