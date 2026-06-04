use std::fs;
use std::path::Path;

use anyhow::Result;
use chrono::Utc;

use crate::collectors::common::{
    agent_id, current_session, domain, hostname, parse_hex_ipv4, parse_os_release, read_trimmed,
    role_security_events, tcp_state, username,
};
use crate::config::AgentRole;
use crate::telemetry::{
    IdentityInfo, NetworkConnectionInfo, NetworkInterfaceInfo, NetworkSnapshot, ProcessInfo,
    ResourceInfo, SecurityEventInfo, SessionSnapshot, TelemetryCollector, WorkforceActivityInfo,
    dedupe_sessions, diagnostics_for_sessions, empty_workforce_activity,
};

#[derive(Debug, Clone)]
pub struct LinuxCollector {
    role: AgentRole,
}

impl LinuxCollector {
    pub fn new(role: AgentRole) -> Self {
        Self { role }
    }
}

impl TelemetryCollector for LinuxCollector {
    fn collect_identity(&self) -> Result<IdentityInfo> {
        let host = hostname();
        let (os_name, os_version) = parse_os_release(Path::new("/etc/os-release"));
        Ok(IdentityInfo {
            agent_id: agent_id(&host),
            hostname: host,
            os_name,
            os_version,
            platform: "linux".to_string(),
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
        let host = hostname();
        let active = dedupe_sessions(&host, active);
        let ssh = dedupe_sessions(&host, ssh);
        let diagnostics = diagnostics_for_sessions(&active, &[], "env_sessionname_fallback", None);
        Ok(SessionSnapshot {
            active_sessions: active,
            rdp_sessions: Vec::new(),
            ssh_sessions: ssh,
            diagnostics,
        })
    }

    fn collect_processes(&self) -> Result<Vec<ProcessInfo>> {
        Ok(read_processes(128))
    }

    fn collect_resources(&self) -> Result<ResourceInfo> {
        let uptime_seconds = fs::read_to_string("/proc/uptime")
            .ok()
            .and_then(|text| text.split_whitespace().next()?.parse::<f64>().ok())
            .map(|value| value as u64)
            .unwrap_or(0);
        let (memory_total, memory_available) = read_meminfo();
        Ok(ResourceInfo {
            uptime_seconds,
            cpu_usage_percent: read_loadavg_percent(),
            memory_total,
            memory_used: memory_total.saturating_sub(memory_available),
        })
    }

    fn collect_network(&self) -> Result<NetworkSnapshot> {
        Ok(NetworkSnapshot {
            interfaces: read_interfaces(),
            connections: read_connections(),
        })
    }

    fn collect_security_events(&self) -> Result<Vec<SecurityEventInfo>> {
        let mut events = role_security_events(self.role);
        if let Some(summary) = recent_syslog_summary() {
            events.push(SecurityEventInfo {
                event_id: "linux-syslog-summary".to_string(),
                source: "syslog".to_string(),
                severity: "INFO".to_string(),
                summary,
                timestamp: Utc::now(),
                evidence: vec!["/var/log/syslog or /var/log/messages".to_string()],
            });
        }
        Ok(events)
    }

    fn collect_workforce_activity(&self) -> Result<WorkforceActivityInfo> {
        let mut activity = empty_workforce_activity();
        activity.active_today = true;
        activity.explanation = vec![
            "Linux collector reports presence and process/network context; application weighting is calculated server-side".to_string(),
        ];
        Ok(activity)
    }
}

fn read_meminfo() -> (u64, u64) {
    let mut total = 0;
    let mut available = 0;
    let text = fs::read_to_string("/proc/meminfo").unwrap_or_default();
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("MemTotal:") {
            total = parse_kib(value);
        }
        if let Some(value) = line.strip_prefix("MemAvailable:") {
            available = parse_kib(value);
        }
    }
    (total, available)
}

fn parse_kib(value: &str) -> u64 {
    value
        .split_whitespace()
        .next()
        .and_then(|item| item.parse::<u64>().ok())
        .unwrap_or(0)
        * 1024
}

fn read_loadavg_percent() -> f64 {
    fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|text| text.split_whitespace().next()?.parse::<f64>().ok())
        .map(|load| (load * 100.0).clamp(0.0, 100.0))
        .unwrap_or(0.0)
}

