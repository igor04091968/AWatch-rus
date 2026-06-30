//! In-process Prometheus-style metrics for the portal.
//!
//! CONTRACT: metric names and label keys are part of the operational contract
//! used by dashboards and smoke checks. Additive metrics are allowed; renaming
//! existing metrics requires synchronized dashboard/documentation changes.

use std::collections::BTreeMap;
use std::fmt::Write as FmtWrite;
use std::sync::{Mutex, OnceLock};

use tiny_http::StatusCode;

use super::readiness::build_readyz;
use super::request_context::HttpRequestMetadata;
use crate::Cli;

static PORTAL_METRICS: OnceLock<Mutex<PortalMetrics>> = OnceLock::new();

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct HttpMetricKey {
    method: String,
    route: String,
    status: u16,
    module: String,
}

#[derive(Clone, Debug, Default)]
struct HttpMetricValue {
    requests_total: u64,
    duration_seconds_sum: f64,
    duration_seconds_count: u64,
}

#[derive(Clone, Debug, Default)]
struct PortalMetrics {
    http: BTreeMap<HttpMetricKey, HttpMetricValue>,
    report_requests_total: u64,
    report_cache_hits_total: u64,
    report_cache_misses_total: u64,
    report_cache_stale_hits_total: u64,
    reports_generated_total: u64,
    ingestion_records_total: u64,
    ingestion_rejected_total: u64,
    role_denied_total: u64,
}

fn portal_metrics() -> &'static Mutex<PortalMetrics> {
    PORTAL_METRICS.get_or_init(|| Mutex::new(PortalMetrics::default()))
}

pub(crate) fn record_http_metric(metadata: &HttpRequestMetadata, status: StatusCode) {
    if let Ok(mut metrics) = portal_metrics().lock() {
        let entry = metrics
            .http
            .entry(HttpMetricKey {
                method: metadata.method.clone(),
                route: metadata.route.clone(),
                status: status.0,
                module: metadata.module.clone(),
            })
            .or_default();
        entry.requests_total = entry.requests_total.saturating_add(1);
        entry.duration_seconds_sum += metadata.latency_ms as f64 / 1_000.0;
        entry.duration_seconds_count = entry.duration_seconds_count.saturating_add(1);
        if status.0 == 403 {
            metrics.role_denied_total = metrics.role_denied_total.saturating_add(1);
        }
    }
}

pub(crate) fn record_report_generated() {
    if let Ok(mut metrics) = portal_metrics().lock() {
        metrics.reports_generated_total = metrics.reports_generated_total.saturating_add(1);
    }
}

pub(crate) fn record_report_request() {
    if let Ok(mut metrics) = portal_metrics().lock() {
        metrics.report_requests_total = metrics.report_requests_total.saturating_add(1);
    }
}

pub(crate) fn record_report_cache_hit() {
    if let Ok(mut metrics) = portal_metrics().lock() {
        metrics.report_cache_hits_total = metrics.report_cache_hits_total.saturating_add(1);
    }
}

pub(crate) fn record_report_cache_stale_hit() {
    if let Ok(mut metrics) = portal_metrics().lock() {
        metrics.report_cache_hits_total = metrics.report_cache_hits_total.saturating_add(1);
        metrics.report_cache_stale_hits_total =
            metrics.report_cache_stale_hits_total.saturating_add(1);
    }
}

pub(crate) fn record_report_cache_miss() {
    if let Ok(mut metrics) = portal_metrics().lock() {
        metrics.report_cache_misses_total = metrics.report_cache_misses_total.saturating_add(1);
    }
}

pub(crate) fn record_ingestion_accepted() {
    if let Ok(mut metrics) = portal_metrics().lock() {
        metrics.ingestion_records_total = metrics.ingestion_records_total.saturating_add(1);
    }
}

