pub mod common;
pub mod freebsd;
pub mod linux;
pub mod windows;

use anyhow::{Result, bail};

use crate::config::AgentRole;
use crate::telemetry::TelemetryCollector;

pub fn platform_collector(role: AgentRole) -> Result<Box<dyn TelemetryCollector>> {
    if cfg!(target_os = "linux") {
        return Ok(Box::new(linux::LinuxCollector::new(role)));
    }
    if cfg!(target_os = "windows") {
        return Ok(Box::new(windows::WindowsCollector::new(role)));
    }
    if cfg!(target_os = "freebsd") {
        return Ok(Box::new(freebsd::FreeBsdCollector::new(role)));
    }
    bail!("unsupported platform for awatch-agent-rs")
}
