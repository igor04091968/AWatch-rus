//! Production-facing portal runtime support.
//!
//! This module groups the cross-cutting concerns that must stay consistent
//! across all portal routes: health/readiness/version contracts, query and
//! configuration limits, structured logging, Prometheus-style metrics and
//! request correlation metadata.
//!
//! CONTRACT: keep this module free from role-specific business rendering. It is
//! the operational boundary around the portal, not the workforce/security report
//! implementation itself.

pub(crate) mod health;
pub(crate) mod limits;
pub(crate) mod logging;
pub(crate) mod metrics;
pub(crate) mod readiness;
pub(crate) mod request_context;
pub(crate) mod version;

pub(crate) use health::build_healthz;
pub(crate) use limits::{is_limited_api_route, validate_api_query_limits, validate_portal_config};
pub(crate) use logging::log_http_request;
pub(crate) use metrics::{
    record_http_metric, record_ingestion_accepted, record_ingestion_rejected,
    record_report_cache_hit, record_report_cache_miss, record_report_cache_stale_hit,
    record_report_generated, record_report_request, render_prometheus_metrics,
};
pub(crate) use readiness::build_readyz;
pub(crate) use request_context::{http_request_metadata, mark_request_started};
pub(crate) use version::build_version;
