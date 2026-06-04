use anyhow::{Result, bail};

use crate::config::AgentRole;
use crate::telemetry::{
    IdentityInfo, NetworkSnapshot, ProcessInfo, ResourceInfo, SecurityEventInfo, SessionSnapshot,
    TelemetryCollector, WorkforceActivityInfo,
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
        let _ = self.role;
        unsupported()
    }
    fn collect_sessions(&self) -> Result<SessionSnapshot> {
        unsupported()
    }
    fn collect_processes(&self) -> Result<Vec<ProcessInfo>> {
        unsupported()
    }
    fn collect_resources(&self) -> Result<ResourceInfo> {
        unsupported()
    }
    fn collect_network(&self) -> Result<NetworkSnapshot> {
        unsupported()
    }
    fn collect_security_events(&self) -> Result<Vec<SecurityEventInfo>> {
        unsupported()
    }
    fn collect_workforce_activity(&self) -> Result<WorkforceActivityInfo> {
        unsupported()
    }
}

fn unsupported<T>() -> Result<T> {
    bail!(
        "Windows collector requires target_os=windows WinAPI/WMI implementation; PowerShell is not a primary collector"
    )
}