fn read_processes(limit: usize) -> Vec<ProcessInfo> {
    let mut items = fs::read_dir("/proc")
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(|entry| entry.ok()))
        .filter_map(|entry| {
            let pid = entry.file_name().to_string_lossy().parse::<u32>().ok()?;
            let stat = fs::read_to_string(entry.path().join("stat")).ok()?;
            let name = stat.split_once('(')?.1.split_once(')')?.0.to_string();
            let exe = fs::read_link(entry.path().join("exe"))
                .ok()
                .map(|path| path.display().to_string());
            let status = fs::read_to_string(entry.path().join("status")).unwrap_or_default();
            let ppid = status
                .lines()
                .find_map(|line| line.strip_prefix("PPid:"))
                .and_then(|value| value.trim().parse::<u32>().ok());
            let memory_bytes = status
                .lines()
                .find_map(|line| line.strip_prefix("VmRSS:"))
                .map(parse_kib);
            Some(ProcessInfo {
                pid,
                ppid,
                name,
                exe,
                username: None,
                cpu_percent: None,
                memory_bytes,
                started_at: None,
            })
        })
        .collect::<Vec<_>>();
    items.sort_by_key(|item| item.pid);
    items.truncate(limit);
    items
}

fn read_interfaces() -> Vec<NetworkInterfaceInfo> {
    fs::read_dir("/sys/class/net")
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(|entry| entry.ok()))
        .map(|entry| {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let up = read_trimmed(path.join("operstate")).is_some_and(|state| state == "up");
            let rx_bytes =
                read_trimmed(path.join("statistics/rx_bytes")).and_then(|v| v.parse().ok());
            let tx_bytes =
                read_trimmed(path.join("statistics/tx_bytes")).and_then(|v| v.parse().ok());
            NetworkInterfaceInfo {
                name,
                mac: read_trimmed(path.join("address")),
                addresses: Vec::new(),
                up,
                rx_bytes,
                tx_bytes,
            }
        })
        .collect()
}

fn read_connections() -> Vec<NetworkConnectionInfo> {
    let mut items = Vec::new();
    read_proc_net("/proc/net/tcp", "tcp", &mut items);
    read_proc_net("/proc/net/udp", "udp", &mut items);
    items.truncate(256);
    items
}

fn read_proc_net(path: &str, protocol: &str, items: &mut Vec<NetworkConnectionInfo>) {
    let text = fs::read_to_string(path).unwrap_or_default();
    for line in text.lines().skip(1) {
        let cols = line.split_whitespace().collect::<Vec<_>>();
        if cols.len() < 4 {
            continue;
        }
        let Some((local_addr, local_port)) = parse_addr(cols[1]) else {
            continue;
        };
        let (remote_addr, remote_port) = parse_addr(cols[2]).unwrap_or_default();
        items.push(NetworkConnectionInfo {
            protocol: protocol.to_string(),
            local_addr,
            local_port,
            remote_addr: if remote_addr == "0.0.0.0" {
                None
            } else {
                Some(remote_addr)
            },
            remote_port: if remote_port == 0 {
                None
            } else {
                Some(remote_port)
            },
            state: if protocol == "tcp" {
                tcp_state(cols[3]).to_string()
            } else {
                "UDP".to_string()
            },
            pid: None,
        });
    }
}

fn parse_addr(value: &str) -> Option<(String, u16)> {
    let (ip, port) = value.split_once(':')?;
    Some((parse_hex_ipv4(ip)?, u16::from_str_radix(port, 16).ok()?))
}

fn recent_syslog_summary() -> Option<String> {
    for path in ["/var/log/syslog", "/var/log/messages"] {
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let count = text
            .lines()
            .rev()
            .take(200)
            .filter(|line| {
                let lower = line.to_lowercase();
                lower.contains("error") || lower.contains("fail") || lower.contains("denied")
            })
            .count();
        return Some(format!("recent syslog warning/error lines: {count}"));
    }
    None
}
