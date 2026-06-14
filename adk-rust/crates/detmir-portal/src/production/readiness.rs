//! Readiness probe payload.
//!
//! CONTRACT: `/readyz` checks whether the portal is safe to receive normal
//! traffic. It must remain conservative: configuration errors and broken state
//! storage make the process `not_ready`; optional integrations can report
//! `disabled`, `not_required` or `contract_only` without failing the whole probe.

use std::path::Path;

use serde_json::{Value, json};

use super::limits::validate_portal_config;
use crate::{Cli, now};

pub(crate) fn build_readyz(args: &Cli) -> Value {
    let config_ok = validate_portal_config(args).is_ok();
    let storage_status = storage_readiness_status(&args.state_dir);
    let telemetry_status = storage_parent_readiness_status(&args.telemetry_store_path);
    let evidence_status = storage_readiness_status(&args.evidence_root);
    let ready = config_ok && !matches!(storage_status.as_str(), "error");
    json!({
        "status": if ready { "ready" } else { "not_ready" },
        "generated_at_utc": now(),
        "checks": {
            "config": if config_ok { "ok" } else { "error" },
            "storage": storage_status,
            "telemetry_store": telemetry_status,
            "evidence_storage": evidence_status,
            "pfsense": "contract_only",
            "security_events": security_events_readiness_status(args),
            "clickhouse": if args.security_events_backend == "clickhouse" { "configured" } else { "not_required" }
        }
    })
}

fn storage_readiness_status(path: &Path) -> String {
    if path.exists() {
        if path.is_dir() {
            "ok".to_string()
        } else {
            "error".to_string()
        }
    } else {
        "not_configured".to_string()
    }
}

fn storage_parent_readiness_status(path: &Path) -> String {
    path.parent()
        .map(storage_readiness_status)
        .unwrap_or_else(|| "not_configured".to_string())
}

fn security_events_readiness_status(args: &Cli) -> &'static str {
    match args
        .security_events_backend
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "disabled" | "" => "disabled",
        "clickhouse" => "configured",
        _ => "configured",
    }
}
