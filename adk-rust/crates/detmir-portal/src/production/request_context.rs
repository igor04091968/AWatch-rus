//! Request correlation and route classification for portal observability.
//!
//! This module derives a low-cardinality route name, business module and role
//! label for each request. These fields are used by structured logs and metrics.
//!
//! CONTRACT: generated route names must not expose volatile identifiers such as
//! case IDs, candidate IDs or evidence IDs; use route templates instead.

use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use tiny_http::Request;

use crate::{
    normalize_path, parse_case_path, parse_case_status_path, parse_evidence_screenshot_path,
    parse_investigation_pack_path, portal_role_from_request,
};

static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static REQUEST_STARTED_AT: RefCell<Option<Instant>> = const { RefCell::new(None) };
}

#[derive(Clone, Debug)]
pub(crate) struct HttpRequestMetadata {
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) route: String,
    pub(crate) module: String,
    pub(crate) role: String,
    pub(crate) request_id: String,
    pub(crate) correlation_id: String,
    pub(crate) latency_ms: u64,
}

pub(crate) fn mark_request_started() {
    REQUEST_STARTED_AT.with(|started| {
        *started.borrow_mut() = Some(Instant::now());
    });
}

fn request_latency_ms() -> u64 {
    REQUEST_STARTED_AT.with(|started| {
        started
            .borrow()
            .as_ref()
            .map(|instant| instant.elapsed().as_millis() as u64)
            .unwrap_or(0)
    })
}

pub(crate) fn http_request_metadata(request: &Request) -> HttpRequestMetadata {
    let raw_url = request.url().to_string();
    let path = normalize_path(&raw_url);
    let method = request.method().as_str().to_string();
    let route = metrics_route(&path);
    let module = metrics_module(&route).to_string();
    let role = portal_role_from_request(request, &raw_url)
        .as_str()
        .to_string();
    let request_id =
        request_header(request, "X-Request-Id").or_else(|| request_header(request, "X-Request-ID"));
    let correlation_id = request_header(request, "X-Correlation-Id")
        .or_else(|| request_header(request, "X-Correlation-ID"));
    let (request_id, correlation_id) = resolve_request_ids(request_id, correlation_id);
    HttpRequestMetadata {
        method,
        path,
        route,
        module,
        role,
        request_id,
        correlation_id,
        latency_ms: request_latency_ms(),
    }
}

fn request_header(request: &Request, name: &str) -> Option<String> {
    request
        .headers()
        .iter()
        .find(|header| header.field.to_string().eq_ignore_ascii_case(name))
        .map(|header| header.value.as_str().to_string())
}

fn resolve_request_ids(
    request_id: Option<String>,
    correlation_id: Option<String>,
) -> (String, String) {
    let request_id = request_id
        .map(sanitize_request_token)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(generate_request_id);
    let correlation_id = correlation_id
        .map(sanitize_request_token)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| request_id.clone());
    (request_id, correlation_id)
}

fn sanitize_request_token(value: String) -> String {
    // SECURITY: log correlation tokens are accepted from reverse proxies and
    // clients, so strip control characters and path separators before they reach
    // logs or metric labels. Truncation bounds accidental high-cardinality input.
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':'))
        .take(96)
        .collect()
}

fn generate_request_id() -> String {
    let seq = REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!("awatch-{millis}-{seq}")
}

fn metrics_route(path: &str) -> String {
    if parse_investigation_pack_path(path).is_some() {
        return "/api/investigation-pack/{candidate_id}".to_string();
    }
    if parse_case_path(path).is_some() {
        return "/api/cases/{case_id}".to_string();
    }
    if parse_case_status_path(path).is_some() {
        return "/api/cases/{case_id}/status".to_string();
    }
    if parse_evidence_screenshot_path(path).is_some() {
        return "/api/dlp/evidence/{evidence_id}/asset".to_string();
    }
    match path {
        "/" | "/operator" | "/manager" | "/owner" | "/incidents" | "/reports" | "/architecture" => {
            path.to_string()
        }
        "/healthz" | "/api/healthz" => "/healthz".to_string(),
        "/readyz" | "/api/readyz" => "/readyz".to_string(),
        "/version" | "/api/version" => "/version".to_string(),
        "/metrics" | "/api/metrics" => "/metrics".to_string(),
        _ if path.starts_with("/api/") => path.to_string(),
        _ => "other".to_string(),
    }
}

fn metrics_module(route: &str) -> &'static str {
    if route.starts_with("/healthz")
        || route.starts_with("/readyz")
        || route.starts_with("/version")
        || route.starts_with("/metrics")
    {
        "runtime"
    } else if route.contains("/workforce") || route == "/manager" {
        "workforce"
    } else if route.contains("/security")
        || route.contains("/incidents")
        || route.contains("/incident-review")
    {
        "security"
    } else if route.contains("/forensics")
        || route.contains("/investigation-pack")
        || route.contains("/cases")
        || route.contains("/dlp/evidence")
    {
        "forensics"
    } else if route.contains("/ueba") {
        "ueba"
    } else if route.contains("/pfsense") {
        "pfsense"
    } else if route.contains("/reports") || route.contains("/executive") || route == "/operator" {
        "reports"
    } else {
        "portal"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_id_and_correlation_id_are_sanitized_and_linked() {
        let (request_id, correlation_id) = resolve_request_ids(
            Some(" req-123\nbad ".to_string()),
            Some("corr-456/ignored".to_string()),
        );
        assert_eq!(request_id, "req-123bad");
        assert_eq!(correlation_id, "corr-456ignored");

        let (request_id, correlation_id) =
            resolve_request_ids(Some("rid_1".to_string()), Some(" \n ".to_string()));
        assert_eq!(request_id, "rid_1");
        assert_eq!(correlation_id, "rid_1");

        let (generated, generated_correlation) = resolve_request_ids(None, None);
        assert!(generated.starts_with("awatch-"));
        assert_eq!(generated, generated_correlation);
    }
}
