use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use chrono::{NaiveDate, SecondsFormat, Utc};
use clap::Parser;
use reqwest::blocking::Client;
use reqwest::header::{CONNECTION, HeaderValue};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

const INDEX_HTML: &str = include_str!("static/index.html");
const APP_CSS: &str = include_str!("static/app.css");
const APP_JS: &str = include_str!("static/app.js");
const UEBA_BASELINE_MIN_SAMPLES: usize = 3;
const SNAPSHOT_CACHE_TTL: Duration = Duration::from_secs(5);

type SnapshotCache = Arc<Mutex<Option<CachedSnapshot>>>;

#[derive(Clone, Debug)]
struct CachedSnapshot {
    created: Instant,
    snapshot: Snapshot,
}

#[derive(Clone, Debug, Parser)]
#[command(about = "Read-only DetMir operator/manager/owner web portal")]
struct Cli {
    #[arg(long, default_value = "127.0.0.1:8720", env = "DETMIR_PORTAL_BIND")]
    bind: String,

    #[arg(
        long,
        default_value = "detmir-status --json",
        env = "DETMIR_PORTAL_STATUS_CMD"
    )]
    status_cmd: String,

    #[arg(
        long,
        default_value = "detmir-check --json",
        env = "DETMIR_PORTAL_CHECK_CMD"
    )]
    check_cmd: String,

    #[arg(
        long,
        default_value = "systemctl --failed --no-pager",
        env = "DETMIR_PORTAL_FAILED_UNITS_CMD"
    )]
    failed_units_cmd: String,

    #[arg(
        long,
        default_value = "http://192.0.2.13:5610",
        env = "DETMIR_PORTAL_WORKTIME_URL"
    )]
    worktime_url: String,

    #[arg(
        long,
        default_value = "http://192.0.2.2:8710",
        env = "DETMIR_PORTAL_ONE_C_URL"
    )]
    one_c_url: String,

    #[arg(
        long,
        default_value = "/etc/detmir-portal-workforce-policy.json",
        env = "DETMIR_PORTAL_WORKFORCE_POLICY_PATH"
    )]
    workforce_policy_path: PathBuf,

    #[arg(
        long,
        default_value = "/etc/detmir-portal-ueba-policy.yaml",
        env = "DETMIR_PORTAL_UEBA_POLICY_PATH"
    )]
    ueba_policy_path: PathBuf,

    #[arg(long, default_value_t = 10, env = "DETMIR_PORTAL_TIMEOUT_SECONDS")]
    timeout_seconds: u64,

    #[arg(
        long,
        default_value = "/var/lib/detmir-portal",
        env = "DETMIR_PORTAL_STATE_DIR"
    )]
    state_dir: PathBuf,

    #[arg(
        long,
        default_value = "/var/lib/activitywatch/dlp_warehouse.sqlite",
        env = "DETMIR_PORTAL_DLP_DB_PATH"
    )]
    dlp_db_path: PathBuf,

    #[arg(
        long,
        default_value = "/var/lib/detmir-portal/evidence",
        env = "DETMIR_PORTAL_EVIDENCE_ROOT"
    )]
    evidence_root: PathBuf,

    #[arg(
        long,
        default_value = "/var/lib/activitywatch/health/readiness-bundle",
        env = "DETMIR_PORTAL_READINESS_BUNDLE_DIR"
    )]
    readiness_bundle_dir: PathBuf,

    #[arg(long, default_value_t = 30, env = "DETMIR_PORTAL_EVIDENCE_LIMIT")]
    evidence_limit: u32,

    #[arg(
        long,
        default_value_t = 8 * 1024 * 1024,
        env = "DETMIR_PORTAL_EVIDENCE_MAX_BYTES"
    )]
    evidence_max_bytes: u64,

    #[arg(long)]
    json_smoke: bool,

    #[arg(long, env = "DETMIR_PORTAL_EVIDENCE_ONLY")]
    evidence_only: bool,

    #[arg(long, env = "DETMIR_PORTAL_EVIDENCE_UPLOAD_TOKEN")]
    evidence_upload_token: Option<String>,

    #[arg(
        long,
        default_value = "change-me",
        env = "DETMIR_PORTAL_TELEMETRY_API_KEY"
    )]
    telemetry_api_key: String,

    #[arg(
        long,
        default_value = "/var/lib/detmir-portal/telemetry.jsonl",
        env = "DETMIR_PORTAL_TELEMETRY_STORE_PATH"
    )]
    telemetry_store_path: PathBuf,

    #[arg(
        long,
        default_value = "config/expected_nodes.json",
        env = "DETMIR_PORTAL_EXPECTED_NODES_PATH"
    )]
    expected_nodes_path: PathBuf,
}

#[derive(Clone, Debug, Serialize)]
struct SourceStatus {
    ok: bool,
    status: String,
    summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<Value>,
}

#[derive(Clone, Debug, Serialize)]
struct AgentQuality {
    collector_source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    collector_error: Option<String>,
    sessions_collected_total: usize,
    active_sessions_total: usize,
    rdp_sessions_total: usize,
    quality_status: String,
}

