use anyhow::Result;
use chrono::Utc;

use crate::collectors::common::{
    agent_id, command_output, current_session, domain, hostname, role_security_events, username,
};
use crate::config::AgentRole;
use crate::telemetry::{
    IdentityInfo, NetworkConnectionInfo, NetworkSnapshot, ProcessInfo, ResourceInfo,
    SecurityEventInfo, SessionSnapshot, TelemetryCollector, WorkforceActivityInfo,
    empty_workforce_activity,
};

#[derive(Debug, Clone)]
pub struct WindowsCollector {
    role: AgentRole,
}

impl WindowsCollector {
    pub fn new(role: AgentRole) -> Self {
        Self { role }
    }
}

impl TelemetryCollector for WindowsCollector {
    fn collect_identity(&self) -> Result<IdentityInfo> {
        let host = hostname();
        Ok(IdentityInfo {
            agent_id: agent_id(&host),
            hostname: host,
            os_name: "Windows".to_string(),
            os_version: windows_version(),
            platform: "windows".to_string(),
            username: username(),
            domain: domain(),
        })
    }

    fn collect_sessions(&self) -> Result<SessionSnapshot> {
        let mut active = vec![current_session("local")];
        let mut rdp = Vec::new();
        if std::env::var("SESSIONNAME")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .contains("rdp")
        {
            let session = current_session("rdp");
            rdp.push(session.clone());
            active.push(session);
        }
        Ok(SessionSnapshot {
            active_sessions: active,
            rdp_sessions: rdp,
            ssh_sessions: Vec::new(),
        })
    }

    fn collect_processes(&self) -> Result<Vec<ProcessInfo>> {
        Ok(windows_processes(128))
    }

    fn collect_resources(&self) -> Result<ResourceInfo> {
        let (memory_total, memory_used) = windows_memory();
        Ok(ResourceInfo {
            uptime_seconds: 0,
            cpu_usage_percent: 0.0,
            memory_total,
            memory_used,
        })
    }

    fn collect_network(&self) -> Result<NetworkSnapshot> {
        Ok(NetworkSnapshot {
            interfaces: Vec::new(),
            connections: windows_connections(256),
        })
    }

    fn collect_security_events(&self) -> Result<Vec<SecurityEventInfo>> {
        let mut events = role_security_events(self.role);
        events.push(SecurityEventInfo {
            event_id: "windows-collector-v03".to_string(),
            source: "awatch-agent-rs".to_string(),
            severity: "INFO".to_string(),
            summary: "Windows read-only collector is active without PowerShell primary collection; WinAPI/ETW/WMI depth is planned behind the same TelemetryRecord contract".to_string(),
            timestamp: Utc::now(),
            evidence: vec!["no PowerShell primary collector".to_string()],
        });
        Ok(events)
    }

    fn collect_workforce_activity(&self) -> Result<WorkforceActivityInfo> {
        let mut activity = empty_workforce_activity();
        activity.active_today = true;
        activity.explanation = vec![
            "Windows collector reports session/process/network context; ActivityWatch/workforce scoring is calculated server-side".to_string(),
        ];
        Ok(activity)
    }
}

fn windows_version() -> String {
    command_output("cmd", &["/C", "ver"])
        .or_else(|| std::env::var("OS").ok())
        .unwrap_or_else(|| "Windows".to_string())
}

fn windows_memory() -> (u64, u64) {
    let Some(raw) = command_output(
        "wmic",
        &[
            "OS",
            "get",
            "FreePhysicalMemory,TotalVisibleMemorySize",
            "/Value",
        ],
    ) else {
        return (0, 0);
    };
    let mut free_kib = 0;
    let mut total_kib = 0;
    for line in raw.lines() {
        if let Some(value) = line.strip_prefix("FreePhysicalMemory=") {
            free_kib = value.trim().parse::<u64>().unwrap_or(0);
        }
        if let Some(value) = line.strip_prefix("TotalVisibleMemorySize=") {
            total_kib = value.trim().parse::<u64>().unwrap_or(0);
        }
    }
    let total = total_kib.saturating_mul(1024);
    let used = total_kib.saturating_sub(free_kib).saturating_mul(1024);
    (total, used)
}

fn windows_processes(limit: usize) -> Vec<ProcessInfo> {
    let Some(raw) = command_output("tasklist", &["/FO", "CSV", "/NH"]) else {
        return Vec::new();
    };
    let mut items = raw
        .lines()
        .filter_map(parse_tasklist_line)
        .collect::<Vec<_>>();
    items.truncate(limit);
    items
}

fn parse_tasklist_line(line: &str) -> Option<ProcessInfo> {
    let cols = parse_csv_line(line);
    let name = cols.first()?.to_string();
    let pid = cols.get(1)?.parse::<u32>().ok()?;
    let memory_bytes = cols.get(4).map(|value| parse_tasklist_memory(value));
    Some(ProcessInfo {
        pid,
        ppid: None,
        name,
        exe: None,
        username: None,
        cpu_percent: None,
        memory_bytes,
        started_at: None,
    })
}

fn parse_csv_line(line: &str) -> Vec<String> {
    line.trim_matches('"')
        .split("\",\"")
        .map(|value| value.trim().to_string())
        .collect()
}

fn parse_tasklist_memory(value: &str) -> u64 {
    value
        .chars()
        .filter(|ch| ch.is_ascii_digit())
        .collect::<String>()
        .parse::<u64>()
        .unwrap_or(0)
        .saturating_mul(1024)
}

fn windows_connections(limit: usize) -> Vec<NetworkConnectionInfo> {
    let Some(raw) = command_output("netstat", &["-ano"]) else {
        return Vec::new();
    };
    let mut items = raw
        .lines()
        .filter_map(parse_netstat_line)
        .collect::<Vec<_>>();
    items.truncate(limit);
    items
}

fn parse_netstat_line(line: &str) -> Option<NetworkConnectionInfo> {
    let cols = line.split_whitespace().collect::<Vec<_>>();
    let protocol = cols.first()?.to_ascii_lowercase();
    if protocol != "tcp" && protocol != "udp" {
        return None;
    }
    let (local_addr, local_port) = split_host_port(cols.get(1)?)?;
    let (remote_addr, remote_port) = split_host_port(cols.get(2)?).unwrap_or_default();
    let state = if protocol == "tcp" {
        cols.get(3).unwrap_or(&"UNKNOWN").to_string()
    } else {
        "UDP".to_string()
    };
    let pid = cols.last().and_then(|value| value.parse::<u32>().ok());
    Some(NetworkConnectionInfo {
        protocol,
        local_addr,
        local_port,
        remote_addr: Some(remote_addr),
        remote_port: Some(remote_port),
        state,
        pid,
    })
}

fn split_host_port(value: &str) -> Option<(String, u16)> {
    let (host, port) = value.rsplit_once(':')?;
    Some((
        host.trim_matches(['[', ']']).to_string(),
        port.parse().ok()?,
    ))
}
