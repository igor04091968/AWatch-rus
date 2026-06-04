use anyhow::{Result, bail};

use crate::config::AgentRole;
use crate::telemetry::{
    IdentityInfo, NetworkSnapshot, ProcessInfo, ResourceInfo, SecurityEventInfo, SessionSnapshot,
    TelemetryCollector, WorkforceActivityInfo,
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
        "FreeBSD/pfSense collector skeleton is present; sysctl/procstat/kvm probes are planned behind the same TelemetryRecord API"
    )
}
