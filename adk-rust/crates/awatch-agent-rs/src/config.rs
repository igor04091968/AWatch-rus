use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

#[derive(Debug, Clone, PartialEq)]
pub struct AgentConfig {
    pub server_url: String,
    pub api_key: String,
    pub collect_interval_seconds: u64,
    pub role: AgentRole,
    pub enable_processes: bool,
    pub enable_network: bool,
    pub enable_security_events: bool,
    pub enable_workforce_activity: bool,
    pub spool_dir: PathBuf,
    pub timeout_seconds: u64,
    pub retry_attempts: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRole {
    Workstation,
    Server,
    Firewall,
}

impl AgentRole {
    pub fn parse(value: &str) -> Self {
        match value.trim().to_lowercase().as_str() {
            "firewall" | "pfsense" => Self::Firewall,
            "server" => Self::Server,
            _ => Self::Workstation,
        }
    }
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            server_url: "https://awatch.local/api/telemetry".to_string(),
            api_key: "change-me".to_string(),
            collect_interval_seconds: 60,
            role: AgentRole::Workstation,
            enable_processes: true,
            enable_network: true,
            enable_security_events: true,
            enable_workforce_activity: true,
            spool_dir: default_spool_dir(),
            timeout_seconds: 10,
            retry_attempts: 3,
        }
    }
}

pub fn default_config_path() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(r"C:\ProgramData\AWatch\agent\awatch-agent.toml")
    } else {
        PathBuf::from("/etc/awatch-agent/awatch-agent.toml")
    }
}

fn default_spool_dir() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(r"C:\ProgramData\AWatch\agent\spool")
    } else {
        PathBuf::from("/var/lib/awatch-agent/spool")
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
                "server_url" => config.server_url = value.to_string(),
                "api_key" => config.api_key = value.to_string(),
                "collect_interval_seconds" => {
                    config.collect_interval_seconds = value.parse().unwrap_or(60)
                }
                "role" => config.role = AgentRole::parse(value),
                "enable_processes" => config.enable_processes = parse_bool(value, true),
                "enable_network" => config.enable_network = parse_bool(value, true),
                "enable_security_events" => config.enable_security_events = parse_bool(value, true),
                "enable_workforce_activity" => {
                    config.enable_workforce_activity = parse_bool(value, true)
                }
                "spool_dir" => config.spool_dir = PathBuf::from(value),
                "timeout_seconds" => config.timeout_seconds = value.parse().unwrap_or(10),
                "retry_attempts" => config.retry_attempts = value.parse().unwrap_or(3),
                _ => {}
            }
        }
        Ok(config)
    }
}

fn parse_bool(value: &str, fallback: bool) -> bool {
    match value.trim().to_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => true,
        "0" | "false" | "no" | "off" => false,
        _ => fallback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_agent_config() {
        let config = AgentConfig::parse_toml_like(
            r#"
server_url = "https://awatch.local/api/telemetry"
api_key = "change-me"
collect_interval_seconds = 30
role = "firewall"
enable_processes = false
spool_dir = "/tmp/awatch-spool"
"#,
        )
        .unwrap();
        assert_eq!(config.role, AgentRole::Firewall);
        assert_eq!(config.collect_interval_seconds, 30);
        assert!(!config.enable_processes);
        assert_eq!(config.spool_dir, PathBuf::from("/tmp/awatch-spool"));
    }
}
