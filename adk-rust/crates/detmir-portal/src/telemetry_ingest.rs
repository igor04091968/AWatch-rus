//! Telemetry ingest API for Rust endpoint/agent diagnostics.
//!
//! CONTRACT: this module owns `/api/telemetry` authentication, request-body
//! validation and JSONL append semantics. Keep accepted fields, status codes,
//! metrics counters and response shape stable unless telemetry contracts are
//! updated in the same PR.

use std::fs;
use std::fs::OpenOptions;
use std::io::Write;

use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};
use tiny_http::{Request, StatusCode};

use crate::production::{record_ingestion_accepted, record_ingestion_rejected};
use crate::{
    Cli, is_payload_too_large, now, read_limited_body, respond_json, respond_json_status,
    respond_payload_too_large,
};

pub(crate) fn telemetry_authorized(request: &Request, args: &Cli) -> bool {
    let expected = args.telemetry_api_key.trim();
    if expected.is_empty() || expected == "change-me" {
        return false;
    }
    let actual = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("x-api-key"))
        .map(|header| header.value.as_str().trim().to_string())
        .or_else(|| bearer_token(request));
    actual
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(|value| constant_time_eq(value.as_bytes(), expected.as_bytes()))
        .unwrap_or(false)
}

pub(crate) fn bearer_token(request: &Request) -> Option<String> {
    request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Authorization"))
        .map(|header| header.value.as_str().trim())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

pub(crate) fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0_u8;
    for (a, b) in left.iter().zip(right.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

pub(crate) fn handle_telemetry_ingest(mut request: Request, args: &Cli) -> Result<()> {
    if !telemetry_authorized(&request, args) {
        record_ingestion_rejected();
        return respond_json_status(
            request,
            StatusCode(401),
            &json!({
                "ok": false,
                "error": "telemetry api key is missing or invalid"
            }),
        );
    }
    let telemetry_limit = args.max_request_body_bytes.min(1024 * 1024);
    let body = match read_limited_body(&mut request, telemetry_limit) {
        Ok(body) => body,
        Err(err) if is_payload_too_large(&err) => {
            record_ingestion_rejected();
            return respond_payload_too_large(request);
        }
        Err(err) => return Err(err),
    };
    let response = apply_telemetry_ingest(args, &body);
    match response {
        Ok(response) => {
            record_ingestion_accepted();
            respond_json(request, &response)
        }
        Err(err) => {
            record_ingestion_rejected();
            respond_json_status(
                request,
                StatusCode(400),
                &json!({
                    "ok": false,
                    "error": err.to_string()
                }),
            )
        }
    }
}

pub(crate) fn apply_telemetry_ingest(args: &Cli, body: &str) -> Result<Value> {
    let payload: Value =
        serde_json::from_str(body).map_err(|err| anyhow!("invalid telemetry JSON: {err}"))?;
    validate_telemetry_payload(&payload)?;
    let received_at_utc = now();
    let envelope = json!({
        "received_at_utc": received_at_utc,
        "prototype": true,
        "record": payload,
    });
    if let Some(parent) = args.telemetry_store_path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&args.telemetry_store_path)
        .with_context(|| format!("open {}", args.telemetry_store_path.display()))?;
    writeln!(file, "{}", serde_json::to_string(&envelope)?)
        .with_context(|| format!("append {}", args.telemetry_store_path.display()))?;
    Ok(json!({
        "ok": true,
        "prototype": true,
        "stored": "file-backed-jsonl",
        "received_at_utc": received_at_utc,
    }))
}

pub(crate) fn validate_telemetry_payload(payload: &Value) -> Result<()> {
    let Some(object) = payload.as_object() else {
        return Err(anyhow!("telemetry payload must be a JSON object"));
    };
    for field in [
        "agent_id",
        "hostname",
        "os_name",
        "os_version",
        "platform",
        "username",
        "timestamp",
        "uptime_seconds",
        "cpu_usage_percent",
        "memory_total",
        "memory_used",
        "active_sessions",
        "rdp_sessions",
        "ssh_sessions",
        "processes",
        "network_interfaces",
        "network_connections",
        "workforce_activity",
        "security_events",
        "collector_version",
    ] {
        if !object.contains_key(field) {
            return Err(anyhow!("telemetry field is missing: {field}"));
        }
    }
    for field in [
        "active_sessions",
        "rdp_sessions",
        "ssh_sessions",
        "processes",
        "network_interfaces",
        "network_connections",
        "security_events",
    ] {
        if payload.get(field).and_then(Value::as_array).is_none() {
            return Err(anyhow!("telemetry field must be an array: {field}"));
        }
    }
    if payload
        .get("workforce_activity")
        .and_then(Value::as_object)
        .is_none()
    {
        return Err(anyhow!(
            "telemetry field must be an object: workforce_activity"
        ));
    }
    Ok(())
}
