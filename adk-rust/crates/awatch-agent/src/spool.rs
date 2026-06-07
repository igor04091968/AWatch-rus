use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::envelope::TelemetryEnvelope;
use crate::metrics::AgentMetrics;

#[derive(Debug, Clone)]
pub struct LocalSpool {
    root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpoolItem {
    pub envelope: TelemetryEnvelope,
    pub enqueued_at: DateTime<Utc>,
    pub retry_count: u32,
    pub last_error: Option<String>,
}

impl LocalSpool {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn enqueue(&self, envelope: TelemetryEnvelope) -> Result<PathBuf> {
        self.ensure_dirs()?;
        let item = SpoolItem {
            envelope,
            enqueued_at: Utc::now(),
            retry_count: 0,
            last_error: None,
        };
        let file_name = format!(
            "{}-{}.json",
            item.enqueued_at.format("%Y%m%dT%H%M%S%.3fZ"),
            sanitize_file_part(&item.envelope.agent_id)
        );
        let path = self.pending_dir().join(file_name);
        write_json_atomic(&path, &item)?;
        Ok(path)
    }

    pub fn pending_paths(&self) -> Result<Vec<PathBuf>> {
        read_json_paths(&self.pending_dir())
    }

    #[cfg(test)]
    pub fn dead_letter_paths(&self) -> Result<Vec<PathBuf>> {
        read_json_paths(&self.dead_letter_dir())
    }

    pub fn metrics(&self) -> Result<AgentMetrics> {
        let paths = self.pending_paths()?;
        let spool_size = paths
            .iter()
            .filter_map(|path| fs::metadata(path).ok())
            .map(|metadata| metadata.len())
            .sum();
        Ok(AgentMetrics {
            queued_records: paths.len(),
            spool_size,
            ..AgentMetrics::default()
        })
    }

    pub fn process_pending<F>(&self, max_retry_count: u32, mut sender: F) -> Result<FlushSummary>
    where
        F: FnMut(&TelemetryEnvelope) -> Result<()>,
    {
        self.ensure_dirs()?;
        let mut summary = FlushSummary::default();
        for path in self.pending_paths()? {
            let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
            let mut item = match serde_json::from_slice::<SpoolItem>(&bytes) {
                Ok(item) => item,
                Err(err) => {
                    self.move_to_dead_letter(&path, Some(format!("corrupt json: {err}")))?;
                    summary.corrupt += 1;
                    continue;
                }
            };
            match sender(&item.envelope) {
                Ok(()) => {
                    fs::remove_file(&path)
                        .with_context(|| format!("remove delivered {}", path.display()))?;
                    summary.delivered += 1;
                }
                Err(err) => {
                    item.retry_count = item.retry_count.saturating_add(1);
                    item.last_error = Some(err.to_string());
                    summary.retried += 1;
                    if item.retry_count >= max_retry_count {
                        write_json_atomic(&path, &item)?;
                        self.move_to_dead_letter(&path, item.last_error.clone())?;
                        summary.dead_lettered += 1;
                    } else {
                        write_json_atomic(&path, &item)?;
                    }
                }
            }
        }
        Ok(summary)
    }

    fn ensure_dirs(&self) -> Result<()> {
        fs::create_dir_all(self.pending_dir())
            .with_context(|| format!("create {}", self.pending_dir().display()))?;
        fs::create_dir_all(self.dead_letter_dir())
            .with_context(|| format!("create {}", self.dead_letter_dir().display()))?;
        Ok(())
    }

    fn pending_dir(&self) -> PathBuf {
        self.root.join("pending")
    }

    fn dead_letter_dir(&self) -> PathBuf {
        self.root.join("dead-letter")
    }

    fn move_to_dead_letter(&self, path: &Path, reason: Option<String>) -> Result<()> {
        self.ensure_dirs()?;
        let file_name = path
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("{}.json", Utc::now().timestamp_millis()));
        let target = self.dead_letter_dir().join(file_name);
        if let Some(reason) = reason {
            let note_path = target.with_extension("reason.txt");
            fs::write(note_path, reason)?;
        }
        fs::rename(path, target).or_else(|_| {
            fs::copy(path, self.dead_letter_dir().join("recovered-corrupt.json"))?;
            fs::remove_file(path)
        })?;
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FlushSummary {
    pub delivered: usize,
    pub retried: usize,
    pub dead_lettered: usize,
    pub corrupt: usize,
}

fn read_json_paths(dir: &Path) -> Result<Vec<PathBuf>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut paths = fs::read_dir(dir)
        .with_context(|| format!("read {}", dir.display()))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(value)?)
        .with_context(|| format!("write {}", tmp.display()))?;
    fs::rename(&tmp, path)
        .with_context(|| format!("rename {} to {}", tmp.display(), path.display()))?;
    Ok(())
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

#[cfg(test)]
mod tests {
    use anyhow::anyhow;
    use tempfile::tempdir;

    use super::*;
    use crate::config::AgentConfig;

    fn envelope() -> TelemetryEnvelope {
        TelemetryEnvelope::heartbeat(&AgentConfig::default())
    }

    #[test]
    fn enqueues_and_delivers_spool_item() {
        let dir = tempdir().unwrap();
        let spool = LocalSpool::new(dir.path());
        spool.enqueue(envelope()).unwrap();
        assert_eq!(spool.pending_paths().unwrap().len(), 1);

        let summary = spool.process_pending(3, |_| Ok(())).unwrap();
        assert_eq!(summary.delivered, 1);
        assert_eq!(spool.pending_paths().unwrap().len(), 0);
    }

    #[test]
    fn retry_keeps_item_until_max_retry_then_dead_letters() {
        let dir = tempdir().unwrap();
        let spool = LocalSpool::new(dir.path());
        spool.enqueue(envelope()).unwrap();

        let first = spool
            .process_pending(2, |_| Err(anyhow!("transport down")))
            .unwrap();
        assert_eq!(first.retried, 1);
        assert_eq!(first.dead_lettered, 0);
        assert_eq!(spool.pending_paths().unwrap().len(), 1);

        let second = spool
            .process_pending(2, |_| Err(anyhow!("transport down")))
            .unwrap();
        assert_eq!(second.dead_lettered, 1);
        assert_eq!(spool.pending_paths().unwrap().len(), 0);
        assert_eq!(spool.dead_letter_paths().unwrap().len(), 1);
    }

    #[test]
    fn corrupt_spool_item_moves_to_dead_letter() {
        let dir = tempdir().unwrap();
        let spool = LocalSpool::new(dir.path());
        fs::create_dir_all(dir.path().join("pending")).unwrap();
        fs::write(dir.path().join("pending/bad.json"), b"{not-json").unwrap();

        let summary = spool.process_pending(3, |_| Ok(())).unwrap();
        assert_eq!(summary.corrupt, 1);
        assert_eq!(spool.pending_paths().unwrap().len(), 0);
        assert_eq!(spool.dead_letter_paths().unwrap().len(), 1);
    }

    #[test]
    fn metrics_report_queue_and_size() {
        let dir = tempdir().unwrap();
        let spool = LocalSpool::new(dir.path());
        spool.enqueue(envelope()).unwrap();
        let metrics = spool.metrics().unwrap();
        assert_eq!(metrics.queued_records, 1);
        assert!(metrics.spool_size > 0);
    }
}
