use anyhow::Result;
use chrono::Utc;
use std::process::Command;

use crate::collectors::common::{
    agent_id, command_output, current_session, domain, hostname, role_security_events, username,
};
use crate::config::AgentRole;
use crate::telemetry::{
    IdentityInfo, NetworkConnectionInfo, NetworkSnapshot, ProcessInfo, ResourceInfo,
    SecurityEventInfo, SessionSnapshot, TelemetryCollector, WorkforceActivityInfo, dedupe_sessions,
    diagnostics_for_sessions, empty_workforce_activity,
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
        let host = hostname();
        let mut collection = windows_query_user_sessions();
        if collection.sessions.is_empty()
            && std::env::var("SESSIONNAME")
                .unwrap_or_default()
                .to_ascii_lowercase()
                .contains("rdp")
        {
            let mut session = current_session("rdp");
            session.session_source = Some("env_sessionname_fallback".to_string());
            collection.sessions.push(session);
            collection.source = "env_sessionname_fallback".to_string();
        }
        if collection.sessions.is_empty() {
            let mut session = current_session("local");
            session.session_source = Some("local_fallback".to_string());
            collection.sessions.push(session);
            collection.source = "local_fallback".to_string();
            collection.error = Some("WTS API and quser did not return sessions".to_string());
        }
        let mut active = dedupe_sessions(&host, collection.sessions);
        let mut rdp = active
            .iter()
            .filter(|session| session.session_type == "rdp")
            .cloned()
            .collect::<Vec<_>>();
        if std::env::var("SESSIONNAME")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .contains("rdp")
        {
            let mut merged = active.clone();
            merged.push(with_session_source(
                current_session("rdp"),
                "env_sessionname_fallback",
            ));
            active = dedupe_sessions(&host, merged);
            rdp = active
                .iter()
                .filter(|session| session.session_type == "rdp")
                .cloned()
                .collect::<Vec<_>>();
        }
        rdp = dedupe_sessions(&host, rdp);
        let diagnostics =
            diagnostics_for_sessions(&active, &rdp, collection.source, collection.error);
        Ok(SessionSnapshot {
            active_sessions: active,
            rdp_sessions: rdp,
            ssh_sessions: Vec::new(),
            diagnostics,
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

#[derive(Debug)]
struct SessionCollection {
    sessions: Vec<crate::telemetry::SessionInfo>,
    source: String,
    error: Option<String>,
}

fn with_session_source(
    mut session: crate::telemetry::SessionInfo,
    source: &str,
) -> crate::telemetry::SessionInfo {
    session.session_source = Some(source.to_string());
    session
}

fn windows_query_user_sessions() -> SessionCollection {
    let native = windows_wts_sessions();
    if !native.is_empty() {
        return SessionCollection {
            sessions: native,
            source: "wts_api".to_string(),
            error: None,
        };
    }
    if let Some(raw) = command_output_utf16le("cmd", &["/U", "/C", "query user"])
        .or_else(|| command_output_utf16le("cmd", &["/U", "/C", "quser"]))
    {
        let sessions = parse_query_user_sessions(&raw, "quser_utf16");
        if !sessions.is_empty() {
            return SessionCollection {
                sessions,
                source: "quser_utf16".to_string(),
                error: None,
            };
        }
    }
    if let Some(raw) = command_output_lossy_combined("cmd", &["/C", "query user"])
        .or_else(|| command_output_lossy_combined("cmd", &["/C", "quser"]))
    {
        let sessions = parse_query_user_sessions(&raw, "quser_lossy");
        if !sessions.is_empty() {
            return SessionCollection {
                sessions,
                source: "quser_lossy".to_string(),
                error: None,
            };
        }
    }
    SessionCollection {
        sessions: Vec::new(),
        source: "local_fallback".to_string(),
        error: Some("WTS API and quser returned no sessions".to_string()),
    }
}

fn parse_query_user_sessions(raw: &str, source: &str) -> Vec<crate::telemetry::SessionInfo> {
    raw.lines()
        .skip(1)
        .filter_map(|line| parse_query_user_line(line, source))
        .collect()
}

#[cfg(windows)]
fn windows_wts_sessions() -> Vec<crate::telemetry::SessionInfo> {
    use std::ptr;
    use windows_sys::Win32::System::RemoteDesktop::{
        WTS_CURRENT_SERVER_HANDLE, WTS_SESSION_INFOW, WTSActive, WTSEnumerateSessionsW,
        WTSFreeMemory, WTSUserName,
    };

    let mut sessions_ptr: *mut WTS_SESSION_INFOW = ptr::null_mut();
    let mut count = 0_u32;
    let ok = unsafe {
        WTSEnumerateSessionsW(
            WTS_CURRENT_SERVER_HANDLE,
            0,
            1,
            &mut sessions_ptr,
            &mut count,
        )
    };
    if ok == 0 || sessions_ptr.is_null() || count == 0 {
        return Vec::new();
    }

    let sessions =
        unsafe { std::slice::from_raw_parts(sessions_ptr, usize::try_from(count).unwrap_or(0)) };
    let mut items = Vec::new();
    for session in sessions {
        let username = wts_session_string(session.SessionId, WTSUserName);
        if username.trim().is_empty() {
            continue;
        }
        let station = unsafe { wide_nul_to_string(session.pWinStationName) };
        let state = session.State;
        let session_type = if station.to_ascii_lowercase().contains("rdp") {
            "rdp"
        } else {
            "local"
        };
        items.push(crate::telemetry::SessionInfo {
            session_id: session.SessionId.to_string(),
            username,
            session_type: session_type.to_string(),
            session_source: Some("wts_api".to_string()),
            remote_addr: None,
            started_at: None,
            active: state == WTSActive,
        });
    }
    unsafe {
        WTSFreeMemory(sessions_ptr.cast());
    }
    items
}

#[cfg(not(windows))]
fn windows_wts_sessions() -> Vec<crate::telemetry::SessionInfo> {
    Vec::new()
}

#[cfg(windows)]
fn wts_session_string(session_id: u32, class: i32) -> String {
    use std::ptr;
    use windows_sys::Win32::System::RemoteDesktop::{
        WTS_CURRENT_SERVER_HANDLE, WTSFreeMemory, WTSQuerySessionInformationW,
    };

    let mut buffer = ptr::null_mut();
    let mut bytes = 0_u32;
    let ok = unsafe {
        WTSQuerySessionInformationW(
            WTS_CURRENT_SERVER_HANDLE,
            session_id,
            class,
            &mut buffer,
            &mut bytes,
        )
    };
    if ok == 0 || buffer.is_null() || bytes == 0 {
        return String::new();
    }
    let len = usize::try_from(bytes / 2).unwrap_or(0);
    let value = unsafe {
        let slice = std::slice::from_raw_parts(buffer, len);
        String::from_utf16_lossy(slice)
            .trim_matches('\0')
            .trim()
            .to_string()
    };
    unsafe {
        WTSFreeMemory(buffer.cast());
    }
    value
}

#[cfg(windows)]
unsafe fn wide_nul_to_string(ptr: *const u16) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let mut len = 0;
    while unsafe { *ptr.add(len) } != 0 {
        len += 1;
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    String::from_utf16_lossy(slice)
}

fn command_output_utf16le(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let mut bytes = output.stdout;
    bytes.extend_from_slice(&output.stderr);
    if bytes.is_empty() {
        return None;
    }
    let mut words = Vec::new();
    for chunk in bytes.chunks_exact(2) {
        words.push(u16::from_le_bytes([chunk[0], chunk[1]]));
    }
    String::from_utf16(&words)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn command_output_lossy_combined(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let mut bytes = output.stdout;
    bytes.extend_from_slice(&output.stderr);
    Some(String::from_utf8_lossy(&bytes).trim().to_string()).filter(|value| !value.is_empty())
}

fn parse_query_user_line(line: &str, source: &str) -> Option<crate::telemetry::SessionInfo> {
    let cleaned = line.trim().trim_start_matches('>').trim();
    if cleaned.is_empty() {
        return None;
    }
    let parts = cleaned.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 3 {
        return None;
    }
    let username = parts.first()?.to_string();
    let (session_name, session_id, state) = if parts.get(1)?.chars().all(|ch| ch.is_ascii_digit()) {
        ("".to_string(), *parts.get(1)?, *parts.get(2)?)
    } else {
        (
            parts.get(1)?.to_string(),
            *parts.get(2)?,
            *parts.get(3).unwrap_or(&"Unknown"),
        )
    };
    let active = session_state_active(state);
    let session_type = if session_name.to_ascii_lowercase().contains("rdp") {
        "rdp"
    } else {
        "local"
    };
    Some(crate::telemetry::SessionInfo {
        session_id: session_id.to_string(),
        username,
        session_type: session_type.to_string(),
        session_source: Some(source.to_string()),
        remote_addr: None,
        started_at: None,
        active,
    })
}

fn session_state_active(state: &str) -> bool {
    let lower = state.to_lowercase();
    lower.contains("active") || lower.contains("актив")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_query_user_line_with_rdp_session() {
        let session = parse_query_user_line(
            " user1                 rdp-tcp#5           3  Active",
            "quser_utf16",
        )
        .unwrap();
        assert_eq!(session.username, "user1");
        assert_eq!(session.session_id, "3");
        assert_eq!(session.session_type, "rdp");
        assert_eq!(session.session_source.as_deref(), Some("quser_utf16"));
        assert!(session.active);
    }

    #[test]
    fn parses_query_user_line_without_session_name() {
        let session =
            parse_query_user_line(" user2                 4  Disc", "quser_lossy").unwrap();
        assert_eq!(session.username, "user2");
        assert_eq!(session.session_id, "4");
        assert_eq!(session.session_type, "local");
        assert_eq!(session.session_source.as_deref(), Some("quser_lossy"));
        assert!(!session.active);
    }

    #[test]
    fn detects_russian_active_state() {
        assert!(session_state_active("Активно"));
    }
}