impl Default for AgentQuality {
    fn default() -> Self {
        Self {
            collector_source: "unknown".to_string(),
            collector_error: None,
            sessions_collected_total: 0,
            active_sessions_total: 0,
            rdp_sessions_total: 0,
            quality_status: "unknown".to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct AgentQualityExplain {
    status: String,
    title: String,
    summary: String,
    recommendation: String,
    kpi_accepted: bool,
}

#[derive(Clone, Debug, Serialize)]
struct AgentQualityHistoryItem {
    date: String,
    status: String,
    source: String,
    kpi_accepted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    collector_error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
struct AgentQualityHistorySummary {
    days_observed: usize,
    ok_days: usize,
    warning_days: usize,
    degraded_days: usize,
    unknown_days: usize,
    kpi_accepted_days: usize,
    kpi_accepted_pct: u8,
}

#[derive(Clone, Debug, Serialize)]
struct AgentQualityNodeItem {
    hostname: String,
    last_seen_utc: String,
    source: String,
    status: String,
    kpi_accepted: bool,
    sessions_total: usize,
    rdp_sessions: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    collector_error: Option<String>,
    recommendation: String,
}

#[derive(Clone, Debug, Default, Serialize)]
struct AgentQualityNodesSummary {
    total_nodes: usize,
    ok_nodes: usize,
    degraded_nodes: usize,
    unknown_nodes: usize,
    accepted_kpi_nodes_pct: u8,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct ExpectedNode {
    hostname: String,
    #[serde(default)]
    department: String,
    #[serde(default)]
    owner: String,
    #[serde(default)]
    criticality: String,
}

#[derive(Clone, Debug, Serialize)]
struct AgentCoverageProblemNode {
    hostname: String,
    department: String,
    owner: String,
    last_seen_utc: String,
    status: String,
    recommendation: String,
}

#[derive(Clone, Debug, Serialize)]
struct AgentCoverageSla {
    expected_nodes: usize,
    reporting_nodes_24h: usize,
    stale_nodes: usize,
    missing_nodes: usize,
    coverage_pct: u8,
    freshness_pct: u8,
    sla_status: String,
    problem_nodes: Vec<AgentCoverageProblemNode>,
}

impl Default for AgentCoverageSla {
    fn default() -> Self {
        Self {
            expected_nodes: 0,
            reporting_nodes_24h: 0,
            stale_nodes: 0,
            missing_nodes: 0,
            coverage_pct: 0,
            freshness_pct: 0,
            sla_status: "UNKNOWN".to_string(),
            problem_nodes: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    ok: bool,
    generated_at_utc: String,
    version: String,
    sources: BTreeMap<String, bool>,
}

#[derive(Debug, Serialize)]
struct SummaryBlock {
    status: String,
    text: String,
}

#[derive(Debug, Serialize)]
struct SummaryResponse {
    severity: String,
    operator_ok: bool,
    headline: String,
    generated_at_utc: String,
    blocks: BTreeMap<String, SummaryBlock>,
}

#[derive(Debug, Serialize)]
struct IncidentItem {
    id: String,
    status: String,
    kind: String,
    source: String,
    summary: String,
    generated_at_utc: String,
    link: String,
    acknowledged: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    acknowledged_at_utc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    assigned_to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    comment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at_utc: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct IncidentStateFile {
    incidents: BTreeMap<String, IncidentActionState>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct IncidentActionState {
    state: String,
    actor: String,
    updated_at_utc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    acknowledged_at_utc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    assigned_to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    comment: Option<String>,
}

#[derive(Debug, Deserialize)]
struct IncidentActionRequest {
    id: String,
    action: String,
    #[serde(default)]
    assigned_to: Option<String>,
    #[serde(default)]
    comment: Option<String>,
}

#[derive(Debug, Serialize)]
struct IncidentActionResponse {
    ok: bool,
    id: String,
    state: IncidentActionState,
}

#[derive(Debug, Serialize)]
struct IncidentAuditEntry {
    generated_at_utc: String,
    actor: String,
    id: String,
    action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    assigned_to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    comment: Option<String>,
}

#[derive(Debug, Serialize)]
struct EvidenceAuditEntry {
    generated_at_utc: String,
    actor: String,
    action: String,
    evidence_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_file: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EvidenceUploadRequest {
    sha256: String,
    #[serde(default)]
    content_base64: String,
    #[serde(default)]
    content_type: Option<String>,
    #[serde(default)]
    source_file: Option<String>,
    #[serde(default)]
    source_path: Option<String>,
    #[serde(default)]
    hostname: Option<String>,
    #[serde(default)]
    username: Option<String>,
}

#[derive(Debug, Serialize)]
struct EvidenceUploadResponse {
    ok: bool,
    sha256: String,
    content_type: String,
    bytes: u64,
    stored: bool,
    path: String,
}

#[derive(Debug, Serialize)]
struct DlpEvidenceResponse {
    ok: bool,
    generated_at_utc: String,
    db_available: bool,
    screenshot_root_available: bool,
    limit: u32,
    items: Vec<DlpEvidenceItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct DlpEvidenceItem {
    id: String,
    event_ts: String,
    bucket_id: String,
    event_id: String,
    stream_type: String,
    hostname: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    severity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    signal_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rule_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_path: Option<String>,
    has_screenshot_metadata: bool,
    screenshot_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    screenshot_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    screenshot_width: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    screenshot_height: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    preview_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    download_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    blocked_reason: Option<String>,
}

#[derive(Debug)]
struct DlpEvidenceRow {
    row_id: i64,
    bucket_id: String,
    event_id: String,
    stream_type: String,
    hostname: String,
    username: Option<String>,
    event_ts: String,
    operation: Option<String>,
    file_path: Option<String>,
    rule_id: Option<String>,
    action: Option<String>,
    severity: Option<String>,
    signal_type: Option<String>,
    message: Option<String>,
    source: Option<String>,
    screenshot_path: Option<String>,
    raw_json: String,
}

#[derive(Debug, Clone)]
struct ScreenshotFile {
    path: PathBuf,
    content_type: &'static str,
    source_file: Option<String>,
    sha256: Option<String>,
}

#[derive(Debug, Serialize)]
struct PortalLinks {
    portal: String,
    grafana_dashboards: String,
    detmir_activitywatch: String,
    dlp_security_dashboard: String,
    dlp_management_dashboard: String,
    dlp_overview_dashboard: String,
    aw_ui: String,
    worktime_report: String,
    file1c_brief: String,
    file1c_actions: String,
}

#[derive(Clone, Debug)]
struct Snapshot {
    generated_at_utc: String,
    detmir_status: SourceStatus,
    detmir_check: SourceStatus,
    failed_units: SourceStatus,
    worktime: SourceStatus,
    worktime_management: SourceStatus,
    one_c: SourceStatus,
    agent_quality: AgentQuality,
    agent_quality_history: Vec<AgentQualityHistoryItem>,
    agent_quality_history_summary: AgentQualityHistorySummary,
    agent_quality_nodes: Vec<AgentQualityNodeItem>,
    agent_quality_nodes_summary: AgentQualityNodesSummary,
    agent_coverage_sla: AgentCoverageSla,
}

#[derive(Debug)]
struct ReportMetrics {
    users_count: usize,
    active_seconds: i64,
    apps_count: usize,
    dlp_ok: u64,
    dlp_warn: u64,
    dlp_fail: u64,
    evidence_total: usize,
    evidence_screenshots: usize,
    open_incidents: usize,
    acknowledged_incidents: usize,
    workforce_index: Option<u8>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct WorkforcePolicy {
    #[serde(default = "default_workforce_role")]
    default_role: String,
    #[serde(default)]
    roles: BTreeMap<String, WorkforceRolePolicy>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct WorkforceRolePolicy {
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    planned_hours_per_day: Option<f64>,
    #[serde(default)]
    default_weight: Option<f64>,
    #[serde(default)]
    application_weights: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct UebaRiskPolicy {
    #[serde(default = "default_ueba_policy_version")]
    version: String,
    #[serde(default = "default_ueba_baseline_status")]
    baseline_status: String,
    #[serde(default = "default_ueba_score_cap")]
    score_cap: u64,
    #[serde(default)]
    weights: BTreeMap<String, u64>,
    #[serde(default)]
    confidence: UebaConfidencePolicy,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct UebaConfidencePolicy {
    #[serde(default)]
    base: Option<f64>,
    #[serde(default)]
    evidence_bonus: Option<f64>,
    #[serde(default)]
    screenshot_bonus: Option<f64>,
    #[serde(default)]
    worktime_bonus: Option<f64>,
    #[serde(default)]
    policy_bonus: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct UebaBaselineState {
    #[serde(default = "default_ueba_baseline_state_version")]
    version: String,
    #[serde(default = "default_ueba_baseline_window_days")]
    baseline_window_days: i64,
    #[serde(default)]
    updated_at_utc: Option<String>,
    #[serde(default)]
    users: BTreeMap<String, UserBaseline>,
    #[serde(default)]
    departments: BTreeMap<String, DepartmentBaseline>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct UserBaseline {
    #[serde(default)]
    user_id: String,
    #[serde(default)]
    user: String,
    #[serde(default)]
    samples: Vec<BaselineSample>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct DepartmentBaseline {
    #[serde(default)]
    name: String,
    #[serde(default)]
    samples: Vec<BaselineSample>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct BaselineSample {
    date: String,
    index: f64,
    #[serde(default)]
    active_seconds: Option<i64>,
    #[serde(default)]
    users_count: Option<i64>,
}

#[derive(Debug, Clone)]
struct CurrentBaselinePoint {
    key: String,
    label: String,
    index: f64,
    active_seconds: Option<i64>,
    users_count: Option<i64>,
}

#[derive(Debug, Clone)]
struct BaselineDeviation {
    scope: &'static str,
    key: String,
    label: String,
    current_index: f64,
    baseline_index: f64,
    deviation_pct: f64,
    samples: usize,
    status: &'static str,
}

#[derive(Debug)]
struct WeightedActivity {
    role: String,
    role_label: String,
    index: Option<u8>,
    formula: String,
    planned_seconds: i64,
    app_seconds: i64,
    weighted_seconds: i64,
    matched_applications: usize,
    explanation: String,
    app_details: Vec<AppWeightDetail>,
    policy_audit: Value,
    employee_details: Vec<Value>,
}

#[derive(Debug)]
struct AppWeightDetail {
    application: String,
    seconds: i64,
    weight: f64,
    weighted_seconds: i64,
    matched_rule: String,
}

#[derive(Debug)]
struct ReportWorkforceSummary {
    departments_count: usize,
    owners_count: usize,
    insights_count: usize,
    trend_status: String,
}

fn main() {
    let code = match run() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("{err:#}");
            1
        }
    };
    std::process::exit(code);
}

fn run() -> Result<i32> {
    let args = Cli::parse();
    if args.json_smoke {
        let snapshot = build_snapshot(&args);
        let incident_state = load_incident_state_best_effort(&args);
        let ueba_baseline_path = ueba_baseline_state_path(&args);
        let smoke = json!({
            "health": build_health(&snapshot),
            "summary": build_summary(&snapshot),
            "reports": build_reports(&snapshot, &incident_state, &build_dlp_evidence_response(&args), &args.workforce_policy_path, &args.ueba_policy_path, &ueba_baseline_path, false),
            "incidents": build_incidents(&snapshot, &incident_state),
            "dlp_evidence": build_dlp_evidence_response(&args),
        });
        println!("{}", serde_json::to_string_pretty(&smoke)?);
        return Ok(if build_health(&snapshot).ok { 0 } else { 2 });
    }

    let server = Server::http(&args.bind).map_err(|err| anyhow!("bind {}: {err}", args.bind))?;
    let snapshot_cache: SnapshotCache = Arc::new(Mutex::new(None));
    eprintln!("detmir-portal listening on http://{}", args.bind);
    for request in server.incoming_requests() {
        let args = args.clone();
        let snapshot_cache = Arc::clone(&snapshot_cache);
        thread::spawn(move || {
            let result = if args.evidence_only {
                handle_evidence_only_request(request, &args)
            } else {
                handle_request(request, &args, &snapshot_cache)
            };
            if let Err(err) = result {
                eprintln!("detmir-portal request failed: {err:#}");
            }
        });
    }
    Ok(0)
}

fn handle_request(request: Request, args: &Cli, snapshot_cache: &SnapshotCache) -> Result<()> {
    let method = request.method().clone();
    let url = request.url().to_string();
    let path = normalize_path(&url);
    let anonymize = query_flag(&url, "anonymize");
    if method == Method::Post && path == "/api/incidents/action" {
        return handle_incident_action(request, args);
    }
    if method == Method::Post && path == "/api/telemetry" {
        return handle_telemetry_ingest(request, args);
    }
    if method != Method::Get {
        return respond_text(request, StatusCode(405), "Method Not Allowed", "text/plain");
    }
    if path == "/api/dlp/evidence" {
        return respond_json(request, &build_dlp_evidence_response(args));
    }
    if let Some((evidence_id, download)) = parse_evidence_screenshot_path(&path) {
        return handle_evidence_screenshot(request, args, &evidence_id, download);
    }
    match path.as_str() {
        "/" | "/operator" | "/manager" | "/owner" | "/incidents" | "/reports" => respond_text(
            request,
            StatusCode(200),
            INDEX_HTML,
            "text/html; charset=utf-8",
        ),
        "/app.css" => respond_text(request, StatusCode(200), APP_CSS, "text/css; charset=utf-8"),
        "/app.js" => respond_text(
            request,
            StatusCode(200),
            APP_JS,
            "application/javascript; charset=utf-8",
        ),
        "/favicon.ico" => respond_text(request, StatusCode(204), "", "image/x-icon"),
        "/api/health" => respond_json(
            request,
            &build_health(&cached_snapshot(args, snapshot_cache)),
        ),
        "/api/readiness/latest" => respond_json(request, &readiness_latest(args)),
        "/api/readiness/bundle" => respond_json(request, &readiness_bundle(args)),
        "/api/readiness/verify" => respond_json(request, &readiness_verify(args)),
        "/api/summary" => respond_json(
            request,
            &build_summary(&cached_snapshot(args, snapshot_cache)),
        ),
        "/api/operator" => {
            let snapshot = cached_snapshot(args, snapshot_cache);
            let incident_state = load_incident_state_best_effort(args);
            respond_json(request, &build_operator(&snapshot, &incident_state))
        }
        "/api/manager" => {
            let snapshot = cached_snapshot(args, snapshot_cache);
            respond_json(request, &build_manager(&snapshot))
        }
        "/api/workforce/policy/explain" => {
            let snapshot = cached_snapshot(args, snapshot_cache);
            respond_json(
                request,
                &build_workforce_policy_explain(&snapshot, &args.workforce_policy_path, anonymize),
            )
        }
        "/api/owner" => {
            let snapshot = cached_snapshot(args, snapshot_cache);
            respond_json(request, &build_owner(&snapshot))
        }
        "/api/reports" => {
            let snapshot = cached_snapshot(args, snapshot_cache);
            let incident_state = load_incident_state_best_effort(args);
            let evidence = build_dlp_evidence_response(args);
            let ueba_baseline_path = ueba_baseline_state_path(args);
            respond_json(
                request,
                &build_reports(
                    &snapshot,
                    &incident_state,
                    &evidence,
                    &args.workforce_policy_path,
                    &args.ueba_policy_path,
                    &ueba_baseline_path,
                    anonymize,
                ),
            )
        }
        "/api/incidents" => {
            let snapshot = cached_snapshot(args, snapshot_cache);
            let incident_state = load_incident_state_best_effort(args);
            respond_json(request, &build_incidents(&snapshot, &incident_state))
        }
        "/api/links" => respond_json(request, &links()),
        _ => respond_text(
            request,
            StatusCode(404),
            "Not Found",
            "text/plain; charset=utf-8",
        ),
    }
}

fn handle_evidence_only_request(request: Request, args: &Cli) -> Result<()> {
    let method = request.method().clone();
    let path = normalize_path(request.url());
    if method == Method::Post && path == "/api/dlp/evidence/upload" {
        return handle_evidence_upload(request, args);
    }
    if method != Method::Get {
        return respond_text(request, StatusCode(405), "Method Not Allowed", "text/plain");
    }
    if path == "/api/health" {
        return respond_json(
            request,
            &json!({
                "ok": args.dlp_db_path.exists(),
                "generated_at_utc": now(),
                "mode": "evidence-only",
                "db_available": args.dlp_db_path.exists(),
                "screenshot_root_available": args.evidence_root.exists(),
                "upload_enabled": upload_enabled(args),
            }),
        );
    }
    if path == "/api/readiness/latest" {
        return respond_json(request, &readiness_latest(args));
    }
    if path == "/api/readiness/bundle" {
        return respond_json(request, &readiness_bundle(args));
    }
    if path == "/api/readiness/verify" {
        return respond_json(request, &readiness_verify(args));
    }
    if path == "/api/dlp/evidence" {
        return respond_json(request, &build_dlp_evidence_response(args));
    }
    if let Some((evidence_id, download)) = parse_evidence_screenshot_path(&path) {
        return handle_evidence_screenshot(request, args, &evidence_id, download);
    }
    respond_text(
        request,
        StatusCode(404),
        "Not Found",
        "text/plain; charset=utf-8",
    )
}

fn normalize_path(url: &str) -> String {
    let path = url.split('?').next().unwrap_or("/");
    let path = path.strip_prefix("/portal").unwrap_or(path);
    if path.is_empty() {
        "/".to_string()
    } else {
        path.to_string()
    }
}

fn readiness_latest(args: &Cli) -> Value {
    read_json_file(
        &args
            .readiness_bundle_dir
            .join("detmir-readiness-latest.json"),
    )
    .unwrap_or_else(|err| {
        json!({
            "ok": false,
            "generated_at_utc": now(),
            "error": err.to_string(),
        })
    })
}

fn readiness_bundle(args: &Cli) -> Value {
    let dir = &args.readiness_bundle_dir;
    let status = read_json_file(&dir.join("detmir-readiness-status.json")).unwrap_or_else(|err| {
        json!({
            "ok": false,
            "error": err.to_string(),
        })
    });
    let latest_dir = fs::read_to_string(dir.join("latest-dir.txt"))
        .unwrap_or_default()
        .trim()
        .to_string();
    let artifacts = [
        "detmir-readiness-latest.json",
        "detmir-readiness-act.md",
        "detmir-readiness-act.html",
        "sha256sums.txt",
        "sha256sums.txt.sig",
        "public-key.pem",
        "detmir-readiness-status.json",
        "detmir-readiness.prom",
    ]
    .into_iter()
    .filter_map(|name| {
        let path = dir.join(name);
        path.metadata().ok().map(|meta| {
            json!({
                "name": name,
                "bytes": meta.len(),
                "available": true,
            })
        })
    })
    .collect::<Vec<_>>();
    json!({
        "ok": status.get("ok").and_then(Value::as_bool).unwrap_or(false),
        "generated_at_utc": now(),
        "bundle_dir": dir.display().to_string(),
        "latest_archive_dir": latest_dir,
        "status": status,
        "artifacts": artifacts,
    })
}

fn readiness_verify(args: &Cli) -> Value {
    let dir = &args.readiness_bundle_dir;
    let checksum = run_in_dir(
        dir,
        Command::new("sha256sum").arg("-c").arg("sha256sums.txt"),
    );
    let sig_path = dir.join("sha256sums.txt.sig");
    let pub_path = dir.join("public-key.pem");
    let signature = if sig_path.is_file() && pub_path.is_file() {
        run_in_dir(
            dir,
            Command::new("openssl")
                .arg("dgst")
                .arg("-sha256")
                .arg("-verify")
                .arg("public-key.pem")
                .arg("-signature")
                .arg("sha256sums.txt.sig")
                .arg("sha256sums.txt"),
        )
    } else {
        Err("signature files are not available".to_string())
    };
    json!({
        "ok": checksum.is_ok() && signature.is_ok(),
        "generated_at_utc": now(),
        "checksum_verified": checksum.is_ok(),
        "signature_verified": signature.is_ok(),
        "checksum_error": checksum.err(),
        "signature_error": signature.err(),
    })
}

fn read_json_file(path: &Path) -> Result<Value> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))
}

fn run_in_dir(dir: &Path, command: &mut Command) -> std::result::Result<(), String> {
    let output = command
        .current_dir(dir)
        .output()
        .map_err(|err| format!("run command in {}: {err}", dir.display()))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .trim()
        .to_string())
    }
}

fn query_flag(url: &str, key: &str) -> bool {
    let Some(query) = url.split_once('?').map(|(_, query)| query) else {
        return false;
    };
    query.split('&').any(|pair| {
        let (name, value) = pair.split_once('=').unwrap_or((pair, "1"));
        name == key && matches!(value, "1" | "true" | "yes" | "on")
    })
}

fn cached_snapshot(args: &Cli, cache: &SnapshotCache) -> Snapshot {
    let mut guard = cache.lock().expect("snapshot cache mutex poisoned");
    if let Some(cached) = guard.as_ref() {
        if cached.created.elapsed() <= SNAPSHOT_CACHE_TTL {
            return cached.snapshot.clone();
        }
    }
    let snapshot = build_snapshot(args);
    *guard = Some(CachedSnapshot {
        created: Instant::now(),
        snapshot: snapshot.clone(),
    });
    snapshot
}

fn build_snapshot(args: &Cli) -> Snapshot {
    let timeout = Duration::from_secs(args.timeout_seconds);
    let agent_quality_history = load_agent_quality_history(&args.telemetry_store_path, 7);
    let agent_quality_history_summary = summarize_agent_quality_history(&agent_quality_history);
    let agent_quality_nodes = load_agent_quality_nodes(&args.telemetry_store_path, 7);
    let agent_quality_nodes_summary = summarize_agent_quality_nodes(&agent_quality_nodes);
    let agent_coverage_sla =
        build_agent_coverage_sla(&args.expected_nodes_path, &agent_quality_nodes, Utc::now());
    Snapshot {
        generated_at_utc: now(),
        detmir_status: command_json_source("detmir_status", &args.status_cmd, timeout),
        detmir_check: command_json_source("detmir_check", &args.check_cmd, timeout),
        failed_units: command_text_source("failed_units", &args.failed_units_cmd, timeout),
        worktime: http_json_source(
            "worktime_api",
            &format!(
                "{}/reports/worktime/today",
                args.worktime_url.trim_end_matches('/')
            ),
            timeout,
        ),
        worktime_management: http_json_source(
            "worktime_management",
            &format!(
                "{}/reports/worktime/management?format=json&allow_stale=1",
                args.worktime_url.trim_end_matches('/')
            ),
            timeout,
        ),
        one_c: http_json_source(
            "one_c",
            &format!("{}/api/health", args.one_c_url.trim_end_matches('/')),
            timeout,
        ),
        agent_quality: load_agent_quality(&args.telemetry_store_path),
        agent_quality_history,
        agent_quality_history_summary,
        agent_quality_nodes,
        agent_quality_nodes_summary,
        agent_coverage_sla,
    }
}

fn load_agent_quality(path: &Path) -> AgentQuality {
    let Some(payload) = latest_telemetry_record(path) else {
        return AgentQuality::default();
    };
    agent_quality_from_record(&payload)
}

fn latest_telemetry_record(path: &Path) -> Option<Value> {
    let text = fs::read_to_string(path).ok()?;
    text.lines().rev().find_map(|line| {
        let envelope = serde_json::from_str::<Value>(line).ok()?;
        envelope.get("record").cloned().or(Some(envelope))
    })
}

fn load_agent_quality_history(path: &Path, days: i64) -> Vec<AgentQualityHistoryItem> {
    load_agent_quality_history_for_date(path, days, Utc::now().date_naive())
}

fn load_agent_quality_history_for_date(
    path: &Path,
    days: i64,
    today: NaiveDate,
) -> Vec<AgentQualityHistoryItem> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let start = today - chrono::Duration::days(days.saturating_sub(1));
    let mut by_date = BTreeMap::new();
    for line in text.lines() {
        let Some((date, record)) = telemetry_record_date_and_payload(line) else {
            continue;
        };
        if date < start || date > today {
            continue;
        }
        by_date.insert(date, agent_quality_history_item(date, &record));
    }
    by_date.into_values().collect()
}

fn telemetry_record_date_and_payload(line: &str) -> Option<(NaiveDate, Value)> {
    telemetry_record_time_date_and_payload(line).map(|(_, date, record)| (date, record))
}

fn telemetry_record_time_date_and_payload(line: &str) -> Option<(String, NaiveDate, Value)> {
    let envelope = serde_json::from_str::<Value>(line).ok()?;
    let record = envelope
        .get("record")
        .cloned()
        .unwrap_or_else(|| envelope.clone());
    let timestamp = record
        .get("timestamp")
        .or_else(|| envelope.get("stored_at_utc"))
        .and_then(Value::as_str)?;
    let parsed = chrono::DateTime::parse_from_rfc3339(timestamp).ok()?;
    let last_seen_utc = parsed
        .with_timezone(&Utc)
        .to_rfc3339_opts(SecondsFormat::Secs, true);
    Some((last_seen_utc, parsed.date_naive(), record))
}

fn agent_quality_history_item(date: NaiveDate, record: &Value) -> AgentQualityHistoryItem {
    let quality = agent_quality_from_record(record);
    let explain = agent_quality_explain(&quality);
    AgentQualityHistoryItem {
        date: date.to_string(),
        status: explain.status,
        source: quality.collector_source,
        kpi_accepted: explain.kpi_accepted,
        collector_error: quality.collector_error,
    }
}

fn summarize_agent_quality_history(
    history: &[AgentQualityHistoryItem],
) -> AgentQualityHistorySummary {
    let days_observed = history.len();
    let ok_days = history.iter().filter(|item| item.status == "OK").count();
    let warning_days = history
        .iter()
        .filter(|item| item.status == "WARNING")
        .count();
    let degraded_days = history
        .iter()
        .filter(|item| item.status == "DEGRADED")
        .count();
    let unknown_days = history
        .iter()
        .filter(|item| item.status == "UNKNOWN")
        .count();
    let kpi_accepted_days = history.iter().filter(|item| item.kpi_accepted).count();
    let kpi_accepted_pct = if days_observed == 0 {
        0
    } else {
        ((kpi_accepted_days * 100) / days_observed) as u8
    };
    AgentQualityHistorySummary {
        days_observed,
        ok_days,
        warning_days,
        degraded_days,
        unknown_days,
        kpi_accepted_days,
        kpi_accepted_pct,
    }
}

fn load_agent_quality_nodes(path: &Path, days: i64) -> Vec<AgentQualityNodeItem> {
    load_agent_quality_nodes_for_date(path, days, Utc::now().date_naive())
}

fn load_agent_quality_nodes_for_date(
    path: &Path,
    days: i64,
    today: NaiveDate,
) -> Vec<AgentQualityNodeItem> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let start = today - chrono::Duration::days(days.saturating_sub(1));
    let mut by_node: BTreeMap<String, (String, Value)> = BTreeMap::new();
    for line in text.lines() {
        let Some((last_seen_utc, date, record)) = telemetry_record_time_date_and_payload(line)
        else {
            continue;
        };
        if date < start || date > today {
            continue;
        }
        let key = telemetry_node_key(&record);
        let should_replace = by_node
            .get(&key)
            .map(|(current_last_seen, _)| last_seen_utc >= *current_last_seen)
            .unwrap_or(true);
        if should_replace {
            by_node.insert(key, (last_seen_utc, record));
        }
    }
    let mut nodes = by_node
        .into_values()
        .map(|(last_seen_utc, record)| agent_quality_node_item(last_seen_utc, &record))
        .collect::<Vec<_>>();
    nodes.sort_by(|left, right| {
        agent_quality_node_sort_rank(left)
            .cmp(&agent_quality_node_sort_rank(right))
            .then_with(|| left.hostname.cmp(&right.hostname))
    });
    nodes
}

fn telemetry_node_key(record: &Value) -> String {
    record
        .get("hostname")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            record
                .get("machine_id")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or("unknown")
        .to_string()
}

fn agent_quality_node_item(last_seen_utc: String, record: &Value) -> AgentQualityNodeItem {
    let quality = agent_quality_from_record(record);
    let explain = agent_quality_explain(&quality);
    AgentQualityNodeItem {
        hostname: telemetry_node_key(record),
        last_seen_utc,
        source: quality.collector_source,
        status: explain.status,
        kpi_accepted: explain.kpi_accepted,
        sessions_total: quality.sessions_collected_total,
        rdp_sessions: quality.rdp_sessions_total,
        collector_error: quality.collector_error,
        recommendation: explain.recommendation,
    }
}

fn agent_quality_node_sort_rank(item: &AgentQualityNodeItem) -> u8 {
    match item.status.as_str() {
        "DEGRADED" => 0,
        "WARNING" => 1,
        "UNKNOWN" => 2,
        "OK" if item.kpi_accepted => 4,
        _ => 3,
    }
}

fn summarize_agent_quality_nodes(nodes: &[AgentQualityNodeItem]) -> AgentQualityNodesSummary {
    let total_nodes = nodes.len();
    let ok_nodes = nodes.iter().filter(|item| item.status == "OK").count();
    let unknown_nodes = nodes.iter().filter(|item| item.status == "UNKNOWN").count();
    let degraded_nodes = nodes
        .iter()
        .filter(|item| item.status != "OK" && item.status != "UNKNOWN")
        .count();
    let accepted_nodes = nodes.iter().filter(|item| item.kpi_accepted).count();
    let accepted_kpi_nodes_pct = if total_nodes == 0 {
        0
    } else {
        ((accepted_nodes * 100) / total_nodes) as u8
    };
    AgentQualityNodesSummary {
        total_nodes,
        ok_nodes,
        degraded_nodes,
        unknown_nodes,
        accepted_kpi_nodes_pct,
    }
}

fn build_agent_coverage_sla(
    expected_nodes_path: &Path,
    nodes: &[AgentQualityNodeItem],
    now_utc: chrono::DateTime<Utc>,
) -> AgentCoverageSla {
    let expected_nodes = load_expected_nodes(expected_nodes_path);
    agent_coverage_sla_from_expected(&expected_nodes, nodes, now_utc)
}

fn load_expected_nodes(path: &Path) -> Vec<ExpectedNode> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    if let Ok(nodes) = serde_json::from_str::<Vec<ExpectedNode>>(&text) {
        return sanitize_expected_nodes(nodes);
    }
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return Vec::new();
    };
    let nodes = value
        .get("nodes")
        .cloned()
        .and_then(|nodes| serde_json::from_value::<Vec<ExpectedNode>>(nodes).ok())
        .unwrap_or_default();
    sanitize_expected_nodes(nodes)
}

fn sanitize_expected_nodes(nodes: Vec<ExpectedNode>) -> Vec<ExpectedNode> {
    let mut by_hostname = BTreeMap::new();
    for node in nodes {
        let hostname = node.hostname.trim();
        if hostname.is_empty() {
            continue;
        }
        by_hostname.insert(
            hostname.to_string(),
            ExpectedNode {
                hostname: hostname.to_string(),
                department: node.department.trim().to_string(),
                owner: node.owner.trim().to_string(),
                criticality: node.criticality.trim().to_string(),
            },
        );
    }
    by_hostname.into_values().collect()
}

fn agent_coverage_sla_from_expected(
    expected_nodes: &[ExpectedNode],
    nodes: &[AgentQualityNodeItem],
    now_utc: chrono::DateTime<Utc>,
) -> AgentCoverageSla {
    let expected_count = expected_nodes.len();
    if expected_count == 0 {
        return AgentCoverageSla::default();
    }
    let by_hostname = nodes
        .iter()
        .map(|node| (node.hostname.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let mut reporting_nodes_24h = 0usize;
    let mut fresh_nodes = 0usize;
    let mut stale_nodes = 0usize;
    let mut missing_nodes = 0usize;
    let mut problem_nodes = Vec::new();
    for expected in expected_nodes {
        let Some(node) = by_hostname.get(expected.hostname.as_str()) else {
            missing_nodes += 1;
            problem_nodes.push(agent_coverage_problem_node(
                expected,
                "-",
                "MISSING",
                "Проверить установку и запуск Rust agent на рабочем месте.",
            ));
            continue;
        };
        let fresh = agent_node_seen_within_24h(&node.last_seen_utc, now_utc);
        if fresh {
            fresh_nodes += 1;
        } else {
            stale_nodes += 1;
            problem_nodes.push(agent_coverage_problem_node(
                expected,
                &node.last_seen_utc,
                "STALE",
                "Проверить связь агента с сервером и очередь отправки telemetry.",
            ));
            continue;
        }
        if node.kpi_accepted {
            reporting_nodes_24h += 1;
        } else {
            problem_nodes.push(agent_coverage_problem_node(
                expected,
                &node.last_seen_utc,
                "DEGRADED",
                "Телеметрия свежая, но источник не подтверждает KPI. Вернуть основной сбор WTS API.",
            ));
        }
    }
    problem_nodes.sort_by(|left, right| {
        coverage_problem_rank(&left.status)
            .cmp(&coverage_problem_rank(&right.status))
            .then_with(|| left.hostname.cmp(&right.hostname))
    });
    let coverage_pct = ((reporting_nodes_24h * 100) / expected_count) as u8;
    let freshness_pct = ((fresh_nodes * 100) / expected_count) as u8;
    AgentCoverageSla {
        expected_nodes: expected_count,
        reporting_nodes_24h,
        stale_nodes,
        missing_nodes,
        coverage_pct,
        freshness_pct,
        sla_status: agent_coverage_sla_status(coverage_pct).to_string(),
        problem_nodes,
    }
}

fn agent_node_seen_within_24h(last_seen_utc: &str, now_utc: chrono::DateTime<Utc>) -> bool {
    let Ok(last_seen) = chrono::DateTime::parse_from_rfc3339(last_seen_utc) else {
        return false;
    };
    let last_seen = last_seen.with_timezone(&Utc);
    last_seen <= now_utc && now_utc - last_seen <= chrono::Duration::hours(24)
}

fn agent_coverage_problem_node(
    expected: &ExpectedNode,
    last_seen_utc: &str,
    status: &str,
    recommendation: &str,
) -> AgentCoverageProblemNode {
    AgentCoverageProblemNode {
        hostname: expected.hostname.clone(),
        department: expected.department.clone(),
        owner: expected.owner.clone(),
        last_seen_utc: last_seen_utc.to_string(),
        status: status.to_string(),
        recommendation: recommendation.to_string(),
    }
}

fn coverage_problem_rank(status: &str) -> u8 {
    match status {
        "MISSING" => 0,
        "STALE" => 1,
        "DEGRADED" => 2,
        _ => 3,
    }
}

fn agent_coverage_sla_status(coverage_pct: u8) -> &'static str {
    if coverage_pct >= 90 {
        "OK"
    } else if coverage_pct >= 75 {
        "WARNING"
    } else {
        "CRITICAL"
    }
}

fn agent_quality_from_record(record: &Value) -> AgentQuality {
    let Some(diagnostics) = record.get("diagnostics").and_then(Value::as_object) else {
        return AgentQuality::default();
    };
    let collector_source = diagnostics
        .get("collector_source")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("unknown")
        .to_string();
    let collector_error = diagnostics
        .get("collector_error")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string);
    AgentQuality {
        sessions_collected_total: diagnostics
            .get("sessions_collected_total")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize,
        active_sessions_total: diagnostics
            .get("active_sessions_total")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize,
        rdp_sessions_total: diagnostics
            .get("rdp_sessions_total")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize,
        quality_status: agent_quality_status(&collector_source, collector_error.as_deref())
            .to_string(),
        collector_source,
        collector_error,
    }
}

fn agent_quality_status(collector_source: &str, collector_error: Option<&str>) -> &'static str {
    if let Some(error) = collector_error.filter(|value| !value.trim().is_empty()) {
        return if critical_collector_error(error) {
            "error"
        } else {
            "degraded"
        };
    }
    match collector_source {
        "wts_api" => "ok",
        "quser_utf16" | "quser_lossy" | "env_sessionname_fallback" => "fallback",
        "local_fallback" => "degraded",
        _ => "unknown",
    }
}

fn critical_collector_error(error: &str) -> bool {
    let lower = error.to_lowercase();
    [
        "access denied",
        "permission",
        "unauthorized",
        "invalid",
        "panic",
        "cannot parse",
        "failed to parse",
        "missing required",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn agent_quality_explain(quality: &AgentQuality) -> AgentQualityExplain {
    let source = quality.collector_source.as_str();
    let has_error = quality.collector_error.is_some();
    if source == "wts_api" && !has_error {
        return AgentQualityExplain {
            status: "OK".to_string(),
            title: "Данные агента подтверждают KPI".to_string(),
            summary: "Сессии собраны основным способом через Windows WTS API; индекс активности можно использовать как рабочий управленческий KPI.".to_string(),
            recommendation: "Использовать отчет как подтвержденный оперативный срез. Для расследований сверять с первичными событиями ActivityWatch.".to_string(),
            kpi_accepted: true,
        };
    }
    if source == "local_fallback" {
        return AgentQualityExplain {
            status: "DEGRADED".to_string(),
            title: "Диагностический режим агента".to_string(),
            summary: "Диагностический режим, данные не засчитываются в KPI.".to_string(),
            recommendation: "Проверить доступность WTS API, права запуска агента и состояние Rust Scheduled Task. Не использовать этот срез как доказательство активности.".to_string(),
            kpi_accepted: false,
        };
    }
    if let Some(error) = &quality.collector_error {
        return AgentQualityExplain {
            status: "DEGRADED".to_string(),
            title: "Достоверность данных снижена".to_string(),
            summary: format!("Коллектор передал ошибку: {error}"),
            recommendation: "Проверить журнал агента, источник сбора сессий и восстановить основной путь WTS API перед использованием отчета как доказательной базы.".to_string(),
            kpi_accepted: false,
        };
    }
    match source {
        "quser_utf16" | "quser_lossy" | "env_sessionname_fallback" => AgentQualityExplain {
            status: "WARNING".to_string(),
            title: "Данные собраны резервным способом".to_string(),
            summary: "Активность собрана не основным WTS API. KPI можно использовать как оперативный ориентир, но доказательная точность ниже.".to_string(),
            recommendation: "Проверить, почему WTS API недоступен, и вернуть агент на основной источник сбора.".to_string(),
            kpi_accepted: true,
        },
        _ => AgentQualityExplain {
            status: "UNKNOWN".to_string(),
            title: "Достоверность данных неизвестна".to_string(),
            summary: "Агент не передал диагностику качества данных.".to_string(),
            recommendation: "Обновить Rust agent до версии с diagnostics и проверить поступление telemetry.jsonl.".to_string(),
            kpi_accepted: false,
        },
    }
}

fn command_json_source(name: &str, command: &str, timeout: Duration) -> SourceStatus {
    match run_shell(command, timeout) {
        Ok((stdout, stderr, success)) => {
            if !success {
                return SourceStatus {
                    ok: false,
                    status: "FAIL".to_string(),
                    summary: format!("{name} command returned non-zero status"),
                    error: Some(stderr.trim().to_string()),
                    payload: None,
                };
            }
            match serde_json::from_str::<Value>(&stdout) {
                Ok(payload) => SourceStatus {
                    ok: payload_bool(&payload, "/ok").unwrap_or(true),
                    status: status_from_payload(&payload),
                    summary: source_summary(name, &payload),
                    error: None,
                    payload: Some(payload),
                },
                Err(err) => SourceStatus {
                    ok: false,
                    status: "FAIL".to_string(),
                    summary: format!("{name} returned invalid JSON"),
                    error: Some(err.to_string()),
                    payload: None,
                },
            }
        }
        Err(err) => SourceStatus {
            ok: false,
            status: "FAIL".to_string(),
            summary: format!("{name} command failed"),
            error: Some(err.to_string()),
            payload: None,
        },
    }
}

fn command_text_source(name: &str, command: &str, timeout: Duration) -> SourceStatus {
    match run_shell(command, timeout) {
        Ok((stdout, stderr, success)) => {
            let text = stdout.trim();
            let no_failed_units =
                text.contains("0 loaded units listed") || text.contains("UNIT LOAD ACTIVE SUB");
            SourceStatus {
                ok: success || no_failed_units,
                status: if success || no_failed_units {
                    "OK"
                } else {
                    "WARN"
                }
                .to_string(),
                summary: if success || no_failed_units {
                    "failed units not reported".to_string()
                } else {
                    "failed units command returned non-zero".to_string()
                },
                error: if stderr.trim().is_empty() {
                    None
                } else {
                    Some(stderr.trim().to_string())
                },
                payload: Some(json!({ "stdout": text })),
            }
        }
        Err(err) => SourceStatus {
            ok: false,
            status: "FAIL".to_string(),
            summary: format!("{name} command failed"),
            error: Some(err.to_string()),
            payload: None,
        },
    }
}

fn http_json_source(name: &str, url: &str, timeout: Duration) -> SourceStatus {
    let client = match Client::builder().timeout(timeout).no_proxy().build() {
        Ok(client) => client,
        Err(err) => {
            return SourceStatus {
                ok: false,
                status: "FAIL".to_string(),
                summary: format!("{name} HTTP client failed"),
                error: Some(err.to_string()),
                payload: None,
            };
        }
    };
    match client
        .get(url)
        .header(CONNECTION, HeaderValue::from_static("close"))
        .send()
    {
        Ok(response) => match response.error_for_status() {
            Ok(response) => match response.json::<Value>() {
                Ok(payload) => SourceStatus {
                    ok: true,
                    status: status_from_payload(&payload),
                    summary: source_summary(name, &payload),
                    error: None,
                    payload: Some(payload),
                },
                Err(err) => SourceStatus {
                    ok: false,
                    status: "FAIL".to_string(),
                    summary: format!("{name} returned invalid JSON"),
                    error: Some(err.to_string()),
                    payload: None,
                },
            },
            Err(err) => SourceStatus {
                ok: false,
                status: "FAIL".to_string(),
                summary: format!("{name} HTTP status failed"),
                error: Some(err.to_string()),
                payload: None,
            },
        },
        Err(err) => SourceStatus {
            ok: false,
            status: "FAIL".to_string(),
            summary: format!("{name} request failed"),
            error: Some(err.to_string()),
            payload: None,
        },
    }
}

fn run_shell(command: &str, timeout: Duration) -> Result<(String, String, bool)> {
    let mut child = Command::new("/bin/sh")
        .arg("-lc")
        .arg(command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("spawn {command}"))?;
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            let mut stdout = String::new();
            let mut stderr = String::new();
            if let Some(mut pipe) = child.stdout.take() {
                pipe.read_to_string(&mut stdout)?;
            }
            if let Some(mut pipe) = child.stderr.take() {
                pipe.read_to_string(&mut stderr)?;
            }
            return Ok((stdout, stderr, status.success()));
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(anyhow!(
                "command timed out after {}s: {command}",
                timeout.as_secs()
            ));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn build_health(snapshot: &Snapshot) -> HealthResponse {
    let mut sources = BTreeMap::new();
    sources.insert("detmir_status".to_string(), snapshot.detmir_status.ok);
    sources.insert("detmir_check".to_string(), snapshot.detmir_check.ok);
    sources.insert("grafana_check".to_string(), grafana_data_ok(snapshot));
    sources.insert("worktime_api".to_string(), snapshot.worktime.ok);
    sources.insert(
        "worktime_management".to_string(),
        snapshot.worktime_management.ok,
    );
    sources.insert("dlp_health".to_string(), dlp_ok(snapshot));
    sources.insert("one_c".to_string(), snapshot.one_c.ok);
    HealthResponse {
        ok: sources.values().all(|value| *value),
        generated_at_utc: snapshot.generated_at_utc.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        sources,
    }
}

fn build_summary(snapshot: &Snapshot) -> SummaryResponse {
    let severity = snapshot
        .detmir_status
        .payload
        .as_ref()
        .and_then(|value| value.get("severity"))
        .and_then(Value::as_str)
        .unwrap_or(if snapshot.detmir_status.ok {
            "OK"
        } else {
            "FAIL"
        })
        .to_string();
    let operator_ok = snapshot
        .detmir_status
        .payload
        .as_ref()
        .and_then(|value| value.get("ok_for_operator"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut blocks = BTreeMap::new();
    blocks.insert(
        "collection".to_string(),
        collection_block(snapshot.detmir_check.payload.as_ref()),
    );
    blocks.insert("grafana".to_string(), grafana_block(snapshot));
    blocks.insert("dlp".to_string(), dlp_block(snapshot));
    blocks.insert("worktime".to_string(), worktime_block(snapshot));
    blocks.insert("one_c".to_string(), one_c_block(snapshot));
    SummaryResponse {
        severity: severity.clone(),
        operator_ok,
        headline: if operator_ok && severity == "OK" {
            "Контур работает штатно".to_string()
        } else {
            "Контур требует внимания".to_string()
        },
        generated_at_utc: snapshot.generated_at_utc.clone(),
        blocks,
    }
}

fn build_operator(snapshot: &Snapshot, incident_state: &IncidentStateFile) -> Value {
    json!({
        "generated_at_utc": snapshot.generated_at_utc,
        "summary": build_summary(snapshot),
        "detmir_status": snapshot.detmir_status,
        "detmir_check": snapshot.detmir_check,
        "failed_units": snapshot.failed_units,
        "grafana_data": grafana_service(snapshot),
        "worktime_management": snapshot.worktime_management,
        "links": links(),
        "incidents": build_incidents(snapshot, incident_state),
    })
}

fn build_manager(snapshot: &Snapshot) -> Value {
    let worktime = snapshot
        .worktime
        .payload
        .clone()
        .unwrap_or_else(|| json!({}));
    let rows = worktime
        .get("rows")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let apps = worktime
        .get("true_active_apps")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let total_active_seconds: i64 = rows
        .iter()
        .filter_map(|row| row.get("active_seconds").and_then(Value::as_i64))
        .sum();
    json!({
        "generated_at_utc": snapshot.generated_at_utc,
        "status": worktime_block(snapshot),
        "report_date": worktime.get("report_date"),
        "users_count": rows.len(),
        "total_active_seconds": total_active_seconds,
        "total_active_hours": (total_active_seconds as f64 / 3600.0),
        "users": rows,
        "applications": apps,
        "links": links(),
        "source": snapshot.worktime,
    })
}

fn build_owner(snapshot: &Snapshot) -> Value {
    let summary = build_summary(snapshot);
    let recommendations = owner_recommendations(snapshot, &summary);
    json!({
        "generated_at_utc": snapshot.generated_at_utc,
        "summary": summary,
        "cards": {
            "work": worktime_block(snapshot),
            "security": dlp_block(snapshot),
            "one_c": one_c_block(snapshot),
            "collection": collection_block(snapshot.detmir_check.payload.as_ref()),
            "grafana": grafana_block(snapshot)
        },
        "recommendations": recommendations,
        "links": links(),
    })
}

fn build_reports(
    snapshot: &Snapshot,
    incident_state: &IncidentStateFile,
    evidence: &DlpEvidenceResponse,
    workforce_policy_path: &Path,
    ueba_policy_path: &Path,
    ueba_baseline_path: &Path,
    anonymize: bool,
) -> Value {
    let summary = build_summary(snapshot);
    let incidents = build_incidents(snapshot, incident_state);
    let (users_count, active_seconds, apps_count) = worktime_totals(snapshot);
    let dlp = dlp_counts(snapshot);
    let metrics = ReportMetrics {
        users_count,
        active_seconds,
        apps_count,
        dlp_ok: dlp.0,
        dlp_warn: dlp.1,
        dlp_fail: dlp.2,
        evidence_total: evidence.items.len(),
        evidence_screenshots: evidence
            .items
            .iter()
            .filter(|item| item.screenshot_available)
            .count(),
        open_incidents: incidents.iter().filter(|item| !item.acknowledged).count(),
        acknowledged_incidents: incidents.iter().filter(|item| item.acknowledged).count(),
        workforce_index: workforce_index(users_count, active_seconds),
    };
    let grafana = grafana_block(snapshot);
    let collection = collection_block(snapshot.detmir_check.payload.as_ref());
    let agent_quality = snapshot.agent_quality.clone();
    let agent_quality_explain = agent_quality_explain(&agent_quality);
    let agent_quality_history = snapshot.agent_quality_history.clone();
    let agent_quality_history_summary = snapshot.agent_quality_history_summary.clone();
    let agent_quality_nodes = snapshot.agent_quality_nodes.clone();
    let agent_quality_nodes_summary = snapshot.agent_quality_nodes_summary.clone();
    let agent_coverage_sla = snapshot.agent_coverage_sla.clone();
    let worktime = worktime_block(snapshot);
    let one_c = one_c_block(snapshot);
    let dlp_block_value = dlp_block(snapshot);
    let department_items = workforce_rollup_items(snapshot, "department_rollups");
    let owner_items = workforce_rollup_items(snapshot, "owner_rollups");
    let trend = workforce_trend_json(snapshot);
    let insight_items = workforce_insight_items(snapshot);
    let workforce_policy_explain =
        build_workforce_policy_explain(snapshot, workforce_policy_path, anonymize);
    let ueba_baseline = build_ueba_baseline_analysis(snapshot, ueba_baseline_path, anonymize);
    let ueba_risk = build_ueba_risk(
        snapshot,
        &metrics,
        &workforce_policy_explain,
        &insight_items,
        &ueba_baseline,
        ueba_policy_path,
    );
    let headline = if summary.operator_ok && summary.severity == "OK" && metrics.open_incidents == 0
    {
        "Контур DetMir работает штатно, критичных действий не требуется"
    } else if metrics.open_incidents > 0 {
        "Контур DetMir работает, есть открытые вопросы для оператора"
    } else {
        "Контур DetMir требует технической проверки"
    };
    let mut executive_points = vec![
        format!("Сбор данных: {}. {}", collection.status, collection.text),
        format!(
            "Достоверность данных агента: {}. {}",
            agent_quality_explain.status, agent_quality_explain.summary
        ),
        format!(
            "Работа сегодня: сотрудников={}, активное время={}",
            metrics.users_count,
            human_duration(metrics.active_seconds)
        ),
        format!(
            "DLP/ИБ: ok={}, warn={}, fail={}, evidence={}, screenshots={}",
            metrics.dlp_ok,
            metrics.dlp_warn,
            metrics.dlp_fail,
            metrics.evidence_total,
            metrics.evidence_screenshots
        ),
        format!(
            "Открытые вопросы: {}, в работе: {}",
            metrics.open_incidents, metrics.acknowledged_incidents
        ),
    ];
    if agent_quality_history_summary.ok_days < 5 {
        executive_points.push("KPI требует валидации: нестабильный сбор данных агента".to_string());
    }
    if agent_quality_nodes_summary.total_nodes > 0
        && agent_quality_nodes_summary.accepted_kpi_nodes_pct < 80
    {
        executive_points
            .push("KPI требует проверки: менее 80% узлов дают подтвержденные данные".to_string());
    }
    match agent_coverage_sla.sla_status.as_str() {
        "CRITICAL" => executive_points.push(
            "Покрытие агентов критически недостаточно: KPI не может считаться репрезентативным"
                .to_string(),
        ),
        "WARNING" => executive_points.push(
            "KPI требует проверки: часть рабочих мест не присылает свежую телеметрию".to_string(),
        ),
        _ => {}
    }
    let recommendations = owner_recommendations(snapshot, &summary);
    let workforce_summary = ReportWorkforceSummary {
        departments_count: department_items.len(),
        owners_count: owner_items.len(),
        insights_count: insight_items.len(),
        trend_status: trend_status(&trend),
    };
    let markdown = render_report_markdown(
        snapshot,
        headline,
        &summary,
        &metrics,
        &recommendations,
        &workforce_summary,
        (&workforce_policy_explain, &ueba_risk),
    );
    json!({
        "generated_at_utc": snapshot.generated_at_utc,
        "period": "оперативный срез за сегодня и текущий runtime",
        "anonymized": anonymize,
        "severity": summary.severity,
        "operator_ok": summary.operator_ok,
        "headline": headline,
        "executive_points": executive_points,
        "kpis": [
            report_kpi("UEBA риск", format!("{}/100", ueba_risk.get("score").and_then(Value::as_u64).unwrap_or(0)), ueba_risk.get("status").and_then(Value::as_str).unwrap_or("UNKNOWN").to_string(), ueba_risk.get("summary").and_then(Value::as_str).unwrap_or("risk score")),
            report_kpi("Качество данных агента", agent_quality.quality_status.clone(), agent_quality.quality_status.clone(), &format!("источник: {}", agent_quality.collector_source)),
            report_kpi("Достоверность данных", agent_quality_explain.status.clone(), agent_quality_explain.status.clone(), &agent_quality_explain.title),
            report_kpi("Индекс активности", workforce_index_text(metrics.workforce_index), workforce_index_status(metrics.workforce_index), "proxy: активное время / плановое рабочее время"),
            weighted_activity_kpi_from_policy(&workforce_policy_explain),
            report_kpi("Сотрудники", metrics.users_count.to_string(), worktime.status.clone(), "строки worktime за сегодня"),
            report_kpi("Активное время", human_duration(metrics.active_seconds), worktime.status.clone(), "сумма active_seconds"),
            report_kpi("Приложения", metrics.apps_count.to_string(), worktime.status.clone(), "true active applications"),
            report_kpi("Подразделения", department_items.len().to_string(), snapshot.worktime_management.status.clone(), "сравнение групп за текущий день"),
            report_kpi("DLP WARN/FAIL", format!("{}/{}", metrics.dlp_warn, metrics.dlp_fail), dlp_block_value.status.clone(), "технические сигналы DLP"),
            report_kpi("Evidence", format!("{}/{}", metrics.evidence_screenshots, metrics.evidence_total), evidence_status(evidence), "скриншоты / все evidence items"),
            report_kpi("Открытые вопросы", metrics.open_incidents.to_string(), incident_status(metrics.open_incidents), "не взятые в работу items")
        ],
        "sections": [
            {
                "title": "Надежность контура",
                "items": [
                    report_item("DetMir status", snapshot.detmir_status.status.clone(), snapshot.detmir_status.summary.clone()),
                    report_item("Сбор данных", collection.status.clone(), collection.text.clone()),
                    report_item("Качество данных агента", agent_quality.quality_status.clone(), format!("source={}, sessions={}, active={}, rdp={}", agent_quality.collector_source, agent_quality.sessions_collected_total, agent_quality.active_sessions_total, agent_quality.rdp_sessions_total)),
                    report_item("Достоверность данных", agent_quality_explain.status.clone(), format!("KPI accepted={}, {}", agent_quality_explain.kpi_accepted, agent_quality_explain.recommendation)),
                    report_item("Grafana", grafana.status.clone(), grafana.text.clone()),
                    report_item("1C analytics", one_c.status.clone(), one_c.text.clone())
                ]
            },
            {
                "title": "Работа и управляемость",
                "items": [
                    report_item("Индекс активности", workforce_index_status(metrics.workforce_index), workforce_index_text(metrics.workforce_index)),
                    weighted_activity_item_from_policy(&workforce_policy_explain, workforce_policy_path),
                    report_item("Worktime", worktime.status.clone(), worktime.text.clone()),
                    report_item("Management report", snapshot.worktime_management.status.clone(), snapshot.worktime_management.summary.clone()),
                    report_item("Активное время", worktime.status.clone(), human_duration(metrics.active_seconds)),
                    report_item("Приложения", worktime.status.clone(), metrics.apps_count.to_string()),
                    report_item("Отчет", "OK", "готов к передаче руководителю")
                ]
            },
            {
                "title": "Выводы Workforce",
                "items": insight_items.clone()
            },
            {
                "title": "UEBA риск",
                "items": ueba_risk.get("reasons").and_then(Value::as_array).cloned().unwrap_or_default()
            },
            {
                "title": "Подразделения сегодня",
                "items": department_items.clone()
            },
            {
                "title": "Ответственные сегодня",
                "items": owner_items.clone()
            },
            {
                "title": "ИБ и evidence",
                "items": [
                    report_item("DLP", dlp_block_value.status.clone(), dlp_block_value.text.clone()),
                    report_item("Evidence metadata", evidence_status(evidence), format!("items={}", metrics.evidence_total)),
                    report_item("Скриншоты", evidence_status(evidence), format!("available={}", metrics.evidence_screenshots)),
                    report_item("Формулировка", "OK", "derived detections/cases, не сертифицированная СЗИ")
                ]
            },
            {
                "title": "Действия",
                "items": recommendations.iter().map(|item| report_item("Рекомендация", "INFO", item)).collect::<Vec<_>>()
            }
        ],
        "ueba_risk": ueba_risk,
        "ueba_baseline": ueba_baseline,
        "agent_quality": agent_quality,
        "agent_quality_explain": agent_quality_explain,
        "agent_quality_history": agent_quality_history,
        "agent_quality_history_summary": agent_quality_history_summary,
        "agent_quality_nodes": agent_quality_nodes,
        "agent_quality_nodes_summary": agent_quality_nodes_summary,
        "agent_coverage_sla": agent_coverage_sla,
        "workforce_policy": workforce_policy_explain,
        "workforce": {
            "department_comparison": department_items,
            "owner_comparison": owner_items,
            "trend": trend,
            "insights": insight_items,
            "trend_status": trend_status(&trend),
            "history_note": "Месячный тренд требует накопленной daily history; текущий слой показывает validated daily management snapshot."
        },
        "markdown": markdown,
        "links": links()
    })
}

fn workforce_rollup_items(snapshot: &Snapshot, key: &str) -> Vec<Value> {
    snapshot
        .worktime_management
        .payload
        .as_ref()
        .and_then(|payload| payload.get(key))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| item.get("users_count").and_then(Value::as_i64).unwrap_or(0) > 0)
                .map(|item| {
                    let name = item
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("Без группы");
                    let coverage = item
                        .get("portfolio_coverage_pct")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0);
                    let users = item.get("users_count").and_then(Value::as_i64).unwrap_or(0);
                    let active = item
                        .get("active_users")
                        .and_then(Value::as_i64)
                        .unwrap_or(0);
                    let hhmm = item
                        .get("workday_total_active_hhmm")
                        .and_then(Value::as_str)
                        .unwrap_or("00:00");
                    report_item(
                        name,
                        coverage_status(coverage),
                        format!("{coverage:.0}% · active {active}/{users} · {hhmm}"),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn workforce_trend_json(snapshot: &Snapshot) -> Value {
    snapshot
        .worktime_management
        .payload
        .as_ref()
        .and_then(|payload| payload.get("trend"))
        .cloned()
        .unwrap_or_else(|| json!([]))
}

fn workforce_insight_items(snapshot: &Snapshot) -> Vec<Value> {
    snapshot
        .worktime_management
        .payload
        .as_ref()
        .and_then(|payload| payload.get("trend_insights"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    let title = item
                        .get("title")
                        .and_then(Value::as_str)
                        .unwrap_or("Вывод Workforce");
                    let subject = item
                        .get("subject")
                        .and_then(Value::as_str)
                        .unwrap_or("Workforce");
                    let severity = item
                        .get("severity")
                        .and_then(Value::as_str)
                        .unwrap_or("INFO");
                    let evidence = item.get("evidence").and_then(Value::as_str).unwrap_or("");
                    let recommendation = item
                        .get("recommendation")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    report_item(
                        &format!("{title}: {subject}"),
                        severity,
                        format!("{evidence} {recommendation}").trim().to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_else(|| {
            vec![report_item(
                "История еще накапливается",
                "INFO",
                "Worktime API пока не вернул trend_insights.",
            )]
        })
}

fn trend_status(trend: &Value) -> String {
    let points = trend.as_array().map(Vec::len).unwrap_or(0);
    if points >= 20 {
        "monthly_ready".to_string()
    } else if points >= 7 {
        "weekly_ready".to_string()
    } else {
        "daily_only".to_string()
    }
}

fn default_ueba_baseline_state_version() -> String {
    "ueba-baseline-v1".to_string()
}

fn default_ueba_baseline_window_days() -> i64 {
    30
}

fn ueba_baseline_state_path(args: &Cli) -> PathBuf {
    args.state_dir.join("ueba-baseline-state.json")
}

fn load_ueba_baseline_state(path: &Path) -> (UebaBaselineState, Option<String>) {
    if !path.exists() {
        return (UebaBaselineState::default(), None);
    }
    match fs::read_to_string(path)
        .with_context(|| format!("read {}", path.display()))
        .and_then(|data| {
            serde_json::from_str::<UebaBaselineState>(&data)
                .with_context(|| format!("parse {}", path.display()))
        }) {
        Ok(mut state) => {
            if state.version.is_empty() {
                state.version = default_ueba_baseline_state_version();
            }
            if state.baseline_window_days <= 0 {
                state.baseline_window_days = default_ueba_baseline_window_days();
            }
            (state, None)
        }
        Err(err) => (UebaBaselineState::default(), Some(err.to_string())),
    }
}

fn save_ueba_baseline_state(path: &Path, state: &UebaBaselineState) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(state)?)
        .with_context(|| format!("write {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("rename {}", path.display()))?;
    Ok(())
}

fn build_ueba_baseline_analysis(snapshot: &Snapshot, path: &Path, anonymize: bool) -> Value {
    let (mut state, load_error) = load_ueba_baseline_state(path);
    if state.version.is_empty() {
        state.version = default_ueba_baseline_state_version();
    }
    if state.baseline_window_days <= 0 {
        state.baseline_window_days = default_ueba_baseline_window_days();
    }
    let report_date = baseline_report_date(snapshot);
    prune_ueba_baseline_state(&mut state, &report_date);
    let user_points = current_user_baseline_points(snapshot);
    let department_points = current_department_baseline_points(snapshot);
    let user_deviations = baseline_deviations_for_users(&state, &user_points);
    let department_deviations = baseline_deviations_for_departments(&state, &department_points);
    let user_samples = state
        .users
        .values()
        .map(|item| item.samples.len())
        .sum::<usize>();
    let department_samples = state
        .departments
        .values()
        .map(|item| item.samples.len())
        .sum::<usize>();
    let user_baseline_available = user_deviations
        .iter()
        .any(|item| item.samples >= UEBA_BASELINE_MIN_SAMPLES);
    let department_baseline_available = department_deviations
        .iter()
        .any(|item| item.samples >= UEBA_BASELINE_MIN_SAMPLES);
    let deviation_score =
        baseline_deviation_score(&user_deviations, &department_deviations).min(25);
    let strongest_deviations =
        strongest_baseline_deviations(&user_deviations, &department_deviations, anonymize);

    update_ueba_baseline_state(
        &mut state,
        &report_date,
        &snapshot.generated_at_utc,
        &user_points,
        &department_points,
    );
    let save_error = save_ueba_baseline_state(path, &state)
        .err()
        .map(|err| err.to_string());

    json!({
        "version": state.version,
        "baseline_status": default_ueba_baseline_status(),
        "path": path.display().to_string(),
        "report_date": report_date,
        "baseline_window_days": state.baseline_window_days,
        "minimum_samples": UEBA_BASELINE_MIN_SAMPLES,
        "user_baseline_available": user_baseline_available,
        "department_baseline_available": department_baseline_available,
        "deviation_score": deviation_score,
        "baseline_samples": {
            "users": user_samples,
            "departments": department_samples,
            "total": user_samples + department_samples
        },
        "current_entities": {
            "users": user_points.len(),
            "departments": department_points.len()
        },
        "strongest_deviations": strongest_deviations,
        "state_error": load_error.or(save_error),
        "updated": true,
        "anonymized": anonymize
    })
}

fn baseline_report_date(snapshot: &Snapshot) -> String {
    snapshot
        .worktime
        .payload
        .as_ref()
        .and_then(|payload| payload.get("report_date"))
        .and_then(Value::as_str)
        .or_else(|| {
            snapshot
                .worktime_management
                .payload
                .as_ref()
                .and_then(|payload| payload.get("report_date"))
                .and_then(Value::as_str)
        })
        .map(|value| value.chars().take(10).collect::<String>())
        .unwrap_or_else(|| snapshot.generated_at_utc.chars().take(10).collect())
}

fn prune_ueba_baseline_state(state: &mut UebaBaselineState, current_date: &str) {
    let Some(current) = parse_baseline_date(current_date) else {
        return;
    };
    let window = state.baseline_window_days.max(1);
    for baseline in state.users.values_mut() {
        baseline.samples.retain(|sample| {
            parse_baseline_date(&sample.date)
                .map(|date| current.signed_duration_since(date).num_days() < window)
                .unwrap_or(true)
        });
    }
    state
        .users
        .retain(|_, baseline| !baseline.samples.is_empty());
    for baseline in state.departments.values_mut() {
        baseline.samples.retain(|sample| {
            parse_baseline_date(&sample.date)
                .map(|date| current.signed_duration_since(date).num_days() < window)
                .unwrap_or(true)
        });
    }
    state
        .departments
        .retain(|_, baseline| !baseline.samples.is_empty());
}

fn parse_baseline_date(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value.get(0..10).unwrap_or(value), "%Y-%m-%d").ok()
}

fn current_user_baseline_points(snapshot: &Snapshot) -> Vec<CurrentBaselinePoint> {
    let Some(rows) = snapshot
        .worktime
        .payload
        .as_ref()
        .and_then(|payload| payload.get("rows"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    rows.iter()
        .enumerate()
        .map(|(idx, row)| {
            let active_seconds = row
                .get("active_seconds")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .max(0);
            let raw_user = row.get("user").and_then(Value::as_str).unwrap_or("");
            let raw_user_id = row
                .get("user_id")
                .and_then(Value::as_str)
                .unwrap_or(raw_user);
            let key = if raw_user_id.trim().is_empty() {
                format!("row-{}", idx + 1)
            } else {
                raw_user_id.to_string()
            };
            let label = if raw_user.trim().is_empty() {
                key.clone()
            } else {
                raw_user.to_string()
            };
            let index = ((active_seconds as f64 / (8.0 * 3600.0)) * 100.0).clamp(0.0, 200.0);
            CurrentBaselinePoint {
                key,
                label,
                index,
                active_seconds: Some(active_seconds),
                users_count: None,
            }
        })
        .collect()
}

fn current_department_baseline_points(snapshot: &Snapshot) -> Vec<CurrentBaselinePoint> {
    snapshot
        .worktime_management
        .payload
        .as_ref()
        .and_then(|payload| payload.get("department_rollups"))
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let name = row.get("name").and_then(Value::as_str).unwrap_or("");
                    if name.trim().is_empty() {
                        return None;
                    }
                    let index = row
                        .get("portfolio_coverage_pct")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0)
                        .clamp(0.0, 200.0);
                    let active_seconds = hhmm_to_seconds(
                        row.get("workday_total_active_hhmm")
                            .and_then(Value::as_str)
                            .unwrap_or(""),
                    );
                    Some(CurrentBaselinePoint {
                        key: name.to_string(),
                        label: name.to_string(),
                        index,
                        active_seconds,
                        users_count: row.get("users_count").and_then(Value::as_i64),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn hhmm_to_seconds(value: &str) -> Option<i64> {
    let mut parts = value.split(':');
    let hours = parts.next()?.parse::<i64>().ok()?;
    let minutes = parts.next()?.parse::<i64>().ok()?;
    Some((hours * 3600 + minutes * 60).max(0))
}

fn baseline_deviations_for_users(
    state: &UebaBaselineState,
    points: &[CurrentBaselinePoint],
) -> Vec<BaselineDeviation> {
    points
        .iter()
        .filter_map(|point| {
            let baseline = state.users.get(&point.key)?;
            baseline_deviation("user", point, &baseline.samples)
        })
        .collect()
}

fn baseline_deviations_for_departments(
    state: &UebaBaselineState,
    points: &[CurrentBaselinePoint],
) -> Vec<BaselineDeviation> {
    points
        .iter()
        .filter_map(|point| {
            let baseline = state.departments.get(&point.key)?;
            baseline_deviation("department", point, &baseline.samples)
        })
        .collect()
}

fn baseline_deviation(
    scope: &'static str,
    point: &CurrentBaselinePoint,
    samples: &[BaselineSample],
) -> Option<BaselineDeviation> {
    if samples.len() < UEBA_BASELINE_MIN_SAMPLES {
        return None;
    }
    let mean = samples.iter().map(|sample| sample.index).sum::<f64>() / samples.len() as f64;
    let deviation_pct = point.index - mean;
    let status = if deviation_pct.abs() >= 25.0 {
        "WARN"
    } else {
        "INFO"
    };
    Some(BaselineDeviation {
        scope,
        key: point.key.clone(),
        label: point.label.clone(),
        current_index: round1(point.index),
        baseline_index: round1(mean),
        deviation_pct: round1(deviation_pct),
        samples: samples.len(),
        status,
    })
}

fn baseline_deviation_score(
    user_deviations: &[BaselineDeviation],
    department_deviations: &[BaselineDeviation],
) -> u64 {
    user_deviations
        .iter()
        .chain(department_deviations.iter())
        .map(|item| deviation_points(item.deviation_pct))
        .sum::<u64>()
}

fn deviation_points(deviation_pct: f64) -> u64 {
    let value = deviation_pct.abs();
    if value >= 40.0 {
        15
    } else if value >= 25.0 {
        10
    } else if value >= 15.0 {
        5
    } else {
        0
    }
}

fn strongest_baseline_deviations(
    user_deviations: &[BaselineDeviation],
    department_deviations: &[BaselineDeviation],
    anonymize: bool,
) -> Vec<Value> {
    let mut items = user_deviations
        .iter()
        .chain(department_deviations.iter())
        .cloned()
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        right
            .deviation_pct
            .abs()
            .partial_cmp(&left.deviation_pct.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    items
        .into_iter()
        .take(12)
        .enumerate()
        .map(|(idx, item)| {
            let (key, label) = if anonymize && item.scope == "user" {
                (
                    format!("EMPLOYEE-{}", idx + 1),
                    format!("Сотрудник {}", idx + 1),
                )
            } else if anonymize && item.scope == "department" {
                (
                    format!("DEPARTMENT-{}", idx + 1),
                    format!("Подразделение {}", idx + 1),
                )
            } else {
                (item.key.clone(), item.label.clone())
            };
            json!({
                "scope": item.scope,
                "key": key,
                "label": label,
                "current_index": item.current_index,
                "baseline_index": item.baseline_index,
                "deviation_pct": item.deviation_pct,
                "samples": item.samples,
                "status": item.status
            })
        })
        .collect()
}

fn update_ueba_baseline_state(
    state: &mut UebaBaselineState,
    report_date: &str,
    generated_at_utc: &str,
    user_points: &[CurrentBaselinePoint],
    department_points: &[CurrentBaselinePoint],
) {
    state.updated_at_utc = Some(generated_at_utc.to_string());
    for point in user_points {
        let entry = state
            .users
            .entry(point.key.clone())
            .or_insert_with(|| UserBaseline {
                user_id: point.key.clone(),
                user: point.label.clone(),
                samples: Vec::new(),
            });
        entry.user = point.label.clone();
        upsert_baseline_sample(
            &mut entry.samples,
            BaselineSample {
                date: report_date.to_string(),
                index: round1(point.index),
                active_seconds: point.active_seconds,
                users_count: None,
            },
        );
    }
    for point in department_points {
        let entry = state
            .departments
            .entry(point.key.clone())
            .or_insert_with(|| DepartmentBaseline {
                name: point.label.clone(),
                samples: Vec::new(),
            });
        entry.name = point.label.clone();
        upsert_baseline_sample(
            &mut entry.samples,
            BaselineSample {
                date: report_date.to_string(),
                index: round1(point.index),
                active_seconds: point.active_seconds,
                users_count: point.users_count,
            },
        );
    }
}

fn upsert_baseline_sample(samples: &mut Vec<BaselineSample>, sample: BaselineSample) {
    if let Some(existing) = samples.iter_mut().find(|item| item.date == sample.date) {
        *existing = sample;
    } else {
        samples.push(sample);
    }
    samples.sort_by(|left, right| left.date.cmp(&right.date));
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn default_ueba_policy_version() -> String {
    "ueba-rule-v1".to_string()
}

fn default_ueba_baseline_status() -> String {
    "per_user_department_baseline_skeleton".to_string()
}

fn default_ueba_score_cap() -> u64 {
    100
}

fn default_ueba_risk_policy() -> UebaRiskPolicy {
    UebaRiskPolicy {
        version: default_ueba_policy_version(),
        baseline_status: default_ueba_baseline_status(),
        score_cap: default_ueba_score_cap(),
        weights: BTreeMap::from([
            ("dlp_fail".to_string(), 35),
            ("dlp_warn".to_string(), 20),
            ("open_incidents".to_string(), 15),
            ("night_activity".to_string(), 20),
            ("weekend_activity".to_string(), 20),
            ("workforce_drop".to_string(), 15),
            ("workforce_anomaly".to_string(), 10),
            ("application_classification_gap".to_string(), 10),
            ("application_classification_gap_large".to_string(), 15),
            ("worktime_unavailable".to_string(), 25),
            ("baseline_deviation".to_string(), 15),
        ]),
        confidence: UebaConfidencePolicy {
            base: Some(0.55),
            evidence_bonus: Some(0.10),
            screenshot_bonus: Some(0.10),
            worktime_bonus: Some(0.15),
            policy_bonus: Some(0.10),
        },
    }
}

fn load_ueba_risk_policy(path: &Path) -> (UebaRiskPolicy, bool, Option<String>) {
    if !path.exists() {
        return (default_ueba_risk_policy(), false, None);
    }
    match fs::read_to_string(path)
        .with_context(|| format!("read {}", path.display()))
        .and_then(|data| {
            serde_yaml::from_str::<UebaRiskPolicy>(&data)
                .with_context(|| format!("parse {}", path.display()))
        }) {
        Ok(mut policy) => {
            let defaults = default_ueba_risk_policy();
            for (key, value) in defaults.weights {
                policy.weights.entry(key).or_insert(value);
            }
            if policy.score_cap == 0 {
                policy.score_cap = default_ueba_score_cap();
            }
            (policy, true, None)
        }
        Err(err) => (default_ueba_risk_policy(), false, Some(err.to_string())),
    }
}

fn risk_weight(policy: &UebaRiskPolicy, key: &str, fallback: u64) -> u64 {
    policy.weights.get(key).copied().unwrap_or(fallback)
}

fn confidence_part(value: Option<f64>, fallback: f64) -> f64 {
    value.unwrap_or(fallback).clamp(0.0, 1.0)
}

fn ueba_confidence(
    metrics: &ReportMetrics,
    workforce_policy: &Value,
    snapshot: &Snapshot,
    policy: &UebaRiskPolicy,
) -> f64 {
    let mut confidence = confidence_part(policy.confidence.base, 0.55);
    if metrics.evidence_total > 0 {
        confidence += confidence_part(policy.confidence.evidence_bonus, 0.10);
    }
    if metrics.evidence_screenshots > 0 {
        confidence += confidence_part(policy.confidence.screenshot_bonus, 0.10);
    }
    if snapshot.worktime.ok && snapshot.worktime_management.ok {
        confidence += confidence_part(policy.confidence.worktime_bonus, 0.15);
    }
    if workforce_policy
        .get("configured")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        confidence += confidence_part(policy.confidence.policy_bonus, 0.10);
    }
    (confidence.clamp(0.0, 1.0) * 100.0).round() / 100.0
}

fn risk_sources(reasons: &[Value]) -> Vec<String> {
    let mut out = Vec::new();
    for reason in reasons {
        let Some(source) = reason.get("source").and_then(Value::as_str) else {
            continue;
        };
        if !out.iter().any(|item| item == source) {
            out.push(source.to_string());
        }
    }
    out
}

fn ueba_calculated_from(
    metrics: &ReportMetrics,
    workforce_policy: &Value,
    insight_items: &[Value],
    ueba_baseline: &Value,
    policy_configured: bool,
    policy_error: Option<&str>,
) -> Vec<Value> {
    vec![
        json!({"source": "dlp_counts", "available": true, "warn": metrics.dlp_warn, "fail": metrics.dlp_fail}),
        json!({"source": "incident_queue", "available": true, "open": metrics.open_incidents}),
        json!({"source": "evidence", "available": true, "items": metrics.evidence_total, "screenshots": metrics.evidence_screenshots, "used_as": "confidence"}),
        json!({"source": "workforce_insights", "available": true, "items": insight_items.len()}),
        json!({"source": "workforce_policy_audit", "available": workforce_policy.get("configured").and_then(Value::as_bool).unwrap_or(false)}),
        json!({"source": "ueba_baseline", "available": ueba_baseline.get("user_baseline_available").and_then(Value::as_bool).unwrap_or(false) || ueba_baseline.get("department_baseline_available").and_then(Value::as_bool).unwrap_or(false), "samples": ueba_baseline.get("baseline_samples").cloned().unwrap_or_else(|| json!({}))}),
        json!({"source": "ueba_policy", "available": policy_configured, "error": policy_error}),
    ]
}

fn build_ueba_risk(
    snapshot: &Snapshot,
    metrics: &ReportMetrics,
    workforce_policy: &Value,
    insight_items: &[Value],
    ueba_baseline: &Value,
    policy_path: &Path,
) -> Value {
    let (policy, policy_configured, policy_error) = load_ueba_risk_policy(policy_path);
    let mut reasons = Vec::new();
    let mut score = 0_u64;

    if metrics.dlp_fail > 0 {
        push_risk_reason(
            &mut reasons,
            &mut score,
            ("dlp_fail", "DLP FAIL", "dlp"),
            "FAIL",
            risk_weight(&policy, "dlp_fail", 35),
            format!("fail={}", metrics.dlp_fail),
            "Проверить DLP/case queue и evidence.",
        );
    }
    if metrics.dlp_warn > 0 {
        push_risk_reason(
            &mut reasons,
            &mut score,
            ("dlp_warn", "DLP WARN", "dlp"),
            "WARN",
            risk_weight(&policy, "dlp_warn", 20),
            format!("warn={}", metrics.dlp_warn),
            "Разобрать предупреждения DLP и подтвердить/отклонить события.",
        );
    }
    if metrics.open_incidents > 0 {
        push_risk_reason(
            &mut reasons,
            &mut score,
            ("open_incidents", "Открытые вопросы", "incidents"),
            "WARN",
            risk_weight(&policy, "open_incidents", 15),
            format!("open={}", metrics.open_incidents),
            "Назначить ответственного и закрыть очередь review.",
        );
    }

    for item in insight_items {
        let status = item.get("status").and_then(Value::as_str).unwrap_or("INFO");
        if status == "OK" || status == "INFO" {
            continue;
        }
        let label = item
            .get("label")
            .and_then(Value::as_str)
            .unwrap_or("Workforce");
        let value = item.get("value").and_then(Value::as_str).unwrap_or("");
        let text = format!("{label} {value}").to_lowercase();
        let (code, title, points) = if text.contains("ноч")
            || text.contains("night")
            || text.contains("вне рабочего")
            || text.contains("off-hours")
        {
            ("night_activity", "Активность вне рабочего окна", 20)
        } else if text.contains("выход")
            || text.contains("weekend")
            || text.contains("суббот")
            || text.contains("воскрес")
        {
            ("weekend_activity", "Работа в выходной", 20)
        } else if text.contains("просад")
            || text.contains("паден")
            || text.contains("drop")
            || text.contains("недогруз")
        {
            ("workforce_drop", "Отклонение активности", 15)
        } else {
            ("workforce_anomaly", "Workforce anomaly", 10)
        };
        push_risk_reason(
            &mut reasons,
            &mut score,
            (code, title, "workforce"),
            status,
            risk_weight(&policy, code, points),
            value,
            "Проверить первичные события ActivityWatch и контекст подразделения.",
        );
    }

    let default_weight_apps = workforce_policy
        .get("policy_audit")
        .and_then(|audit| audit.get("default_weight_applications"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let default_weight_seconds = workforce_policy
        .get("policy_audit")
        .and_then(|audit| audit.get("default_weight_seconds"))
        .and_then(Value::as_i64)
        .unwrap_or(0);
    if default_weight_apps > 0 {
        push_risk_reason(
            &mut reasons,
            &mut score,
            (
                "application_classification_gap",
                "Приложения без явного правила",
                "policy",
            ),
            "WARN",
            if default_weight_seconds >= 3600 {
                risk_weight(&policy, "application_classification_gap_large", 15)
            } else {
                risk_weight(&policy, "application_classification_gap", 10)
            },
            format!(
                "default_weight_apps={}, default_weight_time={}",
                default_weight_apps,
                human_duration(default_weight_seconds)
            ),
            "Уточнить role/application policy, чтобы снизить ошибки классификации.",
        );
    }

    if !snapshot.worktime.ok {
        push_risk_reason(
            &mut reasons,
            &mut score,
            ("worktime_unavailable", "Нет надежного Worktime", "worktime"),
            "FAIL",
            risk_weight(&policy, "worktime_unavailable", 25),
            snapshot.worktime.summary.clone(),
            "Восстановить Worktime API/collectors перед выводами по сотрудникам.",
        );
    }
    let deviation_score = ueba_baseline
        .get("deviation_score")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if deviation_score > 0 {
        let deviations = ueba_baseline
            .get("strongest_deviations")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .take(3)
                    .filter_map(|item| {
                        let label = item.get("label").and_then(Value::as_str)?;
                        let delta = item.get("deviation_pct").and_then(Value::as_f64)?;
                        Some(format!("{label}: {delta:+.1}%"))
                    })
                    .collect::<Vec<_>>()
                    .join("; ")
            })
            .unwrap_or_default();
        push_risk_reason(
            &mut reasons,
            &mut score,
            ("baseline_deviation", "Отклонение от baseline", "baseline"),
            "WARN",
            risk_weight(&policy, "baseline_deviation", deviation_score).min(deviation_score),
            if deviations.is_empty() {
                format!("deviation_score={deviation_score}")
            } else {
                deviations
            },
            "Проверить сотрудника/подразделение относительно обычного профиля активности.",
        );
    }

    let calculated_from = ueba_calculated_from(
        metrics,
        workforce_policy,
        insight_items,
        ueba_baseline,
        policy_configured,
        policy_error.as_deref(),
    );
    let confidence = ueba_confidence(metrics, workforce_policy, snapshot, &policy);
    let risk_sources = risk_sources(&reasons);
    let score = score.min(policy.score_cap.max(1));
    let (level, status) = ueba_risk_level(score);
    json!({
        "score": score,
        "level": level,
        "status": status,
        "summary": format!("{} risk, {} reason(s)", level, reasons.len()),
        "formula": format!("sum(reason_points) capped at {}", policy.score_cap.max(1)),
        "confidence": confidence,
        "risk_sources": risk_sources,
        "baseline_status": ueba_baseline
            .get("baseline_status")
            .and_then(Value::as_str)
            .unwrap_or(&policy.baseline_status),
        "baseline_window_days": ueba_baseline.get("baseline_window_days").and_then(Value::as_i64).unwrap_or(default_ueba_baseline_window_days()),
        "user_baseline_available": ueba_baseline.get("user_baseline_available").and_then(Value::as_bool).unwrap_or(false),
        "department_baseline_available": ueba_baseline.get("department_baseline_available").and_then(Value::as_bool).unwrap_or(false),
        "deviation_score": deviation_score,
        "baseline_samples": ueba_baseline.get("baseline_samples").cloned().unwrap_or_else(|| json!({})),
        "policy_version": policy.version,
        "policy_path": policy_path.display().to_string(),
        "policy_configured": policy_configured,
        "policy_error": policy_error,
        "calculated_from": calculated_from,
        "reasons": reasons,
        "note": "UEBA-compatible rule-based risk scoring v1: read-only мониторинг и приоритизация проверки, без автоматического воздействия на сеть."
    })
}

fn push_risk_reason(
    reasons: &mut Vec<Value>,
    score: &mut u64,
    code_title_source: (&str, &str, &str),
    severity: &str,
    points: u64,
    evidence: impl Into<String>,
    recommendation: &str,
) {
    *score = score.saturating_add(points);
    reasons.push(json!({
        "label": code_title_source.1,
        "status": severity,
        "value": evidence.into(),
        "code": code_title_source.0,
        "source": code_title_source.2,
        "severity": severity,
        "points": points,
        "recommendation": recommendation,
    }));
}

fn ueba_risk_level(score: u64) -> (&'static str, &'static str) {
    if score >= 70 {
        ("high", "FAIL")
    } else if score >= 40 {
        ("medium", "WARN")
    } else if score >= 15 {
        ("low", "WARN")
    } else {
        ("normal", "OK")
    }
}

fn coverage_status(coverage: f64) -> &'static str {
    if coverage >= 75.0 {
        "OK"
    } else if coverage >= 35.0 {
        "WARN"
    } else {
        "FAIL"
    }
}

fn load_workforce_policy(path: &Path) -> Result<Option<WorkforcePolicy>> {
    if !path.exists() {
        return Ok(None);
    }
    let data = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let policy = serde_json::from_str::<WorkforcePolicy>(&data)
        .with_context(|| format!("parse {}", path.display()))?;
    Ok(Some(policy))
}

fn build_workforce_policy_explain(
    snapshot: &Snapshot,
    workforce_policy_path: &Path,
    anonymize: bool,
) -> Value {
    let (users_count, _, _) = worktime_totals(snapshot);
    let workforce_policy = load_workforce_policy(workforce_policy_path).ok().flatten();
    let weighted = workforce_policy
        .as_ref()
        .and_then(|policy| weighted_activity(snapshot, policy, users_count, anonymize));
    workforce_policy_json(
        workforce_policy.as_ref(),
        weighted.as_ref(),
        workforce_policy_path,
        anonymize,
    )
}

fn weighted_activity(
    snapshot: &Snapshot,
    policy: &WorkforcePolicy,
    users_count: usize,
    anonymize: bool,
) -> Option<WeightedActivity> {
    if users_count == 0 {
        return None;
    }
    let role = policy.default_role.trim();
    let role_policy = policy.roles.get(role)?;
    let planned_hours = role_policy
        .planned_hours_per_day
        .unwrap_or(8.0)
        .clamp(1.0, 24.0);
    let planned_seconds = (users_count as f64 * planned_hours * 3600.0).round() as i64;
    if planned_seconds <= 0 {
        return None;
    }
    let default_weight = role_policy.default_weight.unwrap_or(0.0).clamp(0.0, 1.0);
    let apps = snapshot
        .worktime
        .payload
        .as_ref()
        .and_then(|payload| payload.get("true_active_apps"))
        .and_then(Value::as_array)?;
    let mut app_seconds = 0_i64;
    let mut weighted_seconds = 0_f64;
    let mut matched_applications = 0_usize;
    let mut app_details = Vec::new();
    for app in apps {
        let name = app.get("application").and_then(Value::as_str).unwrap_or("");
        let seconds = app
            .get("proved_work_seconds")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .max(0);
        if seconds == 0 {
            continue;
        }
        app_seconds += seconds;
        let (weight, matched_rule) = application_weight_match(role_policy, name, default_weight);
        if weight > 0.0 {
            matched_applications += 1;
        }
        weighted_seconds += seconds as f64 * weight;
        app_details.push(AppWeightDetail {
            application: name.to_string(),
            seconds,
            weight,
            weighted_seconds: (seconds as f64 * weight).round() as i64,
            matched_rule,
        });
    }
    app_details.sort_by_key(|item| -item.weighted_seconds);
    let policy_audit = build_policy_audit(&app_details, default_weight);
    app_details.truncate(12);
    let weighted_seconds_i64 = weighted_seconds.round() as i64;
    let role_label = role_policy
        .label
        .clone()
        .unwrap_or_else(|| role.to_string());
    let explanation = role_policy.description.clone().unwrap_or_else(|| {
        format!(
            "Роль {}: индекс = взвешенное время приложений / плановое время роли.",
            role_label
        )
    });
    Some(WeightedActivity {
        role: role.to_string(),
        role_label,
        index: Some(
            ((weighted_seconds / planned_seconds as f64) * 100.0)
                .round()
                .clamp(0.0, 100.0) as u8,
        ),
        formula: "index = weighted_seconds / planned_seconds × 100".to_string(),
        planned_seconds,
        app_seconds,
        weighted_seconds: weighted_seconds_i64,
        matched_applications,
        explanation,
        app_details,
        policy_audit,
        employee_details: employee_index_details(
            snapshot,
            &role_policy.label,
            planned_hours,
            anonymize,
        ),
    })
}

fn build_policy_audit(app_details: &[AppWeightDetail], default_weight: f64) -> Value {
    let default_items = app_details
        .iter()
        .filter(|item| item.matched_rule == "default_weight")
        .map(|item| {
            json!({
                "application": item.application,
                "seconds": item.seconds,
                "weight": item.weight,
                "weighted_seconds": item.weighted_seconds,
                "reason": "matched no explicit application rule"
            })
        })
        .collect::<Vec<_>>();
    let zero_weight_items = app_details
        .iter()
        .filter(|item| item.weight <= 0.0)
        .map(|item| {
            json!({
                "application": item.application,
                "seconds": item.seconds,
                "matched_rule": item.matched_rule
            })
        })
        .collect::<Vec<_>>();
    let default_seconds = app_details
        .iter()
        .filter(|item| item.matched_rule == "default_weight")
        .map(|item| item.seconds)
        .sum::<i64>();
    json!({
        "default_weight": default_weight,
        "total_applications": app_details.len(),
        "explicit_rule_applications": app_details.iter().filter(|item| item.matched_rule != "default_weight").count(),
        "default_weight_applications": default_items.len(),
        "default_weight_seconds": default_seconds,
        "zero_weight_applications": zero_weight_items.len(),
        "needs_review": default_items.into_iter().take(12).collect::<Vec<_>>(),
        "zero_weight_details": zero_weight_items.into_iter().take(12).collect::<Vec<_>>(),
    })
}

fn employee_index_details(
    snapshot: &Snapshot,
    role_label: &Option<String>,
    planned_hours_per_day: f64,
    anonymize: bool,
) -> Vec<Value> {
    let planned_seconds = (planned_hours_per_day * 3600.0).round() as i64;
    let Some(rows) = snapshot
        .worktime
        .payload
        .as_ref()
        .and_then(|payload| payload.get("rows"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    let role = role_label.as_deref().unwrap_or("default");
    rows.iter()
        .enumerate()
        .map(|(idx, row)| {
            let active_seconds = row
                .get("active_seconds")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .max(0);
            let index = if planned_seconds > 0 {
                ((active_seconds as f64 / planned_seconds as f64) * 100.0)
                    .round()
                    .clamp(0.0, 100.0) as i64
            } else {
                0
            };
            let user = if anonymize {
                format!("Сотрудник {}", idx + 1)
            } else {
                row.get("user")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string()
            };
            let user_id = if anonymize {
                format!("EMPLOYEE-{}", idx + 1)
            } else {
                row.get("user_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string()
            };
            json!({
                "user": user,
                "user_id": user_id,
                "role_label": role,
                "formula": "employee_index = active_seconds / planned_seconds × 100",
                "reason": format!(
                    "active {} / plan {} => {}%",
                    human_duration(active_seconds),
                    human_duration(planned_seconds),
                    index
                ),
                "index": index,
                "status": workforce_index_status(Some(index as u8)),
                "active_seconds": active_seconds,
                "active_hhmm": row.get("active_hhmm").and_then(Value::as_str).unwrap_or(""),
                "planned_seconds": planned_seconds,
                "planned_hhmm": human_duration(planned_seconds),
                "last_activity": row.get("last_activity").and_then(Value::as_str).unwrap_or(""),
                "scope_note": "Это не персональный weighted KPI: per-user app-weight attribution пока отсутствует в worktime payload; веса приложений доступны на уровне портфеля.",
                "anonymized": anonymize
            })
        })
        .collect()
}

fn application_weight_match(
    role_policy: &WorkforceRolePolicy,
    application: &str,
    default_weight: f64,
) -> (f64, String) {
    let app = application.to_lowercase();
    role_policy
        .application_weights
        .iter()
        .find_map(|(pattern, weight)| {
            let pattern = pattern.to_lowercase();
            (!pattern.is_empty() && app.contains(&pattern))
                .then_some((weight.clamp(0.0, 1.0), pattern))
        })
        .unwrap_or((default_weight, "default_weight".to_string()))
}

fn weighted_activity_kpi_from_policy(policy: &Value) -> Value {
    if policy.get("configured").and_then(Value::as_bool) == Some(true) {
        report_kpi(
            "Взвешенная активность",
            workforce_index_text(
                policy
                    .get("index")
                    .and_then(Value::as_u64)
                    .map(|value| value as u8),
            ),
            workforce_index_status(
                policy
                    .get("index")
                    .and_then(Value::as_u64)
                    .map(|value| value as u8),
            ),
            &format!(
                "role={} по весам приложений",
                policy
                    .get("role_label")
                    .and_then(Value::as_str)
                    .unwrap_or("неизвестная роль")
            ),
        )
    } else {
        report_kpi(
            "Взвешенная активность",
            "не настроена".to_string(),
            "UNKNOWN".to_string(),
            "нужен workforce policy с весами приложений",
        )
    }
}

fn weighted_activity_item_from_policy(policy: &Value, policy_path: &Path) -> Value {
    if policy.get("configured").and_then(Value::as_bool) == Some(true) {
        report_item(
            "Взвешенная активность",
            workforce_index_status(
                policy
                    .get("index")
                    .and_then(Value::as_u64)
                    .map(|value| value as u8),
            ),
            format!(
                "{}; роль {}; weighted {}; apps {}",
                workforce_index_text(
                    policy
                        .get("index")
                        .and_then(Value::as_u64)
                        .map(|value| value as u8)
                ),
                policy
                    .get("role_label")
                    .and_then(Value::as_str)
                    .unwrap_or("неизвестная роль"),
                human_duration(
                    policy
                        .get("weighted_seconds")
                        .and_then(Value::as_i64)
                        .unwrap_or(0)
                ),
                policy
                    .get("matched_applications")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
            ),
        )
    } else {
        report_item(
            "Взвешенная активность",
            "UNKNOWN",
            format!("policy не настроена: {}", policy_path.display()),
        )
    }
}

fn workforce_policy_json(
    policy: Option<&WorkforcePolicy>,
    weighted: Option<&WeightedActivity>,
    policy_path: &Path,
    anonymize: bool,
) -> Value {
    match (policy, weighted) {
        (Some(policy), Some(weighted)) => json!({
            "configured": true,
            "anonymized": anonymize,
            "path": policy_path.display().to_string(),
            "default_role": policy.default_role,
            "roles_count": policy.roles.len(),
            "available_roles": workforce_role_catalog(policy),
            "role": weighted.role,
            "role_label": weighted.role_label,
            "explanation": weighted.explanation,
            "formula": weighted.formula,
            "index": weighted.index,
            "planned_seconds": weighted.planned_seconds,
            "app_seconds": weighted.app_seconds,
            "weighted_seconds": weighted.weighted_seconds,
            "matched_applications": weighted.matched_applications,
            "policy_audit": weighted.policy_audit,
            "employee_details": weighted.employee_details,
            "app_details": weighted.app_details.iter().map(|item| json!({
                "application": item.application,
                "seconds": item.seconds,
                "weight": item.weight,
                "weighted_seconds": item.weighted_seconds,
                "matched_rule": item.matched_rule,
            })).collect::<Vec<_>>(),
        }),
        (Some(policy), None) => json!({
            "configured": true,
            "anonymized": anonymize,
            "path": policy_path.display().to_string(),
            "default_role": policy.default_role,
            "roles_count": policy.roles.len(),
            "available_roles": workforce_role_catalog(policy),
            "note": "role/application policy exists, but weighted activity could not be calculated",
        }),
        (None, _) => json!({
            "configured": false,
            "anonymized": anonymize,
            "path": policy_path.display().to_string(),
            "note": "weighted activity requires role/application policy",
        }),
    }
}

fn workforce_role_catalog(policy: &WorkforcePolicy) -> Vec<Value> {
    policy
        .roles
        .iter()
        .map(|(role, item)| {
            json!({
                "role": role,
                "label": item.label.clone().unwrap_or_else(|| role.to_string()),
                "description": item.description.clone().unwrap_or_default(),
                "planned_hours_per_day": item.planned_hours_per_day.unwrap_or(8.0),
                "default_weight": item.default_weight.unwrap_or(0.0),
                "application_rules": item.application_weights.len(),
            })
        })
        .collect()
}

fn default_workforce_role() -> String {
    "default".to_string()
}

fn report_kpi(label: &str, value: String, status: String, context: &str) -> Value {
    json!({
        "label": label,
        "value": value,
        "status": status,
        "context": context,
    })
}

fn report_item(label: &str, status: impl Into<String>, value: impl Into<String>) -> Value {
    json!({
        "label": label,
        "status": status.into(),
        "value": value.into(),
    })
}

fn render_report_markdown(
    snapshot: &Snapshot,
    headline: &str,
    summary: &SummaryResponse,
    metrics: &ReportMetrics,
    recommendations: &[String],
    workforce: &ReportWorkforceSummary,
    explainability: (&Value, &Value),
) -> String {
    let (workforce_policy, ueba_risk) = explainability;
    let mut text = String::new();
    text.push_str("# DetMir оперативный отчет\n\n");
    text.push_str(&format!("Дата снимка: {}\n\n", snapshot.generated_at_utc));
    text.push_str(&format!("Итог: {headline}\n\n"));
    text.push_str("## KPI\n\n");
    text.push_str(&format!("- Общий статус: {}\n", summary.severity));
    text.push_str(&format!(
        "- Готовность для оператора: {}\n",
        if summary.operator_ok {
            "да"
        } else {
            "нет"
        }
    ));
    text.push_str(&format!(
        "- Индекс активности: {}\n",
        workforce_index_text(metrics.workforce_index)
    ));
    text.push_str(&format!(
        "- Качество данных агента: {} через {}\n",
        snapshot.agent_quality.quality_status, snapshot.agent_quality.collector_source
    ));
    text.push_str(&format!(
        "- Сессии агента: всего={}, активные={}, RDP={}\n",
        snapshot.agent_quality.sessions_collected_total,
        snapshot.agent_quality.active_sessions_total,
        snapshot.agent_quality.rdp_sessions_total
    ));
    if let Some(error) = &snapshot.agent_quality.collector_error {
        text.push_str(&format!("- Ошибка коллектора агента: {error}\n"));
    }
    text.push_str(&format!(
        "- Сотрудники за сегодня: {}\n",
        metrics.users_count
    ));
    text.push_str(&format!(
        "- Активное время: {}\n",
        human_duration(metrics.active_seconds)
    ));
    text.push_str(&format!("- Активные приложения: {}\n", metrics.apps_count));
    text.push_str(&format!(
        "- Подразделения/ответственные: {}/{}\n",
        workforce.departments_count, workforce.owners_count
    ));
    text.push_str(&format!(
        "- Автоматические выводы Workforce: {}\n",
        workforce.insights_count
    ));
    text.push_str(&format!("- Статус тренда: {}\n", workforce.trend_status));
    text.push_str(&format!(
        "- DLP технические сигналы: ok={}, warn={}, fail={}\n",
        metrics.dlp_ok, metrics.dlp_warn, metrics.dlp_fail
    ));
    text.push_str(&format!(
        "- Evidence: items={}, screenshots={}\n",
        metrics.evidence_total, metrics.evidence_screenshots
    ));
    text.push_str(&format!(
        "- Открытые вопросы: {}, в работе: {}\n\n",
        metrics.open_incidents, metrics.acknowledged_incidents
    ));
    text.push_str("## Рекомендации\n\n");
    for item in recommendations {
        text.push_str(&format!("- {item}\n"));
    }
    append_agent_quality_markdown(&mut text, &snapshot.agent_quality);
    append_agent_quality_history_markdown(
        &mut text,
        &snapshot.agent_quality_history,
        &snapshot.agent_quality_history_summary,
    );
    append_agent_quality_nodes_markdown(
        &mut text,
        &snapshot.agent_quality_nodes,
        &snapshot.agent_quality_nodes_summary,
    );
    append_agent_coverage_sla_markdown(&mut text, &snapshot.agent_coverage_sla);
    append_ueba_risk_markdown(&mut text, ueba_risk);
    append_workforce_policy_markdown(&mut text, workforce_policy);
    text.push_str("\nПримечание: DLP/case показатели являются derived detections/cases и требуют регламентной валидации перед подачей как подтвержденные инциденты.\n");
    text
}

fn append_agent_quality_markdown(text: &mut String, quality: &AgentQuality) {
    let explain = agent_quality_explain(quality);
    text.push_str("\n## Достоверность данных\n\n");
    text.push_str(&format!("- Источник: {}\n", quality.collector_source));
    text.push_str(&format!("- Статус: {}\n", explain.status));
    text.push_str(&format!(
        "- Принято в KPI: {}\n",
        if explain.kpi_accepted {
            "да"
        } else {
            "нет"
        }
    ));
    text.push_str(&format!(
        "- Сессии: всего={}, активные={}, RDP={}\n",
        quality.sessions_collected_total, quality.active_sessions_total, quality.rdp_sessions_total
    ));
    if let Some(error) = &quality.collector_error {
        text.push_str(&format!("- Ошибка коллектора: {error}\n"));
    }
    text.push_str(&format!("- Рекомендация: {}\n", explain.recommendation));
}

fn append_agent_quality_history_markdown(
    text: &mut String,
    history: &[AgentQualityHistoryItem],
    summary: &AgentQualityHistorySummary,
) {
    text.push_str("\n## Стабильность данных за период\n\n");
    text.push_str(&format!("- Дней с данными: {}\n", summary.days_observed));
    text.push_str(&format!("- OK дней: {}\n", summary.ok_days));
    text.push_str(&format!(
        "- WARNING/DEGRADED/UNKNOWN дней: {}\n",
        summary.warning_days + summary.degraded_days + summary.unknown_days
    ));
    text.push_str(&format!(
        "- KPI принят: {}% дней\n",
        summary.kpi_accepted_pct
    ));
    if history.is_empty() {
        text.push_str("- История качества агента за период отсутствует.\n");
        return;
    }
    for item in history {
        let error = item
            .collector_error
            .as_ref()
            .map(|value| format!(", error={value}"))
            .unwrap_or_default();
        text.push_str(&format!(
            "- {}: status={}, source={}, KPI={}{}\n",
            item.date,
            item.status,
            item.source,
            if item.kpi_accepted { "да" } else { "нет" },
            error
        ));
    }
}

fn append_agent_quality_nodes_markdown(
    text: &mut String,
    nodes: &[AgentQualityNodeItem],
    summary: &AgentQualityNodesSummary,
) {
    text.push_str("\n## Качество данных по узлам\n\n");
    text.push_str(&format!("- Всего узлов: {}\n", summary.total_nodes));
    text.push_str(&format!("- OK узлов: {}\n", summary.ok_nodes));
    text.push_str(&format!(
        "- WARNING/DEGRADED узлов: {}\n",
        summary.degraded_nodes
    ));
    text.push_str(&format!("- UNKNOWN узлов: {}\n", summary.unknown_nodes));
    text.push_str(&format!(
        "- Узлов, принятых в KPI: {}%\n",
        summary.accepted_kpi_nodes_pct
    ));
    if nodes.is_empty() {
        text.push_str("- История качества по рабочим местам отсутствует.\n");
        return;
    }
    let mut displayed = 0usize;
    for item in nodes {
        if item.status == "OK" && item.kpi_accepted && displayed >= 10 {
            continue;
        }
        if item.status == "OK" && item.kpi_accepted && nodes.len() > 10 {
            continue;
        }
        let error = item
            .collector_error
            .as_ref()
            .map(|value| format!(", error={value}"))
            .unwrap_or_default();
        text.push_str(&format!(
            "- {}: status={}, source={}, last_seen={}, sessions={}, rdp={}, KPI={}{}; рекомендация: {}\n",
            item.hostname,
            item.status,
            item.source,
            item.last_seen_utc,
            item.sessions_total,
            item.rdp_sessions,
            if item.kpi_accepted { "да" } else { "нет" },
            error,
            item.recommendation
        ));
        displayed += 1;
        if displayed >= 10 {
            break;
        }
    }
    if displayed == 0 {
        text.push_str("- Проблемных рабочих мест не найдено.\n");
    }
}

fn append_agent_coverage_sla_markdown(text: &mut String, sla: &AgentCoverageSla) {
    text.push_str("\n## SLA покрытия агентов\n\n");
    text.push_str(&format!("- Статус SLA: {}\n", sla.sla_status));
    text.push_str(&format!("- Ожидается узлов: {}\n", sla.expected_nodes));
    text.push_str(&format!(
        "- Прислали подтвержденные данные за 24 часа: {}\n",
        sla.reporting_nodes_24h
    ));
    text.push_str(&format!("- Устаревшие узлы: {}\n", sla.stale_nodes));
    text.push_str(&format!("- Отсутствующие узлы: {}\n", sla.missing_nodes));
    text.push_str(&format!("- Покрытие KPI: {}%\n", sla.coverage_pct));
    text.push_str(&format!("- Свежесть телеметрии: {}%\n", sla.freshness_pct));
    if sla.expected_nodes == 0 {
        text.push_str("- Список ожидаемых рабочих мест не настроен.\n");
        return;
    }
    if sla.problem_nodes.is_empty() {
        text.push_str("- Проблемных узлов не найдено.\n");
        return;
    }
    for item in sla.problem_nodes.iter().take(10) {
        text.push_str(&format!(
            "- {}: department={}, owner={}, last_seen={}, status={}; рекомендация: {}\n",
            item.hostname,
            item.department,
            item.owner,
            item.last_seen_utc,
            item.status,
            item.recommendation
        ));
    }
}

fn append_ueba_risk_markdown(text: &mut String, risk: &Value) {
    text.push_str("\n## UEBA риск\n\n");
    text.push_str(&format!(
        "- Score: {}/100\n",
        risk.get("score").and_then(Value::as_u64).unwrap_or(0)
    ));
    text.push_str(&format!(
        "- Level: {}\n",
        risk.get("level")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
    ));
    text.push_str(&format!(
        "- Formula: {}\n",
        risk.get("formula")
            .and_then(Value::as_str)
            .unwrap_or("sum(reason_points) capped at 100")
    ));
    text.push_str(&format!(
        "- Confidence: {:.0}%\n",
        risk.get("confidence")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
            * 100.0
    ));
    text.push_str(&format!(
        "- Baseline: {}\n",
        risk.get("baseline_status")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
    ));
    text.push_str(&format!(
        "- Baseline window: {} days\n",
        risk.get("baseline_window_days")
            .and_then(Value::as_i64)
            .unwrap_or(default_ueba_baseline_window_days())
    ));
    text.push_str(&format!(
        "- Baseline available: user={}, department={}\n",
        risk.get("user_baseline_available")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        risk.get("department_baseline_available")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    ));
    text.push_str(&format!(
        "- Deviation score: {}\n",
        risk.get("deviation_score")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    ));
    text.push_str(&format!(
        "- Policy version: {}\n",
        risk.get("policy_version")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
    ));
    if let Some(note) = risk.get("note").and_then(Value::as_str) {
        text.push_str(&format!("- Note: {note}\n"));
    }
    text.push_str("\n### Причины риска\n\n");
    let reasons = risk
        .get("reasons")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if reasons.is_empty() {
        text.push_str("- Существенных UEBA-сигналов в текущем срезе нет.\n");
        return;
    }
    for item in reasons.iter().take(12) {
        text.push_str(&format!(
            "- {}: +{} points, {}, evidence: {}\n",
            item.get("label").and_then(Value::as_str).unwrap_or("-"),
            item.get("points").and_then(Value::as_u64).unwrap_or(0),
            item.get("severity")
                .and_then(Value::as_str)
                .unwrap_or("INFO"),
            item.get("value").and_then(Value::as_str).unwrap_or("-")
        ));
        if let Some(recommendation) = item.get("recommendation").and_then(Value::as_str) {
            text.push_str(&format!("  - recommendation: {recommendation}\n"));
        }
    }
}

fn append_workforce_policy_markdown(text: &mut String, policy: &Value) {
    if policy.get("configured").and_then(Value::as_bool) != Some(true) {
        text.push_str("\n## Почему такой индекс\n\n");
        text.push_str("- Role/application policy не настроена.\n");
        return;
    }
    text.push_str("\n## Почему такой индекс\n\n");
    if policy.get("anonymized").and_then(Value::as_bool) == Some(true) {
        text.push_str("- Режим данных: обезличенный demo/export.\n");
    }
    text.push_str(&format!(
        "- Роль: {}\n",
        policy
            .get("role_label")
            .and_then(Value::as_str)
            .unwrap_or("-")
    ));
    text.push_str(&format!(
        "- Формула: {}\n",
        policy
            .get("formula")
            .and_then(Value::as_str)
            .unwrap_or("index = weighted_seconds / planned_seconds × 100")
    ));
    text.push_str(&format!(
        "- Индекс: {}\n",
        workforce_index_text(
            policy
                .get("index")
                .and_then(Value::as_u64)
                .map(|value| value as u8)
        )
    ));
    text.push_str(&format!(
        "- План/App/Weighted: {}/{}/{}\n",
        human_duration(
            policy
                .get("planned_seconds")
                .and_then(Value::as_i64)
                .unwrap_or(0)
        ),
        human_duration(
            policy
                .get("app_seconds")
                .and_then(Value::as_i64)
                .unwrap_or(0)
        ),
        human_duration(
            policy
                .get("weighted_seconds")
                .and_then(Value::as_i64)
                .unwrap_or(0)
        )
    ));
    if let Some(explanation) = policy.get("explanation").and_then(Value::as_str) {
        text.push_str(&format!("- Объяснение: {explanation}\n"));
    }
    text.push_str("\n### Top приложений\n\n");
    for item in policy
        .get("app_details")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(12)
    {
        text.push_str(&format!(
            "- {}: raw {}, weight {:.0}%, weighted {}, rule `{}`\n",
            item.get("application")
                .and_then(Value::as_str)
                .unwrap_or("-"),
            human_duration(item.get("seconds").and_then(Value::as_i64).unwrap_or(0)),
            item.get("weight").and_then(Value::as_f64).unwrap_or(0.0) * 100.0,
            human_duration(
                item.get("weighted_seconds")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
            ),
            item.get("matched_rule")
                .and_then(Value::as_str)
                .unwrap_or("-")
        ));
    }
    let default_items = policy
        .get("policy_audit")
        .and_then(|audit| audit.get("needs_review"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !default_items.is_empty() {
        text.push_str("\n### Аудит policy: default_weight\n\n");
        for item in default_items.iter().take(12) {
            text.push_str(&format!(
                "- {}: raw {}, default weight {:.0}%\n",
                item.get("application")
                    .and_then(Value::as_str)
                    .unwrap_or("-"),
                human_duration(item.get("seconds").and_then(Value::as_i64).unwrap_or(0)),
                item.get("weight").and_then(Value::as_f64).unwrap_or(0.0) * 100.0
            ));
        }
    }
    let employee_items = policy
        .get("employee_details")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !employee_items.is_empty() {
        text.push_str("\n### Drill-down по сотрудникам\n\n");
        text.push_str(
            "Важно: это не персональный weighted KPI; персональный app-weight breakdown пока недоступен в worktime payload.\n\n",
        );
        for item in employee_items.iter().take(12) {
            text.push_str(&format!(
                "- {}: {}%, active {}, plan {}, formula `{}`\n",
                item.get("user").and_then(Value::as_str).unwrap_or("-"),
                item.get("index").and_then(Value::as_i64).unwrap_or(0),
                human_duration(
                    item.get("active_seconds")
                        .and_then(Value::as_i64)
                        .unwrap_or(0)
                ),
                human_duration(
                    item.get("planned_seconds")
                        .and_then(Value::as_i64)
                        .unwrap_or(0)
                ),
                item.get("formula").and_then(Value::as_str).unwrap_or("-")
            ));
            if let Some(reason) = item.get("reason").and_then(Value::as_str) {
                text.push_str(&format!("  - reason: {reason}\n"));
            }
        }
    }
}

fn worktime_totals(snapshot: &Snapshot) -> (usize, i64, usize) {
    let Some(payload) = snapshot.worktime.payload.as_ref() else {
        return (0, 0, 0);
    };
    let rows = payload
        .get("rows")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let apps = payload
        .get("true_active_apps")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let active_seconds = rows
        .iter()
        .filter_map(|row| row.get("active_seconds").and_then(Value::as_i64))
        .sum();
    (rows.len(), active_seconds, apps)
}

fn dlp_counts(snapshot: &Snapshot) -> (u64, u64, u64) {
    let counts = snapshot
        .detmir_status
        .payload
        .as_ref()
        .and_then(|status| status.get("dlp_counts"))
        .unwrap_or(&Value::Null);
    (
        counts.get("ok").and_then(Value::as_u64).unwrap_or(0),
        counts.get("warn").and_then(Value::as_u64).unwrap_or(0),
        counts.get("fail").and_then(Value::as_u64).unwrap_or(0),
    )
}

fn evidence_status(evidence: &DlpEvidenceResponse) -> String {
    if evidence.ok {
        "OK".to_string()
    } else {
        "WARN".to_string()
    }
}

fn incident_status(open_incidents: usize) -> String {
    if open_incidents == 0 {
        "OK".to_string()
    } else {
        "WARN".to_string()
    }
}

fn human_duration(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    format!("{hours:02}:{minutes:02}")
}

fn workforce_index(users_count: usize, active_seconds: i64) -> Option<u8> {
    if users_count == 0 || active_seconds <= 0 {
        return None;
    }
    let planned_seconds = users_count as f64 * 8.0 * 3600.0;
    let value = ((active_seconds as f64 / planned_seconds) * 100.0).round();
    Some(value.clamp(0.0, 100.0) as u8)
}

fn workforce_index_text(value: Option<u8>) -> String {
    value
        .map(|value| format!("{value}%"))
        .unwrap_or_else(|| "нет данных".to_string())
}

fn workforce_index_status(value: Option<u8>) -> String {
    match value {
        Some(value) if value >= 80 => "OK".to_string(),
        Some(value) if value >= 60 => "WARN".to_string(),
        Some(_) => "FAIL".to_string(),
        None => "UNKNOWN".to_string(),
    }
}

fn build_incidents(snapshot: &Snapshot, state: &IncidentStateFile) -> Vec<IncidentItem> {
    let mut incidents = Vec::new();
    for source in [
        ("detmir_status", &snapshot.detmir_status),
        ("detmir_check", &snapshot.detmir_check),
        ("failed_units", &snapshot.failed_units),
        ("worktime", &snapshot.worktime),
        ("one_c", &snapshot.one_c),
    ] {
        if !source.1.ok {
            incidents.push(incident_item(
                &source.1.status,
                "health",
                source.0,
                &source.1.summary,
                &snapshot.generated_at_utc,
                "/portal/operator",
                state,
            ));
        }
    }
    if let Some(check) = snapshot.detmir_check.payload.as_ref() {
        if let Some(services) = check.get("services").and_then(Value::as_array) {
            for service in services {
                if service.get("ok").and_then(Value::as_bool) == Some(false) {
                    let status = if service.get("required").and_then(Value::as_bool) == Some(true) {
                        "FAIL"
                    } else {
                        "WARN"
                    };
                    let source = service
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("service");
                    let summary = service
                        .get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("service check failed");
                    incidents.push(incident_item(
                        status,
                        "service",
                        source,
                        summary,
                        &snapshot.generated_at_utc,
                        "/portal/operator",
                        state,
                    ));
                }
            }
        }
        if let Some(buckets) = check.get("buckets").and_then(Value::as_array) {
            for bucket in buckets {
                if bucket.get("ok").and_then(Value::as_bool) == Some(false) {
                    let status = bucket
                        .get("status")
                        .and_then(Value::as_str)
                        .unwrap_or("WARN");
                    let source = bucket
                        .get("bucket")
                        .and_then(Value::as_str)
                        .unwrap_or("bucket");
                    incidents.push(incident_item(
                        status,
                        "collector",
                        source,
                        &format!(
                            "{} is {}",
                            bucket
                                .get("label")
                                .and_then(Value::as_str)
                                .unwrap_or("bucket"),
                            bucket
                                .get("status")
                                .and_then(Value::as_str)
                                .unwrap_or("not OK")
                        ),
                        &snapshot.generated_at_utc,
                        "/portal/operator",
                        state,
                    ));
                }
            }
        }
    }
    if let Some(status) = snapshot.detmir_status.payload.as_ref() {
        let counts = status.get("dlp_counts").unwrap_or(&Value::Null);
        let warn = counts.get("warn").and_then(Value::as_u64).unwrap_or(0);
        let fail = counts.get("fail").and_then(Value::as_u64).unwrap_or(0);
        if warn > 0 || fail > 0 {
            incidents.push(incident_item(
                if fail > 0 { "FAIL" } else { "WARN" },
                "dlp",
                "dlp_counts",
                &format!("DLP requires review: warn={warn}, fail={fail}"),
                &snapshot.generated_at_utc,
                "/portal/incidents",
                state,
            ));
        }
    }
    incidents
}

fn incident_item(
    status: &str,
    kind: &str,
    source: &str,
    summary: &str,
    generated_at_utc: &str,
    link: &str,
    state: &IncidentStateFile,
) -> IncidentItem {
    let id = incident_id(kind, source, summary);
    let saved = state.incidents.get(&id);
    IncidentItem {
        id,
        status: status.to_string(),
        kind: kind.to_string(),
        source: source.to_string(),
        summary: summary.to_string(),
        generated_at_utc: generated_at_utc.to_string(),
        link: link.to_string(),
        acknowledged: saved
            .map(|item| item.state == "acknowledged")
            .unwrap_or(false),
        acknowledged_at_utc: saved.and_then(|item| item.acknowledged_at_utc.clone()),
        actor: saved.map(|item| item.actor.clone()),
        assigned_to: saved.and_then(|item| item.assigned_to.clone()),
        comment: saved.and_then(|item| item.comment.clone()),
        updated_at_utc: saved.map(|item| item.updated_at_utc.clone()),
    }
}

fn incident_id(kind: &str, source: &str, summary: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in format!("{kind}\n{source}\n{summary}").as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{kind}-{hash:016x}")
}

fn handle_incident_action(mut request: Request, args: &Cli) -> Result<()> {
    let actor = request_actor(&request);
    let mut body = String::new();
    request
        .as_reader()
        .take(32 * 1024)
        .read_to_string(&mut body)?;
    let response = apply_incident_action(args, &actor, &body);
    match response {
        Ok(response) => respond_json(request, &response),
        Err(err) => respond_text(
            request,
            StatusCode(400),
            &serde_json::to_string_pretty(&json!({
                "ok": false,
                "error": err.to_string()
            }))?,
            "application/json; charset=utf-8",
        ),
    }
}

fn handle_telemetry_ingest(mut request: Request, args: &Cli) -> Result<()> {
    if !telemetry_authorized(&request, args) {
        return respond_json_status(
            request,
            StatusCode(401),
            &json!({
                "ok": false,
                "error": "telemetry api key is missing or invalid"
            }),
        );
    }
    let mut body = String::new();
    request
        .as_reader()
        .take(1024 * 1024)
        .read_to_string(&mut body)?;
    let response = apply_telemetry_ingest(args, &body);
    match response {
        Ok(response) => respond_json(request, &response),
        Err(err) => respond_json_status(
            request,
            StatusCode(400),
            &json!({
                "ok": false,
                "error": err.to_string()
            }),
        ),
    }
}

fn apply_telemetry_ingest(args: &Cli, body: &str) -> Result<Value> {
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

fn validate_telemetry_payload(payload: &Value) -> Result<()> {
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

fn apply_incident_action(args: &Cli, actor: &str, body: &str) -> Result<IncidentActionResponse> {
    let action: IncidentActionRequest =
        serde_json::from_str(body).map_err(|err| anyhow!("invalid incident action JSON: {err}"))?;
    let id = validate_short_token(&action.id, "id", 128)?;
    let action_name = validate_action(&action.action)?;
    let assigned_to = sanitize_optional_text(action.assigned_to, 80);
    let comment = sanitize_optional_text(action.comment, 500);
    let now = now();

    let mut state_file = load_incident_state(args)?;
    let previous = state_file.incidents.get(&id).cloned();
    let acknowledged_at_utc = if action_name == "ack" {
        previous
            .as_ref()
            .and_then(|item| item.acknowledged_at_utc.clone())
            .or_else(|| Some(now.clone()))
    } else if action_name == "clear" {
        None
    } else {
        previous
            .as_ref()
            .and_then(|item| item.acknowledged_at_utc.clone())
    };
    let state_name = if action_name == "clear" {
        "open"
    } else if acknowledged_at_utc.is_some() {
        "acknowledged"
    } else {
        "assigned"
    };
    let next = IncidentActionState {
        state: state_name.to_string(),
        actor: actor.to_string(),
        updated_at_utc: now.clone(),
        acknowledged_at_utc,
        assigned_to: assigned_to
            .clone()
            .or_else(|| previous.and_then(|item| item.assigned_to)),
        comment: comment.clone(),
    };
    state_file.incidents.insert(id.clone(), next.clone());
    save_incident_state(args, &state_file)?;
    append_incident_audit(
        args,
        &IncidentAuditEntry {
            generated_at_utc: now,
            actor: actor.to_string(),
            id: id.clone(),
            action: action_name.to_string(),
            assigned_to,
            comment,
        },
    )?;
    Ok(IncidentActionResponse {
        ok: true,
        id,
        state: next,
    })
}

fn load_incident_state_best_effort(args: &Cli) -> IncidentStateFile {
    match load_incident_state(args) {
        Ok(state) => state,
        Err(err) => {
            eprintln!("detmir-portal incident state read failed: {err:#}");
            IncidentStateFile::default()
        }
    }
}

fn load_incident_state(args: &Cli) -> Result<IncidentStateFile> {
    let path = incident_state_path(args);
    if !path.exists() {
        return Ok(IncidentStateFile::default());
    }
    let data = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&data).with_context(|| format!("parse {}", path.display()))
}

fn save_incident_state(args: &Cli, state: &IncidentStateFile) -> Result<()> {
    fs::create_dir_all(&args.state_dir)
        .with_context(|| format!("create {}", args.state_dir.display()))?;
    let path = incident_state_path(args);
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(state)?)
        .with_context(|| format!("write {}", tmp.display()))?;
    fs::rename(&tmp, &path).with_context(|| format!("rename {}", path.display()))?;
    Ok(())
}

fn append_incident_audit(args: &Cli, entry: &IncidentAuditEntry) -> Result<()> {
    fs::create_dir_all(&args.state_dir)
        .with_context(|| format!("create {}", args.state_dir.display()))?;
    let path = args.state_dir.join("audit.jsonl");
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("open {}", path.display()))?;
    serde_json::to_writer(&mut file, entry)?;
    file.write_all(b"\n")?;
    Ok(())
}

fn incident_state_path(args: &Cli) -> PathBuf {
    args.state_dir.join("incidents-state.json")
}

fn build_dlp_evidence_response(args: &Cli) -> DlpEvidenceResponse {
    let generated_at_utc = now();
    let db_available = args.dlp_db_path.exists();
    let screenshot_root_available = args.evidence_root.exists();
    if !db_available {
        return DlpEvidenceResponse {
            ok: true,
            generated_at_utc,
            db_available,
            screenshot_root_available,
            limit: args.evidence_limit,
            items: Vec::new(),
            error: Some(format!(
                "DLP warehouse is absent: {}",
                args.dlp_db_path.display()
            )),
        };
    }
    match load_dlp_evidence_items(args) {
        Ok(items) => DlpEvidenceResponse {
            ok: true,
            generated_at_utc,
            db_available,
            screenshot_root_available,
            limit: args.evidence_limit,
            items,
            error: None,
        },
        Err(err) => DlpEvidenceResponse {
            ok: false,
            generated_at_utc,
            db_available,
            screenshot_root_available,
            limit: args.evidence_limit,
            items: Vec::new(),
            error: Some(err.to_string()),
        },
    }
}

fn load_dlp_evidence_items(args: &Cli) -> Result<Vec<DlpEvidenceItem>> {
    let connection = Connection::open_with_flags(
        &args.dlp_db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .with_context(|| format!("open DLP warehouse {}", args.dlp_db_path.display()))?;
    let mut statement = connection.prepare(
        r#"
        select
            id,
            bucket_id,
            event_id,
            stream_type,
            hostname,
            username,
            event_ts,
            operation,
            file_path,
            rule_id,
            action,
            severity,
            signal_type,
            message,
            source,
            screenshot_path,
            raw_json
        from dlp_events
        where stream_type = 'dlp_incident'
           or screenshot_path is not null
        order by event_ts desc, id desc
        limit ?
        "#,
    )?;
    let rows = statement
        .query_map(params![i64::from(args.evidence_limit)], |row| {
            Ok(DlpEvidenceRow {
                row_id: row.get(0)?,
                bucket_id: row.get(1)?,
                event_id: row.get(2)?,
                stream_type: row.get(3)?,
                hostname: row.get(4)?,
                username: row.get(5)?,
                event_ts: row.get(6)?,
                operation: row.get(7)?,
                file_path: row.get(8)?,
                rule_id: row.get(9)?,
                action: row.get(10)?,
                severity: row.get(11)?,
                signal_type: row.get(12)?,
                message: row.get(13)?,
                source: row.get(14)?,
                screenshot_path: row.get(15)?,
                raw_json: row.get(16)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|row| evidence_item_from_row(args, row))
        .collect()
}

fn evidence_item_from_row(args: &Cli, row: DlpEvidenceRow) -> Result<DlpEvidenceItem> {
    let raw = serde_json::from_str::<Value>(&row.raw_json).unwrap_or(Value::Null);
    let sha256 = json_string(&raw, &["screenshotSha256", "sha256", "captureSha256"])
        .map(|value| value.to_ascii_lowercase())
        .filter(|value| is_sha256_hex(value));
    let source_file = row
        .screenshot_path
        .as_deref()
        .and_then(screenshot_basename)
        .or_else(|| json_string(&raw, &["screenshotFile", "artifactFile"]));
    let id = evidence_id(row.row_id, &row.event_id, row.screenshot_path.as_deref());
    let screenshot = resolve_screenshot_file(args, &source_file, &sha256)?;
    let has_screenshot_metadata = row.screenshot_path.is_some() || sha256.is_some();
    let blocked_reason = if screenshot.is_some() {
        None
    } else if has_screenshot_metadata && sha256.is_none() {
        Some("screenshot sha256 is absent; original is not served".to_string())
    } else if has_screenshot_metadata {
        Some("screenshot is not synced into the server evidence root yet".to_string())
    } else {
        None
    };
    Ok(DlpEvidenceItem {
        id: id.clone(),
        event_ts: row.event_ts,
        bucket_id: row.bucket_id,
        event_id: row.event_id,
        stream_type: row.stream_type,
        hostname: row.hostname,
        username: row.username,
        severity: row.severity,
        signal_type: row.signal_type,
        rule_id: row.rule_id,
        action: row.action.or(row.operation),
        source: row.source,
        message: row.message,
        file_path: row.file_path,
        has_screenshot_metadata,
        screenshot_available: screenshot.is_some(),
        source_file,
        screenshot_sha256: sha256,
        screenshot_width: json_i64(&raw, &["screenshotWidth", "captureWidth"]),
        screenshot_height: json_i64(&raw, &["screenshotHeight", "captureHeight"]),
        preview_url: screenshot
            .as_ref()
            .map(|_| format!("/portal/api/dlp/evidence/{id}/screenshot")),
        download_url: screenshot
            .as_ref()
            .map(|_| format!("/portal/api/dlp/evidence/{id}/download")),
        blocked_reason,
    })
}

fn parse_evidence_screenshot_path(path: &str) -> Option<(String, bool)> {
    let rest = path.strip_prefix("/api/dlp/evidence/")?;
    let (evidence_id, suffix) = rest.rsplit_once('/')?;
    match suffix {
        "screenshot" => Some((evidence_id.to_string(), false)),
        "download" => Some((evidence_id.to_string(), true)),
        _ => None,
    }
}

fn handle_evidence_upload(mut request: Request, args: &Cli) -> Result<()> {
    if !upload_authorized(&request, args) {
        return respond_text(
            request,
            StatusCode(403),
            "Forbidden",
            "text/plain; charset=utf-8",
        );
    }
    let body_limit = args
        .evidence_max_bytes
        .saturating_mul(2)
        .saturating_add(64 * 1024)
        .min(32 * 1024 * 1024);
    let mut body = String::new();
    request
        .as_reader()
        .take(body_limit)
        .read_to_string(&mut body)?;
    match apply_evidence_upload(args, &request_actor(&request), &body) {
        Ok(response) => respond_json(request, &response),
        Err(err) => respond_text(
            request,
            StatusCode(400),
            &serde_json::to_string_pretty(&json!({
                "ok": false,
                "error": err.to_string()
            }))?,
            "application/json; charset=utf-8",
        ),
    }
}

fn apply_evidence_upload(args: &Cli, actor: &str, body: &str) -> Result<EvidenceUploadResponse> {
    let upload: EvidenceUploadRequest =
        serde_json::from_str(body).map_err(|err| anyhow!("invalid evidence upload JSON: {err}"))?;
    let expected_sha256 = upload.sha256.trim().to_ascii_lowercase();
    if !is_sha256_hex(&expected_sha256) {
        return Err(anyhow!("sha256 is invalid"));
    }
    if upload.content_base64.is_empty() {
        return Err(anyhow!("content_base64 is empty"));
    }
    let bytes = BASE64_STANDARD
        .decode(upload.content_base64.as_bytes())
        .map_err(|err| anyhow!("content_base64 decode failed: {err}"))?;
    if bytes.is_empty() {
        return Err(anyhow!("content is empty"));
    }
    if bytes.len() as u64 > args.evidence_max_bytes {
        return Err(anyhow!("content is too large"));
    }
    let actual_sha256 = sha256_bytes(&bytes);
    if actual_sha256 != expected_sha256 {
        return Err(anyhow!("sha256 mismatch"));
    }
    let (content_type, extension) = evidence_image_type(&bytes, upload.content_type.as_deref())?;
    let root = ensure_evidence_root(args)?;
    let screenshots = root.join("screenshots");
    fs::create_dir_all(&screenshots)
        .with_context(|| format!("create {}", screenshots.display()))?;
    let path = screenshots.join(format!("{expected_sha256}.{extension}"));
    let mut stored = false;
    if path.exists() {
        let existing = sha256_file(&path)?;
        if existing != expected_sha256 {
            return Err(anyhow!("existing evidence file hash mismatch"));
        }
    } else {
        let tmp = screenshots.join(format!(
            "{expected_sha256}.{extension}.tmp-{}",
            std::process::id()
        ));
        fs::write(&tmp, &bytes).with_context(|| format!("write {}", tmp.display()))?;
        fs::rename(&tmp, &path).with_context(|| format!("rename {}", path.display()))?;
        stored = true;
    }
    append_evidence_audit(
        args,
        &EvidenceAuditEntry {
            generated_at_utc: now(),
            actor: actor.to_string(),
            action: "upload".to_string(),
            evidence_id: format!("sha256:{expected_sha256}"),
            sha256: Some(expected_sha256.clone()),
            source_file: upload
                .source_file
                .as_deref()
                .and_then(screenshot_basename)
                .or_else(|| upload.source_path.as_deref().and_then(screenshot_basename)),
        },
    )?;
    let meta = json!({
        "generated_at_utc": now(),
        "hostname": upload.hostname.as_deref().map(|value| sanitize_text(value, 80)),
        "username": upload.username.as_deref().map(|value| sanitize_text(value, 80)),
        "source_file": upload.source_file.as_deref().and_then(screenshot_basename),
        "source_path_basename": upload.source_path.as_deref().and_then(screenshot_basename),
        "sha256": expected_sha256,
        "content_type": content_type,
        "bytes": bytes.len(),
        "path": path.file_name().and_then(|name| name.to_str()).unwrap_or(""),
    });
    let _ = fs::write(
        screenshots.join(format!("{}.json", meta["sha256"].as_str().unwrap_or(""))),
        serde_json::to_vec_pretty(&meta)?,
    );
    Ok(EvidenceUploadResponse {
        ok: true,
        sha256: meta["sha256"].as_str().unwrap_or("").to_string(),
        content_type: content_type.to_string(),
        bytes: bytes.len() as u64,
        stored,
        path: path.display().to_string(),
    })
}

fn handle_evidence_screenshot(
    request: Request,
    args: &Cli,
    evidence_id: &str,
    download: bool,
) -> Result<()> {
    let actor = request_actor(&request);
    let id = match validate_short_token(evidence_id, "evidence_id", 128) {
        Ok(id) => id,
        Err(err) => {
            return respond_text(
                request,
                StatusCode(400),
                &format!("Bad evidence id: {err}"),
                "text/plain; charset=utf-8",
            );
        }
    };
    let screenshot = match load_screenshot_for_evidence(args, &id) {
        Ok(Some(screenshot)) => screenshot,
        Ok(None) => {
            return respond_text(
                request,
                StatusCode(404),
                "Evidence screenshot is not available",
                "text/plain; charset=utf-8",
            );
        }
        Err(err) => {
            return respond_text(
                request,
                StatusCode(400),
                &format!("Evidence screenshot rejected: {err}"),
                "text/plain; charset=utf-8",
            );
        }
    };
    append_evidence_audit(
        args,
        &EvidenceAuditEntry {
            generated_at_utc: now(),
            actor,
            action: if download { "download" } else { "view" }.to_string(),
            evidence_id: id,
            sha256: screenshot.sha256.clone(),
            source_file: screenshot.source_file.clone(),
        },
    )?;
    respond_file(
        request,
        &screenshot.path,
        screenshot.content_type,
        download.then_some(
            screenshot
                .source_file
                .as_deref()
                .unwrap_or("dlp-evidence.png"),
        ),
    )
}

fn load_screenshot_for_evidence(
    args: &Cli,
    evidence_id_value: &str,
) -> Result<Option<ScreenshotFile>> {
    let row_id = evidence_row_id(evidence_id_value)?;
    if !args.dlp_db_path.exists() {
        return Ok(None);
    }
    let connection = Connection::open_with_flags(
        &args.dlp_db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .with_context(|| format!("open DLP warehouse {}", args.dlp_db_path.display()))?;
    let row = connection
        .query_row(
            r#"
            select
                id,
                bucket_id,
                event_id,
                stream_type,
                hostname,
                username,
                event_ts,
                operation,
                file_path,
                rule_id,
                action,
                severity,
                signal_type,
                message,
                source,
                screenshot_path,
                raw_json
            from dlp_events
            where id = ?
            "#,
            params![row_id],
            |row| {
                Ok(DlpEvidenceRow {
                    row_id: row.get(0)?,
                    bucket_id: row.get(1)?,
                    event_id: row.get(2)?,
                    stream_type: row.get(3)?,
                    hostname: row.get(4)?,
                    username: row.get(5)?,
                    event_ts: row.get(6)?,
                    operation: row.get(7)?,
                    file_path: row.get(8)?,
                    rule_id: row.get(9)?,
                    action: row.get(10)?,
                    severity: row.get(11)?,
                    signal_type: row.get(12)?,
                    message: row.get(13)?,
                    source: row.get(14)?,
                    screenshot_path: row.get(15)?,
                    raw_json: row.get(16)?,
                })
            },
        )
        .optional()?;
    let Some(row) = row else {
        return Ok(None);
    };
    let expected_id = evidence_id(row.row_id, &row.event_id, row.screenshot_path.as_deref());
    if expected_id != evidence_id_value {
        return Err(anyhow!("evidence id checksum mismatch"));
    }
    let raw = serde_json::from_str::<Value>(&row.raw_json).unwrap_or(Value::Null);
    let sha256 = json_string(&raw, &["screenshotSha256", "sha256", "captureSha256"])
        .map(|value| value.to_ascii_lowercase())
        .filter(|value| is_sha256_hex(value));
    let source_file = row
        .screenshot_path
        .as_deref()
        .and_then(screenshot_basename)
        .or_else(|| json_string(&raw, &["screenshotFile", "artifactFile"]));
    resolve_screenshot_file(args, &source_file, &sha256)
}

fn resolve_screenshot_file(
    args: &Cli,
    source_file: &Option<String>,
    sha256: &Option<String>,
) -> Result<Option<ScreenshotFile>> {
    let Some(expected_sha256) = sha256.as_deref() else {
        return Ok(None);
    };
    if !args.evidence_root.exists() {
        return Ok(None);
    }
    let root = args
        .evidence_root
        .canonicalize()
        .with_context(|| format!("canonicalize {}", args.evidence_root.display()))?;
    let mut candidates = Vec::new();
    candidates.push(
        root.join("screenshots")
            .join(format!("{expected_sha256}.png")),
    );
    candidates.push(
        root.join("screenshots")
            .join(format!("{expected_sha256}.jpg")),
    );
    candidates.push(
        root.join("screenshots")
            .join(format!("{expected_sha256}.jpeg")),
    );
    candidates.push(root.join(format!("{expected_sha256}.png")));
    candidates.push(root.join(format!("{expected_sha256}.jpg")));
    candidates.push(root.join(format!("{expected_sha256}.jpeg")));
    candidates.push(root.join(expected_sha256));
    if let Some(file_name) = source_file
        .as_deref()
        .and_then(screenshot_basename)
        .filter(|name| is_safe_file_name(name))
    {
        candidates.push(root.join("screenshots").join(&file_name));
        candidates.push(root.join(&file_name));
    }
    for candidate in candidates {
        if !candidate.exists() {
            continue;
        }
        let canonical = candidate
            .canonicalize()
            .with_context(|| format!("canonicalize {}", candidate.display()))?;
        if !canonical.starts_with(&root) {
            continue;
        }
        let metadata = fs::metadata(&canonical)?;
        if !metadata.is_file() || metadata.len() > args.evidence_max_bytes {
            continue;
        }
        let Some(content_type) = screenshot_content_type(&canonical) else {
            continue;
        };
        let actual_sha256 = sha256_file(&canonical)?;
        if actual_sha256 != expected_sha256 {
            continue;
        }
        return Ok(Some(ScreenshotFile {
            path: canonical,
            content_type,
            source_file: source_file.clone(),
            sha256: Some(expected_sha256.to_string()),
        }));
    }
    Ok(None)
}

fn append_evidence_audit(args: &Cli, entry: &EvidenceAuditEntry) -> Result<()> {
    fs::create_dir_all(&args.state_dir)
        .with_context(|| format!("create {}", args.state_dir.display()))?;
    let path = args.state_dir.join("evidence-audit.jsonl");
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("open {}", path.display()))?;
    serde_json::to_writer(&mut file, entry)?;
    file.write_all(b"\n")?;
    Ok(())
}

fn evidence_id(row_id: i64, event_id: &str, screenshot_path: Option<&str>) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in format!("{row_id}\n{event_id}\n{}", screenshot_path.unwrap_or("")).as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("ev-{row_id}-{hash:016x}")
}

fn evidence_row_id(evidence_id_value: &str) -> Result<i64> {
    let rest = evidence_id_value
        .strip_prefix("ev-")
        .ok_or_else(|| anyhow!("unsupported evidence id prefix"))?;
    let (row_id, checksum) = rest
        .split_once('-')
        .ok_or_else(|| anyhow!("malformed evidence id"))?;
    if checksum.len() != 16 || !checksum.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(anyhow!("malformed evidence id checksum"));
    }
    row_id
        .parse::<i64>()
        .map_err(|err| anyhow!("malformed evidence row id: {err}"))
}

fn json_string(value: &Value, names: &[&str]) -> Option<String> {
    for name in names {
        if let Some(text) = value.get(*name).and_then(Value::as_str) {
            let text = sanitize_text(text, 512);
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    None
}

fn json_i64(value: &Value, names: &[&str]) -> Option<i64> {
    names
        .iter()
        .find_map(|name| value.get(*name).and_then(Value::as_i64))
}

fn screenshot_basename(path: &str) -> Option<String> {
    if path.split(['/', '\\']).any(|part| part == "..") {
        return None;
    }
    let name = path
        .rsplit(['/', '\\'])
        .next()
        .map(|value| sanitize_text(value, 255))
        .filter(|value| !value.is_empty())?;
    is_safe_file_name(&name).then_some(name)
}

fn is_safe_file_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && !value.contains("..")
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_' | '@'))
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn screenshot_content_type(path: &Path) -> Option<&'static str> {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        _ => None,
    }
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn evidence_image_type(
    bytes: &[u8],
    claimed_content_type: Option<&str>,
) -> Result<(&'static str, &'static str)> {
    let detected = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(("image/png", "png"))
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some(("image/jpeg", "jpg"))
    } else {
        None
    };
    let Some((content_type, extension)) = detected else {
        return Err(anyhow!("unsupported evidence image type"));
    };
    if let Some(claimed) = claimed_content_type.map(|value| value.trim().to_ascii_lowercase()) {
        let allowed = match content_type {
            "image/png" => claimed == "image/png" || claimed == "application/octet-stream",
            "image/jpeg" => {
                claimed == "image/jpeg"
                    || claimed == "image/jpg"
                    || claimed == "application/octet-stream"
            }
            _ => false,
        };
        if !allowed {
            return Err(anyhow!("claimed content_type does not match image bytes"));
        }
    }
    Ok((content_type, extension))
}

fn ensure_evidence_root(args: &Cli) -> Result<PathBuf> {
    fs::create_dir_all(&args.evidence_root)
        .with_context(|| format!("create {}", args.evidence_root.display()))?;
    args.evidence_root
        .canonicalize()
        .with_context(|| format!("canonicalize {}", args.evidence_root.display()))
}

fn upload_enabled(args: &Cli) -> bool {
    args.evidence_upload_token
        .as_deref()
        .map(|token| !token.trim().is_empty())
        .unwrap_or(false)
}

fn upload_authorized(request: &Request, args: &Cli) -> bool {
    let Some(expected) = args
        .evidence_upload_token
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty())
    else {
        return false;
    };
    let Some(actual) = bearer_token(request) else {
        return false;
    };
    constant_time_eq(actual.as_bytes(), expected.as_bytes())
}

fn telemetry_authorized(request: &Request, args: &Cli) -> bool {
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

fn bearer_token(request: &Request) -> Option<String> {
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

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0_u8;
    for (a, b) in left.iter().zip(right.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

fn request_actor(request: &Request) -> String {
    for name in ["X-Remote-User", "X-Gateway-User", "Remote-User"] {
        if let Some(value) = request
            .headers()
            .iter()
            .find(|header| header.field.equiv(name))
            .map(|header| header.value.as_str().trim())
        {
            if !value.is_empty() {
                return sanitize_text(value, 80);
            }
        }
    }
    "local".to_string()
}

fn validate_short_token(value: &str, name: &str, max_len: usize) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > max_len {
        return Err(anyhow!("{name} length is invalid"));
    }
    if !value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':' | '.'))
    {
        return Err(anyhow!("{name} contains unsupported characters"));
    }
    Ok(value.to_string())
}

fn validate_action(value: &str) -> Result<&'static str> {
    match value.trim() {
        "ack" | "acknowledge" => Ok("ack"),
        "assign" => Ok("assign"),
        "clear" | "reopen" => Ok("clear"),
        _ => Err(anyhow!("unsupported incident action")),
    }
}

fn sanitize_optional_text(value: Option<String>, max_len: usize) -> Option<String> {
    value
        .map(|text| sanitize_text(&text, max_len))
        .filter(|text| !text.is_empty())
}

fn sanitize_text(value: &str, max_len: usize) -> String {
    value
        .chars()
        .filter(|ch| !ch.is_control())
        .take(max_len)
        .collect::<String>()
        .trim()
        .to_string()
}

fn links() -> PortalLinks {
    PortalLinks {
        portal: "/portal/".to_string(),
        grafana_dashboards: "/dashboards".to_string(),
        detmir_activitywatch:
            "/d/detmir-aw-main/detmir-activitywatch?orgId=1&from=now-48h&to=now&timezone=browser&var-host=HOST-EXAMPLE&refresh=5m"
                .to_string(),
        dlp_security_dashboard:
            "/d/detmir-dlp-security?orgId=1&from=now-30d&to=now&timezone=browser".to_string(),
        dlp_management_dashboard:
            "/d/detmir-dlp-management?orgId=1&from=now-30d&to=now&timezone=browser".to_string(),
        dlp_overview_dashboard:
            "/d/awatch-dlp-overview?orgId=1&from=now-30d&to=now&timezone=browser".to_string(),
        aw_ui: "/r/aw/".to_string(),
        worktime_report: "/reports/worktime/management?format=html&host=HOST-EXAMPLE".to_string(),
        file1c_brief: "/r/file1c/brief".to_string(),
        file1c_actions: "/r/file1c/actions".to_string(),
    }
}

fn collection_block(check: Option<&Value>) -> SummaryBlock {
    let Some(check) = check else {
        return block("UNKNOWN", "Нет данных detmir-check");
    };
    let summary = check.get("summary").unwrap_or(&Value::Null);
    let stale = summary
        .get("bucket_stale")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let dead = summary
        .get("bucket_dead")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let failures = summary
        .get("service_failures")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if stale == 0 && dead == 0 && failures == 0 {
        block("OK", "Сбор данных свежий")
    } else {
        block(
            "FAIL",
            &format!("Проблемы сбора: stale={stale}, dead={dead}, service_fail={failures}"),
        )
    }
}

fn grafana_block(snapshot: &Snapshot) -> SummaryBlock {
    let Some(service) = grafana_service(snapshot) else {
        return block("WARN", "Grafana check не найден");
    };
    let ok = service.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let age = service
        .pointer("/payload/age_seconds")
        .and_then(Value::as_i64)
        .unwrap_or(-1);
    let fail_count = service
        .pointer("/payload/fail_count")
        .and_then(Value::as_i64)
        .unwrap_or(-1);
    if ok {
        block(
            "OK",
            &format!("Grafana данные актуальны, возраст {} сек", age),
        )
    } else {
        block(
            "FAIL",
            &format!("Grafana check fail_count={fail_count}, age={age}"),
        )
    }
}

fn dlp_block(snapshot: &Snapshot) -> SummaryBlock {
    let Some(status) = snapshot.detmir_status.payload.as_ref() else {
        return block("UNKNOWN", "Нет DLP данных");
    };
    let counts = status.get("dlp_counts").unwrap_or(&Value::Null);
    let ok = counts.get("ok").and_then(Value::as_u64).unwrap_or(0);
    let warn = counts.get("warn").and_then(Value::as_u64).unwrap_or(0);
    let fail = counts.get("fail").and_then(Value::as_u64).unwrap_or(0);
    if warn == 0 && fail == 0 {
        block("OK", &format!("DLP проверки OK: {ok}"))
    } else {
        block("WARN", &format!("DLP: ok={ok}, warn={warn}, fail={fail}"))
    }
}

fn worktime_block(snapshot: &Snapshot) -> SummaryBlock {
    if !snapshot.worktime.ok {
        return block("FAIL", "Worktime API не отвечает");
    }
    let Some(payload) = snapshot.worktime.payload.as_ref() else {
        return block("UNKNOWN", "Нет Worktime JSON");
    };
    let rows = payload
        .get("rows")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let apps = payload
        .get("true_active_apps")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    if rows > 0 {
        block(
            "OK",
            &format!("Есть данные за сегодня: сотрудников={rows}, приложений={apps}"),
        )
    } else {
        block("WARN", "Worktime API ответил, но строк сотрудников нет")
    }
}

fn one_c_block(snapshot: &Snapshot) -> SummaryBlock {
    if !snapshot.one_c.ok {
        return block("FAIL", "1C analytics API не отвечает");
    }
    let companies = snapshot
        .one_c
        .payload
        .as_ref()
        .and_then(|value| value.get("companies_total"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    block(
        "OK",
        &format!("1C analytics отвечает, компаний={companies}"),
    )
}

fn owner_recommendations(snapshot: &Snapshot, summary: &SummaryResponse) -> Vec<String> {
    let mut out = Vec::new();
    if !summary.operator_ok {
        out.push("Проверить технический контур: общий статус не готов для оператора".to_string());
    }
    if !grafana_data_ok(snapshot) {
        out.push(
            "Проверить Grafana data pipeline: dashboard freshness или panel query не OK"
                .to_string(),
        );
    }
    if !snapshot.worktime.ok {
        out.push(
            "Проверить Worktime API и RDP collectors: нет надежного отчета за сегодня".to_string(),
        );
    }
    if !dlp_ok(snapshot) {
        out.push("Открыть DLP обзор: есть предупреждения или ошибки DLP".to_string());
    }
    if !snapshot.one_c.ok {
        out.push("Проверить 1C analytics API: управленческий блок может быть неполным".to_string());
    }
    if out.is_empty() {
        out.push("Критичных действий сейчас не требуется".to_string());
    }
    out
}

fn grafana_data_ok(snapshot: &Snapshot) -> bool {
    grafana_service(snapshot)
        .and_then(|service| service.get("ok").and_then(Value::as_bool))
        .unwrap_or(false)
}

fn grafana_service(snapshot: &Snapshot) -> Option<Value> {
    snapshot
        .detmir_check
        .payload
        .as_ref()
        .and_then(|check| check.get("services"))
        .and_then(Value::as_array)
        .and_then(|services| {
            services
                .iter()
                .find(|service| service.get("name").and_then(Value::as_str) == Some("grafana-data"))
        })
        .cloned()
}

fn dlp_ok(snapshot: &Snapshot) -> bool {
    snapshot
        .detmir_status
        .payload
        .as_ref()
        .and_then(|status| status.get("dlp_ok"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn block(status: &str, text: &str) -> SummaryBlock {
    SummaryBlock {
        status: status.to_string(),
        text: text.to_string(),
    }
}

fn payload_bool(payload: &Value, pointer: &str) -> Option<bool> {
    payload.pointer(pointer).and_then(Value::as_bool)
}

fn status_from_payload(payload: &Value) -> String {
    payload
        .get("severity")
        .or_else(|| payload.get("status"))
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            if payload_bool(payload, "/ok") == Some(false) {
                "FAIL"
            } else {
                "OK"
            }
        })
        .to_string()
}

fn source_summary(name: &str, payload: &Value) -> String {
    match name {
        "detmir_status" => format!(
            "severity={}, operator_ok={}",
            payload
                .get("severity")
                .and_then(Value::as_str)
                .unwrap_or("UNKNOWN"),
            payload
                .get("ok_for_operator")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        ),
        "detmir_check" => {
            let summary = payload.get("summary").unwrap_or(&Value::Null);
            format!(
                "bucket_ok={}, stale={}, dead={}, service_fail={}",
                summary
                    .get("bucket_ok")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                summary
                    .get("bucket_stale")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                summary
                    .get("bucket_dead")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                summary
                    .get("service_failures")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            )
        }
        "worktime_api" => format!(
            "rows={}, apps={}",
            payload
                .get("rows")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0),
            payload
                .get("true_active_apps")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0)
        ),
        "worktime_management" => format!(
            "coverage={:.0}%, departments={}, owners={}",
            payload
                .pointer("/summary/portfolio_coverage_pct")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            payload
                .get("department_rollups")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0),
            payload
                .get("owner_rollups")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0)
        ),
        "one_c" => format!(
            "status={}, companies={}",
            payload
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            payload
                .get("companies_total")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        ),
        _ => "source loaded".to_string(),
    }
}

fn respond_json<T: Serialize>(request: Request, value: &T) -> Result<()> {
    let body = serde_json::to_string_pretty(value)?;
    respond_text(
        request,
        StatusCode(200),
        &body,
        "application/json; charset=utf-8",
    )
}

fn respond_json_status<T: Serialize>(
    request: Request,
    status: StatusCode,
    value: &T,
) -> Result<()> {
    let body = serde_json::to_string_pretty(value)?;
    respond_text(request, status, &body, "application/json; charset=utf-8")
}

fn respond_text(
    request: Request,
    status: StatusCode,
    body: &str,
    content_type: &str,
) -> Result<()> {
    let response = Response::from_string(body.to_string())
        .with_status_code(status)
        .with_header(header("Content-Type", content_type)?)
        .with_header(header("Cache-Control", "no-store")?);
    request.respond(response).map_err(|err| anyhow!("{err}"))
}

fn respond_file(
    request: Request,
    path: &Path,
    content_type: &str,
    download_name: Option<&str>,
) -> Result<()> {
    let data = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let mut response = Response::from_data(data)
        .with_status_code(StatusCode(200))
        .with_header(header("Content-Type", content_type)?)
        .with_header(header("Cache-Control", "no-store")?);
    if let Some(name) = download_name.and_then(screenshot_basename) {
        response = response.with_header(header(
            "Content-Disposition",
            &format!("attachment; filename=\"{}\"", name.replace('"', "")),
        )?);
    }
    request.respond(response).map_err(|err| anyhow!("{err}"))
}

fn header(name: &str, value: &str) -> Result<Header> {
    Header::from_bytes(name.as_bytes(), value.as_bytes())
        .map_err(|_| anyhow!("invalid header {name}: {value}"))
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_gateway_prefix() {
        assert_eq!(normalize_path("/portal/api/health?x=1"), "/api/health");
        assert_eq!(normalize_path("/portal/"), "/");
        assert_eq!(normalize_path("/api/health"), "/api/health");
        assert!(query_flag("/api/reports?anonymize=1", "anonymize"));
        assert!(query_flag("/api/reports?anonymize=true", "anonymize"));
        assert!(!query_flag("/api/reports?anonymize=0", "anonymize"));
    }

    #[test]
    fn links_are_gateway_relative() {
        let links = links();
        assert!(links.detmir_activitywatch.starts_with("/d/"));
        assert!(
            links
                .worktime_report
                .starts_with("/reports/worktime/management?")
        );
        assert!(links.worktime_report.contains("format=html"));
    }

    #[test]
    fn collection_block_detects_green_summary() {
        let value = json!({"summary":{"bucket_stale":0,"bucket_dead":0,"service_failures":0}});
        let block = collection_block(Some(&value));
        assert_eq!(block.status, "OK");
    }

    #[test]
    fn agent_quality_status_prioritizes_collector_risk() {
        assert_eq!(agent_quality_status("wts_api", None), "ok");
        assert_eq!(agent_quality_status("quser_utf16", None), "fallback");
        assert_eq!(agent_quality_status("quser_lossy", None), "fallback");
        assert_eq!(
            agent_quality_status("env_sessionname_fallback", None),
            "fallback"
        );
        assert_eq!(agent_quality_status("local_fallback", None), "degraded");
        assert_eq!(agent_quality_status("unknown", None), "unknown");
        assert_eq!(
            agent_quality_status("wts_api", Some("temporary query failure")),
            "degraded"
        );
        assert_eq!(
            agent_quality_status("wts_api", Some("access denied by WTS API")),
            "error"
        );
    }

    #[test]
    fn agent_quality_defaults_to_unknown_for_old_payloads() {
        let quality = agent_quality_from_record(&json!({
            "agent_id": "agent-legacy",
            "hostname": "HOST-EXAMPLE"
        }));
        assert_eq!(quality.quality_status, "unknown");
        assert_eq!(quality.collector_source, "unknown");
        assert_eq!(quality.sessions_collected_total, 0);
        let explain = agent_quality_explain(&quality);
        assert_eq!(explain.status, "UNKNOWN");
        assert!(!explain.kpi_accepted);
        assert!(explain.summary.contains("не передал диагностику"));
    }

    #[test]
    fn agent_quality_loads_latest_jsonl_diagnostics() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("telemetry.jsonl");
        fs::write(
            &path,
            r#"{"record":{"agent_id":"old","diagnostics":{"collector_source":"local_fallback","sessions_collected_total":1,"active_sessions_total":1,"rdp_sessions_total":0}}}
{"record":{"agent_id":"new","diagnostics":{"collector_source":"wts_api","sessions_collected_total":3,"active_sessions_total":2,"rdp_sessions_total":2}}}
"#,
        )
        .unwrap();
        let quality = load_agent_quality(&path);
        assert_eq!(quality.quality_status, "ok");
        assert_eq!(quality.collector_source, "wts_api");
        assert_eq!(quality.sessions_collected_total, 3);
        assert_eq!(quality.active_sessions_total, 2);
        assert_eq!(quality.rdp_sessions_total, 2);
        let explain = agent_quality_explain(&quality);
        assert_eq!(explain.status, "OK");
        assert!(explain.kpi_accepted);
        assert!(explain.summary.contains("WTS API"));
    }

    #[test]
    fn agent_quality_local_fallback_is_degraded_and_not_accepted_for_kpi() {
        let quality = agent_quality_from_record(&json!({
            "diagnostics": {
                "collector_source": "local_fallback",
                "sessions_collected_total": 1,
                "active_sessions_total": 1,
                "rdp_sessions_total": 0
            }
        }));
        assert_eq!(quality.quality_status, "degraded");
        let explain = agent_quality_explain(&quality);
        assert_eq!(explain.status, "DEGRADED");
        assert!(!explain.kpi_accepted);
        assert_eq!(
            explain.summary,
            "Диагностический режим, данные не засчитываются в KPI."
        );
    }

    #[test]
    fn agent_quality_collector_error_is_visible_in_explain_and_markdown() {
        let quality = agent_quality_from_record(&json!({
            "diagnostics": {
                "collector_source": "wts_api",
                "collector_error": "temporary WTS failure",
                "sessions_collected_total": 0,
                "active_sessions_total": 0,
                "rdp_sessions_total": 0
            }
        }));
        assert_eq!(quality.quality_status, "degraded");
        let explain = agent_quality_explain(&quality);
        assert_eq!(explain.status, "DEGRADED");
        assert!(!explain.kpi_accepted);
        assert!(explain.summary.contains("temporary WTS failure"));

        let mut markdown = String::new();
        append_agent_quality_markdown(&mut markdown, &quality);
        assert!(markdown.contains("## Достоверность данных"));
        assert!(markdown.contains("Принято в KPI: нет"));
        assert!(markdown.contains("Ошибка коллектора: temporary WTS failure"));
    }

    fn telemetry_line(date: &str, source: Option<&str>, error: Option<&str>) -> String {
        let diagnostics = source.map(|collector_source| {
            json!({
                "collector_source": collector_source,
                "collector_error": error,
                "sessions_collected_total": 3,
                "active_sessions_total": 2,
                "rdp_sessions_total": 1
            })
        });
        serde_json::to_string(&json!({
            "record": {
                "agent_id": "agent-1",
                "hostname": "HOST-EXAMPLE",
                "timestamp": format!("{date}T10:00:00Z"),
                "diagnostics": diagnostics
            }
        }))
        .unwrap()
    }

    fn write_history(lines: Vec<String>) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("telemetry.jsonl");
        fs::write(&path, lines.join("\n")).unwrap();
        (dir, path)
    }

    #[test]
    fn agent_quality_history_counts_seven_ok_days() {
        let (_dir, path) = write_history(
            (1..=7)
                .map(|day| telemetry_line(&format!("2026-06-0{day}"), Some("wts_api"), None))
                .collect(),
        );
        let today = NaiveDate::from_ymd_opt(2026, 6, 7).unwrap();
        let history = load_agent_quality_history_for_date(&path, 7, today);
        let summary = summarize_agent_quality_history(&history);
        assert_eq!(history.len(), 7);
        assert_eq!(summary.ok_days, 7);
        assert_eq!(summary.degraded_days, 0);
        assert_eq!(summary.kpi_accepted_pct, 100);
    }

    #[test]
    fn agent_quality_history_counts_mixed_statuses() {
        let (_dir, path) = write_history(vec![
            telemetry_line("2026-06-01", Some("wts_api"), None),
            telemetry_line("2026-06-02", Some("local_fallback"), None),
            telemetry_line("2026-06-03", Some("quser_utf16"), None),
            telemetry_line("2026-06-04", None, None),
        ]);
        let today = NaiveDate::from_ymd_opt(2026, 6, 4).unwrap();
        let history = load_agent_quality_history_for_date(&path, 7, today);
        let summary = summarize_agent_quality_history(&history);
        assert_eq!(summary.days_observed, 4);
        assert_eq!(summary.ok_days, 1);
        assert_eq!(summary.warning_days, 1);
        assert_eq!(summary.degraded_days, 1);
        assert_eq!(summary.unknown_days, 1);
        assert_eq!(summary.kpi_accepted_days, 2);
        assert_eq!(summary.kpi_accepted_pct, 50);
    }

    #[test]
    fn agent_quality_history_handles_absent_history() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.jsonl");
        let today = NaiveDate::from_ymd_opt(2026, 6, 4).unwrap();
        let history = load_agent_quality_history_for_date(&path, 7, today);
        let summary = summarize_agent_quality_history(&history);
        assert!(history.is_empty());
        assert_eq!(summary.days_observed, 0);
        assert_eq!(summary.kpi_accepted_pct, 0);
    }

    #[test]
    fn agent_quality_history_preserves_collector_errors() {
        let (_dir, path) = write_history(vec![telemetry_line(
            "2026-06-04",
            Some("wts_api"),
            Some("collector timeout"),
        )]);
        let today = NaiveDate::from_ymd_opt(2026, 6, 4).unwrap();
        let history = load_agent_quality_history_for_date(&path, 7, today);
        let summary = summarize_agent_quality_history(&history);
        assert_eq!(history[0].status, "DEGRADED");
        assert_eq!(
            history[0].collector_error.as_deref(),
            Some("collector timeout")
        );
        assert_eq!(summary.degraded_days, 1);
        assert_eq!(summary.kpi_accepted_days, 0);
    }

    fn telemetry_node_line(
        date: &str,
        hostname: Option<&str>,
        machine_id: Option<&str>,
        source: Option<&str>,
        error: Option<&str>,
    ) -> String {
        let diagnostics = source.map(|collector_source| {
            json!({
                "collector_source": collector_source,
                "collector_error": error,
                "sessions_collected_total": 4,
                "active_sessions_total": 3,
                "rdp_sessions_total": 2
            })
        });
        serde_json::to_string(&json!({
            "record": {
                "agent_id": "agent-node",
                "hostname": hostname,
                "machine_id": machine_id,
                "timestamp": format!("{date}T10:00:00Z"),
                "diagnostics": diagnostics
            }
        }))
        .unwrap()
    }

    #[test]
    fn agent_quality_nodes_counts_single_ok_node() {
        let (_dir, path) = write_history(vec![telemetry_node_line(
            "2026-06-04",
            Some("HOST-EXAMPLE-1"),
            None,
            Some("wts_api"),
            None,
        )]);
        let today = NaiveDate::from_ymd_opt(2026, 6, 4).unwrap();
        let nodes = load_agent_quality_nodes_for_date(&path, 7, today);
        let summary = summarize_agent_quality_nodes(&nodes);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].hostname, "HOST-EXAMPLE-1");
        assert_eq!(nodes[0].status, "OK");
        assert!(nodes[0].kpi_accepted);
        assert_eq!(summary.total_nodes, 1);
        assert_eq!(summary.ok_nodes, 1);
        assert_eq!(summary.accepted_kpi_nodes_pct, 100);
    }

    #[test]
    fn agent_quality_nodes_count_mixed_ok_degraded_unknown() {
        let (_dir, path) = write_history(vec![
            telemetry_node_line(
                "2026-06-04",
                Some("HOST-EXAMPLE-OK"),
                None,
                Some("wts_api"),
                None,
            ),
            telemetry_node_line(
                "2026-06-04",
                Some("HOST-EXAMPLE-FALLBACK"),
                None,
                Some("quser_utf16"),
                None,
            ),
            telemetry_node_line(
                "2026-06-04",
                Some("HOST-EXAMPLE-DEGRADED"),
                None,
                Some("local_fallback"),
                None,
            ),
            telemetry_node_line("2026-06-04", Some("HOST-EXAMPLE-UNKNOWN"), None, None, None),
        ]);
        let today = NaiveDate::from_ymd_opt(2026, 6, 4).unwrap();
        let nodes = load_agent_quality_nodes_for_date(&path, 7, today);
        let summary = summarize_agent_quality_nodes(&nodes);
        assert_eq!(summary.total_nodes, 4);
        assert_eq!(summary.ok_nodes, 1);
        assert_eq!(summary.degraded_nodes, 2);
        assert_eq!(summary.unknown_nodes, 1);
        assert_eq!(summary.accepted_kpi_nodes_pct, 50);
    }

    #[test]
    fn agent_quality_nodes_local_fallback_is_excluded_from_kpi() {
        let (_dir, path) = write_history(vec![telemetry_node_line(
            "2026-06-04",
            Some("HOST-EXAMPLE-LOCAL"),
            None,
            Some("local_fallback"),
            None,
        )]);
        let today = NaiveDate::from_ymd_opt(2026, 6, 4).unwrap();
        let nodes = load_agent_quality_nodes_for_date(&path, 7, today);
        let summary = summarize_agent_quality_nodes(&nodes);
        assert_eq!(nodes[0].status, "DEGRADED");
        assert!(!nodes[0].kpi_accepted);
        assert!(nodes[0].recommendation.contains("WTS API"));
        assert_eq!(summary.accepted_kpi_nodes_pct, 0);
    }

    #[test]
    fn agent_quality_nodes_collector_error_is_visible() {
        let (_dir, path) = write_history(vec![telemetry_node_line(
            "2026-06-04",
            Some("HOST-EXAMPLE-ERR"),
            None,
            Some("wts_api"),
            Some("collector timeout"),
        )]);
        let today = NaiveDate::from_ymd_opt(2026, 6, 4).unwrap();
        let nodes = load_agent_quality_nodes_for_date(&path, 7, today);
        let summary = summarize_agent_quality_nodes(&nodes);
        assert_eq!(nodes[0].status, "DEGRADED");
        assert_eq!(
            nodes[0].collector_error.as_deref(),
            Some("collector timeout")
        );
        assert!(nodes[0].recommendation.contains("журнал агента"));
        assert_eq!(summary.degraded_nodes, 1);
    }

    #[test]
    fn agent_quality_nodes_handle_empty_history() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.jsonl");
        let today = NaiveDate::from_ymd_opt(2026, 6, 4).unwrap();
        let nodes = load_agent_quality_nodes_for_date(&path, 7, today);
        let summary = summarize_agent_quality_nodes(&nodes);
        assert!(nodes.is_empty());
        assert_eq!(summary.total_nodes, 0);
        assert_eq!(summary.accepted_kpi_nodes_pct, 0);
    }

    #[test]
    fn agent_quality_nodes_use_machine_id_or_unknown_for_old_payloads() {
        let (_dir, path) = write_history(vec![
            telemetry_node_line("2026-06-04", None, Some("MACHINE-EXAMPLE-1"), None, None),
            telemetry_node_line("2026-06-04", None, None, None, None),
        ]);
        let today = NaiveDate::from_ymd_opt(2026, 6, 4).unwrap();
        let nodes = load_agent_quality_nodes_for_date(&path, 7, today);
        let summary = summarize_agent_quality_nodes(&nodes);
        let names = nodes
            .iter()
            .map(|item| item.hostname.as_str())
            .collect::<Vec<_>>();
        assert!(names.contains(&"MACHINE-EXAMPLE-1"));
        assert!(names.contains(&"unknown"));
        assert!(nodes.iter().all(|item| item.status == "UNKNOWN"));
        assert!(nodes.iter().all(|item| !item.kpi_accepted));
        assert_eq!(summary.unknown_nodes, 2);
    }

    fn fixed_sla_now() -> chrono::DateTime<Utc> {
        chrono::DateTime::parse_from_rfc3339("2026-06-04T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn expected_node(hostname: &str) -> ExpectedNode {
        ExpectedNode {
            hostname: hostname.to_string(),
            department: "Подразделение".to_string(),
            owner: "Ответственный".to_string(),
            criticality: "normal".to_string(),
        }
    }

    fn quality_node(
        hostname: &str,
        last_seen_utc: &str,
        status: &str,
        kpi_accepted: bool,
    ) -> AgentQualityNodeItem {
        AgentQualityNodeItem {
            hostname: hostname.to_string(),
            last_seen_utc: last_seen_utc.to_string(),
            source: if kpi_accepted {
                "wts_api".to_string()
            } else {
                "local_fallback".to_string()
            },
            status: status.to_string(),
            kpi_accepted,
            sessions_total: 2,
            rdp_sessions: 1,
            collector_error: None,
            recommendation: "test".to_string(),
        }
    }

    #[test]
    fn agent_coverage_sla_is_unknown_without_expected_nodes() {
        let dir = tempfile::tempdir().unwrap();
        let sla = build_agent_coverage_sla(
            &dir.path().join("missing-expected-nodes.json"),
            &[quality_node(
                "HOST-EXAMPLE-1",
                "2026-06-04T11:00:00Z",
                "OK",
                true,
            )],
            fixed_sla_now(),
        );
        assert_eq!(sla.sla_status, "UNKNOWN");
        assert_eq!(sla.expected_nodes, 0);
        assert_eq!(sla.coverage_pct, 0);
    }

    #[test]
    fn agent_coverage_sla_counts_full_coverage() {
        let expected = vec![
            expected_node("HOST-EXAMPLE-1"),
            expected_node("HOST-EXAMPLE-2"),
        ];
        let nodes = vec![
            quality_node("HOST-EXAMPLE-1", "2026-06-04T11:00:00Z", "OK", true),
            quality_node("HOST-EXAMPLE-2", "2026-06-04T10:00:00Z", "OK", true),
        ];
        let sla = agent_coverage_sla_from_expected(&expected, &nodes, fixed_sla_now());
        assert_eq!(sla.sla_status, "OK");
        assert_eq!(sla.reporting_nodes_24h, 2);
        assert_eq!(sla.coverage_pct, 100);
        assert_eq!(sla.freshness_pct, 100);
        assert!(sla.problem_nodes.is_empty());
    }

    #[test]
    fn agent_coverage_sla_warns_at_eighty_percent() {
        let expected = (1..=5)
            .map(|index| expected_node(&format!("HOST-EXAMPLE-{index}")))
            .collect::<Vec<_>>();
        let nodes = (1..=4)
            .map(|index| {
                quality_node(
                    &format!("HOST-EXAMPLE-{index}"),
                    "2026-06-04T11:00:00Z",
                    "OK",
                    true,
                )
            })
            .collect::<Vec<_>>();
        let sla = agent_coverage_sla_from_expected(&expected, &nodes, fixed_sla_now());
        assert_eq!(sla.sla_status, "WARNING");
        assert_eq!(sla.coverage_pct, 80);
        assert_eq!(sla.missing_nodes, 1);
        assert_eq!(sla.problem_nodes[0].status, "MISSING");
    }

    #[test]
    fn agent_coverage_sla_is_critical_at_sixty_percent() {
        let expected = (1..=5)
            .map(|index| expected_node(&format!("HOST-EXAMPLE-{index}")))
            .collect::<Vec<_>>();
        let nodes = (1..=3)
            .map(|index| {
                quality_node(
                    &format!("HOST-EXAMPLE-{index}"),
                    "2026-06-04T11:00:00Z",
                    "OK",
                    true,
                )
            })
            .collect::<Vec<_>>();
        let sla = agent_coverage_sla_from_expected(&expected, &nodes, fixed_sla_now());
        assert_eq!(sla.sla_status, "CRITICAL");
        assert_eq!(sla.coverage_pct, 60);
        assert_eq!(sla.missing_nodes, 2);
    }

    #[test]
    fn agent_coverage_sla_reports_stale_nodes() {
        let expected = vec![
            expected_node("HOST-EXAMPLE-1"),
            expected_node("HOST-EXAMPLE-2"),
        ];
        let nodes = vec![
            quality_node("HOST-EXAMPLE-1", "2026-06-04T11:00:00Z", "OK", true),
            quality_node("HOST-EXAMPLE-2", "2026-06-03T10:00:00Z", "OK", true),
        ];
        let sla = agent_coverage_sla_from_expected(&expected, &nodes, fixed_sla_now());
        assert_eq!(sla.stale_nodes, 1);
        assert_eq!(sla.missing_nodes, 0);
        assert_eq!(sla.coverage_pct, 50);
        assert!(sla.problem_nodes.iter().any(|item| item.status == "STALE"));
    }

    #[test]
    fn agent_coverage_sla_reports_missing_nodes() {
        let expected = vec![
            expected_node("HOST-EXAMPLE-1"),
            expected_node("HOST-EXAMPLE-2"),
        ];
        let nodes = vec![quality_node(
            "HOST-EXAMPLE-1",
            "2026-06-04T11:00:00Z",
            "OK",
            true,
        )];
        let sla = agent_coverage_sla_from_expected(&expected, &nodes, fixed_sla_now());
        assert_eq!(sla.missing_nodes, 1);
        assert_eq!(sla.problem_nodes[0].hostname, "HOST-EXAMPLE-2");
        assert_eq!(sla.problem_nodes[0].status, "MISSING");
    }

    #[test]
    fn agent_coverage_sla_excludes_local_fallback_from_confirmed_kpi() {
        let expected = vec![expected_node("HOST-EXAMPLE-1")];
        let nodes = vec![quality_node(
            "HOST-EXAMPLE-1",
            "2026-06-04T11:00:00Z",
            "DEGRADED",
            false,
        )];
        let sla = agent_coverage_sla_from_expected(&expected, &nodes, fixed_sla_now());
        assert_eq!(sla.freshness_pct, 100);
        assert_eq!(sla.coverage_pct, 0);
        assert_eq!(sla.reporting_nodes_24h, 0);
        assert_eq!(sla.problem_nodes[0].status, "DEGRADED");
    }

    #[test]
    fn incident_id_is_stable() {
        assert_eq!(
            incident_id("service", "grafana-data", "stale"),
            incident_id("service", "grafana-data", "stale")
        );
        assert_ne!(
            incident_id("service", "grafana-data", "stale"),
            incident_id("service", "grafana-data", "fresh")
        );
    }

    #[test]
    fn incident_item_applies_ack_state() {
        let id = incident_id("service", "grafana-data", "stale");
        let mut state = IncidentStateFile::default();
        state.incidents.insert(
            id.clone(),
            IncidentActionState {
                state: "acknowledged".to_string(),
                actor: "detmir".to_string(),
                updated_at_utc: "2026-06-02T18:00:00Z".to_string(),
                acknowledged_at_utc: Some("2026-06-02T18:00:00Z".to_string()),
                assigned_to: Some("operator".to_string()),
                comment: Some("checking".to_string()),
            },
        );
        let item = incident_item(
            "FAIL",
            "service",
            "grafana-data",
            "stale",
            "2026-06-02T18:01:00Z",
            "/portal/operator",
            &state,
        );
        assert_eq!(item.id, id);
        assert!(item.acknowledged);
        assert_eq!(item.actor.as_deref(), Some("detmir"));
        assert_eq!(item.assigned_to.as_deref(), Some("operator"));
    }

    #[test]
    fn evidence_id_is_stable_and_parseable() {
        let id = evidence_id(42, "event-1", Some(r"C:\tmp\shot.png"));
        assert_eq!(id, evidence_id(42, "event-1", Some(r"C:\tmp\shot.png")));
        assert_ne!(id, evidence_id(42, "event-2", Some(r"C:\tmp\shot.png")));
        assert_eq!(evidence_row_id(&id).unwrap(), 42);
    }

    #[test]
    fn screenshot_basename_rejects_traversal() {
        assert_eq!(
            screenshot_basename(r"C:\Users\operator\shot-1.png").as_deref(),
            Some("shot-1.png")
        );
        assert_eq!(screenshot_basename("../secret.png"), None);
        assert_eq!(screenshot_basename("..\\secret.png"), None);
    }

    #[test]
    fn screenshot_resolution_requires_matching_hash_inside_root() {
        let dir = tempfile::tempdir().unwrap();
        let screenshots = dir.path().join("screenshots");
        fs::create_dir_all(&screenshots).unwrap();
        let data = b"not a real png, but content type is extension-bound";
        let digest = {
            let mut hasher = Sha256::new();
            hasher.update(data);
            format!("{:x}", hasher.finalize())
        };
        fs::write(screenshots.join(format!("{digest}.png")), data).unwrap();
        let args = Cli {
            bind: "127.0.0.1:0".to_string(),
            status_cmd: "true".to_string(),
            check_cmd: "true".to_string(),
            failed_units_cmd: "true".to_string(),
            worktime_url: "http://127.0.0.1".to_string(),
            one_c_url: "http://127.0.0.1".to_string(),
            workforce_policy_path: dir.path().join("workforce-policy.json"),
            ueba_policy_path: dir.path().join("ueba-policy.yaml"),
            timeout_seconds: 1,
            state_dir: dir.path().join("state"),
            dlp_db_path: dir.path().join("dlp.sqlite"),
            evidence_root: dir.path().to_path_buf(),
            readiness_bundle_dir: dir.path().join("readiness-bundle"),
            evidence_limit: 10,
            evidence_max_bytes: 1024,
            json_smoke: false,
            evidence_only: false,
            evidence_upload_token: None,
            telemetry_api_key: "test-key".to_string(),
            telemetry_store_path: dir.path().join("telemetry.jsonl"),
            expected_nodes_path: dir.path().join("expected_nodes.json"),
        };
        let found = resolve_screenshot_file(&args, &None, &Some(digest.clone()))
            .unwrap()
            .unwrap();
        assert_eq!(found.content_type, "image/png");
        assert_eq!(found.sha256.as_deref(), Some(digest.as_str()));
        assert!(
            resolve_screenshot_file(&args, &None, &Some("0".repeat(64)))
                .unwrap()
                .is_none()
        );
        assert!(
            resolve_screenshot_file(&args, &None, &None)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn evidence_image_type_checks_magic_and_claim() {
        let png = b"\x89PNG\r\n\x1a\nrest";
        assert_eq!(
            evidence_image_type(png, Some("image/png")).unwrap(),
            ("image/png", "png")
        );
        assert!(evidence_image_type(png, Some("image/jpeg")).is_err());
        let jpg = &[0xff, 0xd8, 0xff, 0xe0, 0x00];
        assert_eq!(
            evidence_image_type(jpg, Some("application/octet-stream")).unwrap(),
            ("image/jpeg", "jpg")
        );
        assert!(evidence_image_type(b"plain text", None).is_err());
    }

    #[test]
    fn constant_time_eq_requires_same_bytes() {
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"other"));
        assert!(!constant_time_eq(b"secret", b"secret2"));
    }

    #[test]
    fn telemetry_ingest_validates_and_appends_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let args = Cli {
            bind: "127.0.0.1:0".to_string(),
            status_cmd: "true".to_string(),
            check_cmd: "true".to_string(),
            failed_units_cmd: "true".to_string(),
            worktime_url: "http://127.0.0.1".to_string(),
            one_c_url: "http://127.0.0.1".to_string(),
            workforce_policy_path: dir.path().join("workforce-policy.json"),
            ueba_policy_path: dir.path().join("ueba-policy.yaml"),
            timeout_seconds: 1,
            state_dir: dir.path().join("state"),
            dlp_db_path: dir.path().join("dlp.sqlite"),
            evidence_root: dir.path().to_path_buf(),
            readiness_bundle_dir: dir.path().join("readiness-bundle"),
            evidence_limit: 10,
            evidence_max_bytes: 1024,
            json_smoke: false,
            evidence_only: false,
            evidence_upload_token: None,
            telemetry_api_key: "test-key".to_string(),
            telemetry_store_path: dir.path().join("telemetry/telemetry.jsonl"),
            expected_nodes_path: dir.path().join("expected_nodes.json"),
        };
        let payload = json!({
            "agent_id": "agent-1",
            "hostname": "HOST-EXAMPLE",
            "os_name": "Linux",
            "os_version": "test",
            "platform": "linux",
            "username": "user",
            "domain": "",
            "timestamp": "2026-06-04T00:00:00Z",
            "uptime_seconds": 1,
            "cpu_usage_percent": 0.0,
            "memory_total": 1,
            "memory_used": 1,
            "active_sessions": [],
            "rdp_sessions": [],
            "ssh_sessions": [],
            "processes": [],
            "network_interfaces": [],
            "network_connections": [],
            "workforce_activity": {"active_today": true, "explanation": []},
            "security_events": [],
            "collector_version": "0.3.0"
        });
        let response = apply_telemetry_ingest(&args, &serde_json::to_string(&payload).unwrap())
            .expect("valid telemetry should be accepted");
        assert_eq!(response["ok"], true);
        assert_eq!(response["stored"], "file-backed-jsonl");
        let stored = fs::read_to_string(&args.telemetry_store_path).unwrap();
        assert!(stored.contains("\"prototype\":true"));
        assert!(stored.contains("HOST-EXAMPLE"));
        assert!(apply_telemetry_ingest(&args, r#"{"agent_id":"only"}"#).is_err());
    }

    #[test]
    fn human_duration_formats_hhmm() {
        assert_eq!(human_duration(0), "00:00");
        assert_eq!(human_duration(3660), "01:01");
        assert_eq!(human_duration(-10), "00:00");
    }

    #[test]
    fn workforce_index_uses_eight_hour_proxy() {
        assert_eq!(workforce_index(1, 8 * 3600), Some(100));
        assert_eq!(workforce_index(2, 8 * 3600), Some(50));
        assert_eq!(workforce_index(0, 8 * 3600), None);
        assert_eq!(workforce_index(1, 0), None);
        assert_eq!(workforce_index_text(Some(84)), "84%");
        assert_eq!(workforce_index_status(Some(84)), "OK");
        assert_eq!(workforce_index_status(Some(70)), "WARN");
        assert_eq!(workforce_index_status(Some(30)), "FAIL");
    }

    #[test]
    fn reports_include_commercial_kpis_and_disclaimer() {
        let snapshot = Snapshot {
            generated_at_utc: "2026-06-03T10:00:00Z".to_string(),
            detmir_status: SourceStatus {
                ok: true,
                status: "OK".to_string(),
                summary: "severity=OK, operator_ok=true".to_string(),
                error: None,
                payload: Some(json!({
                    "severity": "OK",
                    "ok_for_operator": true,
                    "dlp_ok": true,
                    "dlp_counts": {"ok": 22, "warn": 0, "fail": 0}
                })),
            },
            detmir_check: SourceStatus {
                ok: true,
                status: "OK".to_string(),
                summary: "bucket_ok=8, stale=0, dead=0, service_fail=0".to_string(),
                error: None,
                payload: Some(json!({
                    "summary": {
                        "bucket_ok": 8,
                        "bucket_stale": 0,
                        "bucket_dead": 0,
                        "service_failures": 0
                    },
                    "services": [
                        {"name": "grafana-data", "ok": true, "payload": {"age_seconds": 60, "fail_count": 0}}
                    ]
                })),
            },
            failed_units: SourceStatus {
                ok: true,
                status: "OK".to_string(),
                summary: "failed units not reported".to_string(),
                error: None,
                payload: Some(json!({"stdout": "0 loaded units listed"})),
            },
            worktime: SourceStatus {
                ok: true,
                status: "OK".to_string(),
                summary: "rows=2, apps=1".to_string(),
                error: None,
                payload: Some(json!({
                    "rows": [
                        {"user": "USER-1", "active_seconds": 3600},
                        {"user": "USER-2", "active_seconds": 1800}
                    ],
                    "true_active_apps": [
                        {"application": "ERP", "proved_work_human": "00:30", "proved_work_seconds": 3600},
                        {"application": "Browser", "proved_work_human": "00:30", "proved_work_seconds": 3600}
                    ]
                })),
            },
            worktime_management: SourceStatus {
                ok: true,
                status: "OK".to_string(),
                summary: "coverage=50%, departments=1, owners=1".to_string(),
                error: None,
                payload: Some(json!({
                    "summary": {
                        "portfolio_coverage_pct": 50.0
                    },
                    "department_rollups": [
                        {
                            "name": "Бухгалтерия",
                            "users_count": 2,
                            "active_users": 1,
                            "portfolio_coverage_pct": 50.0,
                            "workday_total_active_hhmm": "04:00"
                        }
                    ],
                    "owner_rollups": [
                        {
                            "name": "Ответственный",
                            "users_count": 1,
                            "active_users": 1,
                            "portfolio_coverage_pct": 80.0,
                            "workday_total_active_hhmm": "06:24"
                        }
                    ],
                    "trend": [
                        {
                            "report_date": "2026-06-03",
                            "portfolio_coverage_pct": 50.0
                        }
                    ],
                    "trend_insights": [
                        {
                            "code": "history_insufficient",
                            "severity": "INFO",
                            "scope": "portfolio",
                            "subject": "Workforce",
                            "title": "История еще накапливается",
                            "evidence": "Накоплено 1 daily point.",
                            "recommendation": "Использовать текущий дневной срез."
                        }
                    ]
                })),
            },
            one_c: SourceStatus {
                ok: true,
                status: "OK".to_string(),
                summary: "status=ok, companies=47".to_string(),
                error: None,
                payload: Some(json!({"status": "ok", "companies_total": 47})),
            },
            agent_quality: AgentQuality::default(),
            agent_quality_history: Vec::new(),
            agent_quality_history_summary: AgentQualityHistorySummary::default(),
            agent_quality_nodes: vec![
                AgentQualityNodeItem {
                    hostname: "HOST-EXAMPLE-OK".to_string(),
                    last_seen_utc: "2026-06-03T10:00:00Z".to_string(),
                    source: "wts_api".to_string(),
                    status: "OK".to_string(),
                    kpi_accepted: true,
                    sessions_total: 3,
                    rdp_sessions: 1,
                    collector_error: None,
                    recommendation: "Действий не требуется.".to_string(),
                },
                AgentQualityNodeItem {
                    hostname: "HOST-EXAMPLE-DEGRADED".to_string(),
                    last_seen_utc: "2026-06-03T10:00:00Z".to_string(),
                    source: "local_fallback".to_string(),
                    status: "DEGRADED".to_string(),
                    kpi_accepted: false,
                    sessions_total: 1,
                    rdp_sessions: 0,
                    collector_error: None,
                    recommendation: "Проверить основной сбор WTS API.".to_string(),
                },
            ],
            agent_quality_nodes_summary: AgentQualityNodesSummary {
                total_nodes: 2,
                ok_nodes: 1,
                degraded_nodes: 1,
                unknown_nodes: 0,
                accepted_kpi_nodes_pct: 50,
            },
            agent_coverage_sla: AgentCoverageSla::default(),
        };
        let evidence = DlpEvidenceResponse {
            ok: true,
            generated_at_utc: "2026-06-03T10:00:00Z".to_string(),
            db_available: true,
            screenshot_root_available: true,
            limit: 10,
            items: vec![DlpEvidenceItem {
                id: "ev-1-0000000000000000".to_string(),
                event_ts: "2026-06-03T10:00:00Z".to_string(),
                bucket_id: "aw-dlp-incidents_HOST-EXAMPLE".to_string(),
                event_id: "event-1".to_string(),
                stream_type: "dlp_incident".to_string(),
                hostname: "HOST-EXAMPLE".to_string(),
                username: None,
                severity: Some("medium".to_string()),
                signal_type: Some("clipboard".to_string()),
                rule_id: None,
                action: None,
                source: None,
                message: None,
                file_path: None,
                has_screenshot_metadata: true,
                screenshot_available: true,
                source_file: Some("shot.png".to_string()),
                screenshot_sha256: Some("0".repeat(64)),
                screenshot_width: None,
                screenshot_height: None,
                preview_url: None,
                download_url: None,
                blocked_reason: None,
            }],
            error: None,
        };
        let dir = tempfile::tempdir().unwrap();
        let missing_policy = dir.path().join("detmir-missing-workforce-policy.json");
        let missing_ueba_policy = dir.path().join("detmir-missing-ueba-policy.yaml");
        let baseline_path = dir.path().join("ueba-baseline-state.json");
        let report = build_reports(
            &snapshot,
            &IncidentStateFile::default(),
            &evidence,
            &missing_policy,
            &missing_ueba_policy,
            &baseline_path,
            false,
        );
        assert_eq!(report["operator_ok"], true);
        assert_eq!(report["severity"], "OK");
        assert!(
            report["markdown"]
                .as_str()
                .unwrap()
                .contains("derived detections/cases")
        );
        assert!(report["kpis"].as_array().unwrap().len() >= 6);
        assert_eq!(report["ueba_risk"]["score"], 0);
        assert!(
            report["ueba_risk"]["reasons"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(report["ueba_risk"]["confidence"].as_f64().unwrap() > 0.55);
        assert_eq!(
            report["ueba_risk"]["baseline_status"],
            "per_user_department_baseline_skeleton"
        );
        assert_eq!(report["ueba_risk"]["baseline_window_days"], 30);
        assert_eq!(report["ueba_risk"]["user_baseline_available"], false);
        assert_eq!(report["ueba_risk"]["department_baseline_available"], false);
        assert_eq!(report["ueba_risk"]["deviation_score"], 0);
        assert!(report["ueba_risk"]["baseline_samples"].is_object());
        assert_eq!(report["ueba_risk"]["policy_version"], "ueba-rule-v1");
        assert!(report["ueba_risk"]["calculated_from"].is_array());
        assert!(
            report["markdown"]
                .as_str()
                .unwrap()
                .contains("## UEBA риск")
        );
        assert_eq!(report["workforce_policy"]["configured"], false);
        assert_eq!(report["workforce"]["trend_status"], "daily_only");
        assert_eq!(report["workforce"]["insights"].as_array().unwrap().len(), 1);
        assert_eq!(report["agent_quality"]["quality_status"], "unknown");
        assert!(report["agent_quality_nodes"].is_array());
        assert!(report["agent_quality_nodes_summary"].is_object());
        assert_eq!(
            report["agent_quality_nodes_summary"]["accepted_kpi_nodes_pct"],
            50
        );
        assert_eq!(report["agent_coverage_sla"]["sla_status"], "UNKNOWN");
        assert!(
            report["executive_points"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item.as_str().unwrap().contains("менее 80% узлов"))
        );
        assert!(
            report["markdown"]
                .as_str()
                .unwrap()
                .contains("## Качество данных по узлам")
        );
        assert!(
            report["markdown"]
                .as_str()
                .unwrap()
                .contains("## SLA покрытия агентов")
        );
        assert!(
            report["markdown"]
                .as_str()
                .unwrap()
                .contains("Качество данных агента")
        );
        assert_eq!(
            report["workforce"]["department_comparison"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn ueba_risk_uses_yaml_policy_and_evidence_as_confidence() {
        let dir = tempfile::tempdir().unwrap();
        let policy_path = dir.path().join("ueba-policy.yaml");
        fs::write(
            &policy_path,
            r#"
version: "ueba-rule-v1-test"
baseline_status: "test_baseline"
score_cap: 50
weights:
  dlp_warn: 7
confidence:
  base: 0.2
  evidence_bonus: 0.3
  screenshot_bonus: 0.2
  worktime_bonus: 0.1
  policy_bonus: 0.1
"#,
        )
        .unwrap();
        let snapshot = Snapshot {
            generated_at_utc: "2026-06-03T10:00:00Z".to_string(),
            detmir_status: SourceStatus {
                ok: true,
                status: "WARN".to_string(),
                summary: "".to_string(),
                error: None,
                payload: None,
            },
            detmir_check: SourceStatus {
                ok: true,
                status: "OK".to_string(),
                summary: "".to_string(),
                error: None,
                payload: None,
            },
            failed_units: SourceStatus {
                ok: true,
                status: "OK".to_string(),
                summary: "".to_string(),
                error: None,
                payload: None,
            },
            worktime: SourceStatus {
                ok: true,
                status: "OK".to_string(),
                summary: "".to_string(),
                error: None,
                payload: None,
            },
            worktime_management: SourceStatus {
                ok: true,
                status: "OK".to_string(),
                summary: "".to_string(),
                error: None,
                payload: None,
            },
            one_c: SourceStatus {
                ok: true,
                status: "OK".to_string(),
                summary: "".to_string(),
                error: None,
                payload: None,
            },
            agent_quality: AgentQuality::default(),
            agent_quality_history: Vec::new(),
            agent_quality_history_summary: AgentQualityHistorySummary::default(),
            agent_quality_nodes: Vec::new(),
            agent_quality_nodes_summary: AgentQualityNodesSummary::default(),
            agent_coverage_sla: AgentCoverageSla::default(),
        };
        let metrics = ReportMetrics {
            users_count: 1,
            active_seconds: 3600,
            apps_count: 1,
            dlp_ok: 21,
            dlp_warn: 1,
            dlp_fail: 0,
            evidence_total: 3,
            evidence_screenshots: 1,
            open_incidents: 0,
            acknowledged_incidents: 0,
            workforce_index: Some(13),
        };
        let risk = build_ueba_risk(
            &snapshot,
            &metrics,
            &json!({"configured": false}),
            &[],
            &json!({
                "baseline_window_days": 30,
                "user_baseline_available": false,
                "department_baseline_available": false,
                "deviation_score": 0,
                "baseline_samples": {"users": 0, "departments": 0, "total": 0}
            }),
            &policy_path,
        );
        assert_eq!(risk["score"], 7);
        assert_eq!(risk["policy_version"], "ueba-rule-v1-test");
        assert_eq!(risk["baseline_status"], "test_baseline");
        assert_eq!(risk["policy_configured"], true);
        assert_eq!(risk["confidence"], 0.8);
        assert_eq!(risk["risk_sources"][0], "dlp");
        assert_eq!(risk["reasons"].as_array().unwrap().len(), 1);
        assert_ne!(risk["reasons"][0]["code"], "evidence_present");
    }

    #[test]
    fn ueba_baseline_accumulates_user_and_department_deviation() {
        fn snapshot_for(date: &str, active_seconds: i64, department_coverage: f64) -> Snapshot {
            Snapshot {
                generated_at_utc: format!("{date}T10:00:00Z"),
                detmir_status: SourceStatus {
                    ok: true,
                    status: "OK".to_string(),
                    summary: "".to_string(),
                    error: None,
                    payload: None,
                },
                detmir_check: SourceStatus {
                    ok: true,
                    status: "OK".to_string(),
                    summary: "".to_string(),
                    error: None,
                    payload: None,
                },
                failed_units: SourceStatus {
                    ok: true,
                    status: "OK".to_string(),
                    summary: "".to_string(),
                    error: None,
                    payload: None,
                },
                worktime: SourceStatus {
                    ok: true,
                    status: "OK".to_string(),
                    summary: "".to_string(),
                    error: None,
                    payload: Some(json!({
                        "report_date": date,
                        "rows": [
                            {"user": "USER-1", "user_id": "EMP-1", "active_seconds": active_seconds}
                        ],
                        "true_active_apps": []
                    })),
                },
                worktime_management: SourceStatus {
                    ok: true,
                    status: "OK".to_string(),
                    summary: "".to_string(),
                    error: None,
                    payload: Some(json!({
                        "report_date": date,
                        "department_rollups": [
                            {
                                "name": "DEPT-1",
                                "users_count": 1,
                                "active_users": 1,
                                "portfolio_coverage_pct": department_coverage,
                                "workday_total_active_hhmm": "08:00"
                            }
                        ]
                    })),
                },
                one_c: SourceStatus {
                    ok: true,
                    status: "OK".to_string(),
                    summary: "".to_string(),
                    error: None,
                    payload: None,
                },
                agent_quality: AgentQuality::default(),
                agent_quality_history: Vec::new(),
                agent_quality_history_summary: AgentQualityHistorySummary::default(),
                agent_quality_nodes: Vec::new(),
                agent_quality_nodes_summary: AgentQualityNodesSummary::default(),
                agent_coverage_sla: AgentCoverageSla::default(),
            }
        }

        let dir = tempfile::tempdir().unwrap();
        let baseline_path = dir.path().join("ueba-baseline-state.json");
        for day in ["2026-06-01", "2026-06-02", "2026-06-03"] {
            let snapshot = snapshot_for(day, 8 * 3600, 90.0);
            let analysis = build_ueba_baseline_analysis(&snapshot, &baseline_path, false);
            assert_eq!(analysis["state_error"], Value::Null);
        }

        let snapshot = snapshot_for("2026-06-04", 2 * 3600, 40.0);
        let analysis = build_ueba_baseline_analysis(&snapshot, &baseline_path, false);
        assert_eq!(analysis["baseline_window_days"], 30);
        assert_eq!(analysis["user_baseline_available"], true);
        assert_eq!(analysis["department_baseline_available"], true);
        assert!(analysis["deviation_score"].as_u64().unwrap() > 0);
        assert_eq!(analysis["baseline_samples"]["users"], 3);
        assert_eq!(analysis["baseline_samples"]["departments"], 3);
        assert!(
            analysis["strongest_deviations"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["scope"] == "user")
        );

        let snapshot = snapshot_for("2026-06-05", 3600, 30.0);
        let anonymized = build_ueba_baseline_analysis(&snapshot, &baseline_path, true);
        let first = &anonymized["strongest_deviations"][0];
        if first["scope"] == "user" {
            assert_eq!(first["label"], "Сотрудник 1");
            assert_eq!(first["key"], "EMPLOYEE-1");
        }
    }

    #[test]
    fn weighted_activity_uses_role_application_policy() {
        let snapshot = Snapshot {
            generated_at_utc: "2026-06-03T10:00:00Z".to_string(),
            detmir_status: SourceStatus {
                ok: true,
                status: "OK".to_string(),
                summary: "".to_string(),
                error: None,
                payload: Some(json!({})),
            },
            detmir_check: SourceStatus {
                ok: true,
                status: "OK".to_string(),
                summary: "".to_string(),
                error: None,
                payload: Some(json!({})),
            },
            failed_units: SourceStatus {
                ok: true,
                status: "OK".to_string(),
                summary: "".to_string(),
                error: None,
                payload: None,
            },
            worktime: SourceStatus {
                ok: true,
                status: "OK".to_string(),
                summary: "".to_string(),
                error: None,
                payload: Some(json!({
                    "true_active_apps": [
                        {"application": "1С", "proved_work_seconds": 3600},
                        {"application": "YouTube", "proved_work_seconds": 3600}
                    ]
                })),
            },
            worktime_management: SourceStatus {
                ok: true,
                status: "OK".to_string(),
                summary: "".to_string(),
                error: None,
                payload: Some(json!({})),
            },
            one_c: SourceStatus {
                ok: true,
                status: "OK".to_string(),
                summary: "".to_string(),
                error: None,
                payload: None,
            },
            agent_quality: AgentQuality::default(),
            agent_quality_history: Vec::new(),
            agent_quality_history_summary: AgentQualityHistorySummary::default(),
            agent_quality_nodes: Vec::new(),
            agent_quality_nodes_summary: AgentQualityNodesSummary::default(),
            agent_coverage_sla: AgentCoverageSla::default(),
        };
        let policy = WorkforcePolicy {
            default_role: "accountant".to_string(),
            roles: BTreeMap::from([(
                "accountant".to_string(),
                WorkforceRolePolicy {
                    label: Some("Бухгалтер".to_string()),
                    description: Some(
                        "Бухгалтер: высокий вес 1С и офисных документов; развлекательные сайты не учитываются."
                            .to_string(),
                    ),
                    planned_hours_per_day: Some(8.0),
                    default_weight: Some(0.2),
                    application_weights: BTreeMap::from([
                        ("1с".to_string(), 1.0),
                        ("youtube".to_string(), 0.0),
                    ]),
                },
            )]),
        };
        let weighted = weighted_activity(&snapshot, &policy, 1, false).unwrap();
        assert_eq!(weighted.role, "accountant");
        assert_eq!(weighted.weighted_seconds, 3600);
        assert_eq!(weighted.app_seconds, 7200);
        assert_eq!(weighted.index, Some(13));
    }

    #[test]
    fn workforce_policy_explain_is_lightweight_payload() {
        let dir = tempfile::tempdir().unwrap();
        let policy_path = dir.path().join("workforce-policy.json");
        fs::write(
            &policy_path,
            r#"{
              "default_role": "accountant",
              "roles": {
                "accountant": {
                  "label": "Бухгалтер",
                  "description": "Бухгалтерский профиль",
                  "planned_hours_per_day": 8,
                  "default_weight": 0.2,
                  "application_weights": {"1с": 1.0}
                }
              }
            }"#,
        )
        .unwrap();
        let snapshot = Snapshot {
            generated_at_utc: "2026-06-03T10:00:00Z".to_string(),
            detmir_status: SourceStatus {
                ok: true,
                status: "OK".to_string(),
                summary: "".to_string(),
                error: None,
                payload: None,
            },
            detmir_check: SourceStatus {
                ok: true,
                status: "OK".to_string(),
                summary: "".to_string(),
                error: None,
                payload: None,
            },
            failed_units: SourceStatus {
                ok: true,
                status: "OK".to_string(),
                summary: "".to_string(),
                error: None,
                payload: None,
            },
            worktime: SourceStatus {
                ok: true,
                status: "OK".to_string(),
                summary: "".to_string(),
                error: None,
                payload: Some(json!({
                    "rows": [
                        {"user": "USER-1", "active_seconds": 3600}
                    ],
                    "true_active_apps": [
                        {"application": "1С", "proved_work_seconds": 3600}
                    ]
                })),
            },
            worktime_management: SourceStatus {
                ok: true,
                status: "OK".to_string(),
                summary: "".to_string(),
                error: None,
                payload: None,
            },
            one_c: SourceStatus {
                ok: true,
                status: "OK".to_string(),
                summary: "".to_string(),
                error: None,
                payload: None,
            },
            agent_quality: AgentQuality::default(),
            agent_quality_history: Vec::new(),
            agent_quality_history_summary: AgentQualityHistorySummary::default(),
            agent_quality_nodes: Vec::new(),
            agent_quality_nodes_summary: AgentQualityNodesSummary::default(),
            agent_coverage_sla: AgentCoverageSla::default(),
        };
        let explain = build_workforce_policy_explain(&snapshot, &policy_path, false);
        assert_eq!(explain["configured"], true);
        assert_eq!(explain["role"], "accountant");
        assert_eq!(explain["roles_count"], 1);
        assert_eq!(explain["app_details"].as_array().unwrap().len(), 1);
        assert_eq!(
            explain["formula"],
            "index = weighted_seconds / planned_seconds × 100"
        );
        assert!(explain["planned_seconds"].as_i64().unwrap() > 0);
        assert!(explain["weighted_seconds"].as_i64().unwrap() > 0);
        assert!(explain["policy_audit"].is_object());
        assert!(explain["employee_details"].as_array().unwrap().len() == 1);
        assert_eq!(
            explain["employee_details"][0]["formula"],
            "employee_index = active_seconds / planned_seconds × 100"
        );
        assert!(explain["employee_details"][0].get("reason").is_some());
        assert!(
            explain["employee_details"][0]["scope_note"]
                .as_str()
                .unwrap()
                .contains("не персональный weighted KPI")
        );
        assert!(explain["app_details"][0].get("matched_rule").is_some());
        assert!(explain["app_details"][0].get("weight").is_some());
        assert!(explain.get("workforce").is_none());
        assert!(explain.get("sections").is_none());
        assert!(explain.get("markdown").is_none());

        let anonymized = build_workforce_policy_explain(&snapshot, &policy_path, true);
        assert_eq!(anonymized["anonymized"], true);
        assert_eq!(anonymized["employee_details"][0]["user"], "Сотрудник 1");
        assert_eq!(anonymized["employee_details"][0]["user_id"], "EMPLOYEE-1");
        assert_ne!(anonymized["employee_details"][0]["user"], "USER-1");
    }
}
