//! Portal API contract summary payload.
//!
//! CONTRACT: this module describes stable public API routes exposed by the
//! current Rust HTML/HTMX portal and future clients. Keep changes additive
//! unless the OpenAPI/TypeScript contracts are updated in the same PR.

use serde_json::{Value, json};

pub(crate) fn api_contract_summary() -> Value {
    json!({
        "ok": true,
        "contract_version": "2026-06-06.pilot-v1",
        "generated_by": "detmir-portal",
        "api_base": "/api",
        "compatibility": {
            "policy": "additive",
            "main_ui": "rust-server-rendered-html-htmx-compatible",
            "unknown_fields": "clients must ignore unknown fields",
            "nullable_fields": "clients must tolerate null and missing optional fields",
            "forbidden_ui_stacks": ["dioxus", "react", "tauri", "electron"]
        },
        "targets": ["rust-html", "htmx-compatible"],
        "artifacts": {
            "openapi": "/api/contracts/openapi.json",
            "typescript": "/api/contracts/typescript.d.ts"
        },
        "stable_endpoints": [
            {"method": "GET", "path": "/healthz", "purpose": "process liveness without external dependency checks"},
            {"method": "GET", "path": "/readyz", "purpose": "local readiness and contract-only dependency status"},
            {"method": "GET", "path": "/version", "purpose": "safe build and schema version metadata"},
            {"method": "GET", "path": "/metrics", "purpose": "Prometheus metrics without high-cardinality labels"},
            {"method": "GET", "path": "/api/health", "purpose": "light service health"},
            {"method": "GET", "path": "/api/contracts", "purpose": "contract index"},
            {"method": "GET", "path": "/api/contracts/openapi.json", "purpose": "OpenAPI contract"},
            {"method": "GET", "path": "/api/contracts/typescript.d.ts", "purpose": "TypeScript declarations"},
            {"method": "GET", "path": "/api/operator", "purpose": "portal overview data"},
            {"method": "GET", "path": "/api/reports", "purpose": "management report payload"},
            {"method": "GET", "path": "/api/executive", "purpose": "executive role payload"},
            {"method": "GET", "path": "/api/workforce", "purpose": "workforce role payload"},
            {"method": "GET", "path": "/api/security", "purpose": "security role payload"},
            {"method": "GET", "path": "/api/forensics", "purpose": "forensics role payload"},
            {"method": "GET", "path": "/api/ueba", "purpose": "rule-based UEBA score v1"},
            {"method": "GET", "path": "/api/pfsense", "purpose": "pfSense readiness contracts and demo fixtures"},
            {"method": "GET", "path": "/api/incidents", "purpose": "incident and DLP evidence summary"},
            {"method": "GET", "path": "/api/cases", "purpose": "case list"},
            {"method": "POST", "path": "/api/incident-review", "purpose": "manual candidate review status"},
            {"method": "POST", "path": "/api/cases", "purpose": "manual case creation"},
            {"method": "GET", "path": "/api/investigation-pack/{candidate_id}", "purpose": "candidate investigation pack"},
            {"method": "GET", "path": "/api/dlp/evidence", "purpose": "DLP evidence list"},
            {"method": "GET", "path": "/api/readiness/latest", "purpose": "latest readiness status"},
            {"method": "GET", "path": "/api/workforce/policy/explain", "purpose": "workforce policy explanation"},
            {"method": "GET", "path": "/api/workforce/kpi/explain", "purpose": "rule-based Workforce KPI explanation"},
            {"method": "GET", "path": "/api/risk/narrative", "purpose": "rule-based executive risk narrative"},
            {"method": "GET", "path": "/api/actions", "purpose": "rule-based executive action center"}
        ]
    })
}
