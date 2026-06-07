use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::AgentConfig;

pub const AGENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TelemetryEnvelope {
    pub agent_id: String,
    pub host_id: String,
    pub platform: String,
    pub timestamp: DateTime<Utc>,
    pub records: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Heartbeat {
    pub agent_version: String,
    pub platform: String,
    pub status: AgentStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Online,
    Degraded,
    Offline,
}

impl TelemetryEnvelope {
    pub fn empty(config: &AgentConfig) -> Self {
        Self {
            agent_id: config.agent_id.clone(),
            host_id: config.host_id.clone(),
            platform: config.platform.clone(),
            timestamp: Utc::now(),
            records: Vec::new(),
        }
    }

    pub fn heartbeat(config: &AgentConfig) -> Self {
        let heartbeat = Heartbeat {
            agent_version: AGENT_VERSION.to_string(),
            platform: config.platform.clone(),
            status: AgentStatus::Online,
        };
        Self {
            records: vec![serde_json::json!({
                "type": "heartbeat",
                "payload": heartbeat,
            })],
            ..Self::empty(config)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heartbeat_contract_is_stable_and_contains_no_inventory() {
        let config = AgentConfig::default();
        let envelope = TelemetryEnvelope::heartbeat(&config);
        let value = serde_json::to_value(&envelope).unwrap();
        assert_eq!(value["agent_id"], config.agent_id);
        assert_eq!(value["host_id"], config.host_id);
        assert_eq!(value["platform"], config.platform);
        assert!(value["records"].is_array());
        assert_eq!(value["records"][0]["type"], "heartbeat");
        assert_eq!(value["records"][0]["payload"]["status"], "online");
        assert!(value["records"][0]["payload"].get("hostname").is_none());
        assert!(value["records"][0]["payload"].get("processes").is_none());
    }
}
