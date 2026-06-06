use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

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
const ARCHITECTURE_HTML: &str = include_str!("static/architecture.html");
const APP_CSS: &str = include_str!("static/app.css");
const APP_JS: &str = include_str!("static/app.js");
const API_CONTRACT_OPENAPI: &str = include_str!("contracts/openapi.json");
const API_CONTRACT_TYPESCRIPT: &str = include_str!("contracts/typescript.d.ts");
const UEBA_BASELINE_MIN_SAMPLES: usize = 3;
const SNAPSHOT_CACHE_TTL: Duration = Duration::from_secs(120);
const DEFAULT_DEPARTMENT_LABEL: &str = "Не привязано к подразделению";
const LEGACY_UNASSIGNED_DEPARTMENT_LABEL: &str = "Без подразделения";

#[cfg(unix)]
const SIGKILL: i32 = 9;

#[cfg(unix)]
unsafe extern "C" {
    fn setpgid(pid: i32, pgid: i32) -> i32;
    fn kill(pid: i32, sig: i32) -> i32;
}

type SnapshotCache = Arc<Mutex<Option<CachedSnapshot>>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum PortalRole {
    Executive,
    Manager,
    Security,
    Forensics,
    Admin,
}

impl PortalRole {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "executive" | "owner" | "rukovoditel" | "руководитель" => {
                Some(Self::Executive)
            }
            "manager" | "workforce" | "руководитель_подразделения" => {
                Some(Self::Manager)
            }
            "security" | "ib" | "soc" | "безопасность" => Some(Self::Security),
            "forensics" | "investigation" | "расследования" => Some(Self::Forensics),
            "admin" | "operations" | "operator" | "эксплуатация" => Some(Self::Admin),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Executive => "executive",
            Self::Manager => "manager",
            Self::Security => "security",
            Self::Forensics => "forensics",
            Self::Admin => "admin",
        }
    }

    fn label_ru(self) -> &'static str {
        match self {
            Self::Executive => "Руководитель",
            Self::Manager => "Руководитель подразделения",
            Self::Security => "Безопасность",
            Self::Forensics => "Расследования",
            Self::Admin => "Администратор",
        }
    }

    fn allowed_scopes(self) -> &'static [&'static str] {
        match self {
            Self::Executive => &["executive", "workforce"],
            Self::Manager => &["executive", "workforce"],
            Self::Security => &["security", "incidents", "ueba", "pfsense"],
            Self::Forensics => &["forensics", "incidents", "ueba"],
            Self::Admin => &[
                "executive",
                "workforce",
                "security",
                "forensics",
                "incidents",
                "ueba",
                "pfsense",
                "admin",
            ],
        }
    }

    fn can_access(self, scope: &str) -> bool {
        self.allowed_scopes().contains(&scope)
    }
}

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

    #[arg(long, default_value = "disabled", env = "SECURITY_EVENTS_BACKEND")]
    security_events_backend: String,

    #[arg(long, default_value = "http://127.0.0.1:8123", env = "CLICKHOUSE_URL")]
    clickhouse_url: String,

    #[arg(long, default_value = "analytics_1c", env = "CLICKHOUSE_DATABASE")]
    clickhouse_database: String,

    #[arg(long, default_value = "default", env = "CLICKHOUSE_USER")]
    clickhouse_user: String,

    #[arg(long, default_value = "", env = "CLICKHOUSE_PASSWORD")]
    clickhouse_password: String,
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

#[derive(Clone, Debug)]
struct SecurityEventsConfig {
    backend: String,
    clickhouse_url: String,
    clickhouse_database: String,
    clickhouse_user: String,
    clickhouse_password: String,
    timeout: Duration,
}

#[derive(Clone, Debug, Serialize)]
struct SecurityEventsDepartment {
    department: String,
    events: u64,
}

