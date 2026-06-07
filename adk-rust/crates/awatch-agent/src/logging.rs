use chrono::Utc;
use serde::Serialize;

#[derive(Debug, Serialize)]
struct LogLine<'a> {
    timestamp: String,
    level: &'a str,
    agent_id: &'a str,
    component: &'a str,
    message: &'a str,
}

pub fn log_json(agent_id: &str, level: &str, component: &str, message: &str) {
    let line = LogLine {
        timestamp: Utc::now().to_rfc3339(),
        level,
        agent_id,
        component,
        message,
    };
    if let Ok(json) = serde_json::to_string(&line) {
        eprintln!("{json}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_log_shape_is_serializable() {
        let line = LogLine {
            timestamp: "2026-06-07T00:00:00Z".to_string(),
            level: "INFO",
            agent_id: "agent-1",
            component: "spool",
            message: "queued",
        };
        let value = serde_json::to_value(line).unwrap();
        assert_eq!(value["level"], "INFO");
        assert_eq!(value["component"], "spool");
    }
}
