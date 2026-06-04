use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const COLLECTOR_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TelemetryRecord {
    pub agent_id: String,
    pub hostname: String,
    pub os_name: String,
    pub os_version: String,
    pub platform: String,
    pub username: String,
    pub domain: String,
    pub timestamp: DateTime<Utc>,
    pub uptime_seconds: u64,
    pub cpu_usage_percent: f64,
    pub memory_total: u64,
    pub memory_used: u64,
    pub active_sessions: Vec<SessionInfo>,
    pub rdp_sessions: Vec<SessionInfo>,
    pub ssh_sessions: Vec<SessionInfo>,
    pub processes: Vec<ProcessInfo>,
    pub network_interfaces: Vec<NetworkInterfaceInfo>,
    pub network_connections: Vec<NetworkConnectionInfo>,
    pub workforce_activity: WorkforceActivityInfo,
    pub security_events: Vec<SecurityEventInfo>,
    pub diagnostics: AgentDiagnostics,
    pub collector_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IdentityInfo {
    pub agent_id: String,
    pub hostname: String,
    pub os_name: String,
    pub os_version: String,
    pub platform: String,
    pub username: String,
    pub domain: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceInfo {
    pub uptime_seconds: u64,
    pub cpu_usage_percent: f64,
    pub memory_total: u64,
    pub memory_used: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionInfo {
    pub session_id: String,
    pub username: String,
    pub session_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_source: Option<String>,
    pub remote_addr: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentDiagnostics {
    pub sessions_collected_total: usize,
    pub rdp_sessions_total: usize,
    pub active_sessions_total: usize,
    pub collector_source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collector_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProcessInfo {
    pub pid: u32,
    pub ppid: Option<u32>,
    pub name: String,
    pub exe: Option<String>,
    pub username: Option<String>,
    pub cpu_percent: Option<f64>,
    pub memory_bytes: Option<u64>,
    pub started_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkInterfaceInfo {
    pub name: String,
    pub mac: Option<String>,
    pub addresses: Vec<String>,
    pub up: bool,
    pub rx_bytes: Option<u64>,
    pub tx_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkConnectionInfo {
    pub protocol: String,
    pub local_addr: String,
    pub local_port: u16,
    pub remote_addr: Option<String>,
    pub remote_port: Option<u16>,
    pub state: String,
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkforceActivityInfo {
    pub active_today: bool,
    pub activity_index: Option<u8>,
    pub department: Option<String>,
    pub owner: Option<String>,
    pub work_applications: Vec<String>,
    pub idle_seconds: Option<u64>,
    pub explanation: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SecurityEventInfo {
    pub event_id: String,
    pub source: String,
    pub severity: String,
    pub summary: String,
    pub timestamp: DateTime<Utc>,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionSnapshot {
    pub active_sessions: Vec<SessionInfo>,
    pub rdp_sessions: Vec<SessionInfo>,
    pub ssh_sessions: Vec<SessionInfo>,
    pub diagnostics: AgentDiagnostics,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkSnapshot {
    pub interfaces: Vec<NetworkInterfaceInfo>,
    pub connections: Vec<NetworkConnectionInfo>,
}

pub trait TelemetryCollector {
    fn collect_identity(&self) -> Result<IdentityInfo>;
    fn collect_sessions(&self) -> Result<SessionSnapshot>;
    fn collect_processes(&self) -> Result<Vec<ProcessInfo>>;
    fn collect_resources(&self) -> Result<ResourceInfo>;
    fn collect_network(&self) -> Result<NetworkSnapshot>;
    fn collect_security_events(&self) -> Result<Vec<SecurityEventInfo>>;
    fn collect_workforce_activity(&self) -> Result<WorkforceActivityInfo>;

    fn collect_all(&self) -> Result<TelemetryRecord> {
        let identity = self.collect_identity()?;
        let sessions = self.collect_sessions()?;
        let resources = self.collect_resources()?;
        let network = self.collect_network()?;
        Ok(TelemetryRecord {
            agent_id: identity.agent_id,
            hostname: identity.hostname,
            os_name: identity.os_name,
            os_version: identity.os_version,
            platform: identity.platform,
            username: identity.username,
            domain: identity.domain,
            timestamp: Utc::now(),
            uptime_seconds: resources.uptime_seconds,
            cpu_usage_percent: resources.cpu_usage_percent,
            memory_total: resources.memory_total,
            memory_used: resources.memory_used,
            active_sessions: sessions.active_sessions,
            rdp_sessions: sessions.rdp_sessions,
            ssh_sessions: sessions.ssh_sessions,
            processes: self.collect_processes()?,
            network_interfaces: network.interfaces,
            network_connections: network.connections,
            workforce_activity: self.collect_workforce_activity()?,
            security_events: self.collect_security_events()?,
            diagnostics: sessions.diagnostics,
            collector_version: COLLECTOR_VERSION.to_string(),
        })
    }
}

pub fn diagnostics_for_sessions(
    active_sessions: &[SessionInfo],
    rdp_sessions: &[SessionInfo],
    collector_source: impl Into<String>,
    collector_error: Option<String>,
) -> AgentDiagnostics {
    AgentDiagnostics {
        sessions_collected_total: active_sessions.len(),
        rdp_sessions_total: rdp_sessions.len(),
        active_sessions_total: active_sessions
            .iter()
            .filter(|session| session.active)
            .count(),
        collector_source: collector_source.into(),
        collector_error,
    }
}

pub fn dedupe_sessions(hostname: &str, sessions: Vec<SessionInfo>) -> Vec<SessionInfo> {
    let mut seen = BTreeSet::new();
    sessions
        .into_iter()
        .filter(|session| {
            seen.insert(format!(
                "{}\u{1f}{}\u{1f}{}\u{1f}{}",
                hostname, session.username, session.session_id, session.session_type
            ))
        })
        .collect()
}

pub fn empty_workforce_activity() -> WorkforceActivityInfo {
    WorkforceActivityInfo {
        active_today: false,
        activity_index: None,
        department: None,
        owner: None,
        work_applications: Vec::new(),
        idle_seconds: None,
        explanation: vec!["activity scoring requires workstation activity events".to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_record_serializes_required_fields() {
        let record = TelemetryRecord {
            agent_id: "agent-1".to_string(),
            hostname: "HOST-EXAMPLE".to_string(),
            os_name: "Linux".to_string(),
            os_version: "test".to_string(),
            platform: "linux".to_string(),
            username: "user".to_string(),
            domain: "".to_string(),
            timestamp: Utc::now(),
            uptime_seconds: 1,
            cpu_usage_percent: 0.0,
            memory_total: 10,
            memory_used: 5,
            active_sessions: Vec::new(),
            rdp_sessions: Vec::new(),
            ssh_sessions: Vec::new(),
            processes: Vec::new(),
            network_interfaces: Vec::new(),
            network_connections: Vec::new(),
            workforce_activity: empty_workforce_activity(),
            security_events: Vec::new(),
            diagnostics: diagnostics_for_sessions(&[], &[], "test", None),
            collector_version: COLLECTOR_VERSION.to_string(),
        };
        let value = serde_json::to_value(record).unwrap();
        assert_eq!(value["agent_id"], "agent-1");
        assert!(value.get("network_connections").unwrap().is_array());
        assert!(value.get("workforce_activity").is_some());
        assert_eq!(value["diagnostics"]["collector_source"], "test");
    }

    #[test]
    fn deduplicates_sessions_by_host_user_id_and_type() {
        let session = SessionInfo {
            session_id: "2".to_string(),
            username: "user".to_string(),
            session_type: "rdp".to_string(),
            session_source: Some("wts_api".to_string()),
            remote_addr: None,
            started_at: None,
            active: true,
        };
        let deduped = dedupe_sessions("HOST-EXAMPLE", vec![session.clone(), session]);
        assert_eq!(deduped.len(), 1);
    }
}
