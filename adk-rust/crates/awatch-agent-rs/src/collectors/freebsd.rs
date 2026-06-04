use std::fs;

use anyhow::Result;
use chrono::Utc;

use crate::collectors::common::{
    agent_id, command_output, current_session, domain, hostname, role_security_events, username,
};
use crate::config::AgentRole;
use crate::telemetry::{
    IdentityInfo, NetworkConnectionInfo, NetworkInterfaceInfo, NetworkSnapshot, ProcessInfo,
    ResourceInfo, SecurityEventInfo, SessionSnapshot, TelemetryCollector, WorkforceActivityInfo,
    empty_workforce_activity,
};

#[derive(Debug, Clone)]
pub struct FreeBsdCollector {
    role: AgentRole,
}

impl FreeBsdCollector {
    pub fn new(role: AgentRole) -> Self {
        Self { role }
    }
}

impl TelemetryCollector for FreeBsdCollector {
    fn collect_identity(&self) -> Result<IdentityInfo> {
        let host = hostname();
        Ok(IdentityInfo {
            agent_id: agent_id(&host),
            hostname: host,
            os_name: command_output("uname", &["-s"]).unwrap_or_else(|| "FreeBSD".to_string()),
            os_version: command_output("uname", &["-r"]).unwrap_or_default(),
            platform: "freebsd".to_string(),
            username: username(),
            domain: domain(),
        })
    }

    fn collect_sessions(&self) -> Result<SessionSnapshot> {
        let mut active = vec![current_session("local")];
        let mut ssh = Vec::new();
        if std::env::var("SSH_CLIENT").is_ok() || std::env::var("SSH_TTY").is_ok() {
            let session = current_session("ssh");
            ssh.push(session.clone());
            active.push(session);
        }
        Ok(SessionSnapshot {
            active_sessions: active,
            rdp_sessions: Vec::new(),
            ssh_sessions: ssh,
        })
    }

    fn collect_processes(&self) -> Result<Vec<ProcessInfo>> {
        Ok(freebsd_processes(128))
    }

    fn collect_resources(&self) -> Result<ResourceInfo> {
        let memory_total = command_output("sysctl", &["-n", "hw.physmem"])
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        Ok(ResourceInfo {
            uptime_seconds: 0,
            cpu_usage_percent: 0.0,
            memory_total,
            memory_used: 0,
        })
    }

    fn collect_network(&self) -> Result<NetworkSnapshot> {
        Ok(NetworkSnapshot {
            interfaces: freebsd_interfaces(),
            connections: freebsd_connections(256),
        })
    }

    fn collect_security_events(&self) -> Result<Vec<SecurityEventInfo>> {
        let mut events = role_security_events(self.role);
        if let Some(summary) = freebsd_syslog_summary() {
            events.push(SecurityEventInfo {
                event_id: "freebsd-syslog-summary".to_string(),
                source: "syslog".to_string(),
                severity: "INFO".to_string(),
                summary,
                timestamp: Utc::now(),
                evidence: vec!["/var/log/messages".to_string()],
            });
        }
        Ok(events)
    }

    fn collect_workforce_activity(&self) -> Result<WorkforceActivityInfo> {
        let mut activity = empty_workforce_activity();
        activity.active_today = true;
        activity.explanation = vec![
            "FreeBSD collector reports host/session/process/network context; pfSense mode is read-only".to_string(),
        ];
        Ok(activity)
    }
}

fn freebsd_processes(limit: usize) -> Vec<ProcessInfo> {
    let Some(raw) = command_output("ps", &["-axo", "pid,ppid,comm,rss"]) else {
        return Vec::new();
    };
    let mut items = raw
        .lines()
        .skip(1)
        .filter_map(|line| {
            let cols = line.split_whitespace().collect::<Vec<_>>();
            let pid = cols.first()?.parse::<u32>().ok()?;
            let ppid = cols.get(1).and_then(|value| value.parse::<u32>().ok());
            let name = cols.get(2).unwrap_or(&"process").to_string();
            let memory_bytes = cols
                .get(3)
                .and_then(|value| value.parse::<u64>().ok())
                .map(|value| value.saturating_mul(1024));
            Some(ProcessInfo {
                pid,
                ppid,
                name,
                exe: None,
                username: None,
                cpu_percent: None,
                memory_bytes,
                started_at: None,
            })
        })
        .collect::<Vec<_>>();
    items.truncate(limit);
    items
}

fn freebsd_interfaces() -> Vec<NetworkInterfaceInfo> {
    let Some(raw) = command_output("ifconfig", &["-l"]) else {
        return Vec::new();
    };
    raw.split_whitespace()
        .map(|name| NetworkInterfaceInfo {
            name: name.to_string(),
            mac: None,
            addresses: Vec::new(),
            up: true,
            rx_bytes: None,
            tx_bytes: None,
        })
        .collect()
}

fn freebsd_connections(limit: usize) -> Vec<NetworkConnectionInfo> {
    let Some(raw) = command_output("sockstat", &["-4", "-6"]) else {
        return Vec::new();
    };
    let mut items = raw
        .lines()
        .skip(1)
        .filter_map(parse_sockstat_line)
        .collect::<Vec<_>>();
    items.truncate(limit);
    items
}

fn parse_sockstat_line(line: &str) -> Option<NetworkConnectionInfo> {
    let cols = line.split_whitespace().collect::<Vec<_>>();
    let protocol = cols.get(4)?.to_ascii_lowercase();
    if protocol != "tcp" && protocol != "udp" {
        return None;
    }
    let (local_addr, local_port) = split_host_port(cols.get(5)?)?;
    let (remote_addr, remote_port) = cols
        .get(6)
        .and_then(|value| split_host_port(value))
        .unwrap_or_default();
    Some(NetworkConnectionInfo {
        protocol: protocol.clone(),
        local_addr,
        local_port,
        remote_addr: Some(remote_addr),
        remote_port: Some(remote_port),
        state: if protocol == "tcp" { "OPEN" } else { "UDP" }.to_string(),
        pid: cols.get(2).and_then(|value| value.parse::<u32>().ok()),
    })
}

fn split_host_port(value: &str) -> Option<(String, u16)> {
    let (host, port) = value.rsplit_once(':')?;
    Some((
        host.trim_matches(['[', ']']).to_string(),
        port.parse().ok()?,
    ))
}

fn freebsd_syslog_summary() -> Option<String> {
    let text = fs::read_to_string("/var/log/messages").ok()?;
    let count = text
        .lines()
        .rev()
        .take(200)
        .filter(|line| {
            let lower = line.to_lowercase();
            lower.contains("error") || lower.contains("fail") || lower.contains("denied")
        })
        .count();
    Some(format!(
        "recent FreeBSD syslog warning/error lines: {count}"
    ))
}