pub(crate) fn record_ingestion_rejected() {
    if let Ok(mut metrics) = portal_metrics().lock() {
        metrics.ingestion_rejected_total = metrics.ingestion_rejected_total.saturating_add(1);
    }
}

pub(crate) fn render_prometheus_metrics(args: &Cli) -> String {
    let mut text = String::new();
    let ready_value = if build_readyz(args)
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("not_ready")
        == "ready"
    {
        1
    } else {
        0
    };
    writeln!(
        &mut text,
        "# HELP awatch_http_requests_total HTTP requests handled by AWatch-rus portal"
    )
    .ok();
    writeln!(&mut text, "# TYPE awatch_http_requests_total counter").ok();
    let metrics = portal_metrics()
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default();
    for (key, value) in &metrics.http {
        writeln!(
            &mut text,
            "awatch_http_requests_total{{method=\"{}\",route=\"{}\",status=\"{}\",module=\"{}\"}} {}",
            prom_escape(&key.method),
            prom_escape(&key.route),
            key.status,
            prom_escape(&key.module),
            value.requests_total
        )
        .ok();
    }
    writeln!(
        &mut text,
        "# HELP awatch_http_request_duration_seconds HTTP request duration in seconds"
    )
    .ok();
    writeln!(
        &mut text,
        "# TYPE awatch_http_request_duration_seconds summary"
    )
    .ok();
    for (key, value) in &metrics.http {
        writeln!(
            &mut text,
            "awatch_http_request_duration_seconds_sum{{method=\"{}\",route=\"{}\",status=\"{}\",module=\"{}\"}} {:.6}",
            prom_escape(&key.method),
            prom_escape(&key.route),
            key.status,
            prom_escape(&key.module),
            value.duration_seconds_sum
        )
        .ok();
        writeln!(
            &mut text,
            "awatch_http_request_duration_seconds_count{{method=\"{}\",route=\"{}\",status=\"{}\",module=\"{}\"}} {}",
            prom_escape(&key.method),
            prom_escape(&key.route),
            key.status,
            prom_escape(&key.module),
            value.duration_seconds_count
        )
        .ok();
    }
    for (name, help, value) in [
        (
            "awatch_report_requests_total",
            "Report payload requests handled by the portal cache layer",
            metrics.report_requests_total,
        ),
        (
            "awatch_report_cache_hits_total",
            "Report payload requests served from the in-process cache",
            metrics.report_cache_hits_total,
        ),
        (
            "awatch_report_cache_misses_total",
            "Report payload requests that triggered report regeneration",
            metrics.report_cache_misses_total,
        ),
        (
            "awatch_report_cache_stale_hits_total",
            "Report payload requests served from stale cache while refresh runs",
            metrics.report_cache_stale_hits_total,
        ),
        (
            "awatch_reports_generated_total",
            "Reports generated by the portal",
            metrics.reports_generated_total,
        ),
        (
            "awatch_ingestion_records_total",
            "Telemetry ingestion records accepted",
            metrics.ingestion_records_total,
        ),
        (
            "awatch_ingestion_rejected_total",
            "Telemetry ingestion records rejected",
            metrics.ingestion_rejected_total,
        ),
        (
            "awatch_role_denied_total",
            "Requests denied by role gates",
            metrics.role_denied_total,
        ),
    ] {
        writeln!(&mut text, "# HELP {name} {help}").ok();
        writeln!(&mut text, "# TYPE {name} counter").ok();
        writeln!(&mut text, "{name} {value}").ok();
    }
    writeln!(
        &mut text,
        "# HELP awatch_readyz_status Portal readiness status, 1=ready, 0=not_ready"
    )
    .ok();
    writeln!(&mut text, "# TYPE awatch_readyz_status gauge").ok();
    writeln!(&mut text, "awatch_readyz_status {ready_value}").ok();
    text
}

fn prom_escape(value: &str) -> String {
    // SECURITY: metric label values are route/module tokens, but escaping keeps
    // the endpoint safe if future callers pass proxy-derived values.
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
