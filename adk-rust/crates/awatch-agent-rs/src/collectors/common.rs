use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

use chrono::Utc;

use crate::config::AgentRole;
use crate::telemetry::{SecurityEventInfo, SessionInfo};

pub fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn hostname() -> String {
    env::var("HOSTNAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| fs::read_to_string("/etc/hostname").ok())
        .or_else(|| command_output("hostname", &[]))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "HOST-EXAMPLE".to_string())
}

pub fn username() -> String {
    env::var("USER")
        .or_else(|_| env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

pub fn domain() -> String {
    env::var("USERDOMAIN")
        .or_else(|_| env::var("DOMAIN"))
        .unwrap_or_default()
}

pub fn agent_id(hostname: &str) -> String {
    format!("awatch-{hostname}")
}

pub fn role_security_events(role: AgentRole) -> Vec<SecurityEventInfo> {
    if role == AgentRole::Firewall {
        vec![SecurityEventInfo {
            event_id: "pfsense-mode-prototype".to_string(),
            source: "awatch-agent-rs".to_string(),
            severity: "INFO".to_string(),
            summary: "pfSense/firewall mode enabled; counters are collected from platform-specific probes when available".to_string(),
            timestamp: Utc::now(),
            evidence: vec!["read-only mode".to_string()],
        }]
    } else {
        Vec::new()
    }
}

pub fn current_session(session_type: &str) -> SessionInfo {
    SessionInfo {
        session_id: format!("{}-{}", session_type, username()),
        username: username(),
        session_type: session_type.to_string(),
        session_source: Some("env_sessionname_fallback".to_string()),
        remote_addr: std::env::var("SSH_CLIENT")
            .ok()
            .and_then(|value| value.split_whitespace().next().map(str::to_string)),
        started_at: None,
        active: true,
    }
}

pub fn read_trimmed(path: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn parse_os_release(path: &Path) -> (String, String) {
    let text = fs::read_to_string(path).unwrap_or_default();
    let mut name = String::new();
    let mut version = String::new();
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("NAME=") {
            name = value.trim_matches('"').to_string();
        }
        if let Some(value) = line.strip_prefix("VERSION_ID=") {
            version = value.trim_matches('"').to_string();
        }
    }
    if name.is_empty() {
        name = "Linux".to_string();
    }
    (name, version)
}

pub fn parse_hex_ipv4(value: &str) -> Option<String> {
    if value.len() != 8 {
        return None;
    }
    let raw = u32::from_str_radix(value, 16).ok()?;
    let bytes = raw.to_le_bytes();
    Some(format!(
        "{}.{}.{}.{}",
        bytes[0], bytes[1], bytes[2], bytes[3]
    ))
}

pub fn tcp_state(value: &str) -> &'static str {
    match value {
        "01" => "ESTABLISHED",
        "02" => "SYN_SENT",
        "03" => "SYN_RECV",
        "04" => "FIN_WAIT1",
        "05" => "FIN_WAIT2",
        "06" => "TIME_WAIT",
        "07" => "CLOSE",
        "08" => "CLOSE_WAIT",
        "09" => "LAST_ACK",
        "0A" => "LISTEN",
        "0B" => "CLOSING",
        _ => "UNKNOWN",
    }
}
