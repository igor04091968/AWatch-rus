//! Structured HTTP access logging for the portal runtime.
//!
//! CONTRACT: logs are emitted as single-line JSON to stderr so systemd/journald,
//! container runtimes and log forwarders can parse them without scraping free
//! text. Do not log raw request bodies, secrets, evidence bytes or personal
//! payloads here.

use serde_json::{Value, json};
use tiny_http::StatusCode;

use super::request_context::HttpRequestMetadata;
use crate::now;

pub(crate) fn log_http_request(
    metadata: &HttpRequestMetadata,
    status: StatusCode,
    response_bytes: usize,
) {
    let level = if status.0 >= 500 {
        "ERROR"
    } else if status.0 >= 400 {
        "WARN"
    } else {
        "INFO"
    };
    let error_code = if status.0 >= 400 {
        Value::String(format!("http_{}", status.0))
    } else {
        Value::Null
    };

    // SECURITY: include routing/correlation fields, but do not include query
    // values, request body, headers or tokens. Those can contain employee data,
    // screenshots, evidence references or API keys.
    eprintln!(
        "{}",
        json!({
            "timestamp": now(),
            "level": level,
            "request_id": &metadata.request_id,
            "correlation_id": &metadata.correlation_id,
            "method": &metadata.method,
            "path": &metadata.path,
            "route": &metadata.route,
            "status": status.0,
            "latency_ms": metadata.latency_ms,
            "user_role": &metadata.role,
            "module": &metadata.module,
            "error_code": error_code,
            "response_bytes": response_bytes,
        })
    );
}
