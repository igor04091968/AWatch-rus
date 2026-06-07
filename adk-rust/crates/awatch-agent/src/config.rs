use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentConfig {
    pub agent_id: String,
    pub host_id: String,
    pub platform: String,
    pub server_url: String,
    pub spool_dir: PathBuf,
    pub health_bind: String,
    pub request_timeout_seconds: u64,
    pub retry_max_attempts: u32,
    pub retry_base_backoff_ms: u64,
}

impl Default for AgentConfig {
    fn default() -> Self {
        let hostname = local_hostname();
        Self {
            agent_id: uuid_from_seed(&format!("agent:{hostname}")),
            host_id: uuid_from_seed(&format!("host:{hostname}")),
            platform: current_platform().to_string(),
            server_url: "http://127.0.0.1:9/api/agent/telemetry".to_string(),
            spool_dir: default_spool_dir(),
            health_bind: "127.0.0.1:8787".to_string(),
            request_timeout_seconds: 10,
            retry_max_attempts: 3,
            retry_base_backoff_ms: 250,
        }
    }
}

impl AgentConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        Self::parse_toml_like(&text)
    }

    pub fn parse_toml_like(text: &str) -> Result<Self> {
        let mut config = Self::default();
        for raw in text.lines() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim().trim_matches('"');
            match key {
                "agent_id" => config.agent_id = value.to_string(),
                "host_id" => config.host_id = value.to_string(),
                "platform" => config.platform = value.to_string(),
                "server_url" => config.server_url = value.to_string(),
                "spool_dir" => config.spool_dir = PathBuf::from(value),
                "health_bind" => config.health_bind = value.to_string(),
                "request_timeout_seconds" => {
                    config.request_timeout_seconds = value.parse().unwrap_or(10)
                }
                "retry_max_attempts" => config.retry_max_attempts = value.parse().unwrap_or(3),
                "retry_base_backoff_ms" => {
                    config.retry_base_backoff_ms = value.parse().unwrap_or(250)
                }
                _ => {}
            }
        }
        Ok(config)
    }

    pub fn request_timeout(&self) -> Duration {
        Duration::from_secs(self.request_timeout_seconds)
    }
}

pub fn default_config_path() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(r"C:\ProgramData\AWatch-rus\agent\awatch-agent.toml")
    } else {
        PathBuf::from("/etc/awatch-agent/awatch-agent.toml")
    }
}

fn default_spool_dir() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(r"C:\ProgramData\AWatch-rus\agent\spool")
    } else {
        PathBuf::from("/var/lib/awatch-agent/spool")
    }
}

fn current_platform() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "freebsd") {
        "freebsd"
    } else {
        "linux"
    }
}

fn local_hostname() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "HOST-EXAMPLE".to_string())
}

fn uuid_from_seed(seed: &str) -> String {
    let digest = Sha256::digest(seed.as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_config_without_external_toml_dependency() {
        let config = AgentConfig::parse_toml_like(
            r#"
agent_id = "00000000-0000-5000-8000-000000000001"
host_id = "00000000-0000-5000-8000-000000000002"
platform = "windows"
server_url = "https://awatch.example/api/agent/telemetry"
spool_dir = "/tmp/awatch-agent-spool"
health_bind = "127.0.0.1:8788"
request_timeout_seconds = 2
retry_max_attempts = 5
retry_base_backoff_ms = 50
"#,
        )
        .unwrap();
        assert_eq!(config.platform, "windows");
        assert_eq!(config.retry_max_attempts, 5);
        assert_eq!(config.spool_dir, PathBuf::from("/tmp/awatch-agent-spool"));
    }

    #[test]
    fn generated_ids_are_uuid_shaped() {
        let id = uuid_from_seed("HOST-EXAMPLE");
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "5");
        assert!(matches!(&id[19..20], "8" | "9" | "a" | "b"));
    }
}