#[derive(Clone, Debug, Serialize)]
struct SecurityEventsSummary {
    status: String,
    backend: String,
    events_24h: u64,
    failed_logins_24h: u64,
    suspicious_logins_24h: u64,
    rdp_sessions_24h: u64,
    account_changes_24h: u64,
    agent_errors_24h: u64,
    top_departments: Vec<SecurityEventsDepartment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_event_utc: Option<String>,
    query_ms: u128,
    fallback_used: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl SecurityEventsSummary {
    fn disabled() -> Self {
        Self {
            status: "disabled".to_string(),
            backend: "disabled".to_string(),
            events_24h: 0,
            failed_logins_24h: 0,
            suspicious_logins_24h: 0,
            rdp_sessions_24h: 0,
            account_changes_24h: 0,
            agent_errors_24h: 0,
            top_departments: Vec::new(),
            last_event_utc: None,
            query_ms: 0,
            fallback_used: false,
            error: None,
        }
    }

    fn fallback(error: impl Into<String>, query_ms: u128) -> Self {
        Self {
            status: "fallback".to_string(),
            backend: "clickhouse".to_string(),
            events_24h: 0,
            failed_logins_24h: 0,
            suspicious_logins_24h: 0,
            rdp_sessions_24h: 0,
            account_changes_24h: 0,
            agent_errors_24h: 0,
            top_departments: Vec::new(),
            last_event_utc: None,
            query_ms,
            fallback_used: true,
            error: Some(error.into()),
        }
    }
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

#[derive(Clone, Copy)]
struct BusinessRiskSignals {
    trend_delta: Option<f64>,
    missing_nodes_count: usize,
    stale_nodes_count: usize,
    problem_nodes_count: usize,
    security_events_24h: u64,
    activity_missing: bool,
}

#[derive(Clone, Copy)]
struct RiskLayerMetrics<'a> {
    trust_kpi_score: Option<u8>,
    activity_score: Option<u8>,
    agent_coverage_pct: Option<u8>,
    business_risk_level: Option<&'a str>,
    open_cases: usize,
    critical_candidates: usize,
    security_events_24h: u64,
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

#[derive(Clone, Debug, Serialize)]
struct BusinessRiskItem {
    department: String,
    trust_score: u8,
    activity_score: u8,
    trend: String,
    risk_level: String,
    reasons: Vec<String>,
    recommendation: String,
    problem_nodes_count: usize,
    missing_nodes_count: usize,
    stale_nodes_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    security_events_24h: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
struct BusinessRiskHistoryItem {
    date: String,
    department: String,
    risk_level: String,
    trust_score: u8,
    activity_score: u8,
    reasons: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
struct BusinessRiskHistorySummary {
    departments_worsened: usize,
    departments_improved: usize,
    stable_high_risk: usize,
    new_high_risk: usize,
}

#[derive(Clone, Debug, Serialize)]
struct RiskHeatmapItem {
    department: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    trust_kpi_score: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    activity_score: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_coverage_pct: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    business_risk_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    open_cases: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    critical_candidates: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    security_events_24h: Option<u64>,
    heat_level: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    links: Vec<RiskNarrativeLink>,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct SecurityCorrelationItem {
    department: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    trust_kpi_score: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    activity_score: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    business_risk_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    critical_candidates: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    open_cases: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    security_events_24h: Option<u64>,
    correlation_score: u8,
    correlation_reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    explanation: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct RiskNarrativeLink {
    target: String,
    label: String,
    summary: String,
}

#[derive(Clone, Debug, Serialize)]
struct RiskIncidentCandidate {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    department: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hostname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    risk_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    evidence: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    first_seen_utc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_seen_utc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    recommendation: Option<String>,
    incident_review: IncidentReviewState,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    incident_review_audit: Vec<IncidentReviewAuditEntry>,
}

#[derive(Clone, Debug, Serialize)]
struct InvestigationPack {
    candidate_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    department: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hostname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    risk_level: Option<String>,
    reasons: Vec<String>,
    evidence: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    first_seen_utc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_seen_utc: Option<String>,
    current_review_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    review_comment: Option<String>,
    review_audit_history: Vec<IncidentReviewAuditEntry>,
    trust_kpi_snapshot: Value,
    agent_quality_snapshot: Value,
    business_risk_snapshot: Value,
    markdown: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CaseItem {
    case_id: String,
    candidate_id: String,
    title: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    owner: Option<String>,
    created_at_utc: String,
    updated_at_utc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    decision: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct CaseFile {
    cases: BTreeMap<String, CaseItem>,
}

#[derive(Debug, Deserialize)]
struct CreateCaseRequest {
    candidate_id: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CaseStatusRequest {
    status: String,
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    decision: Option<String>,
}

#[derive(Debug, Serialize)]
struct CaseResponse {
    ok: bool,
    case: CaseItem,
}

#[derive(Debug, Serialize)]
struct CaseListResponse {
    ok: bool,
    cases: Vec<CaseItem>,
}

#[derive(Debug, Serialize)]
struct CaseDetailsResponse {
    ok: bool,
    case: CaseItem,
    #[serde(skip_serializing_if = "Option::is_none")]
    investigation_pack: Option<InvestigationPack>,
    markdown: String,
}

#[derive(Clone, Debug, Serialize)]
struct ExecutiveRiskDepartment {
    department: String,
    risk_level: String,
    trust_score: u8,
    activity_score: u8,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    reasons: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct ExecutiveCandidateSummary {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    department: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hostname: Option<String>,
    risk_level: String,
    reason: String,
}

#[derive(Clone, Debug, Serialize)]
struct ExecutiveDashboardSummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    risk_narrative_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    main_risk_cause: Option<String>,
    main_risk: String,
    main_improvement: String,
    main_data_gap: String,
}

#[derive(Clone, Debug, Serialize)]
struct ExecutiveDashboard {
    #[serde(skip_serializing_if = "Option::is_none")]
    trust_kpi_score: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_coverage_pct: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    high_risk_departments: Option<Vec<ExecutiveRiskDepartment>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    critical_candidates: Option<Vec<ExecutiveCandidateSummary>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    open_cases: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resolved_cases_30d: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    forensics_readiness: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    security_events_24h: Option<u64>,
    summary: ExecutiveDashboardSummary,
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

#[derive(Debug, Default, Deserialize, Serialize)]
struct IncidentReviewFile {
    reviews: BTreeMap<String, IncidentReviewState>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct IncidentReviewState {
    candidate_id: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reviewer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    comment: Option<String>,
    updated_at: String,
}

impl Default for IncidentReviewState {
    fn default() -> Self {
        Self {
            candidate_id: String::new(),
            status: "NEW".to_string(),
            reviewer: None,
            comment: None,
            updated_at: String::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct IncidentReviewRequest {
    candidate_id: String,
    status: String,
    #[serde(default)]
    reviewer: Option<String>,
    #[serde(default)]
    comment: Option<String>,
}

#[derive(Debug, Serialize)]
struct IncidentReviewResponse {
    ok: bool,
    review: IncidentReviewState,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct IncidentReviewAuditEntry {
    candidate_id: String,
    old_status: String,
    new_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reviewer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    comment: Option<String>,
    changed_at_utc: String,
}

#[derive(Clone, Debug, Default, Serialize)]
struct IncidentReviewAuditSummary {
    total_changes: usize,
    confirmed_count: usize,
    false_positive_count: usize,
    postponed_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_change_utc: Option<String>,
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
    one_c_overview: SourceStatus,
    agent_quality: AgentQuality,
    agent_quality_history: Vec<AgentQualityHistoryItem>,
    agent_quality_history_summary: AgentQualityHistorySummary,
    agent_quality_nodes: Vec<AgentQualityNodeItem>,
    agent_quality_nodes_summary: AgentQualityNodesSummary,
    agent_coverage_sla: AgentCoverageSla,
    security_events_summary: SecurityEventsSummary,
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

struct ReportMarkdownContext<'a> {
    workforce: &'a ReportWorkforceSummary,
    executive_dashboard: &'a ExecutiveDashboard,
    workforce_policy: &'a Value,
    ueba_risk: &'a Value,
    business_risk: &'a [BusinessRiskItem],
    business_risk_history: &'a [BusinessRiskHistoryItem],
    business_risk_history_summary: &'a BusinessRiskHistorySummary,
    risk_heatmap: &'a [RiskHeatmapItem],
    security_correlation: &'a [SecurityCorrelationItem],
    security_events_summary: &'a SecurityEventsSummary,
    risk_incident_candidates: &'a [RiskIncidentCandidate],
    incident_review_audit_summary: &'a IncidentReviewAuditSummary,
}

struct ReportRuntimeInputs<'a> {
    incident_state: &'a IncidentStateFile,
    incident_reviews: &'a IncidentReviewFile,
    incident_review_audit: &'a [IncidentReviewAuditEntry],
    cases: &'a CaseFile,
    evidence: &'a DlpEvidenceResponse,
}

struct ExecutiveDashboardInputs<'a> {
    agent_quality_explain: &'a AgentQualityExplain,
    business_risk: &'a [BusinessRiskItem],
    business_risk_history_summary: &'a BusinessRiskHistorySummary,
    risk_heatmap: &'a [RiskHeatmapItem],
    security_correlation: &'a [SecurityCorrelationItem],
    security_events_summary: &'a SecurityEventsSummary,
    candidates: &'a [RiskIncidentCandidate],
    cases: &'a CaseFile,
    evidence: &'a DlpEvidenceResponse,
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
        let incident_reviews = load_incident_review_best_effort(&args);
        let incident_review_audit = load_incident_review_audit_best_effort(&args);
        let cases = load_cases_best_effort(&args);
        let evidence = build_dlp_evidence_response(&args);
        let ueba_baseline_path = ueba_baseline_state_path(&args);
        let smoke = json!({
            "health": build_health(&snapshot),
            "summary": build_summary(&snapshot),
            "reports": build_reports(&snapshot, ReportRuntimeInputs {
                incident_state: &incident_state,
                incident_reviews: &incident_reviews,
                incident_review_audit: &incident_review_audit,
                cases: &cases,
                evidence: &evidence,
            }, &args.workforce_policy_path, &args.ueba_policy_path, &ueba_baseline_path, false),
            "incidents": build_incidents(&snapshot, &incident_state),
            "dlp_evidence": evidence,
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
    let role = portal_role_from_request(&request, &url);
    if method == Method::Post && path == "/api/incidents/action" {
        if !role.can_access("incidents") {
            return respond_forbidden(request, role, "incidents");
        }
        return handle_incident_action(request, args);
    }
    if method == Method::Post && path == "/api/incident-review" {
        if !role.can_access("security") {
            return respond_forbidden(request, role, "security");
        }
        return handle_incident_review(request, args);
    }
    if method == Method::Post && path == "/api/cases" {
        if !role.can_access("forensics") && !role.can_access("incidents") {
            return respond_forbidden(request, role, "forensics");
        }
        return handle_create_case(request, args, snapshot_cache);
    }
    if method == Method::Post {
        if let Some(case_id) = parse_case_status_path(&path) {
            if !role.can_access("forensics") && !role.can_access("incidents") {
                return respond_forbidden(request, role, "forensics");
            }
            return handle_case_status(request, args, &case_id);
        }
    }
    if method == Method::Post && path == "/api/telemetry" {
        return handle_telemetry_ingest(request, args);
    }
    if method != Method::Get {
        return respond_text(request, StatusCode(405), "Method Not Allowed", "text/plain");
    }
    if path == "/api/dlp/evidence" {
        if !role.can_access("forensics") && !role.can_access("incidents") {
            return respond_forbidden(request, role, "forensics");
        }
        return respond_json(request, &build_dlp_evidence_response(args));
    }
    if let Some(candidate_id) = parse_investigation_pack_path(&path) {
        if !role.can_access("forensics") && !role.can_access("incidents") {
            return respond_forbidden(request, role, "forensics");
        }
        return handle_investigation_pack(request, args, snapshot_cache, &url, &candidate_id);
    }
    if let Some(case_id) = parse_case_path(&path) {
        if !role.can_access("forensics") && !role.can_access("incidents") {
            return respond_forbidden(request, role, "forensics");
        }
        return handle_case_details(request, args, snapshot_cache, &url, &case_id);
    }
    if let Some((evidence_id, download)) = parse_evidence_screenshot_path(&path) {
        return handle_evidence_screenshot(request, args, &evidence_id, download);
    }
    if let Some(html) = portal_html_route(&path) {
        return respond_text(request, StatusCode(200), html, "text/html; charset=utf-8");
    }
    match path.as_str() {
        "/app.css" => respond_text(request, StatusCode(200), APP_CSS, "text/css; charset=utf-8"),
        "/app.js" => respond_text(
            request,
            StatusCode(200),
            APP_JS,
            "application/javascript; charset=utf-8",
        ),
        "/favicon.ico" => respond_text(request, StatusCode(204), "", "image/x-icon"),
        "/api/contracts" => respond_json(request, &api_contract_summary()),
        "/api/contracts/openapi.json" => respond_text(
            request,
            StatusCode(200),
            API_CONTRACT_OPENAPI,
            "application/json; charset=utf-8",
        ),
        "/api/contracts/typescript.d.ts" => respond_text(
            request,
            StatusCode(200),
            API_CONTRACT_TYPESCRIPT,
            "text/plain; charset=utf-8",
        ),
        "/api/health" => respond_json(request, &build_fast_health(snapshot_cache)),
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
            if !role.can_access("workforce") {
                return respond_forbidden(request, role, "workforce");
            }
            let snapshot = cached_snapshot(args, snapshot_cache);
            respond_json(request, &build_manager(&snapshot))
        }
        "/api/workforce/policy/explain" => {
            if !role.can_access("workforce") {
                return respond_forbidden(request, role, "workforce");
            }
            let snapshot = cached_snapshot(args, snapshot_cache);
            respond_json(
                request,
                &build_workforce_policy_explain(&snapshot, &args.workforce_policy_path, anonymize),
            )
        }
        "/api/owner" => {
            if !role.can_access("security") {
                return respond_forbidden(request, role, "security");
            }
            let snapshot = cached_snapshot(args, snapshot_cache);
            respond_json(request, &build_owner(&snapshot))
        }
        "/api/reports" => {
            let report = build_report_payload(args, snapshot_cache, anonymize);
            respond_json(request, &role_filtered_report(report, role))
        }
        "/api/executive" => {
            if !role.can_access("executive") {
                return respond_forbidden(request, role, "executive");
            }
            let report = build_report_payload(args, snapshot_cache, anonymize);
            respond_json(request, &build_role_api_payload(report, role, "executive"))
        }
        "/api/workforce" => {
            if !role.can_access("workforce") {
                return respond_forbidden(request, role, "workforce");
            }
            let report = build_report_payload(args, snapshot_cache, anonymize);
            respond_json(request, &build_role_api_payload(report, role, "workforce"))
        }
        "/api/security" => {
            if !role.can_access("security") {
                return respond_forbidden(request, role, "security");
            }
            let report = build_report_payload(args, snapshot_cache, anonymize);
            respond_json(request, &build_role_api_payload(report, role, "security"))
        }
        "/api/forensics" => {
            if !role.can_access("forensics") {
                return respond_forbidden(request, role, "forensics");
            }
            let report = build_report_payload(args, snapshot_cache, anonymize);
            respond_json(request, &build_role_api_payload(report, role, "forensics"))
        }
        "/api/ueba" => {
            if !role.can_access("ueba") {
                return respond_forbidden(request, role, "ueba");
            }
            let report = build_report_payload(args, snapshot_cache, anonymize);
            respond_json(request, &build_ueba_api_payload(&report, role))
        }
        "/api/pfsense" => {
            if !role.can_access("pfsense") {
                return respond_forbidden(request, role, "pfsense");
            }
            respond_json(request, &build_pfsense_readiness_payload(role))
        }
        "/api/incidents" => {
            if !role.can_access("incidents") {
                return respond_forbidden(request, role, "incidents");
            }
            let snapshot = cached_snapshot(args, snapshot_cache);
            let incident_state = load_incident_state_best_effort(args);
            respond_json(request, &build_incidents(&snapshot, &incident_state))
        }
        "/api/cases" => {
            if !role.can_access("forensics") && !role.can_access("incidents") {
                return respond_forbidden(request, role, "forensics");
            }
            respond_json(request, &build_case_list(args))
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

fn portal_html_route(path: &str) -> Option<&'static str> {
    match path {
        "/" | "/operator" | "/manager" | "/owner" | "/incidents" | "/reports" => Some(INDEX_HTML),
        "/architecture" => Some(ARCHITECTURE_HTML),
        _ => None,
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

fn api_contract_summary() -> Value {
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
            {"method": "GET", "path": "/api/workforce/policy/explain", "purpose": "workforce policy explanation"}
        ]
    })
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

fn query_param(url: &str, key: &str) -> Option<String> {
    let query = url.split_once('?').map(|(_, query)| query)?;
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        (name == key && !value.is_empty()).then(|| value.to_string())
    })
}

fn portal_role_from_request(request: &Request, url: &str) -> PortalRole {
    query_param(url, "role")
        .as_deref()
        .and_then(PortalRole::parse)
        .or_else(|| {
            request
                .headers()
                .iter()
                .find(|header| header.field.equiv("X-AWatch-Role"))
                .and_then(|header| PortalRole::parse(header.value.as_str()))
        })
        .unwrap_or(PortalRole::Executive)
}

fn role_envelope(role: PortalRole, scope: &str) -> Value {
    json!({
        "role": role.as_str(),
        "role_label": role.label_ru(),
        "scope": scope,
        "allowed_scopes": role.allowed_scopes(),
        "server_enforced": true,
    })
}

fn respond_forbidden(request: Request, role: PortalRole, scope: &str) -> Result<()> {
    respond_json_status(
        request,
        StatusCode(403),
        &json!({
            "ok": false,
            "error": "forbidden",
            "message": format!("Роль {} не имеет доступа к контуру {scope}", role.label_ru()),
            "role": role.as_str(),
            "scope": scope,
            "server_enforced": true,
        }),
    )
}

fn parse_investigation_pack_path(path: &str) -> Option<String> {
    path.strip_prefix("/api/investigation-pack/")
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.contains('/'))
        .map(ToString::to_string)
}

fn parse_case_path(path: &str) -> Option<String> {
    path.strip_prefix("/api/cases/")
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.contains('/'))
        .map(ToString::to_string)
}

fn parse_case_status_path(path: &str) -> Option<String> {
    path.strip_prefix("/api/cases/")
        .and_then(|value| value.strip_suffix("/status"))
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.contains('/'))
        .map(ToString::to_string)
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

fn build_fast_health(cache: &SnapshotCache) -> HealthResponse {
    match cache.try_lock() {
        Ok(guard) => guard
            .as_ref()
            .map(|cached| build_health(&cached.snapshot))
            .unwrap_or_else(lightweight_health),
        Err(_) => lightweight_health(),
    }
}

fn lightweight_health() -> HealthResponse {
    let mut sources = BTreeMap::new();
    sources.insert("portal".to_string(), true);
    HealthResponse {
        ok: true,
        generated_at_utc: now(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        sources,
    }
}

fn build_snapshot(args: &Cli) -> Snapshot {
    let timeout = Duration::from_secs(args.timeout_seconds);
    let security_events_config = SecurityEventsConfig {
        backend: args.security_events_backend.clone(),
        clickhouse_url: args.clickhouse_url.clone(),
        clickhouse_database: args.clickhouse_database.clone(),
        clickhouse_user: args.clickhouse_user.clone(),
        clickhouse_password: args.clickhouse_password.clone(),
        timeout: timeout.min(Duration::from_secs(5)),
    };
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
        one_c_overview: http_json_source(
            "one_c_overview",
            &format!(
                "{}/api/1/analytics-1c/companies/overview",
                args.one_c_url.trim_end_matches('/')
            ),
            timeout,
        ),
        agent_quality: load_agent_quality(&args.telemetry_store_path),
        agent_quality_history,
        agent_quality_history_summary,
        agent_quality_nodes,
        agent_quality_nodes_summary,
        agent_coverage_sla,
        security_events_summary: build_security_events_summary(&security_events_config),
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

fn display_department_name(value: Option<&str>) -> String {
    display_name_opt(value, DEFAULT_DEPARTMENT_LABEL)
}

fn display_rollup_name(value: Option<&str>, key: &str) -> String {
    let fallback = if key == "department_rollups" {
        DEFAULT_DEPARTMENT_LABEL
    } else {
        "Без группы"
    };
    display_name_opt(value, fallback)
}

fn display_name_or(value: &str, fallback: &str) -> String {
    display_name_opt(Some(value), fallback)
}

fn display_name_opt(value: Option<&str>, fallback: &str) -> String {
    let Some(value) = value else {
        return fallback.to_string();
    };
    let value = value.trim();
    if value.is_empty()
        || value == LEGACY_UNASSIGNED_DEPARTMENT_LABEL
        || has_broken_display_chars(value)
    {
        fallback.to_string()
    } else {
        value.to_string()
    }
}

fn display_text_opt(value: Option<&str>, fallback: &str) -> String {
    let Some(value) = value else {
        return fallback.to_string();
    };
    repair_broken_display_text(value, fallback)
}

fn repair_broken_display_text(value: &str, fallback: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        return fallback.to_string();
    }
    if !has_broken_display_chars(value) {
        return value.to_string();
    }
    let mut repaired = String::new();
    let mut replacement_open = false;
    for ch in value.chars() {
        if ch == '\u{FFFD}' {
            if !replacement_open {
                repaired.push_str(fallback);
                replacement_open = true;
            }
            continue;
        }
        replacement_open = false;
        if ch.is_control() && ch != '\t' {
            continue;
        }
        repaired.push(ch);
    }
    let repaired = repaired.trim();
    if repaired.is_empty() {
        fallback.to_string()
    } else {
        repaired.to_string()
    }
}

fn sanitize_workforce_json(value: Value) -> Value {
    match value {
        Value::Array(items) => {
            Value::Array(items.into_iter().map(sanitize_workforce_json).collect())
        }
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| {
                    let value = match (key.as_str(), value) {
                        ("name", Value::String(value)) => {
                            Value::String(display_name_or(&value, "Без группы"))
                        }
                        ("user", Value::String(value)) => {
                            Value::String(display_name_or(&value, "Пользователь не определён"))
                        }
                        ("user_id", Value::String(value)) => {
                            Value::String(display_text_opt(Some(&value), "unknown"))
                        }
                        (_, value) => sanitize_workforce_json(value),
                    };
                    (key, value)
                })
                .collect(),
        ),
        Value::String(value) => Value::String(display_text_opt(Some(&value), "Без значения")),
        value => value,
    }
}

fn has_broken_display_chars(value: &str) -> bool {
    value
        .chars()
        .any(|ch| ch == '\u{FFFD}' || (ch.is_control() && ch != '\t'))
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
                department: display_name_or(&node.department, "Не задано"),
                owner: display_name_or(&node.owner, "Не назначен"),
                criticality: display_name_or(&node.criticality, "normal"),
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
                "Телеметрия свежая, но источник не подтверждает показатели. Вернуть основной сбор Windows.",
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
            title: "Данные агента подтверждают показатели".to_string(),
            summary: "Сессии собраны основным способом Windows; индекс активности можно использовать как рабочий управленческий показатель.".to_string(),
            recommendation: "Использовать отчет как подтвержденный оперативный срез. Для расследований сверять с первичными событиями ActivityWatch.".to_string(),
            kpi_accepted: true,
        };
    }
    if source == "local_fallback" {
        return AgentQualityExplain {
            status: "DEGRADED".to_string(),
            title: "Диагностический режим агента".to_string(),
            summary: "Диагностический режим, данные не засчитываются в показатели активности.".to_string(),
            recommendation: "Проверить доступность основного источника Windows, права запуска агента и состояние задания агента. Не использовать этот срез как доказательство активности.".to_string(),
            kpi_accepted: false,
        };
    }
    if let Some(error) = &quality.collector_error {
        return AgentQualityExplain {
            status: "DEGRADED".to_string(),
            title: "Достоверность данных снижена".to_string(),
            summary: format!("Коллектор передал ошибку: {error}"),
            recommendation: "Проверить журнал агента, источник сбора сессий и восстановить основной способ Windows перед использованием отчета как доказательной базы.".to_string(),
            kpi_accepted: false,
        };
    }
    match source {
        "quser_utf16" | "quser_lossy" | "env_sessionname_fallback" => AgentQualityExplain {
            status: "WARNING".to_string(),
            title: "Данные собраны резервным способом".to_string(),
            summary: "Активность собрана резервным способом. Показатели можно использовать как оперативный ориентир, но доказательная точность ниже.".to_string(),
            recommendation: "Проверить, почему основной источник Windows недоступен, и вернуть агент на основной источник сбора.".to_string(),
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
        Ok((stdout, stderr, success)) => match serde_json::from_str::<Value>(&stdout) {
            Ok(payload) => SourceStatus {
                ok: payload_bool(&payload, "/ok").unwrap_or(true),
                status: status_from_payload(&payload),
                summary: source_summary(name, &payload),
                error: if success || stderr.trim().is_empty() {
                    None
                } else {
                    Some(stderr.trim().to_string())
                },
                payload: Some(payload),
            },
            Err(err) => SourceStatus {
                ok: false,
                status: "FAIL".to_string(),
                summary: if success {
                    format!("{name} returned invalid JSON")
                } else {
                    format!("{name} command returned non-zero status")
                },
                error: Some(err.to_string()),
                payload: None,
            },
        },
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

fn build_security_events_summary(config: &SecurityEventsConfig) -> SecurityEventsSummary {
    let backend = config.backend.trim().to_ascii_lowercase();
    if backend.is_empty() || backend == "disabled" {
        return SecurityEventsSummary::disabled();
    }
    if backend != "clickhouse" {
        return SecurityEventsSummary::fallback(
            format!("неизвестный источник событий безопасности: {backend}"),
            0,
        );
    }
    let started = Instant::now();
    match query_clickhouse_security_events(config) {
        Ok(mut summary) => {
            summary.query_ms = started.elapsed().as_millis();
            summary
        }
        Err(err) => SecurityEventsSummary::fallback(err.to_string(), started.elapsed().as_millis()),
    }
}

fn query_clickhouse_security_events(
    config: &SecurityEventsConfig,
) -> Result<SecurityEventsSummary> {
    let database = clickhouse_identifier(&config.clickhouse_database)
        .ok_or_else(|| anyhow!("некорректное имя базы ClickHouse"))?;
    let aggregate_sql = format!(
        r#"
WITH now() - INTERVAL 24 HOUR AS since
SELECT
    toUInt64(count()) AS events_24h,
    toUInt64(countIf((positionCaseInsensitive(search, 'failed') > 0 AND positionCaseInsensitive(search, 'login') > 0) OR positionCaseInsensitive(search, '4625') > 0)) AS failed_logins_24h,
    toUInt64(countIf(positionCaseInsensitive(search, 'suspicious') > 0 OR positionCaseInsensitive(search, 'anomaly') > 0 OR lower(severity) IN ('high', 'critical'))) AS suspicious_logins_24h,
    toUInt64((SELECT coalesce(sum(rdp_sessions), 0) FROM {database}.host_events WHERE ts >= since)) AS rdp_sessions_24h,
    toUInt64(countIf(positionCaseInsensitive(search, 'account') > 0 OR positionCaseInsensitive(search, 'user change') > 0 OR positionCaseInsensitive(search, '4720') > 0 OR positionCaseInsensitive(search, '4726') > 0)) AS account_changes_24h,
    toUInt64(countIf((positionCaseInsensitive(search, 'agent') > 0 OR positionCaseInsensitive(search, 'collector') > 0) AND (positionCaseInsensitive(search, 'error') > 0 OR positionCaseInsensitive(search, 'fail') > 0))) AS agent_errors_24h,
    nullIf(formatDateTime(max(ts), '%Y-%m-%dT%H:%i:%SZ'), '1970-01-01T00:00:00Z') AS last_event_utc
FROM (
    SELECT ts, severity, concat(event_type, ' ', severity, ' ', source, ' ', summary) AS search
    FROM {database}.entity_timeline
    WHERE ts >= since
)
FORMAT JSONEachRow
"#
    );
    let top_sql = format!(
        r#"
WITH now() - INTERVAL 24 HOUR AS since
SELECT if(empty(infobase), 'Не привязано к подразделению', infobase) AS department, toUInt64(count()) AS events
FROM {database}.entity_timeline
WHERE ts >= since
GROUP BY department
ORDER BY events DESC, department ASC
LIMIT 5
FORMAT JSONEachRow
"#
    );
    let aggregate = clickhouse_query_first_json(config, &aggregate_sql)?
        .ok_or_else(|| anyhow!("ClickHouse не вернул агрегаты событий безопасности"))?;
    let top_departments = clickhouse_query_json_lines(config, &top_sql)?
        .into_iter()
        .map(|row| SecurityEventsDepartment {
            department: row
                .get("department")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(DEFAULT_DEPARTMENT_LABEL)
                .to_string(),
            events: json_u64(&row, "events"),
        })
        .collect();
    Ok(SecurityEventsSummary {
        status: "ok".to_string(),
        backend: "clickhouse".to_string(),
        events_24h: json_u64(&aggregate, "events_24h"),
        failed_logins_24h: json_u64(&aggregate, "failed_logins_24h"),
        suspicious_logins_24h: json_u64(&aggregate, "suspicious_logins_24h"),
        rdp_sessions_24h: json_u64(&aggregate, "rdp_sessions_24h"),
        account_changes_24h: json_u64(&aggregate, "account_changes_24h"),
        agent_errors_24h: json_u64(&aggregate, "agent_errors_24h"),
        top_departments,
        last_event_utc: aggregate
            .get("last_event_utc")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(ToString::to_string),
        query_ms: 0,
        fallback_used: false,
        error: None,
    })
}

fn clickhouse_identifier(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || !trimmed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return None;
    }
    Some(trimmed.to_string())
}

fn clickhouse_query_first_json(config: &SecurityEventsConfig, sql: &str) -> Result<Option<Value>> {
    Ok(clickhouse_query_json_lines(config, sql)?.into_iter().next())
}

fn clickhouse_query_json_lines(config: &SecurityEventsConfig, sql: &str) -> Result<Vec<Value>> {
    let client = Client::builder()
        .timeout(config.timeout)
        .no_proxy()
        .build()
        .context("ClickHouse HTTP client")?;
    let url = config.clickhouse_url.trim_end_matches('/');
    let mut request = client
        .post(url)
        .query(&[("database", config.clickhouse_database.trim())])
        .body(sql.to_string());
    if !config.clickhouse_user.trim().is_empty() {
        request = request.basic_auth(
            config.clickhouse_user.trim().to_string(),
            Some(config.clickhouse_password.clone()),
        );
    }
    let text = request
        .send()
        .context("ClickHouse request")?
        .error_for_status()
        .context("ClickHouse HTTP status")?
        .text()
        .context("ClickHouse response body")?;
    let mut rows = Vec::new();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        rows.push(serde_json::from_str::<Value>(line).context("ClickHouse JSONEachRow")?);
    }
    Ok(rows)
}

fn json_u64(value: &Value, key: &str) -> u64 {
    value
        .get(key)
        .and_then(|item| item.as_u64().or_else(|| item.as_str()?.parse().ok()))
        .unwrap_or(0)
}

fn security_events_department_count(summary: &SecurityEventsSummary, department: &str) -> u64 {
    let normalized = display_department_name(Some(department));
    summary
        .top_departments
        .iter()
        .find(|item| display_department_name(Some(&item.department)) == normalized)
        .map(|item| item.events)
        .unwrap_or(0)
}

fn security_events_summary_status(summary: &SecurityEventsSummary) -> String {
    if summary.backend == "disabled" || summary.status == "disabled" {
        "UNKNOWN".to_string()
    } else if summary.fallback_used
        || summary.events_24h > 0
        || summary.failed_logins_24h > 0
        || summary.suspicious_logins_24h > 0
        || summary.account_changes_24h > 0
        || summary.agent_errors_24h > 0
    {
        "WARN".to_string()
    } else {
        "OK".to_string()
    }
}

fn security_events_summary_text(summary: &SecurityEventsSummary) -> String {
    if summary.backend == "disabled" || summary.status == "disabled" {
        return "Источник событий безопасности отключён. Используется локальный режим без ClickHouse."
            .to_string();
    }
    if summary.fallback_used {
        return format!(
            "События безопасности временно недоступны. Используется локальный режим без ClickHouse; причина: {}",
            summary.error.as_deref().unwrap_or("неизвестная ошибка")
        );
    }
    format!(
        "События безопасности доступны: событий={}, неуспешных входов={}, подозрительных входов={}, RDP={}, изменения учетных записей={}, ошибки агентов={}",
        summary.events_24h,
        summary.failed_logins_24h,
        summary.suspicious_logins_24h,
        summary.rdp_sessions_24h,
        summary.account_changes_24h,
        summary.agent_errors_24h
    )
}

fn security_events_executive_text(summary: &SecurityEventsSummary) -> String {
    if summary.backend == "disabled" || summary.status == "disabled" {
        return "Источник событий безопасности отключён.".to_string();
    }
    if summary.fallback_used {
        return "События безопасности временно недоступны.".to_string();
    }
    format!(
        "События безопасности доступны: событий за 24 часа {}.",
        summary.events_24h
    )
}

fn security_events_block(summary: &SecurityEventsSummary) -> SummaryBlock {
    SummaryBlock {
        status: security_events_summary_status(summary),
        text: security_events_summary_text(summary),
    }
}

fn run_shell(command: &str, timeout: Duration) -> Result<(String, String, bool)> {
    let stdout_path = command_output_path("stdout");
    let stderr_path = command_output_path("stderr");
    let stdout_file =
        File::create(&stdout_path).with_context(|| format!("create {}", stdout_path.display()))?;
    let stderr_file =
        File::create(&stderr_path).with_context(|| format!("create {}", stderr_path.display()))?;
    let mut shell = Command::new("/bin/sh");
    shell
        .arg("-lc")
        .arg(command)
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file));
    #[cfg(unix)]
    unsafe {
        shell.pre_exec(|| {
            // Isolate portal probes so a timeout can kill helper grandchildren
            // such as detmir-check, not only the shell wrapper.
            if setpgid(0, 0) == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        });
    }
    let mut child = shell.spawn().with_context(|| format!("spawn {command}"))?;
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            let stdout = read_command_output(&stdout_path)?;
            let stderr = read_command_output(&stderr_path)?;
            cleanup_command_output(&stdout_path, &stderr_path);
            return Ok((stdout, stderr, status.success()));
        }
        if started.elapsed() > timeout {
            kill_shell_tree(&mut child);
            let _ = child.wait();
            cleanup_command_output(&stdout_path, &stderr_path);
            return Err(anyhow!(
                "command timed out after {}s: {command}",
                timeout.as_secs()
            ));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn command_output_path(stream: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "detmir-portal-command-{}-{nanos}-{stream}.log",
        std::process::id()
    ))
}

fn read_command_output(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn cleanup_command_output(stdout_path: &Path, stderr_path: &Path) {
    let _ = fs::remove_file(stdout_path);
    let _ = fs::remove_file(stderr_path);
}

#[cfg(unix)]
fn kill_shell_tree(child: &mut std::process::Child) {
    let pgid = child.id() as i32;
    // Negative pid targets the process group created in pre_exec above.
    // Best-effort cleanup: the caller still waits on the direct child.
    unsafe {
        let _ = kill(-pgid, SIGKILL);
    }
}

#[cfg(not(unix))]
fn kill_shell_tree(child: &mut std::process::Child) {
    let _ = child.kill();
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
    sources.insert(
        "security_events".to_string(),
        snapshot.security_events_summary.status != "error",
    );
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
    blocks.insert(
        "security_events".to_string(),
        security_events_block(&snapshot.security_events_summary),
    );
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
        "security_events_summary": snapshot.security_events_summary,
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
    inputs: ReportRuntimeInputs<'_>,
    workforce_policy_path: &Path,
    ueba_policy_path: &Path,
    ueba_baseline_path: &Path,
    anonymize: bool,
) -> Value {
    let summary = build_summary(snapshot);
    let incidents = build_incidents(snapshot, inputs.incident_state);
    let (users_count, active_seconds, apps_count) = worktime_totals(snapshot);
    let dlp = dlp_counts(snapshot);
    let metrics = ReportMetrics {
        users_count,
        active_seconds,
        apps_count,
        dlp_ok: dlp.0,
        dlp_warn: dlp.1,
        dlp_fail: dlp.2,
        evidence_total: inputs.evidence.items.len(),
        evidence_screenshots: inputs
            .evidence
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
    let security_events_summary = snapshot.security_events_summary.clone();
    let worktime = worktime_block(snapshot);
    let one_c = one_c_block(snapshot);
    let dlp_block_value = dlp_block(snapshot);
    let department_items = workforce_rollup_items(snapshot, "department_rollups");
    let owner_items = workforce_rollup_items(snapshot, "owner_rollups");
    let business_risk = build_business_risk(snapshot, &department_items);
    let business_risk_history = build_business_risk_history(snapshot);
    let business_risk_history_summary = summarize_business_risk_history(&business_risk_history);
    let mut risk_incident_candidates =
        build_risk_incident_candidates(snapshot, &business_risk, &business_risk_history);
    apply_incident_reviews_to_candidates(
        &mut risk_incident_candidates,
        inputs.incident_reviews,
        inputs.incident_review_audit,
    );
    let incident_review_audit_summary =
        summarize_incident_review_audit(inputs.incident_review_audit);
    let risk_heatmap = build_risk_heatmap(
        snapshot,
        &business_risk,
        &risk_incident_candidates,
        inputs.cases,
    );
    let security_correlation = build_security_correlation(&risk_heatmap, &security_events_summary);
    let executive_dashboard = build_executive_dashboard(
        snapshot,
        ExecutiveDashboardInputs {
            agent_quality_explain: &agent_quality_explain,
            business_risk: &business_risk,
            business_risk_history_summary: &business_risk_history_summary,
            risk_heatmap: &risk_heatmap,
            security_correlation: &security_correlation,
            security_events_summary: &security_events_summary,
            candidates: &risk_incident_candidates,
            cases: inputs.cases,
            evidence: inputs.evidence,
        },
    );
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
    let mut executive_points = Vec::new();
    executive_points.push(format!(
        "Главный управленческий вывод: {}",
        executive_dashboard
            .summary
            .main_risk_cause
            .as_deref()
            .unwrap_or("связанный риск не выражен")
    ));
    executive_points.push(format!(
        "Статус связанной картины риска: {}",
        executive_dashboard
            .summary
            .risk_narrative_status
            .as_deref()
            .unwrap_or("NORMAL")
    ));
    executive_points.push(format!(
        "События безопасности за 24 часа: {}",
        security_events_executive_text(&security_events_summary)
    ));
    executive_points.extend([
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
            "Проверки безопасности: ok={}, warn={}, fail={}, материалы={}, скриншоты={}",
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
    ]);
    if agent_quality_history_summary.ok_days < 5 {
        executive_points
            .push("Показатели требуют проверки: нестабильный сбор данных агента".to_string());
    }
    if agent_quality_nodes_summary.total_nodes > 0
        && agent_quality_nodes_summary.accepted_kpi_nodes_pct < 80
    {
        executive_points.push(
            "Показатели требуют проверки: менее 80% рабочих мест дают подтвержденные данные"
                .to_string(),
        );
    }
    match agent_coverage_sla.sla_status.as_str() {
        "CRITICAL" => executive_points.push(
            "Полнота данных критически недостаточна: показатели нельзя считать репрезентативными"
                .to_string(),
        ),
        "WARNING" => executive_points.push(
            "Показатели требуют проверки: часть рабочих мест не присылает свежую телеметрию"
                .to_string(),
        ),
        _ => {}
    }
    for department in stable_high_risk_departments(&business_risk_history, 3) {
        executive_points.push(format!(
            "Подразделение {department} сохраняет высокий риск несколько дней подряд."
        ));
    }
    executive_points.push(format!(
        "Главное улучшение: {}",
        executive_dashboard.summary.main_improvement
    ));
    executive_points.push(format!(
        "Главный пробел в данных: {}",
        executive_dashboard.summary.main_data_gap
    ));
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
        ReportMarkdownContext {
            workforce: &workforce_summary,
            executive_dashboard: &executive_dashboard,
            workforce_policy: &workforce_policy_explain,
            ueba_risk: &ueba_risk,
            business_risk: &business_risk,
            business_risk_history: &business_risk_history,
            business_risk_history_summary: &business_risk_history_summary,
            risk_heatmap: &risk_heatmap,
            security_correlation: &security_correlation,
            security_events_summary: &security_events_summary,
            risk_incident_candidates: &risk_incident_candidates,
            incident_review_audit_summary: &incident_review_audit_summary,
        },
    );
    json!({
        "generated_at_utc": snapshot.generated_at_utc,
        "period": "оперативный срез за сегодня и текущий runtime",
        "anonymized": anonymize,
        "severity": summary.severity,
        "operator_ok": summary.operator_ok,
        "headline": headline,
        "executive_points": executive_points,
        "executive_dashboard": executive_dashboard,
        "kpis": [
            report_kpi("Оценка риска", format!("{}/100", ueba_risk.get("score").and_then(Value::as_u64).unwrap_or(0)), ueba_risk.get("status").and_then(Value::as_str).unwrap_or("UNKNOWN").to_string(), ueba_risk.get("summary").and_then(Value::as_str).unwrap_or("оценка риска")),
            report_kpi("Качество данных", agent_quality.quality_status.clone(), agent_quality.quality_status.clone(), &format!("источник: {}", agent_quality.collector_source)),
            report_kpi("Достоверность данных", agent_quality_explain.status.clone(), agent_quality_explain.status.clone(), &agent_quality_explain.title),
            report_kpi("Индекс активности", workforce_index_text(metrics.workforce_index), workforce_index_status(metrics.workforce_index), "proxy: активное время / плановое рабочее время"),
            weighted_activity_kpi_from_policy(&workforce_policy_explain),
            report_kpi("Сотрудники", metrics.users_count.to_string(), worktime.status.clone(), "строки worktime за сегодня"),
            report_kpi("Активное время", human_duration(metrics.active_seconds), worktime.status.clone(), "сумма active_seconds"),
            report_kpi("Приложения", metrics.apps_count.to_string(), worktime.status.clone(), "true active applications"),
            report_kpi("Подразделения", department_items.len().to_string(), snapshot.worktime_management.status.clone(), "сравнение групп за текущий день"),
            report_kpi("Сигналы проверки", format!("{}/{}", metrics.dlp_warn, metrics.dlp_fail), dlp_block_value.status.clone(), "технические сигналы безопасности"),
            report_kpi("События безопасности", security_events_summary.events_24h.to_string(), security_events_summary_status(&security_events_summary), "агрегированная сводка за 24 часа, без сырых логов"),
            report_kpi("Материалы проверки", format!("{}/{}", metrics.evidence_screenshots, metrics.evidence_total), evidence_status(inputs.evidence), "скриншоты / все материалы"),
            report_kpi("Открытые вопросы", metrics.open_incidents.to_string(), incident_status(metrics.open_incidents), "не взятые в работу записи")
        ],
        "sections": [
            {
                "title": "Надежность контура",
                "items": [
                    report_item("DetMir status", snapshot.detmir_status.status.clone(), snapshot.detmir_status.summary.clone()),
                    report_item("Сбор данных", collection.status.clone(), collection.text.clone()),
                    report_item("Качество данных", agent_quality.quality_status.clone(), format!("источник={}, сессии={}, активные={}, удаленные={}", agent_quality.collector_source, agent_quality.sessions_collected_total, agent_quality.active_sessions_total, agent_quality.rdp_sessions_total)),
                    report_item("Достоверность показателей", agent_quality_explain.status.clone(), format!("участвует в показателях={}, {}", agent_quality_explain.kpi_accepted, agent_quality_explain.recommendation)),
                    report_item("Grafana", grafana.status.clone(), grafana.text.clone()),
                    report_item("1C analytics", one_c.status.clone(), one_c.text.clone())
                ]
            },
            {
                "title": "Работа и управляемость",
                "items": [
                    report_item("Индекс активности", workforce_index_status(metrics.workforce_index), workforce_index_text(metrics.workforce_index)),
                    weighted_activity_item_from_policy(&workforce_policy_explain, workforce_policy_path),
                    report_item("Рабочее время", worktime.status.clone(), worktime.text.clone()),
                    report_item("Сводка руководителя", snapshot.worktime_management.status.clone(), snapshot.worktime_management.summary.clone()),
                    report_item("Активное время", worktime.status.clone(), human_duration(metrics.active_seconds)),
                    report_item("Приложения", worktime.status.clone(), metrics.apps_count.to_string()),
                    report_item("Отчет", "OK", "готов к передаче руководителю")
                ]
            },
            {
                "title": "Выводы по активности",
                "items": insight_items.clone()
            },
            {
                "title": "Оценка риска",
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
                "title": "Безопасность и материалы проверки",
                "items": [
                    report_item("Проверки безопасности", dlp_block_value.status.clone(), dlp_block_value.text.clone()),
                    report_item("События безопасности за 24 часа", security_events_summary_status(&security_events_summary), security_events_summary_text(&security_events_summary)),
                    report_item("Материалы проверки", evidence_status(inputs.evidence), format!("записей={}", metrics.evidence_total)),
                    report_item("Скриншоты", evidence_status(inputs.evidence), format!("available={}", metrics.evidence_screenshots)),
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
        "security_events_summary": security_events_summary,
        "business_risk": business_risk,
        "business_risk_history": business_risk_history,
        "business_risk_history_summary": business_risk_history_summary,
        "risk_heatmap": risk_heatmap,
        "security_correlation": security_correlation,
        "risk_incident_candidates": risk_incident_candidates,
        "incident_review_audit_summary": incident_review_audit_summary,
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

fn build_report_payload(args: &Cli, snapshot_cache: &SnapshotCache, anonymize: bool) -> Value {
    let snapshot = cached_snapshot(args, snapshot_cache);
    let incident_state = load_incident_state_best_effort(args);
    let incident_reviews = load_incident_review_best_effort(args);
    let incident_review_audit = load_incident_review_audit_best_effort(args);
    let cases = load_cases_best_effort(args);
    let evidence = build_dlp_evidence_response(args);
    let ueba_baseline_path = ueba_baseline_state_path(args);
    build_reports(
        &snapshot,
        ReportRuntimeInputs {
            incident_state: &incident_state,
            incident_reviews: &incident_reviews,
            incident_review_audit: &incident_review_audit,
            cases: &cases,
            evidence: &evidence,
        },
        &args.workforce_policy_path,
        &args.ueba_policy_path,
        &ueba_baseline_path,
        anonymize,
    )
}

fn role_filtered_report(report: Value, role: PortalRole) -> Value {
    if role == PortalRole::Admin {
        let mut full = report;
        if let Some(object) = full.as_object_mut() {
            object.insert("ok".to_string(), Value::Bool(true));
            object.insert("role_context".to_string(), role_envelope(role, "admin"));
        }
        return full;
    }

    let mut out = serde_json::Map::new();
    let Some(object) = report.as_object() else {
        out.insert("ok".to_string(), Value::Bool(false));
        out.insert("role_context".to_string(), role_envelope(role, "unknown"));
        return Value::Object(out);
    };

    let base_keys = [
        "generated_at_utc",
        "period",
        "anonymized",
        "severity",
        "operator_ok",
        "headline",
        "links",
    ];
    for key in base_keys {
        copy_json_key(object, &mut out, key);
    }

    match role {
        PortalRole::Executive => {
            for key in [
                "executive_points",
                "executive_dashboard",
                "kpis",
                "business_risk",
                "business_risk_history_summary",
                "risk_heatmap",
                "security_events_summary",
                "workforce",
                "markdown",
            ] {
                copy_json_key(object, &mut out, key);
            }
            out.insert(
                "scope_note".to_string(),
                json!("Руководитель видит управленческий вывод и Workforce без ИБ-детализации."),
            );
            out.insert("role_context".to_string(), role_envelope(role, "executive"));
        }
        PortalRole::Manager => {
            for key in [
                "executive_dashboard",
                "kpis",
                "business_risk",
                "risk_heatmap",
                "workforce",
                "workforce_policy",
                "markdown",
            ] {
                copy_json_key(object, &mut out, key);
            }
            out.insert(
                "scope_note".to_string(),
                json!("Руководитель подразделения видит Workforce и Executive Dashboard без очереди ИБ."),
            );
            out.insert("role_context".to_string(), role_envelope(role, "workforce"));
        }
        PortalRole::Security => {
            for key in [
                "ueba_risk",
                "ueba_baseline",
                "security_events_summary",
                "security_correlation",
                "risk_incident_candidates",
                "incident_review_audit_summary",
                "business_risk",
                "risk_heatmap",
                "markdown",
            ] {
                copy_json_key(object, &mut out, key);
            }
            out.insert(
                "scope_note".to_string(),
                json!("Безопасность видит риски, события и кандидатов без управленческого Workforce Dashboard."),
            );
            out.insert("role_context".to_string(), role_envelope(role, "security"));
        }
        PortalRole::Forensics => {
            for key in [
                "risk_incident_candidates",
                "incident_review_audit_summary",
                "security_events_summary",
                "ueba_risk",
                "markdown",
            ] {
                copy_json_key(object, &mut out, key);
            }
            out.insert(
                "forensics".to_string(),
                build_forensics_contract_payload(&report),
            );
            out.insert(
                "scope_note".to_string(),
                json!("Расследования видят карточки, timeline и экспорт материалов без управленческого Workforce Dashboard."),
            );
            out.insert("role_context".to_string(), role_envelope(role, "forensics"));
        }
        PortalRole::Admin => {}
    }
    out.insert("ok".to_string(), Value::Bool(true));
    Value::Object(out)
}

fn copy_json_key(
    source: &serde_json::Map<String, Value>,
    target: &mut serde_json::Map<String, Value>,
    key: &str,
) {
    if let Some(value) = source.get(key) {
        target.insert(key.to_string(), value.clone());
    }
}

fn build_role_api_payload(report: Value, role: PortalRole, scope: &str) -> Value {
    let mut payload = role_filtered_report(report, role);
    if let Some(object) = payload.as_object_mut() {
        object.insert("role_context".to_string(), role_envelope(role, scope));
    }
    payload
}

fn build_ueba_api_payload(report: &Value, role: PortalRole) -> Value {
    json!({
        "ok": true,
        "role_context": role_envelope(role, "ueba"),
        "score": report.pointer("/ueba_risk/score").cloned().unwrap_or(Value::Null),
        "severity": report.pointer("/ueba_risk/level").cloned().unwrap_or_else(|| json!("normal")),
        "status": report.pointer("/ueba_risk/status").cloned().unwrap_or_else(|| json!("OK")),
        "score_components": report.pointer("/ueba_risk/score_components").cloned().unwrap_or_else(|| json!({
            "activity_anomaly": 0,
            "time_anomaly": 0,
            "application_anomaly": 0,
            "network_anomaly": 0,
            "history_anomaly": 0,
        })),
        "reason_codes": report.pointer("/ueba_risk/reason_codes").cloned().or_else(|| {
            report
                .pointer("/ueba_risk/reasons")
                .and_then(Value::as_array)
                .map(|items| {
                    Value::Array(
                        items
                            .iter()
                            .filter_map(|item| item.get("code").and_then(Value::as_str).map(Value::from))
                            .collect::<Vec<_>>(),
                    )
                })
        }).unwrap_or_else(|| json!([])),
        "explanation": report.pointer("/ueba_risk/human_explanation")
            .or_else(|| report.pointer("/ueba_risk/summary"))
            .cloned()
            .unwrap_or_else(|| json!("Оценка риска по правилам v1.")),
        "model": {
            "version": "ueba-score-v1",
            "type": "rule_based",
            "formula": "activity anomaly + time anomaly + application anomaly + network anomaly + history anomaly",
            "ml_used": false,
            "llm_used": false
        },
        "risk": report.get("ueba_risk").cloned().unwrap_or_else(|| json!({})),
    })
}

fn build_forensics_contract_payload(report: &Value) -> Value {
    let candidates = report
        .get("risk_incident_candidates")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let investigations = candidates
        .iter()
        .take(20)
        .map(|candidate| {
            let id = candidate
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("candidate-unknown");
            let department = candidate
                .get("department")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_DEPARTMENT_LABEL);
            let host = candidate
                .get("hostname")
                .and_then(Value::as_str)
                .unwrap_or("host-demo");
            json!({
                "investigation_id": format!("case-{id}"),
                "candidate_id": id,
                "title": format!("Проверка кандидата {id}"),
                "status": candidate.pointer("/incident_review/status").and_then(Value::as_str).unwrap_or("NEW"),
                "department": department,
                "risk_level": candidate.get("risk_level").cloned().unwrap_or_else(|| json!("UNKNOWN")),
                "summary": candidate.get("reason").cloned().unwrap_or_else(|| json!("требуется проверка")),
                "timeline": forensics_timeline_for_candidate(candidate),
                "links": {
                    "markdown": format!("/portal/api/investigation-pack/{id}?format=markdown"),
                    "json": format!("/portal/api/investigation-pack/{id}")
                },
                "entities": {
                    "user": candidate.get("owner").cloned().unwrap_or_else(|| json!("employee-demo")),
                    "host": host,
                    "app": "activity-source",
                    "network_event": "not_available"
                }
            })
        })
        .collect::<Vec<_>>();
    json!({
        "contract_version": "forensics-v1",
        "investigations": investigations,
        "timeline_schema": ["timestamp", "kind", "entity", "summary", "source"],
        "demo_privacy": "В demo-режиме использовать только обезличенные user/host/app/network identifiers.",
    })
}

fn forensics_timeline_for_candidate(candidate: &Value) -> Vec<Value> {
    let first_seen = candidate
        .get("first_seen_utc")
        .and_then(Value::as_str)
        .unwrap_or("2026-06-01T09:00:00Z");
    let last_seen = candidate
        .get("last_seen_utc")
        .and_then(Value::as_str)
        .unwrap_or(first_seen);
    let id = candidate
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("candidate-unknown");
    vec![
        json!({
            "timestamp": first_seen,
            "kind": "candidate_created",
            "entity": id,
            "summary": candidate.get("reason").cloned().unwrap_or_else(|| json!("кандидат требует проверки")),
            "source": "risk_rules"
        }),
        json!({
            "timestamp": last_seen,
            "kind": "evidence_snapshot",
            "entity": candidate.get("hostname").and_then(Value::as_str).unwrap_or("host-demo"),
            "summary": "Связка user / host / app / network event подготовлена для ручного расследования.",
            "source": "portal_contract"
        }),
    ]
}

#[derive(Clone, Debug, Serialize)]
struct PfsenseFirewallEvent {
    timestamp: String,
    source_host: String,
    destination: String,
    action: String,
    rule_id: String,
    protocol: String,
}

#[derive(Clone, Debug, Serialize)]
struct PfsenseVpnEvent {
    timestamp: String,
    source_host: String,
    user_ref: String,
    action: String,
    tunnel: String,
}

#[derive(Clone, Debug, Serialize)]
struct PfsenseTopDestination {
    destination: String,
    bytes: u64,
    connections: u64,
}

fn build_pfsense_readiness_payload(role: PortalRole) -> Value {
    let firewall_events = vec![
        PfsenseFirewallEvent {
            timestamp: "2026-06-01T10:00:00Z".to_string(),
            source_host: "host-demo-01".to_string(),
            destination: "203.0.113.10:443".to_string(),
            action: "pass".to_string(),
            rule_id: "demo-fw-allow-web".to_string(),
            protocol: "tcp".to_string(),
        },
        PfsenseFirewallEvent {
            timestamp: "2026-06-01T10:05:00Z".to_string(),
            source_host: "host-demo-02".to_string(),
            destination: "198.51.100.25:22".to_string(),
            action: "block".to_string(),
            rule_id: "demo-fw-block-admin".to_string(),
            protocol: "tcp".to_string(),
        },
    ];
    let vpn_events = vec![PfsenseVpnEvent {
        timestamp: "2026-06-01T08:30:00Z".to_string(),
        source_host: "198.51.100.77".to_string(),
        user_ref: "employee-demo-001".to_string(),
        action: "connect".to_string(),
        tunnel: "vpn-demo".to_string(),
    }];
    let top_destinations = vec![
        PfsenseTopDestination {
            destination: "203.0.113.10".to_string(),
            bytes: 42_000,
            connections: 12,
        },
        PfsenseTopDestination {
            destination: "198.51.100.25".to_string(),
            bytes: 8_000,
            connections: 3,
        },
    ];
    json!({
        "ok": true,
        "role_context": role_envelope(role, "pfsense"),
        "contract_version": "pfsense-readiness-v1",
        "status": "contract_only",
        "siem": false,
        "ingestion_available": false,
        "ingestion_note": "Реальный ingestion не заявлен: подготовлены только контракт, fixtures и API-заготовка.",
        "schemas": {
            "firewall_event": ["timestamp", "source_host", "destination", "action", "rule_id", "protocol"],
            "vpn_event": ["timestamp", "source_host", "user_ref", "action", "tunnel"],
            "traffic_summary": ["timestamp", "source_host", "destination", "bytes", "connections"],
            "top_destination": ["destination", "bytes", "connections"]
        },
        "firewall_events": firewall_events,
        "vpn_events": vpn_events,
        "traffic_summary": {
            "timestamp": "2026-06-01T10:10:00Z",
            "source_host": "host-demo-01",
            "destination": "203.0.113.10",
            "action": "summary",
            "bytes": 42000,
            "connections": 12
        },
        "top_destinations": top_destinations,
        "demo_data_policy": "Используются только RFC 5737 documentation IP ranges и обезличенные identifiers.",
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
                    let name = display_rollup_name(item.get("name").and_then(Value::as_str), key);
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
                        &name,
                        coverage_status(coverage),
                        format!("{coverage:.0}% · active {active}/{users} · {hhmm}"),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn build_business_risk(snapshot: &Snapshot, department_items: &[Value]) -> Vec<BusinessRiskItem> {
    let mut departments = BTreeMap::<String, Option<u8>>::new();
    for item in department_items {
        let department = display_department_name(item.get("label").and_then(Value::as_str));
        let activity = item
            .get("value")
            .and_then(Value::as_str)
            .and_then(first_percent_score);
        departments.insert(department, activity);
    }
    for node in &snapshot.agent_coverage_sla.problem_nodes {
        let department = display_name_or(&node.department, "Не задано");
        departments.entry(department).or_insert(None);
    }
    let mut items = departments
        .into_iter()
        .map(|(department, activity)| business_risk_item(snapshot, &department, activity))
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        business_risk_rank(&right.risk_level)
            .cmp(&business_risk_rank(&left.risk_level))
            .then_with(|| left.trust_score.cmp(&right.trust_score))
            .then_with(|| left.activity_score.cmp(&right.activity_score))
            .then_with(|| left.department.cmp(&right.department))
    });
    items
}

fn business_risk_item(
    snapshot: &Snapshot,
    department: &str,
    activity: Option<u8>,
) -> BusinessRiskItem {
    let activity_score = activity.unwrap_or(0);
    let trend_delta = department_trend_delta(snapshot, department);
    let trend = business_risk_trend_label(trend_delta);
    let (problem_nodes_count, missing_nodes_count, stale_nodes_count) =
        business_risk_problem_counts(snapshot, department);
    let security_events_24h =
        security_events_department_count(&snapshot.security_events_summary, department);
    let trust_score = business_trust_score(snapshot, problem_nodes_count);
    let assessment = business_risk_assessment(
        trust_score,
        activity_score,
        BusinessRiskSignals {
            trend_delta,
            missing_nodes_count,
            stale_nodes_count,
            problem_nodes_count,
            security_events_24h,
            activity_missing: activity.is_none(),
        },
    );
    BusinessRiskItem {
        department: department.to_string(),
        trust_score,
        activity_score,
        trend,
        risk_level: assessment.0,
        reasons: assessment.1,
        recommendation: assessment.2,
        problem_nodes_count,
        missing_nodes_count,
        stale_nodes_count,
        security_events_24h: (security_events_24h > 0).then_some(security_events_24h),
    }
}

fn business_risk_problem_counts(snapshot: &Snapshot, department: &str) -> (usize, usize, usize) {
    let department_problem_nodes = snapshot
        .agent_coverage_sla
        .problem_nodes
        .iter()
        .filter(|node| {
            let node_department = display_name_or(&node.department, "Не задано");
            node_department == department
        })
        .collect::<Vec<_>>();
    let problem_nodes_count = department_problem_nodes.len();
    let missing_nodes_count = department_problem_nodes
        .iter()
        .filter(|node| node.status == "MISSING")
        .count();
    let stale_nodes_count = department_problem_nodes
        .iter()
        .filter(|node| node.status == "STALE")
        .count();
    (problem_nodes_count, missing_nodes_count, stale_nodes_count)
}

fn business_risk_assessment(
    trust_score: u8,
    activity_score: u8,
    signals: BusinessRiskSignals,
) -> (String, Vec<String>, String) {
    let mut score = 0u64;
    let mut reasons = Vec::new();
    if trust_score < 50 {
        score += 40;
        reasons.push("низкая достоверность показателей".to_string());
    } else if trust_score < 75 {
        score += 25;
        reasons.push("низкая достоверность показателей".to_string());
    } else if trust_score < 90 {
        score += 10;
    }
    if activity_score < 35 {
        score += 35;
        reasons.push("низкая активность".to_string());
    } else if activity_score < 60 {
        score += 20;
        reasons.push("низкая активность".to_string());
    } else if activity_score < 75 {
        score += 10;
    }
    if let Some(delta) = signals.trend_delta {
        if delta <= -20.0 {
            score += 25;
            reasons.push("падающий тренд".to_string());
        } else if delta <= -5.0 {
            score += 10;
            reasons.push("падающий тренд".to_string());
        }
    } else if signals.activity_missing {
        score += 15;
        reasons.push("нет свежей телеметрии".to_string());
    }
    if signals.missing_nodes_count > 0 || signals.stale_nodes_count > 0 {
        reasons.push("нет свежей телеметрии".to_string());
    }
    if signals.problem_nodes_count > 0 {
        reasons.push("много проблемных узлов".to_string());
    }
    if signals.security_events_24h > 0 {
        score += signals.security_events_24h.saturating_mul(5).min(25);
        reasons.push(format!(
            "агрегированные события безопасности за 24 часа: {}",
            signals.security_events_24h
        ));
    }
    reasons.sort();
    reasons.dedup();
    score += (signals.problem_nodes_count as u64)
        .saturating_mul(15)
        .min(30);
    let recommendation = business_risk_recommendation(trust_score, activity_score, signals);
    (
        business_risk_level(score).to_string(),
        reasons,
        recommendation,
    )
}

fn business_risk_recommendation(
    trust_score: u8,
    activity_score: u8,
    signals: BusinessRiskSignals,
) -> String {
    if signals.security_events_24h > 0 {
        return "Проверить агрегированные события безопасности подразделения за 24 часа и сопоставить их с активностью сотрудников."
            .to_string();
    }
    if signals.missing_nodes_count > 0 || signals.stale_nodes_count > 0 {
        return "Восстановить свежую телеметрию агентов и проверить expected nodes по подразделению."
            .to_string();
    }
    if trust_score < 75 || signals.problem_nodes_count > 0 {
        return "Проверить качество данных агентов, резервные источники и подтверждение показателей по рабочим местам."
            .to_string();
    }
    if activity_score < 60 || signals.trend_delta.is_some_and(|value| value <= -5.0) {
        return "Поручить ответственному сверить план работ, загрузку сотрудников и причины падения активности."
            .to_string();
    }
    "Держать подразделение под наблюдением; срочных действий не требуется.".to_string()
}

fn build_business_risk_history(snapshot: &Snapshot) -> Vec<BusinessRiskHistoryItem> {
    let Some(trend) = snapshot
        .worktime_management
        .payload
        .as_ref()
        .and_then(|payload| payload.get("trend"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };

    let mut previous_activity = BTreeMap::<String, u8>::new();
    let mut rows = Vec::new();
    for day in trend {
        let date = day
            .get("report_date")
            .or_else(|| day.get("date"))
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let Some(rollups) = day.get("department_rollups").and_then(Value::as_array) else {
            continue;
        };
        let mut current_day = BTreeMap::<String, u8>::new();
        for item in rollups {
            let department = display_department_name(item.get("name").and_then(Value::as_str));
            let activity_score = item
                .get("portfolio_coverage_pct")
                .and_then(Value::as_f64)
                .map(percent_to_score)
                .unwrap_or(0);
            let trend_delta = previous_activity
                .get(&department)
                .map(|previous| f64::from(activity_score) - f64::from(*previous));
            let (problem_nodes_count, missing_nodes_count, stale_nodes_count) =
                business_risk_problem_counts(snapshot, &department);
            let trust_score = business_trust_score(snapshot, problem_nodes_count);
            let (risk_level, reasons, _) = business_risk_assessment(
                trust_score,
                activity_score,
                BusinessRiskSignals {
                    trend_delta,
                    missing_nodes_count,
                    stale_nodes_count,
                    problem_nodes_count,
                    security_events_24h: security_events_department_count(
                        &snapshot.security_events_summary,
                        &department,
                    ),
                    activity_missing: false,
                },
            );
            rows.push(BusinessRiskHistoryItem {
                date: date.clone(),
                department: department.clone(),
                risk_level,
                trust_score,
                activity_score,
                reasons,
            });
            current_day.insert(department, activity_score);
        }
        previous_activity = current_day;
    }

    rows.sort_by(|left, right| {
        left.date
            .cmp(&right.date)
            .then_with(|| left.department.cmp(&right.department))
    });
    let mut dates = rows
        .iter()
        .map(|item| item.date.clone())
        .collect::<Vec<_>>();
    dates.sort();
    dates.dedup();
    let keep_from = dates.len().saturating_sub(30);
    let keep_dates = dates[keep_from..].iter().cloned().collect::<BTreeSet<_>>();
    rows.into_iter()
        .filter(|item| keep_dates.contains(&item.date))
        .collect()
}

fn summarize_business_risk_history(
    history: &[BusinessRiskHistoryItem],
) -> BusinessRiskHistorySummary {
    let mut by_department = BTreeMap::<String, Vec<&BusinessRiskHistoryItem>>::new();
    for item in history {
        by_department
            .entry(item.department.clone())
            .or_default()
            .push(item);
    }
    let mut summary = BusinessRiskHistorySummary::default();
    for items in by_department.values_mut() {
        items.sort_by(|left, right| left.date.cmp(&right.date));
        let Some(first) = items.first() else {
            continue;
        };
        let Some(last) = items.last() else {
            continue;
        };
        let first_rank = business_risk_rank(&first.risk_level);
        let last_rank = business_risk_rank(&last.risk_level);
        if last_rank > first_rank {
            summary.departments_worsened += 1;
        } else if last_rank < first_rank {
            summary.departments_improved += 1;
        }
        if latest_high_risk_streak(items) >= 3 {
            summary.stable_high_risk += 1;
        }
        if business_risk_is_high(&last.risk_level)
            && !items
                .iter()
                .take(items.len().saturating_sub(1))
                .any(|item| business_risk_is_high(&item.risk_level))
        {
            summary.new_high_risk += 1;
        }
    }
    summary
}

fn stable_high_risk_departments(history: &[BusinessRiskHistoryItem], limit: usize) -> Vec<String> {
    let mut by_department = BTreeMap::<String, Vec<&BusinessRiskHistoryItem>>::new();
    for item in history {
        by_department
            .entry(item.department.clone())
            .or_default()
            .push(item);
    }
    let mut departments = by_department
        .into_iter()
        .filter_map(|(department, mut items)| {
            items.sort_by(|left, right| left.date.cmp(&right.date));
            (latest_high_risk_streak(&items) >= 3).then_some(department)
        })
        .collect::<Vec<_>>();
    departments.sort();
    departments.truncate(limit);
    departments
}

fn build_risk_incident_candidates(
    snapshot: &Snapshot,
    risks: &[BusinessRiskItem],
    history: &[BusinessRiskHistoryItem],
) -> Vec<RiskIncidentCandidate> {
    let mut candidates = Vec::new();
    let problem_nodes_by_host = snapshot
        .agent_coverage_sla
        .problem_nodes
        .iter()
        .map(|node| (node.hostname.clone(), node))
        .collect::<BTreeMap<_, _>>();

    candidates.extend(stable_high_risk_candidates(history));
    candidates.extend(low_trust_risk_candidates(risks, snapshot));
    candidates.extend(agent_quality_candidates(snapshot, &problem_nodes_by_host));
    candidates.extend(agent_coverage_candidates(snapshot));
    if candidates.is_empty() {
        candidates.extend(workforce_insight_risk_candidates(snapshot, risks));
    }

    candidates.sort_by(|left, right| {
        risk_incident_candidate_rank(right)
            .cmp(&risk_incident_candidate_rank(left))
            .then_with(|| left.id.cmp(&right.id))
    });
    let mut seen = BTreeSet::new();
    candidates
        .into_iter()
        .filter(|item| seen.insert(item.id.clone()))
        .collect()
}

fn build_reviewed_risk_incident_candidates(
    snapshot: &Snapshot,
    incident_reviews: &IncidentReviewFile,
    audit_entries: &[IncidentReviewAuditEntry],
) -> (Vec<RiskIncidentCandidate>, Vec<BusinessRiskItem>) {
    let department_items = workforce_rollup_items(snapshot, "department_rollups");
    let business_risk = build_business_risk(snapshot, &department_items);
    let business_risk_history = build_business_risk_history(snapshot);
    let mut candidates =
        build_risk_incident_candidates(snapshot, &business_risk, &business_risk_history);
    apply_incident_reviews_to_candidates(&mut candidates, incident_reviews, audit_entries);
    (candidates, business_risk)
}

fn build_risk_heatmap(
    snapshot: &Snapshot,
    business_risk: &[BusinessRiskItem],
    candidates: &[RiskIncidentCandidate],
    cases: &CaseFile,
) -> Vec<RiskHeatmapItem> {
    let open_case_counts = open_case_counts_by_department(candidates, cases);
    let critical_candidate_counts = critical_candidate_counts_by_department(candidates);
    let mut departments = BTreeSet::new();
    for item in business_risk {
        departments.insert(item.department.clone());
    }
    for department in open_case_counts.keys() {
        departments.insert(department.clone());
    }
    for department in critical_candidate_counts.keys() {
        departments.insert(department.clone());
    }
    for department in &snapshot.security_events_summary.top_departments {
        departments.insert(display_department_name(Some(&department.department)));
    }
    let risk_by_department = business_risk
        .iter()
        .map(|item| (item.department.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    let mut items = departments
        .into_iter()
        .map(|department| {
            let risk = risk_by_department.get(department.as_str()).copied();
            let trust_kpi_score = risk.map(|item| item.trust_score);
            let activity_score = risk.map(|item| item.activity_score);
            let agent_coverage_pct =
                risk.and_then(|item| department_agent_coverage_pct(snapshot, item));
            let business_risk_level = risk.map(|item| item.risk_level.clone());
            let open_cases = open_case_counts.get(&department).copied().unwrap_or(0);
            let critical_candidates = critical_candidate_counts
                .get(&department)
                .copied()
                .unwrap_or(0);
            let security_events_24h =
                security_events_department_count(&snapshot.security_events_summary, &department);
            let metrics = RiskLayerMetrics {
                trust_kpi_score,
                activity_score,
                agent_coverage_pct,
                business_risk_level: business_risk_level.as_deref(),
                open_cases,
                critical_candidates,
                security_events_24h,
            };
            let heat_level = risk_heatmap_level(metrics);
            let links = risk_narrative_links(&heat_level, metrics);
            let summary = risk_heatmap_summary(&heat_level, metrics);
            RiskHeatmapItem {
                department,
                trust_kpi_score,
                activity_score,
                agent_coverage_pct,
                business_risk_level,
                open_cases: Some(open_cases),
                critical_candidates: Some(critical_candidates),
                security_events_24h: (security_events_24h > 0).then_some(security_events_24h),
                heat_level,
                links,
                summary: Some(summary),
            }
        })
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        heatmap_rank(&right.heat_level)
            .cmp(&heatmap_rank(&left.heat_level))
            .then_with(|| {
                right
                    .critical_candidates
                    .unwrap_or(0)
                    .cmp(&left.critical_candidates.unwrap_or(0))
            })
            .then_with(|| {
                right
                    .open_cases
                    .unwrap_or(0)
                    .cmp(&left.open_cases.unwrap_or(0))
            })
            .then_with(|| {
                left.trust_kpi_score
                    .unwrap_or(101)
                    .cmp(&right.trust_kpi_score.unwrap_or(101))
            })
            .then_with(|| left.department.cmp(&right.department))
    });
    items
}

fn open_case_counts_by_department(
    candidates: &[RiskIncidentCandidate],
    cases: &CaseFile,
) -> BTreeMap<String, usize> {
    let candidate_departments = candidates
        .iter()
        .map(|item| {
            (
                item.id.as_str(),
                display_name_opt(item.department.as_deref(), "Не задано"),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut counts = BTreeMap::new();
    for item in cases
        .cases
        .values()
        .filter(|item| case_is_open(&item.status))
    {
        let department = candidate_departments
            .get(item.candidate_id.as_str())
            .cloned()
            .unwrap_or_else(|| "Не задано".to_string());
        *counts.entry(department).or_insert(0) += 1;
    }
    counts
}

fn critical_candidate_counts_by_department(
    candidates: &[RiskIncidentCandidate],
) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for item in candidates.iter().filter(|item| {
        matches!(
            item.risk_level.as_deref().unwrap_or("UNKNOWN"),
            "HIGH" | "CRITICAL"
        )
    }) {
        let department = display_name_opt(item.department.as_deref(), "Не задано");
        *counts.entry(department).or_insert(0) += 1;
    }
    counts
}

fn case_is_open(status: &str) -> bool {
    matches!(status, "OPEN" | "IN_PROGRESS")
}

fn department_agent_coverage_pct(snapshot: &Snapshot, risk: &BusinessRiskItem) -> Option<u8> {
    if snapshot.agent_coverage_sla.expected_nodes == 0 {
        return None;
    }
    let penalty = (risk.missing_nodes_count as u8)
        .saturating_mul(40)
        .saturating_add((risk.stale_nodes_count as u8).saturating_mul(25))
        .saturating_add((risk.problem_nodes_count as u8).saturating_mul(10));
    Some(
        snapshot
            .agent_coverage_sla
            .coverage_pct
            .saturating_sub(penalty),
    )
}

fn risk_heatmap_level(metrics: RiskLayerMetrics<'_>) -> String {
    if metrics.trust_kpi_score.is_none()
        && metrics.activity_score.is_none()
        && metrics.agent_coverage_pct.is_none()
        && metrics.business_risk_level.is_none()
        && metrics.open_cases == 0
        && metrics.critical_candidates == 0
        && metrics.security_events_24h == 0
    {
        return "UNKNOWN".to_string();
    }
    let mut score = match metrics.business_risk_level.unwrap_or("UNKNOWN") {
        "CRITICAL" => 80,
        "HIGH" => 60,
        "MEDIUM" => 35,
        "LOW" => 10,
        _ => 0,
    } as u64;
    if let Some(value) = metrics.trust_kpi_score {
        if value < 50 {
            score += 30;
        } else if value < 75 {
            score += 20;
        } else if value < 90 {
            score += 10;
        }
    }
    if let Some(value) = metrics.activity_score {
        if value < 35 {
            score += 25;
        } else if value < 60 {
            score += 15;
        }
    }
    if let Some(value) = metrics.agent_coverage_pct {
        if value < 75 {
            score += 30;
        } else if value < 90 {
            score += 15;
        }
    }
    score += (metrics.open_cases as u64).saturating_mul(15).min(30);
    score += (metrics.critical_candidates as u64)
        .saturating_mul(20)
        .min(40);
    score += metrics.security_events_24h.saturating_mul(5).min(25);
    if score >= 100 {
        "CRITICAL".to_string()
    } else if score >= 70 {
        "HIGH".to_string()
    } else if score >= 35 {
        "MEDIUM".to_string()
    } else {
        "LOW".to_string()
    }
}

fn heatmap_rank(level: &str) -> u8 {
    match level {
        "CRITICAL" => 4,
        "HIGH" => 3,
        "MEDIUM" => 2,
        "LOW" => 1,
        _ => 0,
    }
}

fn risk_narrative_links(heat_level: &str, metrics: RiskLayerMetrics<'_>) -> Vec<RiskNarrativeLink> {
    let mut links = vec![
        RiskNarrativeLink {
            target: "trust_kpi".to_string(),
            label: "Достоверность показателей".to_string(),
            summary: format!(
                "достоверность {}, активность {}",
                optional_score_text(metrics.trust_kpi_score),
                optional_score_text(metrics.activity_score)
            ),
        },
        RiskNarrativeLink {
            target: "business_risk".to_string(),
            label: "Риски подразделений".to_string(),
            summary: format!(
                "уровень {}",
                metrics.business_risk_level.unwrap_or("UNKNOWN")
            ),
        },
        RiskNarrativeLink {
            target: "risk_heatmap".to_string(),
            label: "Карта риска".to_string(),
            summary: format!("итоговый статус карты: {heat_level}"),
        },
        RiskNarrativeLink {
            target: "security_correlation".to_string(),
            label: "Связь рисков и активности".to_string(),
            summary: "связь активности и рисков по подразделению".to_string(),
        },
        RiskNarrativeLink {
            target: "incident_candidates".to_string(),
            label: "Требует проверки".to_string(),
            summary: format!("записей высокого риска: {}", metrics.critical_candidates),
        },
        RiskNarrativeLink {
            target: "cases".to_string(),
            label: "Расследования".to_string(),
            summary: format!("активных расследований: {}", metrics.open_cases),
        },
        RiskNarrativeLink {
            target: "agent_coverage".to_string(),
            label: "Полнота данных".to_string(),
            summary: format!(
                "покрытие {}",
                optional_score_text(metrics.agent_coverage_pct)
            ),
        },
    ];
    if metrics.security_events_24h > 0 {
        links.push(RiskNarrativeLink {
            target: "security_events".to_string(),
            label: "События безопасности".to_string(),
            summary: format!(
                "агрегированных событий за 24 часа: {}",
                metrics.security_events_24h
            ),
        });
    }
    links
}

fn risk_heatmap_summary(heat_level: &str, metrics: RiskLayerMetrics<'_>) -> String {
    format!(
        "Достоверность {} → полнота данных {} → риск подразделения {} → события безопасности {} → карта рисков {} → связь рисков и активности → требует проверки {} → расследования {} → {}",
        optional_score_text(metrics.trust_kpi_score),
        optional_score_text(metrics.agent_coverage_pct),
        metrics.business_risk_level.unwrap_or("UNKNOWN"),
        metrics.security_events_24h,
        heat_level,
        metrics.critical_candidates,
        metrics.open_cases,
        risk_narrative_conclusion(metrics)
    )
}

fn risk_narrative_conclusion(metrics: RiskLayerMetrics<'_>) -> String {
    let mut reasons = Vec::new();
    if metrics.trust_kpi_score.is_some_and(|value| value < 75) {
        reasons.push("низкой достоверности показателей");
    }
    if metrics.activity_score.is_some_and(|value| value < 60) {
        reasons.push("падения активности");
    }
    if metrics.agent_coverage_pct.is_some_and(|value| value < 90)
        || metrics.agent_coverage_pct.is_none()
    {
        reasons.push("неполных данных по рабочим местам");
    }
    if matches!(
        metrics.business_risk_level.unwrap_or("UNKNOWN"),
        "HIGH" | "CRITICAL" | "MEDIUM"
    ) {
        reasons.push("повышенного риска подразделения");
    }
    if metrics.critical_candidates > 0 {
        reasons.push("записей, которые срочно нужно проверить");
    }
    if metrics.open_cases > 0 {
        reasons.push("открытых расследований");
    }
    if metrics.security_events_24h > 0 {
        reasons.push("агрегированных событий безопасности");
    }
    if reasons.is_empty() {
        "связанный риск не выражен".to_string()
    } else {
        format!("риск связан из-за {}", reasons.join(", "))
    }
}

fn build_security_correlation(
    heatmap: &[RiskHeatmapItem],
    security_events_summary: &SecurityEventsSummary,
) -> Vec<SecurityCorrelationItem> {
    let mut items = heatmap
        .iter()
        .map(|item| security_correlation_item(item, security_events_summary))
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        right
            .correlation_score
            .cmp(&left.correlation_score)
            .then_with(|| {
                heatmap_rank(right.business_risk_level.as_deref().unwrap_or("UNKNOWN")).cmp(
                    &heatmap_rank(left.business_risk_level.as_deref().unwrap_or("UNKNOWN")),
                )
            })
            .then_with(|| left.department.cmp(&right.department))
    });
    items
}

fn security_correlation_item(
    item: &RiskHeatmapItem,
    security_events_summary: &SecurityEventsSummary,
) -> SecurityCorrelationItem {
    let mut score = 0u64;
    let mut reasons = Vec::new();
    let trust = item.trust_kpi_score;
    let activity = item.activity_score;
    let business_level = item.business_risk_level.as_deref().unwrap_or("UNKNOWN");
    let critical_candidates = item.critical_candidates.unwrap_or(0);
    let open_cases = item.open_cases.unwrap_or(0);
    let security_events_24h = item.security_events_24h.unwrap_or_else(|| {
        security_events_department_count(security_events_summary, &item.department)
    });

    match business_level {
        "CRITICAL" => score += 40,
        "HIGH" => score += 30,
        "MEDIUM" => score += 15,
        _ => {}
    }
    if matches!(business_level, "HIGH" | "CRITICAL") && trust.is_some_and(|value| value < 75) {
        reasons.push("низкая достоверность показателей + высокий риск".to_string());
    }
    if let Some(value) = trust {
        if value < 50 {
            score += 25;
        } else if value < 75 {
            score += 15;
        }
    }
    if let Some(value) = activity {
        if value < 35 {
            score += 20;
        } else if value < 60 {
            score += 10;
        }
    }
    if activity.is_some_and(|value| value < 60) && critical_candidates > 0 {
        reasons.push("снижение активности + рост записей на проверку".to_string());
    }
    if let Some(value) = item.agent_coverage_pct {
        if value < 75 {
            score += 25;
            reasons.push("много рабочих мест без свежих данных".to_string());
        } else if value < 90 {
            score += 10;
        }
    } else if business_level != "UNKNOWN" {
        reasons.push("полнота данных не подтверждена".to_string());
    }
    if critical_candidates > 0 {
        score += (critical_candidates as u64).saturating_mul(20).min(35);
    }
    if open_cases > 0 {
        score += (open_cases as u64).saturating_mul(15).min(30);
        reasons.push("рост открытых расследований".to_string());
    }
    if security_events_24h > 0 {
        score += security_events_24h.saturating_mul(5).min(25);
        reasons.push(format!(
            "агрегированные события безопасности за 24 часа: {security_events_24h}"
        ));
    }
    if reasons.is_empty() {
        reasons.push("прямая связь активности и рисков не выражена".to_string());
    }
    SecurityCorrelationItem {
        department: item.department.clone(),
        trust_kpi_score: item.trust_kpi_score,
        activity_score: item.activity_score,
        business_risk_level: item.business_risk_level.clone(),
        critical_candidates: item.critical_candidates,
        open_cases: item.open_cases,
        security_events_24h: (security_events_24h > 0).then_some(security_events_24h),
        correlation_score: score.min(100) as u8,
        correlation_reason: reasons.join("; "),
        explanation: Some(security_correlation_explanation(item, &reasons)),
    }
}

fn security_correlation_explanation(item: &RiskHeatmapItem, reasons: &[String]) -> String {
    format!(
        "Связаны слои: достоверность показателей {}, активность {}, риск подразделения {}, события безопасности {}, требует проверки {}, расследования {}. Причина: {}. Для руководителя это означает, что управленческую просадку нужно проверять вместе с качеством данных и очередью ручной проверки.",
        optional_score_text(item.trust_kpi_score),
        optional_score_text(item.activity_score),
        item.business_risk_level.as_deref().unwrap_or("UNKNOWN"),
        item.security_events_24h.unwrap_or(0),
        item.critical_candidates.unwrap_or(0),
        item.open_cases.unwrap_or(0),
        reasons.join("; ")
    )
}

fn build_executive_dashboard(
    snapshot: &Snapshot,
    inputs: ExecutiveDashboardInputs<'_>,
) -> ExecutiveDashboard {
    let trust_kpi_score = executive_trust_kpi_score(snapshot, inputs.agent_quality_explain);
    let agent_coverage_pct = (snapshot.agent_coverage_sla.expected_nodes > 0)
        .then_some(snapshot.agent_coverage_sla.coverage_pct);
    let high_risk_departments = inputs
        .business_risk
        .iter()
        .filter(|item| business_risk_is_high(&item.risk_level))
        .take(10)
        .map(|item| ExecutiveRiskDepartment {
            department: item.department.clone(),
            risk_level: item.risk_level.clone(),
            trust_score: item.trust_score,
            activity_score: item.activity_score,
            reasons: item.reasons.clone(),
        })
        .collect::<Vec<_>>();
    let critical_candidates = inputs
        .candidates
        .iter()
        .filter(|item| {
            matches!(
                item.risk_level.as_deref().unwrap_or("UNKNOWN"),
                "HIGH" | "CRITICAL"
            )
        })
        .take(10)
        .map(|item| ExecutiveCandidateSummary {
            id: item.id.clone(),
            department: item.department.clone(),
            hostname: item.hostname.clone(),
            risk_level: item
                .risk_level
                .clone()
                .unwrap_or_else(|| "UNKNOWN".to_string()),
            reason: item
                .reason
                .clone()
                .unwrap_or_else(|| "требуется проверка".to_string()),
        })
        .collect::<Vec<_>>();
    let open_cases = inputs
        .cases
        .cases
        .values()
        .filter(|item| matches!(item.status.as_str(), "OPEN" | "IN_PROGRESS"))
        .count();
    let resolved_cases_30d = resolved_cases_30d(inputs.cases);
    let forensics_readiness = forensics_readiness(snapshot, inputs.candidates, inputs.evidence);
    let summary = ExecutiveDashboardSummary {
        risk_narrative_status: Some(risk_narrative_status(
            inputs.risk_heatmap,
            inputs.security_correlation,
            &forensics_readiness,
        )),
        main_risk_cause: executive_main_risk_cause(
            inputs.risk_heatmap,
            inputs.security_correlation,
            inputs.security_events_summary,
            &forensics_readiness,
        ),
        main_risk: executive_main_risk(
            &high_risk_departments,
            &critical_candidates,
            &snapshot.agent_coverage_sla,
        ),
        main_improvement: executive_main_improvement(
            inputs.business_risk_history_summary,
            resolved_cases_30d,
            &critical_candidates,
        ),
        main_data_gap: executive_main_data_gap(snapshot, inputs.agent_quality_explain),
    };
    ExecutiveDashboard {
        trust_kpi_score,
        agent_coverage_pct,
        high_risk_departments: Some(high_risk_departments),
        critical_candidates: Some(critical_candidates),
        open_cases: Some(open_cases),
        resolved_cases_30d: Some(resolved_cases_30d),
        forensics_readiness: Some(forensics_readiness),
        security_events_24h: (inputs.security_events_summary.backend != "disabled")
            .then_some(inputs.security_events_summary.events_24h),
        summary,
    }
}

fn executive_trust_kpi_score(
    snapshot: &Snapshot,
    agent_quality_explain: &AgentQualityExplain,
) -> Option<u8> {
    if snapshot.agent_quality_nodes_summary.total_nodes > 0 {
        Some(snapshot.agent_quality_nodes_summary.accepted_kpi_nodes_pct)
    } else if agent_quality_explain.status == "UNKNOWN" {
        None
    } else if agent_quality_explain.kpi_accepted {
        Some(100)
    } else {
        Some(0)
    }
}

fn resolved_cases_30d(cases: &CaseFile) -> usize {
    let since = Utc::now() - chrono::Duration::days(30);
    cases
        .cases
        .values()
        .filter(|item| item.status == "RESOLVED")
        .filter(|item| {
            chrono::DateTime::parse_from_rfc3339(&item.updated_at_utc)
                .map(|timestamp| timestamp.with_timezone(&Utc) >= since)
                .unwrap_or(false)
        })
        .count()
}

fn forensics_readiness(
    snapshot: &Snapshot,
    candidates: &[RiskIncidentCandidate],
    evidence: &DlpEvidenceResponse,
) -> String {
    let has_audit_ready_candidates = candidates
        .iter()
        .any(|item| !item.incident_review_audit.is_empty() || item.incident_review.status != "NEW");
    if evidence.items.iter().any(|item| item.screenshot_available) && has_audit_ready_candidates {
        "READY".to_string()
    } else if !evidence.items.is_empty() || !candidates.is_empty() {
        "PARTIAL".to_string()
    } else if matches!(
        snapshot.agent_quality.quality_status.as_str(),
        "ok" | "fallback" | "degraded"
    ) {
        "OBSERVE".to_string()
    } else {
        "LIMITED".to_string()
    }
}

fn executive_main_risk(
    high_risk_departments: &[ExecutiveRiskDepartment],
    critical_candidates: &[ExecutiveCandidateSummary],
    sla: &AgentCoverageSla,
) -> String {
    if sla.sla_status == "CRITICAL" {
        return "полнота данных критически недостаточна, показатели нерепрезентативны".to_string();
    }
    if let Some(item) = high_risk_departments.first() {
        let reason = item
            .reasons
            .first()
            .cloned()
            .unwrap_or_else(|| "требуется управленческая проверка".to_string());
        return format!(
            "подразделение {}: {} — {}",
            item.department, item.risk_level, reason
        );
    }
    if let Some(item) = critical_candidates.first() {
        return format!(
            "требует проверки {}: {} — {}",
            item.id, item.risk_level, item.reason
        );
    }
    "критичных управленческих рисков в текущем срезе нет".to_string()
}

fn executive_main_improvement(
    history_summary: &BusinessRiskHistorySummary,
    resolved_cases_30d: usize,
    critical_candidates: &[ExecutiveCandidateSummary],
) -> String {
    if resolved_cases_30d > 0 {
        return format!("закрыто дел за 30 дней: {resolved_cases_30d}");
    }
    if history_summary.departments_improved > 0 {
        return format!(
            "улучшились подразделения: {}",
            history_summary.departments_improved
        );
    }
    if critical_candidates.is_empty() {
        return "нет критичных записей для проверки".to_string();
    }
    "улучшение пока не подтверждено накопленной историей".to_string()
}

fn executive_main_data_gap(
    snapshot: &Snapshot,
    agent_quality_explain: &AgentQualityExplain,
) -> String {
    if snapshot.agent_coverage_sla.expected_nodes == 0 {
        return "не настроен список ожидаемых рабочих мест для контроля полноты данных".to_string();
    }
    if snapshot.agent_coverage_sla.coverage_pct < 90 {
        return format!(
            "полнота данных {}%, часть рабочих мест не подтверждает показатели",
            snapshot.agent_coverage_sla.coverage_pct
        );
    }
    if snapshot.security_events_summary.fallback_used {
        return "агрегированные события безопасности недоступны: проверить ClickHouse и переменные SECURITY_EVENTS_BACKEND/CLICKHOUSE_*"
            .to_string();
    }
    if !agent_quality_explain.kpi_accepted {
        return agent_quality_explain.summary.clone();
    }
    "критичных пробелов в данных не выявлено".to_string()
}

fn risk_narrative_status(
    risk_heatmap: &[RiskHeatmapItem],
    security_correlation: &[SecurityCorrelationItem],
    forensics_readiness: &str,
) -> String {
    let worst_heat_rank = risk_heatmap
        .iter()
        .map(|item| heatmap_rank(item.heat_level.as_str()))
        .max()
        .unwrap_or_else(|| heatmap_rank("UNKNOWN"));
    let max_correlation = security_correlation
        .iter()
        .map(|item| item.correlation_score)
        .max()
        .unwrap_or(0);
    if worst_heat_rank >= heatmap_rank("CRITICAL") || max_correlation >= 85 {
        "CRITICAL".to_string()
    } else if worst_heat_rank >= heatmap_rank("HIGH") || max_correlation >= 60 {
        "HIGH_RISK".to_string()
    } else if worst_heat_rank >= heatmap_rank("MEDIUM")
        || max_correlation > 0
        || !matches!(forensics_readiness, "READY" | "OBSERVE")
    {
        "ATTENTION".to_string()
    } else {
        "NORMAL".to_string()
    }
}

fn executive_main_risk_cause(
    risk_heatmap: &[RiskHeatmapItem],
    security_correlation: &[SecurityCorrelationItem],
    security_events_summary: &SecurityEventsSummary,
    forensics_readiness: &str,
) -> Option<String> {
    if security_events_summary.fallback_used {
        return Some(
            "агрегированные события безопасности временно недоступны: ClickHouse не ответил"
                .to_string(),
        );
    }
    if risk_heatmap.is_empty() && security_events_summary.events_24h > 0 {
        return Some(format!(
            "за 24 часа обнаружены агрегированные события безопасности: {}",
            security_events_summary.events_24h
        ));
    }
    let top = risk_heatmap
        .iter()
        .find(|item| !matches!(item.heat_level.as_str(), "LOW" | "UNKNOWN"))?;
    let correlation = security_correlation
        .iter()
        .find(|item| item.department == top.department);
    let mut statement = risk_narrative_statement(top, correlation);
    if !matches!(forensics_readiness, "READY") {
        statement.push_str(&format!(
            " Готовность к расследованию: {forensics_readiness}, материалы требуют проверки."
        ));
    }
    Some(statement)
}

fn risk_narrative_statement(
    item: &RiskHeatmapItem,
    correlation: Option<&SecurityCorrelationItem>,
) -> String {
    let metrics = RiskLayerMetrics {
        trust_kpi_score: item.trust_kpi_score,
        activity_score: item.activity_score,
        agent_coverage_pct: item.agent_coverage_pct,
        business_risk_level: item.business_risk_level.as_deref(),
        open_cases: item.open_cases.unwrap_or(0),
        critical_candidates: item.critical_candidates.unwrap_or(0),
        security_events_24h: item.security_events_24h.unwrap_or(0),
    };
    let correlation_score = correlation
        .map(|value| format!("{}/100", value.correlation_score))
        .unwrap_or_else(|| "0/100".to_string());
    let correlation_reason = correlation
        .map(|value| format!(" Причина корреляции: {}.", value.correlation_reason))
        .unwrap_or_default();
    format!(
        "В подразделении {} связаны слои: достоверность показателей {}, полнота данных {}, риск подразделения {}, события безопасности за 24 часа {}, карта рисков {}, связь рисков и активности {}, требует проверки {}, расследования {}. {}.{}",
        item.department,
        optional_score_text(item.trust_kpi_score),
        optional_score_text(item.agent_coverage_pct),
        item.business_risk_level.as_deref().unwrap_or("UNKNOWN"),
        item.security_events_24h.unwrap_or(0),
        item.heat_level,
        correlation_score,
        item.critical_candidates.unwrap_or(0),
        item.open_cases.unwrap_or(0),
        risk_narrative_conclusion(metrics),
        correlation_reason
    )
}

fn build_investigation_pack(
    snapshot: &Snapshot,
    candidate_id: &str,
    incident_reviews: &IncidentReviewFile,
    audit_entries: &[IncidentReviewAuditEntry],
) -> Result<InvestigationPack> {
    let candidate_id = validate_short_token(candidate_id, "candidate_id", 128)?;
    let (candidates, business_risk) =
        build_reviewed_risk_incident_candidates(snapshot, incident_reviews, audit_entries);
    let candidate = candidates
        .iter()
        .find(|item| item.id == candidate_id)
        .ok_or_else(|| anyhow!("candidate not found: {candidate_id}"))?;
    let related_business_risk = candidate.department.as_deref().and_then(|department| {
        business_risk
            .iter()
            .find(|item| item.department == department)
    });
    let reasons = investigation_reasons(candidate, related_business_risk);
    let review = &candidate.incident_review;
    let trust_kpi_snapshot = investigation_trust_kpi_snapshot(
        candidate,
        related_business_risk,
        &snapshot.agent_quality,
        &snapshot.agent_quality_nodes_summary,
        &snapshot.agent_coverage_sla,
    );
    let agent_quality_snapshot = investigation_agent_quality_snapshot(snapshot, candidate);
    let business_risk_snapshot = related_business_risk
        .map(|item| serde_json::to_value(item).unwrap_or_else(|_| json!({})))
        .unwrap_or_else(|| {
            json!({
                "available": false,
                "department": candidate.department,
                "risk_level": candidate.risk_level,
                "reasons": reasons,
            })
        });
    let mut pack = InvestigationPack {
        candidate_id: candidate.id.clone(),
        department: candidate.department.clone(),
        owner: candidate.owner.clone(),
        hostname: candidate.hostname.clone(),
        risk_level: candidate.risk_level.clone(),
        reasons,
        evidence: candidate.evidence.clone(),
        first_seen_utc: candidate.first_seen_utc.clone(),
        last_seen_utc: candidate.last_seen_utc.clone(),
        current_review_status: review.status.clone(),
        review_comment: review.comment.clone(),
        review_audit_history: candidate.incident_review_audit.clone(),
        trust_kpi_snapshot,
        agent_quality_snapshot,
        business_risk_snapshot,
        markdown: String::new(),
    };
    pack.markdown = render_investigation_pack_markdown(&pack);
    Ok(pack)
}

fn investigation_reasons(
    candidate: &RiskIncidentCandidate,
    business_risk: Option<&BusinessRiskItem>,
) -> Vec<String> {
    let mut reasons = Vec::new();
    if let Some(reason) = candidate
        .reason
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        reasons.push(reason.to_string());
    }
    if let Some(risk) = business_risk {
        reasons.extend(risk.reasons.iter().cloned());
    }
    if reasons.is_empty() {
        reasons.push("кандидат требует ручной проверки".to_string());
    }
    reasons.sort();
    reasons.dedup();
    reasons
}

fn investigation_trust_kpi_snapshot(
    candidate: &RiskIncidentCandidate,
    business_risk: Option<&BusinessRiskItem>,
    agent_quality: &AgentQuality,
    agent_quality_nodes_summary: &AgentQualityNodesSummary,
    agent_coverage_sla: &AgentCoverageSla,
) -> Value {
    json!({
        "department": candidate.department,
        "hostname": candidate.hostname,
        "trust_score": business_risk.map(|item| item.trust_score),
        "activity_score": business_risk.map(|item| item.activity_score),
        "business_risk_level": business_risk.map(|item| item.risk_level.clone()),
        "agent_quality_status": agent_quality.quality_status,
        "agent_quality_source": agent_quality.collector_source,
        "accepted_kpi_nodes_pct": agent_quality_nodes_summary.accepted_kpi_nodes_pct,
        "coverage_sla_status": agent_coverage_sla.sla_status,
        "coverage_pct": agent_coverage_sla.coverage_pct,
        "freshness_pct": agent_coverage_sla.freshness_pct,
    })
}

fn investigation_agent_quality_snapshot(
    snapshot: &Snapshot,
    candidate: &RiskIncidentCandidate,
) -> Value {
    let node = candidate.hostname.as_deref().and_then(|hostname| {
        snapshot
            .agent_quality_nodes
            .iter()
            .find(|item| item.hostname == hostname)
    });
    json!({
        "global": snapshot.agent_quality,
        "global_explain": agent_quality_explain(&snapshot.agent_quality),
        "node": node,
        "nodes_summary": snapshot.agent_quality_nodes_summary,
        "coverage_sla": snapshot.agent_coverage_sla,
    })
}

fn render_investigation_pack_markdown(pack: &InvestigationPack) -> String {
    let mut text = String::new();
    text.push_str("# Пакет расследования кандидата\n\n");
    text.push_str("## Краткое резюме\n\n");
    text.push_str(&format!("- Кандидат: {}\n", pack.candidate_id));
    text.push_str(&format!(
        "- Подразделение: {}\n",
        pack.department.as_deref().unwrap_or("-")
    ));
    text.push_str(&format!(
        "- Ответственный: {}\n",
        pack.owner.as_deref().unwrap_or("-")
    ));
    text.push_str(&format!(
        "- Узел: {}\n",
        pack.hostname.as_deref().unwrap_or("-")
    ));
    text.push_str(&format!(
        "- Риск: {}\n",
        pack.risk_level.as_deref().unwrap_or("UNKNOWN")
    ));
    text.push_str(&format!(
        "- Период наблюдения: {} - {}\n",
        pack.first_seen_utc.as_deref().unwrap_or("-"),
        pack.last_seen_utc.as_deref().unwrap_or("-")
    ));
    text.push_str(&format!(
        "- Текущий статус проверки: {}\n",
        pack.current_review_status
    ));
    text.push_str(&format!(
        "- Комментарий проверки: {}\n",
        pack.review_comment.as_deref().unwrap_or("-")
    ));
    text.push_str("\n## Причины риска\n\n");
    for reason in &pack.reasons {
        text.push_str(&format!("- {reason}\n"));
    }
    text.push_str("\n## Доказательства\n\n");
    if pack.evidence.is_empty() {
        text.push_str("- Материалы отсутствуют, требуется ручная проверка первичных источников.\n");
    } else {
        for item in &pack.evidence {
            text.push_str(&format!("- {item}\n"));
        }
    }
    text.push_str("\n## История проверки\n\n");
    if pack.review_audit_history.is_empty() {
        text.push_str("- История изменений отсутствует.\n");
    } else {
        for entry in &pack.review_audit_history {
            text.push_str(&format!(
                "- {}: {} -> {}, проверяющий={}, комментарий={}\n",
                entry.changed_at_utc,
                entry.old_status,
                entry.new_status,
                entry.reviewer.as_deref().unwrap_or("-"),
                entry.comment.as_deref().unwrap_or("-")
            ));
        }
    }
    text.push_str("\n## Снимок достоверности показателей\n\n");
    append_json_markdown(&mut text, &pack.trust_kpi_snapshot);
    text.push_str("\n## Качество данных\n\n");
    append_json_markdown(&mut text, &pack.agent_quality_snapshot);
    text.push_str("\n## Риск подразделения\n\n");
    append_json_markdown(&mut text, &pack.business_risk_snapshot);
    text.push_str("\n## Вывод\n\n");
    text.push_str("- Пакет является экспортом кандидата для ручной проверки.\n");
    text.push_str("- Автоматическое создание или подтверждение инцидента не выполнялось.\n");
    text.push_str(
        "- Решение принимает ответственный сотрудник после проверки первичных источников.\n",
    );
    text
}

fn append_json_markdown(text: &mut String, value: &Value) {
    text.push_str("```json\n");
    text.push_str(&serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string()));
    text.push_str("\n```\n");
}

fn apply_incident_reviews_to_candidates(
    candidates: &mut [RiskIncidentCandidate],
    reviews: &IncidentReviewFile,
    audit_entries: &[IncidentReviewAuditEntry],
) {
    for candidate in candidates {
        candidate.incident_review =
            reviews
                .reviews
                .get(&candidate.id)
                .cloned()
                .unwrap_or_else(|| IncidentReviewState {
                    candidate_id: candidate.id.clone(),
                    status: "NEW".to_string(),
                    reviewer: None,
                    comment: None,
                    updated_at: String::new(),
                });
        candidate.incident_review_audit = audit_entries
            .iter()
            .filter(|entry| entry.candidate_id == candidate.id)
            .cloned()
            .collect();
    }
}

fn summarize_incident_review_audit(
    entries: &[IncidentReviewAuditEntry],
) -> IncidentReviewAuditSummary {
    let mut summary = IncidentReviewAuditSummary {
        total_changes: entries.len(),
        ..IncidentReviewAuditSummary::default()
    };
    for entry in entries {
        match entry.new_status.as_str() {
            "CONFIRMED" => summary.confirmed_count += 1,
            "FALSE_POSITIVE" => summary.false_positive_count += 1,
            "POSTPONED" => summary.postponed_count += 1,
            _ => {}
        }
        if summary
            .last_change_utc
            .as_ref()
            .is_none_or(|current| entry.changed_at_utc > *current)
        {
            summary.last_change_utc = Some(entry.changed_at_utc.clone());
        }
    }
    summary
}

fn stable_high_risk_candidates(history: &[BusinessRiskHistoryItem]) -> Vec<RiskIncidentCandidate> {
    let mut by_department = BTreeMap::<String, Vec<&BusinessRiskHistoryItem>>::new();
    for item in history {
        by_department
            .entry(item.department.clone())
            .or_default()
            .push(item);
    }
    by_department
        .into_iter()
        .filter_map(|(department, mut items)| {
            items.sort_by(|left, right| left.date.cmp(&right.date));
            let streak = latest_high_risk_streak(&items);
            if streak < 3 {
                return None;
            }
            let streak_items = items.iter().rev().take(streak).collect::<Vec<_>>();
            let first = streak_items.last()?.date.clone();
            let last = streak_items.first()?.date.clone();
            let latest = items.last()?;
            Some(RiskIncidentCandidate {
                id: risk_candidate_id("stable-high-risk", &department, latest.risk_level.as_str()),
                department: Some(department),
                owner: None,
                hostname: None,
                risk_level: Some(latest.risk_level.clone()),
                reason: Some("подразделение 3+ дня HIGH/CRITICAL".to_string()),
                evidence: vec![
                    format!("risk_streak_days={streak}"),
                    format!("latest_activity_score={}%", latest.activity_score),
                    format!("latest_reasons={}", risk_reasons_text(&latest.reasons)),
                ],
                first_seen_utc: Some(date_to_utc_start(&first)),
                last_seen_utc: Some(date_to_utc_end(&last)),
                recommendation: Some(
                    "Открыть управленческую проверку причин устойчивого высокого риска."
                        .to_string(),
                ),
                incident_review: IncidentReviewState::default(),
                incident_review_audit: Vec::new(),
            })
        })
        .collect()
}

fn low_trust_risk_candidates(
    risks: &[BusinessRiskItem],
    snapshot: &Snapshot,
) -> Vec<RiskIncidentCandidate> {
    risks
        .iter()
        .filter(|item| item.trust_score < 50)
        .map(|item| RiskIncidentCandidate {
            id: risk_candidate_id("low-trust", &item.department, &item.risk_level),
            department: Some(item.department.clone()),
            owner: None,
            hostname: None,
            risk_level: Some(item.risk_level.clone()),
            reason: Some("trust_score < 50".to_string()),
            evidence: vec![
                format!("trust_score={}%", item.trust_score),
                format!("activity_score={}%", item.activity_score),
                format!("reasons={}", risk_reasons_text(&item.reasons)),
            ],
            first_seen_utc: Some(snapshot.generated_at_utc.clone()),
            last_seen_utc: Some(snapshot.generated_at_utc.clone()),
            recommendation: Some(item.recommendation.clone()),
            incident_review: IncidentReviewState::default(),
            incident_review_audit: Vec::new(),
        })
        .collect()
}

fn agent_quality_candidates(
    snapshot: &Snapshot,
    problem_nodes_by_host: &BTreeMap<String, &AgentCoverageProblemNode>,
) -> Vec<RiskIncidentCandidate> {
    let mut items = Vec::new();
    for node in &snapshot.agent_quality_nodes {
        let coverage = problem_nodes_by_host.get(&node.hostname).copied();
        if !node.kpi_accepted {
            items.push(RiskIncidentCandidate {
                id: risk_candidate_id("kpi-not-accepted", &node.hostname, &node.status),
                department: coverage.map(|item| display_name_or(&item.department, "Не задано")),
                owner: coverage.map(|item| display_name_or(&item.owner, "Не назначен")),
                hostname: Some(node.hostname.clone()),
                risk_level: Some(agent_quality_candidate_level(node)),
                reason: Some("KPI не принят".to_string()),
                evidence: vec![
                    format!("agent_status={}", node.status),
                    format!("collector_source={}", node.source),
                    format!("sessions_total={}", node.sessions_total),
                    format!("rdp_sessions={}", node.rdp_sessions),
                ],
                first_seen_utc: Some(node.last_seen_utc.clone()),
                last_seen_utc: Some(node.last_seen_utc.clone()),
                recommendation: Some(node.recommendation.clone()),
                incident_review: IncidentReviewState::default(),
                incident_review_audit: Vec::new(),
            });
        }
        if let Some(error) = &node.collector_error {
            items.push(RiskIncidentCandidate {
                id: risk_candidate_id("collector-error", &node.hostname, error),
                department: coverage.map(|item| display_name_or(&item.department, "Не задано")),
                owner: coverage.map(|item| display_name_or(&item.owner, "Не назначен")),
                hostname: Some(node.hostname.clone()),
                risk_level: Some("HIGH".to_string()),
                reason: Some("collector_error".to_string()),
                evidence: vec![
                    format!("collector_error={error}"),
                    format!("collector_source={}", node.source),
                    format!("agent_status={}", node.status),
                ],
                first_seen_utc: Some(node.last_seen_utc.clone()),
                last_seen_utc: Some(node.last_seen_utc.clone()),
                recommendation: Some(
                    "Проверить журнал агента и восстановить основной сбор Windows.".to_string(),
                ),
                incident_review: IncidentReviewState::default(),
                incident_review_audit: Vec::new(),
            });
        }
    }
    items
}

fn agent_coverage_candidates(snapshot: &Snapshot) -> Vec<RiskIncidentCandidate> {
    snapshot
        .agent_coverage_sla
        .problem_nodes
        .iter()
        .filter(|node| matches!(node.status.as_str(), "MISSING" | "STALE"))
        .map(|node| RiskIncidentCandidate {
            id: risk_candidate_id("coverage-node", &node.hostname, &node.status),
            department: Some(display_name_or(&node.department, "Не задано")),
            owner: Some(display_name_or(&node.owner, "Не назначен")),
            hostname: Some(node.hostname.clone()),
            risk_level: Some(if node.status == "MISSING" {
                "HIGH".to_string()
            } else {
                "MEDIUM".to_string()
            }),
            reason: Some(format!("{} node", node.status)),
            evidence: vec![
                format!("coverage_status={}", node.status),
                format!("last_seen_utc={}", node.last_seen_utc),
                format!("sla_status={}", snapshot.agent_coverage_sla.sla_status),
            ],
            first_seen_utc: Some(node.last_seen_utc.clone()),
            last_seen_utc: Some(snapshot.generated_at_utc.clone()),
            recommendation: Some(node.recommendation.clone()),
            incident_review: IncidentReviewState::default(),
            incident_review_audit: Vec::new(),
        })
        .collect()
}

fn workforce_insight_risk_candidates(
    snapshot: &Snapshot,
    risks: &[BusinessRiskItem],
) -> Vec<RiskIncidentCandidate> {
    let primary_risk = risks.first();
    let department = primary_risk
        .map(|item| item.department.clone())
        .unwrap_or_else(|| DEFAULT_DEPARTMENT_LABEL.to_string());
    let risk_level = primary_risk
        .map(|item| item.risk_level.clone())
        .unwrap_or_else(|| "MEDIUM".to_string());
    let recommendation = primary_risk
        .map(|item| item.recommendation.clone())
        .unwrap_or_else(|| {
            "Проверить первичные события ActivityWatch и причины отклонения активности.".to_string()
        });
    workforce_insight_items(snapshot)
        .into_iter()
        .filter(|item| {
            let status = item.get("status").and_then(Value::as_str).unwrap_or("INFO");
            !matches!(status, "OK" | "INFO")
        })
        .take(3)
        .map(|item| {
            let label = item
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or("Отклонение активности");
            let value = item.get("value").and_then(Value::as_str).unwrap_or("");
            RiskIncidentCandidate {
                id: risk_candidate_id(
                    "workforce-insight",
                    &department,
                    &format!("{label}:{value}"),
                ),
                department: Some(department.clone()),
                owner: None,
                hostname: None,
                risk_level: Some(risk_level.clone()),
                reason: Some(label.to_string()),
                evidence: vec![
                    format!("activity_signal={label}"),
                    format!("details={value}"),
                    format!(
                        "security_events_24h={}",
                        snapshot.security_events_summary.events_24h
                    ),
                ],
                first_seen_utc: Some(snapshot.generated_at_utc.clone()),
                last_seen_utc: Some(snapshot.generated_at_utc.clone()),
                recommendation: Some(recommendation.clone()),
                incident_review: IncidentReviewState::default(),
                incident_review_audit: Vec::new(),
            }
        })
        .collect()
}

fn agent_quality_candidate_level(node: &AgentQualityNodeItem) -> String {
    match node.status.as_str() {
        "DEGRADED" => "HIGH",
        "WARNING" => "MEDIUM",
        "UNKNOWN" => "MEDIUM",
        _ if !node.kpi_accepted => "MEDIUM",
        _ => "LOW",
    }
    .to_string()
}

fn risk_incident_candidate_rank(item: &RiskIncidentCandidate) -> u8 {
    business_risk_rank(item.risk_level.as_deref().unwrap_or("UNKNOWN"))
}

fn risk_candidate_id(kind: &str, primary: &str, reason: &str) -> String {
    incident_id("risk-candidate", kind, &format!("{primary}:{reason}"))
}

fn risk_reasons_text(reasons: &[String]) -> String {
    if reasons.is_empty() {
        "нет существенных причин".to_string()
    } else {
        reasons.join("; ")
    }
}

fn date_to_utc_start(date: &str) -> String {
    if date.contains('T') {
        return date.to_string();
    }
    format!("{date}T00:00:00Z")
}

fn date_to_utc_end(date: &str) -> String {
    if date.contains('T') {
        return date.to_string();
    }
    format!("{date}T23:59:59Z")
}

fn latest_high_risk_streak(items: &[&BusinessRiskHistoryItem]) -> usize {
    items
        .iter()
        .rev()
        .take_while(|item| business_risk_is_high(&item.risk_level))
        .count()
}

fn business_risk_is_high(level: &str) -> bool {
    matches!(level, "HIGH" | "CRITICAL")
}

fn percent_to_score(value: f64) -> u8 {
    if !value.is_finite() {
        return 0;
    }
    value.round().clamp(0.0, 100.0) as u8
}

fn optional_score_text(value: Option<u8>) -> String {
    value
        .map(|score| format!("{score}%"))
        .unwrap_or_else(|| "UNKNOWN".to_string())
}

fn first_percent_score(text: &str) -> Option<u8> {
    let bytes = text.as_bytes();
    for index in 0..bytes.len() {
        if bytes[index] == b'%' {
            let mut start = index;
            while start > 0 && bytes[start - 1].is_ascii_digit() {
                start -= 1;
            }
            if start < index {
                let value = text[start..index].parse::<u8>().ok()?;
                return Some(value.min(100));
            }
        }
    }
    None
}

fn department_trend_delta(snapshot: &Snapshot, department: &str) -> Option<f64> {
    let trend = snapshot
        .worktime_management
        .payload
        .as_ref()
        .and_then(|payload| payload.get("trend"))
        .and_then(Value::as_array)?;
    let points = trend
        .iter()
        .filter_map(|day| {
            day.get("department_rollups")
                .and_then(Value::as_array)?
                .iter()
                .find(|item| {
                    display_department_name(item.get("name").and_then(Value::as_str)) == department
                })
                .and_then(|item| item.get("portfolio_coverage_pct").and_then(Value::as_f64))
        })
        .collect::<Vec<_>>();
    if points.len() < 2 {
        return None;
    }
    Some(points.last()? - points.first()?)
}

fn business_risk_trend_label(delta: Option<f64>) -> String {
    match delta {
        Some(value) if value <= -5.0 => "FALLING".to_string(),
        Some(value) if value >= 5.0 => "RISING".to_string(),
        Some(_) => "STABLE".to_string(),
        None => "UNKNOWN".to_string(),
    }
}

fn business_trust_score(snapshot: &Snapshot, problem_nodes: usize) -> u8 {
    let base = if snapshot.agent_coverage_sla.expected_nodes > 0 {
        snapshot.agent_coverage_sla.coverage_pct
    } else if snapshot.agent_quality_nodes_summary.total_nodes > 0 {
        snapshot.agent_quality_nodes_summary.accepted_kpi_nodes_pct
    } else {
        50
    };
    base.saturating_sub((problem_nodes as u8).saturating_mul(20))
}

fn business_risk_level(score: u64) -> &'static str {
    if score >= 80 {
        "CRITICAL"
    } else if score >= 55 {
        "HIGH"
    } else if score >= 25 {
        "MEDIUM"
    } else {
        "LOW"
    }
}

fn business_risk_rank(level: &str) -> u8 {
    match level {
        "CRITICAL" => 4,
        "HIGH" => 3,
        "MEDIUM" => 2,
        "LOW" => 1,
        _ => 0,
    }
}

fn workforce_trend_json(snapshot: &Snapshot) -> Value {
    snapshot
        .worktime_management
        .payload
        .as_ref()
        .and_then(|payload| payload.get("trend"))
        .cloned()
        .map(sanitize_workforce_json)
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
                    let title = display_text_opt(
                        item.get("title").and_then(Value::as_str),
                        "Вывод по активности",
                    );
                    let subject =
                        display_name_opt(item.get("subject").and_then(Value::as_str), "Активность");
                    let severity = item
                        .get("severity")
                        .and_then(Value::as_str)
                        .unwrap_or("INFO");
                    let evidence =
                        display_text_opt(item.get("evidence").and_then(Value::as_str), "");
                    let recommendation =
                        display_text_opt(item.get("recommendation").and_then(Value::as_str), "");
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
                    let name = display_department_name(row.get("name").and_then(Value::as_str));
                    if name == DEFAULT_DEPARTMENT_LABEL {
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
                        key: name.clone(),
                        label: name,
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
            ("dlp_fail", "Критичный сигнал проверки", "dlp"),
            "FAIL",
            risk_weight(&policy, "dlp_fail", 35),
            format!("fail={}", metrics.dlp_fail),
            "Проверить очередь ручной проверки и материалы.",
        );
    }
    if metrics.dlp_warn > 0 {
        push_risk_reason(
            &mut reasons,
            &mut score,
            ("dlp_warn", "Предупреждение проверки", "dlp"),
            "WARN",
            risk_weight(&policy, "dlp_warn", 20),
            format!("warn={}", metrics.dlp_warn),
            "Разобрать предупреждения проверки и подтвердить/отклонить события.",
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
            .unwrap_or("Активность");
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
            ("workforce_anomaly", "Отклонение активности", 10)
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

    let confidence = ueba_confidence(metrics, workforce_policy, snapshot, &policy);
    let risk_sources = risk_sources(&reasons);
    let score = score.min(policy.score_cap.max(1));
    let (level, status) = ueba_risk_level(score);
    let score_components = ueba_score_components(&reasons, score);
    let reason_codes = ueba_reason_codes(&reasons);
    let calculated_from = ueba_calculated_from(
        metrics,
        workforce_policy,
        insight_items,
        ueba_baseline,
        policy_configured,
        policy_error.as_deref(),
    );
    json!({
        "score": score,
        "level": level,
        "severity": level,
        "status": status,
        "summary": format!("Уровень риска: {level}; факторов: {}", reasons.len()),
        "human_explanation": ueba_human_explanation(level, &score_components, reasons.len()),
        "formula": "activity anomaly + time anomaly + application anomaly + network anomaly + history anomaly",
        "score_cap": policy.score_cap.max(1),
        "score_components": score_components,
        "reason_codes": reason_codes,
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
        "note": "Оценка риска по правилам v1: мониторинг только для чтения и приоритизация проверки, без автоматического воздействия на сеть."
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

fn ueba_reason_codes(reasons: &[Value]) -> Vec<String> {
    reasons
        .iter()
        .filter_map(|reason| reason.get("code").and_then(Value::as_str))
        .map(ToString::to_string)
        .collect()
}

fn ueba_component_for_reason(code: &str, source: &str) -> &'static str {
    if matches!(code, "night_activity" | "weekend_activity") {
        "time_anomaly"
    } else if matches!(
        code,
        "dlp_fail"
            | "dlp_warn"
            | "application_classification_gap"
            | "application_classification_gap_large"
    ) {
        "application_anomaly"
    } else if matches!(code, "open_incidents" | "baseline_deviation") {
        "history_anomaly"
    } else if source.contains("network")
        || source.contains("pfsense")
        || source.contains("firewall")
        || source.contains("vpn")
        || code.contains("network")
        || code.contains("firewall")
        || code.contains("vpn")
    {
        "network_anomaly"
    } else {
        "activity_anomaly"
    }
}

fn ueba_score_components(reasons: &[Value], target_score: u64) -> BTreeMap<String, u64> {
    let keys = [
        "activity_anomaly",
        "time_anomaly",
        "application_anomaly",
        "network_anomaly",
        "history_anomaly",
    ];
    let mut raw = keys
        .iter()
        .map(|key| ((*key).to_string(), 0_u64))
        .collect::<BTreeMap<_, _>>();
    for reason in reasons {
        let code = reason
            .get("code")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let source = reason
            .get("source")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let points = reason.get("points").and_then(Value::as_u64).unwrap_or(0);
        let component = ueba_component_for_reason(code, source).to_string();
        *raw.entry(component).or_insert(0) += points;
    }

    let raw_total = raw.values().sum::<u64>();
    if raw_total == 0 || target_score >= raw_total {
        return raw;
    }

    let mut scaled = raw
        .iter()
        .map(|(key, value)| {
            let product = value.saturating_mul(target_score);
            let base = product / raw_total;
            let remainder = product % raw_total;
            (key.clone(), base, remainder)
        })
        .collect::<Vec<_>>();
    let mut assigned = scaled.iter().map(|(_, base, _)| *base).sum::<u64>();
    scaled.sort_by(|left, right| right.2.cmp(&left.2).then_with(|| left.0.cmp(&right.0)));
    for (_, base, _) in scaled.iter_mut() {
        if assigned >= target_score {
            break;
        }
        *base += 1;
        assigned += 1;
    }

    scaled
        .into_iter()
        .map(|(key, value, _)| (key, value))
        .collect::<BTreeMap<_, _>>()
}

fn ueba_human_explanation(
    level: &str,
    components: &BTreeMap<String, u64>,
    reason_count: usize,
) -> String {
    let top_component = components
        .iter()
        .max_by_key(|(_, points)| *points)
        .filter(|(_, points)| **points > 0)
        .map(|(component, points)| format!("{component}: {points}"));
    match top_component {
        Some(component) => format!(
            "UEBA v1 по правилам: уровень {level}, факторов {reason_count}, основной вклад {component}."
        ),
        None => format!("UEBA v1 по правилам: уровень {level}, значимых факторов нет."),
    }
}

fn ueba_risk_level(score: u64) -> (&'static str, &'static str) {
    if score >= 85 {
        ("critical", "FAIL")
    } else if score >= 70 {
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
                display_name_opt(
                    row.get("user").and_then(Value::as_str),
                    &format!("Сотрудник {}", idx + 1),
                )
            };
            let user_id = if anonymize {
                format!("EMPLOYEE-{}", idx + 1)
            } else {
                display_text_opt(
                    row.get("user_id").and_then(Value::as_str),
                    &format!("EMPLOYEE-{}", idx + 1),
                )
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
                "scope_note": "Это не персональный взвешенный показатель: персональный разбор по весам приложений пока отсутствует в данных рабочего времени; веса приложений доступны на уровне портфеля.",
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
    context: ReportMarkdownContext<'_>,
) -> String {
    let mut text = String::new();
    text.push_str("# DetMir оперативный отчет\n\n");
    text.push_str(&format!("Дата снимка: {}\n\n", snapshot.generated_at_utc));
    text.push_str(&format!("Итог: {headline}\n\n"));
    append_linked_risk_narrative_markdown(
        &mut text,
        context.risk_heatmap,
        context.security_correlation,
    );
    append_executive_dashboard_markdown(&mut text, context.executive_dashboard);
    text.push_str("## Ключевые показатели\n\n");
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
        "- Сессии агента: всего={}, активные={}, удаленные={}\n",
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
        context.workforce.departments_count, context.workforce.owners_count
    ));
    text.push_str(&format!(
        "- Автоматические выводы по активности: {}\n",
        context.workforce.insights_count
    ));
    text.push_str(&format!(
        "- Статус тренда: {}\n",
        context.workforce.trend_status
    ));
    text.push_str(&format!(
        "- Сигналы проверки безопасности: ok={}, warn={}, fail={}\n",
        metrics.dlp_ok, metrics.dlp_warn, metrics.dlp_fail
    ));
    text.push_str(&format!(
        "- События безопасности за 24 часа: {}\n",
        security_events_summary_text(context.security_events_summary)
    ));
    text.push_str(&format!(
        "- Материалы проверки: записи={}, скриншоты={}\n",
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
    append_security_events_markdown(&mut text, context.security_events_summary);
    append_business_risk_markdown(&mut text, context.business_risk);
    append_business_risk_history_markdown(
        &mut text,
        context.business_risk_history,
        context.business_risk_history_summary,
    );
    append_risk_heatmap_markdown(&mut text, context.risk_heatmap);
    append_security_correlation_markdown(&mut text, context.security_correlation);
    append_risk_incident_candidates_markdown(&mut text, context.risk_incident_candidates);
    append_incident_review_markdown(&mut text, context.risk_incident_candidates);
    append_incident_review_audit_markdown(
        &mut text,
        context.risk_incident_candidates,
        context.incident_review_audit_summary,
    );
    append_ueba_risk_markdown(&mut text, context.ueba_risk);
    append_workforce_policy_markdown(&mut text, context.workforce_policy);
    text.push_str("\nПримечание: сигналы проверки и расследования являются расчетными выводами и требуют регламентной валидации перед подачей как подтвержденные инциденты.\n");
    text
}

fn append_executive_dashboard_markdown(text: &mut String, dashboard: &ExecutiveDashboard) {
    text.push_str("\n## Сводка руководителя\n\n");
    text.push_str(&format!(
        "- Статус главного вывода: {}\n",
        dashboard
            .summary
            .risk_narrative_status
            .as_deref()
            .unwrap_or("NORMAL")
    ));
    text.push_str(&format!(
        "- Главная причина риска: {}\n",
        dashboard
            .summary
            .main_risk_cause
            .as_deref()
            .unwrap_or("связанный риск не выражен")
    ));
    text.push_str("- Подтверждающие слои: достоверность показателей, полнота данных, риски подразделений, карта рисков, связь рисков и активности, требует проверки, расследования\n");
    text.push_str(&format!(
        "- Достоверность показателей: {}\n",
        dashboard
            .trust_kpi_score
            .map(|value| format!("{value}%"))
            .unwrap_or_else(|| "нет данных".to_string())
    ));
    text.push_str(&format!(
        "- Полнота данных: {}\n",
        dashboard
            .agent_coverage_pct
            .map(|value| format!("{value}%"))
            .unwrap_or_else(|| "не настроено".to_string())
    ));
    text.push_str(&format!(
        "- Подразделения высокого риска: {}\n",
        dashboard
            .high_risk_departments
            .as_ref()
            .map(Vec::len)
            .unwrap_or(0)
    ));
    text.push_str(&format!(
        "- Срочно проверить: {}\n",
        dashboard
            .critical_candidates
            .as_ref()
            .map(Vec::len)
            .unwrap_or(0)
    ));
    text.push_str(&format!(
        "- Активные расследования: {}\n",
        dashboard.open_cases.unwrap_or(0)
    ));
    text.push_str(&format!(
        "- Завершенные расследования за 30 дней: {}\n",
        dashboard.resolved_cases_30d.unwrap_or(0)
    ));
    text.push_str(&format!(
        "- Готовность к расследованию: {}\n",
        dashboard
            .forensics_readiness
            .as_deref()
            .unwrap_or("UNKNOWN")
    ));
    if let Some(events) = dashboard.security_events_24h {
        text.push_str(&format!("- События безопасности за 24 часа: {events}\n"));
    }
    text.push_str(&format!(
        "- Главный риск: {}\n",
        dashboard.summary.main_risk
    ));
    text.push_str(&format!(
        "- Главное улучшение: {}\n",
        dashboard.summary.main_improvement
    ));
    text.push_str(&format!(
        "- Главный пробел в данных: {}\n",
        dashboard.summary.main_data_gap
    ));
}

fn append_agent_quality_markdown(text: &mut String, quality: &AgentQuality) {
    let explain = agent_quality_explain(quality);
    text.push_str("\n## Качество данных\n\n");
    text.push_str(&format!("- Источник: {}\n", quality.collector_source));
    text.push_str(&format!("- Статус: {}\n", explain.status));
    text.push_str(&format!(
        "- Участвует в показателях: {}\n",
        if explain.kpi_accepted {
            "да"
        } else {
            "нет"
        }
    ));
    text.push_str(&format!(
        "- Сессии: всего={}, активные={}, удаленные={}\n",
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
        "- Показатели подтверждены: {}% дней\n",
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
            "- {}: status={}, source={}, показатели={}{}\n",
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
    text.push_str("\n## Качество данных по рабочим местам\n\n");
    text.push_str(&format!("- Всего узлов: {}\n", summary.total_nodes));
    text.push_str(&format!("- OK узлов: {}\n", summary.ok_nodes));
    text.push_str(&format!(
        "- WARNING/DEGRADED узлов: {}\n",
        summary.degraded_nodes
    ));
    text.push_str(&format!("- UNKNOWN узлов: {}\n", summary.unknown_nodes));
    text.push_str(&format!(
        "- Рабочих мест с подтвержденными показателями: {}%\n",
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
            "- {}: status={}, source={}, last_seen={}, sessions={}, удаленные={}, показатели={}{}; рекомендация: {}\n",
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
    text.push_str("\n## Полнота данных\n\n");
    text.push_str(&format!("- Статус полноты данных: {}\n", sla.sla_status));
    text.push_str(&format!("- Ожидается узлов: {}\n", sla.expected_nodes));
    text.push_str(&format!(
        "- Прислали подтвержденные данные за 24 часа: {}\n",
        sla.reporting_nodes_24h
    ));
    text.push_str(&format!("- Устаревшие узлы: {}\n", sla.stale_nodes));
    text.push_str(&format!("- Отсутствующие узлы: {}\n", sla.missing_nodes));
    text.push_str(&format!(
        "- Полнота подтвержденных показателей: {}%\n",
        sla.coverage_pct
    ));
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

fn append_security_events_markdown(text: &mut String, summary: &SecurityEventsSummary) {
    text.push_str("\n## События безопасности за 24 часа\n\n");
    text.push_str(&format!(
        "- Статус: {}\n",
        security_events_summary_status(summary)
    ));
    text.push_str(&format!("- Источник: {}\n", summary.backend));
    text.push_str(&format!("- Событий всего: {}\n", summary.events_24h));
    text.push_str(&format!(
        "- Неуспешные/подозрительные входы: {}/{}\n",
        summary.failed_logins_24h, summary.suspicious_logins_24h
    ));
    text.push_str(&format!("- RDP-сессии: {}\n", summary.rdp_sessions_24h));
    text.push_str(&format!(
        "- Изменения учетных записей: {}\n",
        summary.account_changes_24h
    ));
    text.push_str(&format!("- Ошибки агентов: {}\n", summary.agent_errors_24h));
    text.push_str(&format!(
        "- Последнее событие: {}\n",
        summary.last_event_utc.as_deref().unwrap_or("нет данных")
    ));
    text.push_str(&format!("- Время запроса: {} ms\n", summary.query_ms));
    if summary.fallback_used {
        text.push_str(&format!(
            "- Резервный режим: да; причина: {}\n",
            summary.error.as_deref().unwrap_or("неизвестная ошибка")
        ));
    }
    if summary.top_departments.is_empty() {
        text.push_str("- Топ подразделений: нет данных.\n");
    } else {
        text.push_str("- Топ подразделений:\n");
        for item in &summary.top_departments {
            text.push_str(&format!("  - {}: {}\n", item.department, item.events));
        }
    }
    text.push_str(
        "- Примечание: это агрегированная сводка, а не SIEM-журнал и не сырой лог событий.\n",
    );
}

fn append_business_risk_markdown(text: &mut String, risks: &[BusinessRiskItem]) {
    text.push_str("\n## Риски подразделений\n\n");
    if risks.is_empty() {
        text.push_str("- Риски подразделений пока не рассчитаны.\n");
        return;
    }
    for item in risks.iter().take(10) {
        let reasons = if item.reasons.is_empty() {
            "нет существенных причин".to_string()
        } else {
            item.reasons.join("; ")
        };
        text.push_str(&format!(
            "- {}: уровень={}, доверие={}%, активность={}%, тренд={}, проблемные_рабочие_места={}, отсутствуют={}, устарели={}, события_безопасности_24ч={}\n",
            item.department,
            item.risk_level,
            item.trust_score,
            item.activity_score,
            item.trend,
            item.problem_nodes_count,
            item.missing_nodes_count,
            item.stale_nodes_count,
            item.security_events_24h.unwrap_or(0)
        ));
        text.push_str(&format!("  - причины: {reasons}\n"));
        text.push_str(&format!("  - рекомендация: {}\n", item.recommendation));
    }
}

fn append_business_risk_history_markdown(
    text: &mut String,
    history: &[BusinessRiskHistoryItem],
    summary: &BusinessRiskHistorySummary,
) {
    text.push_str("\n## Динамика бизнес-рисков\n\n");
    text.push_str(&format!("- Ухудшились: {}\n", summary.departments_worsened));
    text.push_str(&format!("- Улучшились: {}\n", summary.departments_improved));
    text.push_str(&format!(
        "- Стабильно высокий риск: {}\n",
        summary.stable_high_risk
    ));
    text.push_str(&format!(
        "- Новый высокий риск: {}\n",
        summary.new_high_risk
    ));
    if history.is_empty() {
        text.push_str("- История бизнес-рисков пока не накоплена.\n");
        return;
    }
    for item in history.iter().rev().take(10) {
        let reasons = if item.reasons.is_empty() {
            "нет существенных причин".to_string()
        } else {
            item.reasons.join("; ")
        };
        text.push_str(&format!(
            "- {} · {}: уровень={}, доверие={}%, активность={}%, причины: {}\n",
            item.date,
            item.department,
            item.risk_level,
            item.trust_score,
            item.activity_score,
            reasons
        ));
    }
}

fn append_risk_heatmap_markdown(text: &mut String, items: &[RiskHeatmapItem]) {
    text.push_str("\n## Карта рисков\n\n");
    if items.is_empty() {
        text.push_str(
            "- Карта рисков пока не рассчитана: недостаточно данных по подразделениям.\n",
        );
        return;
    }
    for item in items.iter().take(10) {
        text.push_str(&format!(
            "- {}: уровень={}, достоверность={}, активность={}, полнота={}, риск_подразделения={}, события_безопасности_24ч={}, активные_расследования={}, срочно_проверить={}\n",
            item.department,
            item.heat_level,
            optional_score_text(item.trust_kpi_score),
            optional_score_text(item.activity_score),
            optional_score_text(item.agent_coverage_pct),
            item.business_risk_level.as_deref().unwrap_or("UNKNOWN"),
            item.security_events_24h.unwrap_or(0),
            item.open_cases.unwrap_or(0),
            item.critical_candidates.unwrap_or(0)
        ));
    }
}

fn append_security_correlation_markdown(text: &mut String, items: &[SecurityCorrelationItem]) {
    text.push_str("\n## Связь рисков и активности\n\n");
    if items.is_empty() {
        text.push_str("- Связь рисков и активности пока не рассчитана: недостаточно данных по подразделениям.\n");
        return;
    }
    text.push_str("- Примечание: это аналитическая связка признаков, она не создает инциденты автоматически.\n");
    for item in items.iter().take(10) {
        text.push_str(&format!(
            "- {}: уровень_взаимосвязи={}/100, достоверность={}, активность={}, риск_подразделения={}, события_безопасности_24ч={}, срочно_проверить={}, активные_расследования={}\n",
            item.department,
            item.correlation_score,
            optional_score_text(item.trust_kpi_score),
            optional_score_text(item.activity_score),
            item.business_risk_level.as_deref().unwrap_or("UNKNOWN"),
            item.security_events_24h.unwrap_or(0),
            item.critical_candidates.unwrap_or(0),
            item.open_cases.unwrap_or(0)
        ));
        text.push_str(&format!("  - причина: {}\n", item.correlation_reason));
    }
}

fn append_linked_risk_narrative_markdown(
    text: &mut String,
    heatmap: &[RiskHeatmapItem],
    correlations: &[SecurityCorrelationItem],
) {
    text.push_str("\n## Главный вывод\n\n");
    if heatmap.is_empty() {
        text.push_str("- Главный вывод пока не сформирован: нет данных по подразделениям.\n");
        return;
    }
    let correlations_by_department = correlations
        .iter()
        .map(|item| (item.department.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    for item in heatmap.iter().take(10) {
        let correlation = correlations_by_department
            .get(item.department.as_str())
            .copied();
        let correlation_text = correlation
            .map(|value| format!("{}/100", value.correlation_score))
            .unwrap_or_else(|| "0/100".to_string());
        text.push_str(&format!(
            "- {} → достоверность {} → полнота данных {} → риск подразделения {} → события безопасности {} → карта рисков {} → связь рисков и активности {} → требует проверки {} → расследования {} → {}\n",
            item.department,
            optional_score_text(item.trust_kpi_score),
            optional_score_text(item.agent_coverage_pct),
            item.business_risk_level.as_deref().unwrap_or("UNKNOWN"),
            item.security_events_24h.unwrap_or(0),
            item.heat_level,
            correlation_text,
            item.critical_candidates.unwrap_or(0),
            item.open_cases.unwrap_or(0),
            item.summary.as_deref().unwrap_or("вывод не сформирован")
        ));
        if let Some(value) = correlation.and_then(|value| value.explanation.as_deref()) {
            text.push_str(&format!("  - объяснение: {value}\n"));
        }
    }
}

fn append_risk_incident_candidates_markdown(
    text: &mut String,
    candidates: &[RiskIncidentCandidate],
) {
    text.push_str("\n## Требует проверки\n\n");
    if candidates.is_empty() {
        text.push_str("- Записи для ручной проверки не найдены.\n");
        return;
    }
    text.push_str("- Важно: эти записи не являются автоматически созданными инцидентами; требуется ручная проверка.\n");
    for item in candidates.iter().take(10) {
        let evidence = if item.evidence.is_empty() {
            "нет материалов".to_string()
        } else {
            item.evidence.join("; ")
        };
        text.push_str(&format!(
            "- {}: подразделение={}, ответственный={}, узел={}, уровень={}, причина={}, первое_обнаружение={}, последнее_обнаружение={}\n",
            item.id,
            item.department.as_deref().unwrap_or("-"),
            item.owner.as_deref().unwrap_or("-"),
            item.hostname.as_deref().unwrap_or("-"),
            item.risk_level.as_deref().unwrap_or("UNKNOWN"),
            item.reason.as_deref().unwrap_or("требуется проверка"),
            item.first_seen_utc.as_deref().unwrap_or("-"),
            item.last_seen_utc.as_deref().unwrap_or("-")
        ));
        text.push_str(&format!("  - материалы: {evidence}\n"));
        text.push_str(&format!(
            "  - рекомендация: {}\n",
            item.recommendation
                .as_deref()
                .unwrap_or("Назначить ответственную ручную проверку.")
        ));
    }
}

fn append_incident_review_markdown(text: &mut String, candidates: &[RiskIncidentCandidate]) {
    text.push_str("\n## Проверка кандидатов в инциденты\n\n");
    if candidates.is_empty() {
        text.push_str("- Очередь проверки пуста.\n");
        return;
    }
    for item in candidates.iter().take(10) {
        let review = &item.incident_review;
        text.push_str(&format!(
            "- {}: status={}, reviewer={}, updated_at={}, comment={}\n",
            review.candidate_id,
            review.status,
            review.reviewer.as_deref().unwrap_or("-"),
            if review.updated_at.is_empty() {
                "-"
            } else {
                review.updated_at.as_str()
            },
            review.comment.as_deref().unwrap_or("-")
        ));
    }
}

fn append_incident_review_audit_markdown(
    text: &mut String,
    candidates: &[RiskIncidentCandidate],
    summary: &IncidentReviewAuditSummary,
) {
    text.push_str("\n## Аудит проверки инцидентов\n\n");
    text.push_str(&format!("- Всего изменений: {}\n", summary.total_changes));
    text.push_str(&format!("- Подтверждено: {}\n", summary.confirmed_count));
    text.push_str(&format!(
        "- Ложных срабатываний: {}\n",
        summary.false_positive_count
    ));
    text.push_str(&format!("- Отложено: {}\n", summary.postponed_count));
    text.push_str(&format!(
        "- Последнее изменение: {}\n",
        summary.last_change_utc.as_deref().unwrap_or("-")
    ));
    let mut shown = 0usize;
    for entry in candidates
        .iter()
        .flat_map(|candidate| candidate.incident_review_audit.iter())
        .rev()
    {
        text.push_str(&format!(
            "- {}: {} -> {}, reviewer={}, changed_at={}, comment={}\n",
            entry.candidate_id,
            entry.old_status,
            entry.new_status,
            entry.reviewer.as_deref().unwrap_or("-"),
            entry.changed_at_utc,
            entry.comment.as_deref().unwrap_or("-")
        ));
        shown += 1;
        if shown >= 20 {
            break;
        }
    }
    if shown == 0 {
        text.push_str("- История изменений отсутствует.\n");
    }
}

fn append_ueba_risk_markdown(text: &mut String, risk: &Value) {
    text.push_str("\n## Оценка риска\n\n");
    text.push_str(&format!(
        "- Оценка: {}/100\n",
        risk.get("score").and_then(Value::as_u64).unwrap_or(0)
    ));
    text.push_str(&format!(
        "- Уровень: {}\n",
        risk.get("level")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
    ));
    text.push_str(&format!(
        "- Формула: {}\n",
        risk.get("formula")
            .and_then(Value::as_str)
            .unwrap_or("sum(reason_points) capped at 100")
    ));
    text.push_str(&format!(
        "- Достоверность: {:.0}%\n",
        risk.get("confidence")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
            * 100.0
    ));
    text.push_str(&format!(
        "- Обычный профиль: {}\n",
        risk.get("baseline_status")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
    ));
    text.push_str(&format!(
        "- Окно обычного профиля: {} дней\n",
        risk.get("baseline_window_days")
            .and_then(Value::as_i64)
            .unwrap_or(default_ueba_baseline_window_days())
    ));
    text.push_str(&format!(
        "- Обычный профиль доступен: сотрудник={}, подразделение={}\n",
        risk.get("user_baseline_available")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        risk.get("department_baseline_available")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    ));
    text.push_str(&format!(
        "- Оценка отклонения: {}\n",
        risk.get("deviation_score")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    ));
    text.push_str(&format!(
        "- Версия правил: {}\n",
        risk.get("policy_version")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
    ));
    if let Some(note) = risk.get("note").and_then(Value::as_str) {
        text.push_str(&format!("- Примечание: {note}\n"));
    }
    text.push_str("\n### Причины риска\n\n");
    let reasons = risk
        .get("reasons")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if reasons.is_empty() {
        text.push_str("- Существенных сигналов риска в текущем срезе нет.\n");
        return;
    }
    for item in reasons.iter().take(12) {
        text.push_str(&format!(
            "- {}: +{} баллов, {}, материал: {}\n",
            item.get("label").and_then(Value::as_str).unwrap_or("-"),
            item.get("points").and_then(Value::as_u64).unwrap_or(0),
            item.get("severity")
                .and_then(Value::as_str)
                .unwrap_or("INFO"),
            item.get("value").and_then(Value::as_str).unwrap_or("-")
        ));
        if let Some(recommendation) = item.get("recommendation").and_then(Value::as_str) {
            text.push_str(&format!("  - рекомендация: {recommendation}\n"));
        }
    }
}

fn append_workforce_policy_markdown(text: &mut String, policy: &Value) {
    if policy.get("configured").and_then(Value::as_bool) != Some(true) {
        text.push_str("\n## Почему такой индекс\n\n");
        text.push_str("- Правила ролей и приложений не настроены.\n");
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
        "- План/приложения/с учетом правил: {}/{}/{}\n",
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
        text.push_str("\n### Проверка правил: вес по умолчанию\n\n");
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
        text.push_str("\n### Разбор по сотрудникам\n\n");
        text.push_str(
            "Важно: это не персональный взвешенный показатель; персональный разбор по весам приложений пока недоступен в данных рабочего времени.\n\n",
        );
        for item in employee_items.iter().take(12) {
            text.push_str(&format!(
                "- {}: {}%, активность {}, план {}, формула `{}`\n",
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
                text.push_str(&format!("  - причина: {reason}\n"));
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
                &format!("Проверки требуют разбора: warn={warn}, fail={fail}"),
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

fn handle_incident_review(mut request: Request, args: &Cli) -> Result<()> {
    let actor = request_actor(&request);
    let mut body = String::new();
    request
        .as_reader()
        .take(32 * 1024)
        .read_to_string(&mut body)?;
    let response = apply_incident_review(args, &actor, &body);
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

fn handle_investigation_pack(
    request: Request,
    args: &Cli,
    snapshot_cache: &SnapshotCache,
    url: &str,
    candidate_id: &str,
) -> Result<()> {
    let snapshot = cached_snapshot(args, snapshot_cache);
    let incident_reviews = load_incident_review_best_effort(args);
    let incident_review_audit = load_incident_review_audit_best_effort(args);
    let pack = build_investigation_pack(
        &snapshot,
        candidate_id,
        &incident_reviews,
        &incident_review_audit,
    );
    match pack {
        Ok(pack) => {
            let format = query_param(url, "format")
                .unwrap_or_else(|| "json".to_string())
                .to_ascii_lowercase();
            if matches!(format.as_str(), "md" | "markdown") {
                let filename = format!(
                    "investigation-pack-{}.md",
                    safe_download_stem(&pack.candidate_id)
                );
                respond_text_download(
                    request,
                    StatusCode(200),
                    &pack.markdown,
                    "text/markdown; charset=utf-8",
                    &filename,
                )
            } else {
                respond_json(request, &pack)
            }
        }
        Err(err) => respond_json_status(
            request,
            StatusCode(404),
            &json!({
                "ok": false,
                "error": err.to_string()
            }),
        ),
    }
}

fn handle_create_case(
    mut request: Request,
    args: &Cli,
    snapshot_cache: &SnapshotCache,
) -> Result<()> {
    let mut body = String::new();
    request
        .as_reader()
        .take(32 * 1024)
        .read_to_string(&mut body)?;
    match apply_create_case(args, &cached_snapshot(args, snapshot_cache), &body) {
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

fn handle_case_status(mut request: Request, args: &Cli, case_id: &str) -> Result<()> {
    let mut body = String::new();
    request
        .as_reader()
        .take(32 * 1024)
        .read_to_string(&mut body)?;
    match apply_case_status(args, case_id, &body) {
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

fn handle_case_details(
    request: Request,
    args: &Cli,
    snapshot_cache: &SnapshotCache,
    url: &str,
    case_id: &str,
) -> Result<()> {
    match build_case_details(args, &cached_snapshot(args, snapshot_cache), case_id) {
        Ok(details) => {
            let format = query_param(url, "format")
                .unwrap_or_else(|| "json".to_string())
                .to_ascii_lowercase();
            if matches!(format.as_str(), "md" | "markdown") {
                let filename = format!("case-{}.md", safe_download_stem(&details.case.case_id));
                respond_text_download(
                    request,
                    StatusCode(200),
                    &details.markdown,
                    "text/markdown; charset=utf-8",
                    &filename,
                )
            } else {
                respond_json(request, &details)
            }
        }
        Err(err) => respond_json_status(
            request,
            StatusCode(404),
            &json!({
                "ok": false,
                "error": err.to_string()
            }),
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

fn apply_incident_review(args: &Cli, actor: &str, body: &str) -> Result<IncidentReviewResponse> {
    let request: IncidentReviewRequest =
        serde_json::from_str(body).map_err(|err| anyhow!("invalid incident review JSON: {err}"))?;
    let candidate_id = validate_short_token(&request.candidate_id, "candidate_id", 128)?;
    let status = validate_incident_review_status(&request.status)?;
    let reviewer = sanitize_optional_text(request.reviewer, 80)
        .or_else(|| Some(sanitize_text(actor, 80)))
        .filter(|value| !value.is_empty());
    let comment = sanitize_optional_text(request.comment, 500);
    let mut state = load_incident_review(args)?;
    let old_status = state
        .reviews
        .get(&candidate_id)
        .map(|review| review.status.clone())
        .unwrap_or_else(|| "NEW".to_string());
    let changed_at_utc = now();
    let review = IncidentReviewState {
        candidate_id: candidate_id.clone(),
        status: status.to_string(),
        reviewer: reviewer.clone(),
        comment: comment.clone(),
        updated_at: changed_at_utc.clone(),
    };
    state.reviews.insert(candidate_id.clone(), review.clone());
    save_incident_review(args, &state)?;
    append_incident_review_audit(
        args,
        &IncidentReviewAuditEntry {
            candidate_id,
            old_status,
            new_status: status.to_string(),
            reviewer,
            comment,
            changed_at_utc,
        },
    )?;
    Ok(IncidentReviewResponse { ok: true, review })
}

fn apply_create_case(args: &Cli, snapshot: &Snapshot, body: &str) -> Result<CaseResponse> {
    let request: CreateCaseRequest =
        serde_json::from_str(body).map_err(|err| anyhow!("invalid case JSON: {err}"))?;
    let candidate_id = validate_short_token(&request.candidate_id, "candidate_id", 128)?;
    let incident_reviews = load_incident_review_best_effort(args);
    let incident_review_audit = load_incident_review_audit_best_effort(args);
    let pack = build_investigation_pack(
        snapshot,
        &candidate_id,
        &incident_reviews,
        &incident_review_audit,
    )?;
    if pack.current_review_status != "CONFIRMED" {
        return Err(anyhow!("case can be created only from CONFIRMED candidate"));
    }

    let mut case_file = load_cases(args)?;
    if let Some(existing) = case_file
        .cases
        .values()
        .find(|item| item.candidate_id == candidate_id && item.status != "ARCHIVED")
        .cloned()
    {
        return Ok(CaseResponse {
            ok: true,
            case: existing,
        });
    }

    let now = now();
    let title = sanitize_optional_text(request.title, 160)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("Дело по кандидату {}", pack.candidate_id));
    let owner = sanitize_optional_text(request.owner, 80)
        .or_else(|| pack.owner.clone())
        .filter(|value| !value.is_empty());
    let summary = sanitize_optional_text(request.summary, 1000)
        .or_else(|| pack.reasons.first().cloned())
        .filter(|value| !value.is_empty());
    let case_id = unique_case_id(&case_file, &pack.candidate_id, &now);
    let case = CaseItem {
        case_id: case_id.clone(),
        candidate_id: pack.candidate_id,
        title,
        status: "OPEN".to_string(),
        owner,
        created_at_utc: now.clone(),
        updated_at_utc: now,
        summary,
        decision: None,
    };
    case_file.cases.insert(case_id, case.clone());
    save_cases(args, &case_file)?;
    Ok(CaseResponse { ok: true, case })
}

fn apply_case_status(args: &Cli, case_id: &str, body: &str) -> Result<CaseResponse> {
    let case_id = validate_short_token(case_id, "case_id", 128)?;
    let request: CaseStatusRequest =
        serde_json::from_str(body).map_err(|err| anyhow!("invalid case status JSON: {err}"))?;
    let status = validate_case_status(&request.status)?;
    let mut case_file = load_cases(args)?;
    let Some(case) = case_file.cases.get_mut(&case_id) else {
        return Err(anyhow!("case not found: {case_id}"));
    };
    case.status = status.to_string();
    if let Some(owner) = sanitize_optional_text(request.owner, 80) {
        case.owner = (!owner.is_empty()).then_some(owner);
    }
    if let Some(summary) = sanitize_optional_text(request.summary, 1000) {
        case.summary = (!summary.is_empty()).then_some(summary);
    }
    if let Some(decision) = sanitize_optional_text(request.decision, 1000) {
        case.decision = (!decision.is_empty()).then_some(decision);
    }
    case.updated_at_utc = now();
    let case = case.clone();
    save_cases(args, &case_file)?;
    Ok(CaseResponse { ok: true, case })
}

fn build_case_list(args: &Cli) -> CaseListResponse {
    let mut cases = load_cases_best_effort(args)
        .cases
        .into_values()
        .collect::<Vec<_>>();
    cases.sort_by(|left, right| {
        right
            .updated_at_utc
            .cmp(&left.updated_at_utc)
            .then_with(|| left.case_id.cmp(&right.case_id))
    });
    CaseListResponse { ok: true, cases }
}

fn build_case_details(
    args: &Cli,
    snapshot: &Snapshot,
    case_id: &str,
) -> Result<CaseDetailsResponse> {
    let case_id = validate_short_token(case_id, "case_id", 128)?;
    let case_file = load_cases(args)?;
    let case = case_file
        .cases
        .get(&case_id)
        .cloned()
        .ok_or_else(|| anyhow!("case not found: {case_id}"))?;
    let incident_reviews = load_incident_review_best_effort(args);
    let incident_review_audit = load_incident_review_audit_best_effort(args);
    let investigation_pack = build_investigation_pack(
        snapshot,
        &case.candidate_id,
        &incident_reviews,
        &incident_review_audit,
    )
    .ok();
    let markdown = render_case_markdown(&case, investigation_pack.as_ref());
    Ok(CaseDetailsResponse {
        ok: true,
        case,
        investigation_pack,
        markdown,
    })
}

fn unique_case_id(case_file: &CaseFile, candidate_id: &str, timestamp: &str) -> String {
    let mut case_id = incident_id("case", candidate_id, timestamp);
    let mut counter = 1usize;
    while case_file.cases.contains_key(&case_id) {
        counter += 1;
        case_id = incident_id("case", candidate_id, &format!("{timestamp}:{counter}"));
    }
    case_id
}

fn render_case_markdown(case: &CaseItem, pack: Option<&InvestigationPack>) -> String {
    let mut text = String::new();
    text.push_str("# Карточка дела\n\n");
    text.push_str("## Сведения о деле\n\n");
    text.push_str(&format!("- Дело: {}\n", case.case_id));
    text.push_str(&format!("- Кандидат: {}\n", case.candidate_id));
    text.push_str(&format!("- Название: {}\n", case.title));
    text.push_str(&format!("- Статус: {}\n", case.status));
    text.push_str(&format!(
        "- Ответственный: {}\n",
        case.owner.as_deref().unwrap_or("-")
    ));
    text.push_str(&format!("- Создано: {}\n", case.created_at_utc));
    text.push_str(&format!("- Обновлено: {}\n", case.updated_at_utc));
    text.push_str(&format!(
        "- Резюме: {}\n",
        case.summary.as_deref().unwrap_or("-")
    ));
    text.push_str(&format!(
        "- Решение: {}\n",
        case.decision.as_deref().unwrap_or("-")
    ));
    text.push_str("\n## Пакет расследования\n\n");
    if let Some(pack) = pack {
        text.push_str(&pack.markdown);
    } else {
        text.push_str("- Пакет расследования недоступен для текущего кандидата.\n");
    }
    text.push_str("\n## Решение по делу\n\n");
    text.push_str(
        case.decision
            .as_deref()
            .unwrap_or("Решение пока не зафиксировано. Дело требует ручного рассмотрения."),
    );
    text.push('\n');
    text
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

fn load_incident_review_best_effort(args: &Cli) -> IncidentReviewFile {
    match load_incident_review(args) {
        Ok(state) => state,
        Err(err) => {
            eprintln!("detmir-portal incident review read failed: {err:#}");
            IncidentReviewFile::default()
        }
    }
}

fn load_incident_review_audit_best_effort(args: &Cli) -> Vec<IncidentReviewAuditEntry> {
    match load_incident_review_audit(args) {
        Ok(entries) => entries,
        Err(err) => {
            eprintln!("detmir-portal incident review audit read failed: {err:#}");
            Vec::new()
        }
    }
}

fn load_cases_best_effort(args: &Cli) -> CaseFile {
    match load_cases(args) {
        Ok(state) => state,
        Err(err) => {
            eprintln!("detmir-portal cases read failed: {err:#}");
            CaseFile::default()
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

fn load_incident_review(args: &Cli) -> Result<IncidentReviewFile> {
    let path = incident_review_path(args);
    if !path.exists() {
        return Ok(IncidentReviewFile::default());
    }
    let data = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&data).with_context(|| format!("parse {}", path.display()))
}

fn load_incident_review_audit(args: &Cli) -> Result<Vec<IncidentReviewAuditEntry>> {
    let path = incident_review_audit_path(args);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let mut entries = Vec::new();
    for (index, line) in data.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let entry = serde_json::from_str(line)
            .with_context(|| format!("parse {} line {}", path.display(), index + 1))?;
        entries.push(entry);
    }
    Ok(entries)
}

fn load_cases(args: &Cli) -> Result<CaseFile> {
    let path = cases_path(args);
    if !path.exists() {
        return Ok(CaseFile::default());
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

fn save_cases(args: &Cli, state: &CaseFile) -> Result<()> {
    let path = cases_path(args);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(state)?)
        .with_context(|| format!("write {}", tmp.display()))?;
    fs::rename(&tmp, &path).with_context(|| format!("rename {}", path.display()))?;
    Ok(())
}

fn save_incident_review(args: &Cli, state: &IncidentReviewFile) -> Result<()> {
    let path = incident_review_path(args);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
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

fn append_incident_review_audit(args: &Cli, entry: &IncidentReviewAuditEntry) -> Result<()> {
    let path = incident_review_audit_path(args);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
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

fn incident_review_path(args: &Cli) -> PathBuf {
    args.state_dir.join("data").join("incident_reviews.json")
}

fn incident_review_audit_path(args: &Cli) -> PathBuf {
    args.state_dir
        .join("data")
        .join("incident_review_audit.jsonl")
}

fn cases_path(args: &Cli) -> PathBuf {
    args.state_dir.join("data").join("cases.json")
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

fn validate_incident_review_status(value: &str) -> Result<&'static str> {
    match value.trim() {
        "NEW" => Ok("NEW"),
        "IN_REVIEW" => Ok("IN_REVIEW"),
        "CONFIRMED" => Ok("CONFIRMED"),
        "FALSE_POSITIVE" => Ok("FALSE_POSITIVE"),
        "POSTPONED" => Ok("POSTPONED"),
        _ => Err(anyhow!("unsupported incident review status")),
    }
}

fn validate_case_status(value: &str) -> Result<&'static str> {
    match value.trim() {
        "OPEN" => Ok("OPEN"),
        "IN_PROGRESS" => Ok("IN_PROGRESS"),
        "RESOLVED" => Ok("RESOLVED"),
        "REJECTED" => Ok("REJECTED"),
        "ARCHIVED" => Ok("ARCHIVED"),
        _ => Err(anyhow!("unsupported case status")),
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
    let portfolio_companies = snapshot
        .one_c_overview
        .payload
        .as_ref()
        .and_then(one_c_overview_count)
        .unwrap_or(0);
    let analytics_records = snapshot
        .one_c
        .payload
        .as_ref()
        .and_then(|value| value.get("companies_total"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let overview_note = if snapshot.one_c_overview.ok {
        format!("компаний в портфеле={portfolio_companies}")
    } else {
        "портфель компаний не получен".to_string()
    };
    block(
        "OK",
        &format!("1C analytics отвечает, {overview_note}, записей аналитики={analytics_records}"),
    )
}

fn one_c_overview_count(value: &Value) -> Option<u64> {
    value.get("count").and_then(Value::as_u64).or_else(|| {
        value
            .get("items")?
            .as_array()
            .map(|items| items.len() as u64)
    })
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
            "status={}, analytics_records={}",
            payload
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            payload
                .get("companies_total")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        ),
        "one_c_overview" => format!(
            "portfolio_companies={}",
            one_c_overview_count(payload).unwrap_or(0)
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

fn respond_text_download(
    request: Request,
    status: StatusCode,
    body: &str,
    content_type: &str,
    download_name: &str,
) -> Result<()> {
    let response = Response::from_string(body.to_string())
        .with_status_code(status)
        .with_header(header("Content-Type", content_type)?)
        .with_header(header("Cache-Control", "no-store")?)
        .with_header(header(
            "Content-Disposition",
            &format!(
                "attachment; filename=\"{}\"",
                download_name.replace('"', "")
            ),
        )?);
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

fn safe_download_stem(value: &str) -> String {
    let stem = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .take(96)
        .collect::<String>();
    if stem.is_empty() {
        "candidate".to_string()
    } else {
        stem
    }
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
        assert_eq!(normalize_path("/portal/architecture"), "/architecture");
        assert_eq!(normalize_path("/api/health"), "/api/health");
        assert!(query_flag("/api/reports?anonymize=1", "anonymize"));
        assert!(query_flag("/api/reports?anonymize=true", "anonymize"));
        assert!(!query_flag("/api/reports?anonymize=0", "anonymize"));
        assert_eq!(
            query_param("/api/investigation-pack/c1?format=markdown", "format").as_deref(),
            Some("markdown")
        );
        assert_eq!(
            parse_investigation_pack_path("/api/investigation-pack/risk-candidate-123").as_deref(),
            Some("risk-candidate-123")
        );
        assert_eq!(safe_download_stem("risk:candidate/1"), "risk_candidate_1");
    }

    #[test]
    fn portal_architecture_page_is_informational_and_status_labeled() {
        assert!(portal_html_route("/architecture").is_some());
        let body = portal_html_route("/architecture").unwrap();
        for marker in [
            "Rust Agent",
            "PowerShell Provider",
            "implemented",
            "planned",
            "future",
            "contract_only",
            "Страница является информационной",
        ] {
            assert!(body.contains(marker), "architecture page missing {marker}");
        }
    }

    #[test]
    fn portal_pilot_demo_navigation_is_present() {
        for marker in [
            "Pilot v1 demo",
            "Executive demo",
            "Manager demo",
            "Security demo",
            "Forensics demo",
            "Admin demo",
            "data-demo-view-mode=\"executive\"",
            "data-demo-view-mode=\"security\"",
            "data-demo-view-mode=\"forensics\"",
            "data-demo-view-mode=\"admin\"",
        ] {
            assert!(
                INDEX_HTML.contains(marker),
                "demo navigation missing {marker}"
            );
        }
    }

    #[test]
    fn api_contract_artifacts_are_valid_and_future_ui_ready() {
        let openapi: Value =
            serde_json::from_str(API_CONTRACT_OPENAPI).expect("OpenAPI contract must be JSON");
        assert_eq!(openapi["openapi"], "3.1.0");
        assert!(
            openapi["info"]["version"]
                .as_str()
                .is_some_and(|value| !value.trim().is_empty())
        );
        for path in [
            "/contracts",
            "/reports",
            "/executive",
            "/workforce",
            "/security",
            "/forensics",
            "/ueba",
            "/pfsense",
            "/incidents",
            "/cases",
            "/readiness/latest",
        ] {
            assert!(
                openapi["paths"][path].is_object(),
                "OpenAPI path missing: {path}"
            );
        }
        assert!(openapi["paths"]["/incident-review"].is_object());
        for required_type in [
            "ContractIndex",
            "ReportsResponse",
            "RoleContext",
            "UebaResponse",
            "PfsenseReadinessResponse",
            "CaseListResponse",
            "IncidentReviewRequest",
            "export interface DetMirPortalApi",
        ] {
            assert!(
                API_CONTRACT_TYPESCRIPT.contains(required_type),
                "TypeScript declaration missing {required_type}"
            );
        }
        for (idx, _) in API_CONTRACT_TYPESCRIPT.match_indices("Promise") {
            assert_eq!(
                API_CONTRACT_TYPESCRIPT[idx + "Promise".len()..]
                    .chars()
                    .next(),
                Some('<'),
                "TypeScript declarations must not contain bare Promise return types"
            );
        }

        let summary = api_contract_summary();
        assert_eq!(summary["ok"], true);
        assert_eq!(summary["api_base"], "/api");
        assert_eq!(
            summary["artifacts"]["openapi"],
            "/api/contracts/openapi.json"
        );
        assert_eq!(
            summary["artifacts"]["typescript"],
            "/api/contracts/typescript.d.ts"
        );
    }

    #[test]
    fn portal_roles_parse_and_enforce_scopes() {
        assert_eq!(PortalRole::parse("executive"), Some(PortalRole::Executive));
        assert_eq!(PortalRole::parse("operations"), Some(PortalRole::Admin));
        assert!(PortalRole::Executive.can_access("workforce"));
        assert!(!PortalRole::Executive.can_access("security"));
        assert!(PortalRole::Security.can_access("incidents"));
        assert!(!PortalRole::Security.can_access("workforce"));
        assert!(PortalRole::Forensics.can_access("forensics"));
        assert!(PortalRole::Admin.can_access("pfsense"));
    }

    #[test]
    fn role_filtered_reports_do_not_cross_default_scopes() {
        let report = json!({
            "generated_at_utc": "2026-06-06T00:00:00Z",
            "headline": "demo",
            "executive_dashboard": {"summary": {"main_risk": "demo"}},
            "workforce": {"department_comparison": []},
            "workforce_policy": {"configured": false},
            "security_events_summary": {"events_24h": 1},
            "security_correlation": [],
            "risk_incident_candidates": [{"id": "candidate-demo"}],
            "ueba_risk": {"score": 10, "level": "low", "status": "WARN", "reasons": [{"code": "activity_anomaly"}]},
            "incident_review_audit_summary": {"total_changes": 0}
        });

        let executive = role_filtered_report(report.clone(), PortalRole::Executive);
        assert!(executive.get("workforce").is_some());
        assert!(executive.get("executive_dashboard").is_some());
        assert!(executive.get("security_events_summary").is_some());
        assert!(executive.get("risk_incident_candidates").is_none());
        assert!(executive.get("security_correlation").is_none());

        let security = role_filtered_report(report.clone(), PortalRole::Security);
        assert!(security.get("ueba_risk").is_some());
        assert!(security.get("risk_incident_candidates").is_some());
        assert!(security.get("workforce").is_none());
        assert!(security.get("workforce_policy").is_none());

        let forensics = role_filtered_report(report, PortalRole::Forensics);
        assert!(forensics.get("forensics").is_some());
        assert!(forensics.get("workforce").is_none());
    }

    #[test]
    fn ueba_and_pfsense_contracts_are_stable_demo_safe() {
        let report = json!({
            "ueba_risk": {
                "score": 55,
                "level": "medium",
                "status": "WARN",
                "summary": "medium risk, 1 reason(s)",
                "score_components": {
                    "activity_anomaly": 15,
                    "time_anomaly": 0,
                    "application_anomaly": 20,
                    "network_anomaly": 0,
                    "history_anomaly": 20
                },
                "reasons": [{"code": "activity_anomaly"}]
            }
        });
        let ueba = build_ueba_api_payload(&report, PortalRole::Security);
        assert_eq!(ueba["score"], 55);
        assert_eq!(ueba["severity"], "medium");
        assert_eq!(ueba["score_components"]["activity_anomaly"], 15);
        assert_eq!(ueba["score_components"]["application_anomaly"], 20);
        assert_eq!(ueba["reason_codes"][0], "activity_anomaly");
        assert_eq!(ueba["model"]["ml_used"], false);
        assert_eq!(ueba["model"]["llm_used"], false);

        let pfsense = build_pfsense_readiness_payload(PortalRole::Security);
        assert_eq!(pfsense["status"], "contract_only");
        assert_eq!(pfsense["siem"], false);
        assert_eq!(pfsense["ingestion_available"], false);
        let text = pfsense.to_string();
        assert!(text.contains("203.0.113."));
        assert!(text.contains("198.51.100."));
        assert!(!text.contains("10.10."));
        assert!(!text.contains("192.168."));
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
        assert!(explain.summary.contains("основным способом Windows"));
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
            "Диагностический режим, данные не засчитываются в показатели активности."
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
        assert!(markdown.contains("## Качество данных"));
        assert!(markdown.contains("Участвует в показателях: нет"));
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
    fn display_department_name_replaces_corrupt_runtime_labels() {
        assert_eq!(display_department_name(Some("Бухгалтерия")), "Бухгалтерия");
        assert_eq!(display_department_name(Some("")), DEFAULT_DEPARTMENT_LABEL);
        assert_eq!(
            display_department_name(Some("Без подразделения")),
            DEFAULT_DEPARTMENT_LABEL
        );
        assert_eq!(
            display_department_name(Some("\u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD}")),
            DEFAULT_DEPARTMENT_LABEL
        );
        assert_eq!(
            display_department_name(Some("  \u{FFFD}\u{FFFD}  ")),
            DEFAULT_DEPARTMENT_LABEL
        );
    }

    #[test]
    fn sanitize_workforce_json_replaces_corrupt_nested_strings() {
        assert_eq!(
            display_text_opt(
                Some("Текущая недогрузка: \u{FFFD}\u{FFFD}\u{FFFD}"),
                "Без значения"
            ),
            "Текущая недогрузка: Без значения"
        );
        let value = sanitize_workforce_json(json!({
            "department_rollups": [{"name": "\u{FFFD}\u{FFFD}"}],
            "rows": [{"user": "\u{FFFD}", "user_id": "HOST\\\u{FFFD}\u{FFFD}"}]
        }));
        assert_eq!(value["department_rollups"][0]["name"], "Без группы");
        assert_eq!(value["rows"][0]["user"], "Пользователь не определён");
        assert_eq!(value["rows"][0]["user_id"], "HOST\\unknown");
        assert!(!serde_json::to_string(&value).unwrap().contains("\\uFFFD"));
    }

    #[cfg(unix)]
    #[test]
    fn run_shell_timeout_kills_grandchildren_without_blocking_stdout() {
        let started = Instant::now();
        let err = run_shell("sh -c 'sleep 5 & wait'", Duration::from_millis(200)).unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(err.to_string().contains("command timed out"));
    }

    #[cfg(unix)]
    #[test]
    fn run_shell_does_not_block_on_background_stdout_handle() {
        let started = Instant::now();
        let (stdout, _, success) =
            run_shell("sh -c 'sleep 5 & printf ok'", Duration::from_secs(2)).unwrap();
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(success);
        assert_eq!(stdout, "ok");
    }

    #[test]
    fn command_json_source_accepts_valid_json_with_nonzero_exit() {
        let source = command_json_source(
            "detmir_status",
            "printf '%s' '{\"severity\":\"OK\",\"ok_for_operator\":false}'; exit 2",
            Duration::from_secs(1),
        );
        assert!(source.ok);
        assert_eq!(source.status, "OK");
        assert!(source.payload.is_some());
        assert!(!source.summary.contains("command returned non-zero"));
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
        assert!(
            nodes[0]
                .recommendation
                .contains("основного источника Windows")
        );
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
    fn business_risk_helpers_map_scores_and_percentages() {
        assert_eq!(first_percent_score("50% · active 1/2"), Some(50));
        assert_eq!(first_percent_score("нет данных"), None);
        assert_eq!(percent_to_score(49.6), 50);
        assert_eq!(percent_to_score(101.0), 100);
        assert_eq!(business_risk_level(0), "LOW");
        assert_eq!(business_risk_level(25), "MEDIUM");
        assert_eq!(business_risk_level(55), "HIGH");
        assert_eq!(business_risk_level(80), "CRITICAL");
        assert_eq!(business_risk_trend_label(Some(-6.0)), "FALLING");
        assert_eq!(business_risk_trend_label(Some(6.0)), "RISING");
        assert_eq!(business_risk_trend_label(Some(0.0)), "STABLE");
        assert_eq!(business_risk_trend_label(None), "UNKNOWN");
    }

    #[test]
    fn business_risk_history_summarizes_negative_trends() {
        let history = vec![
            BusinessRiskHistoryItem {
                date: "2026-06-01".to_string(),
                department: "Продажи".to_string(),
                risk_level: "MEDIUM".to_string(),
                trust_score: 80,
                activity_score: 65,
                reasons: vec!["падающий тренд".to_string()],
            },
            BusinessRiskHistoryItem {
                date: "2026-06-02".to_string(),
                department: "Продажи".to_string(),
                risk_level: "HIGH".to_string(),
                trust_score: 70,
                activity_score: 45,
                reasons: vec!["низкая активность".to_string()],
            },
            BusinessRiskHistoryItem {
                date: "2026-06-03".to_string(),
                department: "Продажи".to_string(),
                risk_level: "HIGH".to_string(),
                trust_score: 70,
                activity_score: 40,
                reasons: vec!["низкая активность".to_string()],
            },
            BusinessRiskHistoryItem {
                date: "2026-06-04".to_string(),
                department: "Продажи".to_string(),
                risk_level: "CRITICAL".to_string(),
                trust_score: 40,
                activity_score: 30,
                reasons: vec!["низкая достоверность показателей".to_string()],
            },
            BusinessRiskHistoryItem {
                date: "2026-06-01".to_string(),
                department: "ИТ".to_string(),
                risk_level: "HIGH".to_string(),
                trust_score: 70,
                activity_score: 40,
                reasons: vec!["низкая активность".to_string()],
            },
            BusinessRiskHistoryItem {
                date: "2026-06-04".to_string(),
                department: "ИТ".to_string(),
                risk_level: "LOW".to_string(),
                trust_score: 95,
                activity_score: 90,
                reasons: Vec::new(),
            },
            BusinessRiskHistoryItem {
                date: "2026-06-01".to_string(),
                department: "Логистика".to_string(),
                risk_level: "LOW".to_string(),
                trust_score: 95,
                activity_score: 90,
                reasons: Vec::new(),
            },
            BusinessRiskHistoryItem {
                date: "2026-06-04".to_string(),
                department: "Логистика".to_string(),
                risk_level: "HIGH".to_string(),
                trust_score: 65,
                activity_score: 45,
                reasons: vec!["низкая активность".to_string()],
            },
        ];

        let summary = summarize_business_risk_history(&history);

        assert_eq!(summary.departments_worsened, 2);
        assert_eq!(summary.departments_improved, 1);
        assert_eq!(summary.stable_high_risk, 1);
        assert_eq!(summary.new_high_risk, 1);
        assert_eq!(
            stable_high_risk_departments(&history, 3),
            vec!["Продажи".to_string()]
        );
        let stable_candidates = stable_high_risk_candidates(&history);
        assert_eq!(stable_candidates.len(), 1);
        assert_eq!(stable_candidates[0].department.as_deref(), Some("Продажи"));
        assert_eq!(stable_candidates[0].risk_level.as_deref(), Some("CRITICAL"));
        assert!(
            stable_candidates[0]
                .reason
                .as_deref()
                .unwrap()
                .contains("3+ дня")
        );
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
    fn incident_review_persists_and_applies_to_candidates() {
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
            telemetry_store_path: dir.path().join("telemetry.jsonl"),
            expected_nodes_path: dir.path().join("expected_nodes.json"),
            security_events_backend: "disabled".to_string(),
            clickhouse_url: "http://127.0.0.1:8123".to_string(),
            clickhouse_database: "analytics_1c".to_string(),
            clickhouse_user: "default".to_string(),
            clickhouse_password: String::new(),
        };
        let body = json!({
            "candidate_id": "risk-candidate-123",
            "status": "CONFIRMED",
            "reviewer": "operator",
            "comment": "confirmed by manual check"
        })
        .to_string();

        let response = apply_incident_review(&args, "fallback-actor", &body).unwrap();

        assert!(response.ok);
        assert_eq!(response.review.status, "CONFIRMED");
        assert!(incident_review_path(&args).ends_with("data/incident_reviews.json"));
        let stored = load_incident_review(&args).unwrap();
        assert_eq!(
            stored.reviews["risk-candidate-123"].comment.as_deref(),
            Some("confirmed by manual check")
        );
        assert!(incident_review_audit_path(&args).ends_with("data/incident_review_audit.jsonl"));
        let audit = load_incident_review_audit(&args).unwrap();
        assert_eq!(audit.len(), 1);
        assert_eq!(audit[0].candidate_id, "risk-candidate-123");
        assert_eq!(audit[0].old_status, "NEW");
        assert_eq!(audit[0].new_status, "CONFIRMED");
        assert_eq!(audit[0].reviewer.as_deref(), Some("operator"));
        assert_eq!(summarize_incident_review_audit(&audit).confirmed_count, 1);
        let mut candidates = vec![RiskIncidentCandidate {
            id: "risk-candidate-123".to_string(),
            department: Some("Подразделение".to_string()),
            owner: None,
            hostname: None,
            risk_level: Some("HIGH".to_string()),
            reason: Some("KPI не принят".to_string()),
            evidence: vec!["test".to_string()],
            first_seen_utc: None,
            last_seen_utc: None,
            recommendation: None,
            incident_review: IncidentReviewState::default(),
            incident_review_audit: Vec::new(),
        }];
        apply_incident_reviews_to_candidates(&mut candidates, &stored, &audit);
        assert_eq!(candidates[0].incident_review.status, "CONFIRMED");
        assert_eq!(candidates[0].incident_review_audit.len(), 1);
        assert_eq!(
            validate_incident_review_status("FALSE_POSITIVE").unwrap(),
            "FALSE_POSITIVE"
        );
        assert!(validate_incident_review_status("AUTO_CREATE").is_err());
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
            security_events_backend: "disabled".to_string(),
            clickhouse_url: "http://127.0.0.1:8123".to_string(),
            clickhouse_database: "analytics_1c".to_string(),
            clickhouse_user: "default".to_string(),
            clickhouse_password: String::new(),
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
            security_events_backend: "disabled".to_string(),
            clickhouse_url: "http://127.0.0.1:8123".to_string(),
            clickhouse_database: "analytics_1c".to_string(),
            clickhouse_user: "default".to_string(),
            clickhouse_password: String::new(),
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
    fn security_events_disabled_summary_is_safe_default() {
        let summary = build_security_events_summary(&SecurityEventsConfig {
            backend: "disabled".to_string(),
            clickhouse_url: "http://127.0.0.1:8123".to_string(),
            clickhouse_database: "analytics_1c".to_string(),
            clickhouse_user: "default".to_string(),
            clickhouse_password: String::new(),
            timeout: Duration::from_millis(10),
        });
        assert_eq!(summary.status, "disabled");
        assert_eq!(summary.backend, "disabled");
        assert!(!summary.fallback_used);
        assert_eq!(security_events_summary_status(&summary), "UNKNOWN");
    }

    #[test]
    fn security_events_clickhouse_bad_database_falls_back() {
        let summary = build_security_events_summary(&SecurityEventsConfig {
            backend: "clickhouse".to_string(),
            clickhouse_url: "http://127.0.0.1:8123".to_string(),
            clickhouse_database: "bad;database".to_string(),
            clickhouse_user: "default".to_string(),
            clickhouse_password: String::new(),
            timeout: Duration::from_millis(10),
        });
        assert_eq!(summary.backend, "clickhouse");
        assert_eq!(summary.status, "fallback");
        assert!(summary.fallback_used);
        assert!(
            summary
                .error
                .as_deref()
                .unwrap_or("")
                .contains("ClickHouse")
        );
    }

    #[test]
    fn security_events_summary_texts_are_pilot_safe() {
        let disabled = SecurityEventsSummary::disabled();
        assert!(
            security_events_summary_text(&disabled)
                .contains("Источник событий безопасности отключён")
        );
        assert!(
            security_events_summary_text(&disabled)
                .contains("Используется локальный режим без ClickHouse")
        );

        let fallback = SecurityEventsSummary::fallback("network unavailable", 1);
        assert!(
            security_events_summary_text(&fallback)
                .contains("События безопасности временно недоступны")
        );
        assert!(
            security_events_summary_text(&fallback)
                .contains("Используется локальный режим без ClickHouse")
        );

        let available = SecurityEventsSummary {
            status: "ok".to_string(),
            backend: "clickhouse".to_string(),
            events_24h: 3,
            failed_logins_24h: 1,
            suspicious_logins_24h: 0,
            rdp_sessions_24h: 2,
            account_changes_24h: 0,
            agent_errors_24h: 0,
            top_departments: Vec::new(),
            last_event_utc: Some("2026-06-04T10:00:00Z".to_string()),
            query_ms: 7,
            fallback_used: false,
            error: None,
        };
        assert!(security_events_summary_text(&available).contains("События безопасности доступны"));
        assert_eq!(
            security_events_executive_text(&disabled),
            "Источник событий безопасности отключён."
        );
        assert_eq!(
            security_events_executive_text(&fallback),
            "События безопасности временно недоступны."
        );
        assert!(!security_events_executive_text(&disabled).contains("ClickHouse"));
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
                            "portfolio_coverage_pct": 50.0,
                            "department_rollups": [
                                {"name": "Бухгалтерия", "portfolio_coverage_pct": 50.0}
                            ]
                        },
                        {
                            "report_date": "2026-06-04",
                            "portfolio_coverage_pct": 50.0,
                            "department_rollups": [
                                {"name": "Бухгалтерия", "portfolio_coverage_pct": 50.0}
                            ]
                        },
                        {
                            "report_date": "2026-06-05",
                            "portfolio_coverage_pct": 50.0,
                            "department_rollups": [
                                {"name": "Бухгалтерия", "portfolio_coverage_pct": 50.0}
                            ]
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
                summary: "status=ok, analytics_records=6134".to_string(),
                error: None,
                payload: Some(json!({"status": "ok", "companies_total": 6134})),
            },
            one_c_overview: SourceStatus {
                ok: true,
                status: "OK".to_string(),
                summary: "portfolio_companies=43".to_string(),
                error: None,
                payload: Some(json!({"count": 43, "items": []})),
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
            security_events_summary: SecurityEventsSummary::disabled(),
        };
        let one_c = one_c_block(&snapshot);
        assert_eq!(one_c.status, "OK");
        assert!(one_c.text.contains("компаний в портфеле=43"));
        assert!(one_c.text.contains("записей аналитики=6134"));
        assert!(!one_c.text.contains("компаний=6134"));
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
            ReportRuntimeInputs {
                incident_state: &IncidentStateFile::default(),
                incident_reviews: &IncidentReviewFile::default(),
                incident_review_audit: &[],
                cases: &CaseFile::default(),
                evidence: &evidence,
            },
            &missing_policy,
            &missing_ueba_policy,
            &baseline_path,
            false,
        );
        assert_eq!(report["operator_ok"], true);
        assert_eq!(report["severity"], "OK");
        assert_eq!(report["security_events_summary"]["status"], "disabled");
        assert_eq!(report["security_events_summary"]["backend"], "disabled");
        assert!(
            report["markdown"]
                .as_str()
                .unwrap()
                .contains("расчетными выводами")
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
                .contains("## Оценка риска")
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
        assert!(report["executive_dashboard"].is_object());
        assert_eq!(report["executive_dashboard"]["trust_kpi_score"], 50);
        assert_eq!(report["executive_dashboard"]["open_cases"], 0);
        assert_eq!(report["executive_dashboard"]["resolved_cases_30d"], 0);
        assert!(
            !report["executive_dashboard"]["critical_candidates"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            report["executive_dashboard"]["summary"]["main_data_gap"]
                .as_str()
                .unwrap()
                .contains("не настроен список")
        );
        assert!(
            report["executive_dashboard"]["summary"]["main_risk_cause"]
                .as_str()
                .unwrap()
                .contains("В подразделении")
        );
        assert_eq!(
            report["executive_dashboard"]["summary"]["risk_narrative_status"],
            "HIGH_RISK"
        );
        let main_risk_cause = report["executive_dashboard"]["summary"]["main_risk_cause"]
            .as_str()
            .unwrap();
        for layer in [
            "достоверность показателей",
            "полнота данных",
            "карта рисков",
            "риск подразделения",
            "связь рисков и активности",
            "требует проверки",
            "расследования",
        ] {
            assert!(
                main_risk_cause.contains(layer),
                "main_risk_cause must mention {layer}"
            );
        }
        assert_eq!(report["business_risk"].as_array().unwrap().len(), 1);
        assert_eq!(report["business_risk"][0]["department"], "Бухгалтерия");
        assert_eq!(report["business_risk"][0]["risk_level"], "MEDIUM");
        assert!(report["business_risk"][0]["reasons"].is_array());
        assert!(
            report["business_risk"][0]["reasons"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item.as_str() == Some("низкая активность"))
        );
        assert!(
            report["business_risk"][0]["recommendation"]
                .as_str()
                .unwrap()
                .contains("Проверить")
        );
        assert_eq!(report["business_risk"][0]["problem_nodes_count"], 0);
        assert_eq!(report["business_risk"][0]["missing_nodes_count"], 0);
        assert_eq!(report["business_risk"][0]["stale_nodes_count"], 0);
        assert!(report["risk_heatmap"].is_array());
        assert!(
            report["risk_heatmap"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["department"] == "Бухгалтерия")
        );
        assert_eq!(report["risk_heatmap"][0]["heat_level"], "HIGH");
        assert_eq!(report["risk_heatmap"][0]["trust_kpi_score"], 50);
        assert_eq!(report["risk_heatmap"][0]["activity_score"], 50);
        assert!(report["risk_heatmap"][0]["links"].is_array());
        let heatmap_link_targets = report["risk_heatmap"][0]["links"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|item| item["target"].as_str())
            .collect::<Vec<_>>();
        for target in [
            "risk_heatmap",
            "trust_kpi",
            "agent_coverage",
            "business_risk",
            "security_correlation",
            "incident_candidates",
            "cases",
        ] {
            assert!(
                heatmap_link_targets.contains(&target),
                "risk heatmap links must include {target}"
            );
        }
        assert!(
            report["risk_heatmap"][0]["summary"]
                .as_str()
                .unwrap()
                .contains("связь рисков и активности")
        );
        assert!(report["security_correlation"].is_array());
        assert_eq!(
            report["security_correlation"][0]["department"],
            "Бухгалтерия"
        );
        assert_eq!(report["security_correlation"][0]["correlation_score"], 40);
        assert!(
            report["security_correlation"][0]["correlation_reason"]
                .as_str()
                .unwrap()
                .contains("полнота данных")
        );
        assert!(
            report["security_correlation"][0]["explanation"]
                .as_str()
                .unwrap()
                .contains("Связаны слои")
        );
        assert_eq!(report["business_risk_history"].as_array().unwrap().len(), 3);
        assert_eq!(
            report["business_risk_history"][0]["department"],
            "Бухгалтерия"
        );
        assert!(report["business_risk_history"][0]["reasons"].is_array());
        assert_eq!(
            report["business_risk_history_summary"]["departments_worsened"],
            0
        );
        assert_eq!(
            report["business_risk_history_summary"]["stable_high_risk"],
            0
        );
        assert!(report["risk_incident_candidates"].is_array());
        assert_eq!(
            report["risk_incident_candidates"].as_array().unwrap().len(),
            1
        );
        assert_eq!(
            report["risk_incident_candidates"][0]["hostname"],
            "HOST-EXAMPLE-DEGRADED"
        );
        assert_eq!(
            report["risk_incident_candidates"][0]["reason"],
            "KPI не принят"
        );
        assert!(
            report["risk_incident_candidates"][0]["evidence"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item.as_str().unwrap().contains("collector_source"))
        );
        assert_eq!(
            report["risk_incident_candidates"][0]["incident_review"]["status"],
            "NEW"
        );
        let candidate_id = report["risk_incident_candidates"][0]["id"]
            .as_str()
            .unwrap();
        let pack =
            build_investigation_pack(&snapshot, candidate_id, &IncidentReviewFile::default(), &[])
                .unwrap();
        assert_eq!(pack.candidate_id, candidate_id);
        assert!(pack.markdown.contains("# Пакет расследования кандидата"));
        assert!(pack.markdown.contains("## Причины риска"));
        assert!(pack.markdown.contains("## Доказательства"));
        assert!(pack.markdown.contains("## История проверки"));
        assert!(pack.trust_kpi_snapshot.is_object());
        assert!(pack.agent_quality_snapshot.is_object());
        assert!(pack.business_risk_snapshot.is_object());
        let case_dir = tempfile::tempdir().unwrap();
        let case_args = Cli {
            bind: "127.0.0.1:0".to_string(),
            status_cmd: "true".to_string(),
            check_cmd: "true".to_string(),
            failed_units_cmd: "true".to_string(),
            worktime_url: "http://127.0.0.1".to_string(),
            one_c_url: "http://127.0.0.1".to_string(),
            workforce_policy_path: case_dir.path().join("workforce-policy.json"),
            ueba_policy_path: case_dir.path().join("ueba-policy.yaml"),
            timeout_seconds: 1,
            state_dir: case_dir.path().join("state"),
            dlp_db_path: case_dir.path().join("dlp.sqlite"),
            evidence_root: case_dir.path().to_path_buf(),
            readiness_bundle_dir: case_dir.path().join("readiness-bundle"),
            evidence_limit: 10,
            evidence_max_bytes: 1024,
            json_smoke: false,
            evidence_only: false,
            evidence_upload_token: None,
            telemetry_api_key: "test-key".to_string(),
            telemetry_store_path: case_dir.path().join("telemetry.jsonl"),
            expected_nodes_path: case_dir.path().join("expected_nodes.json"),
            security_events_backend: "disabled".to_string(),
            clickhouse_url: "http://127.0.0.1:8123".to_string(),
            clickhouse_database: "analytics_1c".to_string(),
            clickhouse_user: "default".to_string(),
            clickhouse_password: String::new(),
        };
        assert!(
            apply_create_case(
                &case_args,
                &snapshot,
                &json!({
                    "candidate_id": candidate_id,
                    "title": "Case must fail before confirmation"
                })
                .to_string()
            )
            .is_err()
        );
        let mut confirmed_reviews = IncidentReviewFile::default();
        confirmed_reviews.reviews.insert(
            candidate_id.to_string(),
            IncidentReviewState {
                candidate_id: candidate_id.to_string(),
                status: "CONFIRMED".to_string(),
                reviewer: Some("operator".to_string()),
                comment: Some("confirmed for case".to_string()),
                updated_at: "2026-06-04T12:00:00Z".to_string(),
            },
        );
        save_incident_review(&case_args, &confirmed_reviews).unwrap();
        let case_response = apply_create_case(
            &case_args,
            &snapshot,
            &json!({
                "candidate_id": candidate_id,
                "title": "Проверка KPI",
                "owner": "operator",
                "summary": "manual case"
            })
            .to_string(),
        )
        .unwrap();
        assert_eq!(case_response.case.status, "OPEN");
        assert!(cases_path(&case_args).ends_with("data/cases.json"));
        let list = build_case_list(&case_args);
        assert_eq!(list.cases.len(), 1);
        let status_response = apply_case_status(
            &case_args,
            &case_response.case.case_id,
            &json!({
                "status": "RESOLVED",
                "decision": "issue resolved"
            })
            .to_string(),
        )
        .unwrap();
        assert_eq!(status_response.case.status, "RESOLVED");
        let details =
            build_case_details(&case_args, &snapshot, &case_response.case.case_id).unwrap();
        assert!(details.investigation_pack.is_some());
        assert!(details.markdown.contains("# Карточка дела"));
        assert!(details.markdown.contains("## Пакет расследования"));
        assert_eq!(validate_case_status("ARCHIVED").unwrap(), "ARCHIVED");
        assert!(validate_case_status("AUTO").is_err());
        assert!(
            build_investigation_pack(
                &snapshot,
                "risk-candidate-absent",
                &IncidentReviewFile::default(),
                &[]
            )
            .is_err()
        );
        assert_eq!(report["incident_review_audit_summary"]["total_changes"], 0);
        assert!(
            report["executive_points"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item.as_str().unwrap().contains("менее 80% рабочих мест"))
        );
        assert!(
            report["executive_points"]
                .as_array()
                .unwrap()
                .iter()
                .next()
                .unwrap()
                .as_str()
                .unwrap()
                .contains("Главный управленческий вывод")
        );
        assert!(
            report["executive_points"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item
                    .as_str()
                    .unwrap()
                    .contains("Статус связанной картины риска"))
        );
        let executive_points_text = report["executive_points"].to_string();
        assert!(!executive_points_text.contains("ClickHouse"));
        assert!(!executive_points_text.contains("SECURITY_EVENTS_BACKEND"));
        assert!(
            report["markdown"]
                .as_str()
                .unwrap()
                .contains("## Сводка руководителя")
        );
        assert!(
            report["markdown"]
                .as_str()
                .unwrap()
                .contains("## Карта рисков")
        );
        assert!(
            report["markdown"]
                .as_str()
                .unwrap()
                .contains("## Связь рисков и активности")
        );
        assert!(
            report["markdown"]
                .as_str()
                .unwrap()
                .contains("## Главный вывод")
        );
        let markdown = report["markdown"].as_str().unwrap();
        assert!(
            markdown.find("## Главный вывод").unwrap()
                < markdown.find("## Ключевые показатели").unwrap()
        );
        assert!(
            markdown.find("## Главный вывод").unwrap()
                < markdown.find("## Сводка руководителя").unwrap()
        );
        assert!(
            markdown.find("## Главный вывод").unwrap()
                < markdown.find("## Качество данных").unwrap()
        );
        assert!(
            report["markdown"]
                .as_str()
                .unwrap()
                .contains("## Качество данных по рабочим местам")
        );
        assert!(
            report["markdown"]
                .as_str()
                .unwrap()
                .contains("## Полнота данных")
        );
        assert!(
            report["markdown"]
                .as_str()
                .unwrap()
                .contains("## Риски подразделений")
        );
        assert!(
            report["markdown"]
                .as_str()
                .unwrap()
                .contains("## Динамика бизнес-рисков")
        );
        assert!(
            report["markdown"]
                .as_str()
                .unwrap()
                .contains("## Требует проверки")
        );
        assert!(
            report["markdown"]
                .as_str()
                .unwrap()
                .contains("## Проверка кандидатов в инциденты")
        );
        assert!(
            report["markdown"]
                .as_str()
                .unwrap()
                .contains("## Аудит проверки инцидентов")
        );
        assert!(report["markdown"].as_str().unwrap().contains("причины:"));
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
            one_c_overview: SourceStatus {
                ok: false,
                status: "FAIL".to_string(),
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
            security_events_summary: SecurityEventsSummary::disabled(),
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
                one_c_overview: SourceStatus {
                    ok: false,
                    status: "FAIL".to_string(),
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
                security_events_summary: SecurityEventsSummary::disabled(),
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
            one_c_overview: SourceStatus {
                ok: false,
                status: "FAIL".to_string(),
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
            security_events_summary: SecurityEventsSummary::disabled(),
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
            one_c_overview: SourceStatus {
                ok: false,
                status: "FAIL".to_string(),
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
            security_events_summary: SecurityEventsSummary::disabled(),
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
                .contains("не персональный взвешенный показатель")
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
