use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use chrono::{SecondsFormat, Utc};
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

#[derive(Debug, Parser)]
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
}

#[derive(Debug, Serialize)]
struct SourceStatus {
    ok: bool,
    status: String,
    summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<Value>,
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

#[derive(Debug)]
struct Snapshot {
    generated_at_utc: String,
    detmir_status: SourceStatus,
    detmir_check: SourceStatus,
    failed_units: SourceStatus,
    worktime: SourceStatus,
    one_c: SourceStatus,
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
    planned_hours_per_day: Option<f64>,
    #[serde(default)]
    default_weight: Option<f64>,
    #[serde(default)]
    application_weights: BTreeMap<String, f64>,
}

#[derive(Debug)]
struct WeightedActivity {
    role: String,
    role_label: String,
    index: Option<u8>,
    planned_seconds: i64,
    app_seconds: i64,
    weighted_seconds: i64,
    matched_applications: usize,
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
        let smoke = json!({
            "health": build_health(&snapshot),
            "summary": build_summary(&snapshot),
            "reports": build_reports(&snapshot, &incident_state, &build_dlp_evidence_response(&args), &args.workforce_policy_path),
            "incidents": build_incidents(&snapshot, &incident_state),
            "dlp_evidence": build_dlp_evidence_response(&args),
        });
        println!("{}", serde_json::to_string_pretty(&smoke)?);
        return Ok(if build_health(&snapshot).ok { 0 } else { 2 });
    }

    let server = Server::http(&args.bind).map_err(|err| anyhow!("bind {}: {err}", args.bind))?;
    eprintln!("detmir-portal listening on http://{}", args.bind);
    for request in server.incoming_requests() {
        let result = if args.evidence_only {
            handle_evidence_only_request(request, &args)
        } else {
            handle_request(request, &args)
        };
        if let Err(err) = result {
            eprintln!("detmir-portal request failed: {err:#}");
        }
    }
    Ok(0)
}

fn handle_request(request: Request, args: &Cli) -> Result<()> {
    let method = request.method().clone();
    let path = normalize_path(request.url());
    if method == Method::Post && path == "/api/incidents/action" {
        return handle_incident_action(request, args);
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
        "/api/health" => respond_json(request, &build_health(&build_snapshot(args))),
        "/api/summary" => respond_json(request, &build_summary(&build_snapshot(args))),
        "/api/operator" => {
            let snapshot = build_snapshot(args);
            let incident_state = load_incident_state_best_effort(args);
            respond_json(request, &build_operator(&snapshot, &incident_state))
        }
        "/api/manager" => {
            let snapshot = build_snapshot(args);
            respond_json(request, &build_manager(&snapshot))
        }
        "/api/owner" => {
            let snapshot = build_snapshot(args);
            respond_json(request, &build_owner(&snapshot))
        }
        "/api/reports" => {
            let snapshot = build_snapshot(args);
            let incident_state = load_incident_state_best_effort(args);
            let evidence = build_dlp_evidence_response(args);
            respond_json(
                request,
                &build_reports(
                    &snapshot,
                    &incident_state,
                    &evidence,
                    &args.workforce_policy_path,
                ),
            )
        }
        "/api/incidents" => {
            let snapshot = build_snapshot(args);
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

fn build_snapshot(args: &Cli) -> Snapshot {
    let timeout = Duration::from_secs(args.timeout_seconds);
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
        one_c: http_json_source(
            "one_c",
            &format!("{}/api/health", args.one_c_url.trim_end_matches('/')),
            timeout,
        ),
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
    let worktime = worktime_block(snapshot);
    let one_c = one_c_block(snapshot);
    let dlp_block_value = dlp_block(snapshot);
    let weighted = load_workforce_policy(workforce_policy_path)
        .ok()
        .flatten()
        .and_then(|policy| weighted_activity(snapshot, &policy, metrics.users_count));
    let headline = if summary.operator_ok && summary.severity == "OK" && metrics.open_incidents == 0
    {
        "Контур DetMir работает штатно, критичных действий не требуется"
    } else if metrics.open_incidents > 0 {
        "Контур DetMir работает, есть открытые вопросы для оператора"
    } else {
        "Контур DetMir требует технической проверки"
    };
    let executive_points = vec![
        format!("Сбор данных: {}. {}", collection.status, collection.text),
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
    let recommendations = owner_recommendations(snapshot, &summary);
    let markdown = render_report_markdown(snapshot, headline, &summary, &metrics, &recommendations);
    json!({
        "generated_at_utc": snapshot.generated_at_utc,
        "period": "оперативный срез за сегодня и текущий runtime",
        "severity": summary.severity,
        "operator_ok": summary.operator_ok,
        "headline": headline,
        "executive_points": executive_points,
        "kpis": [
            report_kpi("Индекс активности", workforce_index_text(metrics.workforce_index), workforce_index_status(metrics.workforce_index), "proxy: активное время / плановое рабочее время"),
            weighted_activity_kpi(weighted.as_ref()),
            report_kpi("Сотрудники", metrics.users_count.to_string(), worktime.status.clone(), "строки worktime за сегодня"),
            report_kpi("Активное время", human_duration(metrics.active_seconds), worktime.status.clone(), "сумма active_seconds"),
            report_kpi("Приложения", metrics.apps_count.to_string(), worktime.status.clone(), "true active applications"),
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
                    report_item("Grafana", grafana.status.clone(), grafana.text.clone()),
                    report_item("1C analytics", one_c.status.clone(), one_c.text.clone())
                ]
            },
            {
                "title": "Работа и управляемость",
                "items": [
                    report_item("Индекс активности", workforce_index_status(metrics.workforce_index), workforce_index_text(metrics.workforce_index)),
                    weighted_activity_item(weighted.as_ref(), workforce_policy_path),
                    report_item("Worktime", worktime.status.clone(), worktime.text.clone()),
                    report_item("Активное время", worktime.status.clone(), human_duration(metrics.active_seconds)),
                    report_item("Приложения", worktime.status.clone(), metrics.apps_count.to_string()),
                    report_item("Отчет", "OK", "готов к передаче руководителю")
                ]
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
        "workforce_policy": workforce_policy_json(weighted.as_ref(), workforce_policy_path),
        "markdown": markdown,
        "links": links()
    })
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

fn weighted_activity(
    snapshot: &Snapshot,
    policy: &WorkforcePolicy,
    users_count: usize,
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
        let weight = application_weight(role_policy, name, default_weight);
        if weight > 0.0 {
            matched_applications += 1;
        }
        weighted_seconds += seconds as f64 * weight;
    }
    let weighted_seconds_i64 = weighted_seconds.round() as i64;
    Some(WeightedActivity {
        role: role.to_string(),
        role_label: role_policy
            .label
            .clone()
            .unwrap_or_else(|| role.to_string()),
        index: Some(
            ((weighted_seconds / planned_seconds as f64) * 100.0)
                .round()
                .clamp(0.0, 100.0) as u8,
        ),
        planned_seconds,
        app_seconds,
        weighted_seconds: weighted_seconds_i64,
        matched_applications,
    })
}

fn application_weight(
    role_policy: &WorkforceRolePolicy,
    application: &str,
    default_weight: f64,
) -> f64 {
    let app = application.to_lowercase();
    role_policy
        .application_weights
        .iter()
        .find_map(|(pattern, weight)| {
            let pattern = pattern.to_lowercase();
            (!pattern.is_empty() && app.contains(&pattern)).then_some(weight.clamp(0.0, 1.0))
        })
        .unwrap_or(default_weight)
}

fn weighted_activity_kpi(weighted: Option<&WeightedActivity>) -> Value {
    match weighted {
        Some(weighted) => report_kpi(
            "Взвешенная активность",
            workforce_index_text(weighted.index),
            workforce_index_status(weighted.index),
            &format!("role={} по весам приложений", weighted.role_label),
        ),
        None => report_kpi(
            "Взвешенная активность",
            "не настроена".to_string(),
            "UNKNOWN".to_string(),
            "нужен workforce policy с весами приложений",
        ),
    }
}

fn weighted_activity_item(weighted: Option<&WeightedActivity>, policy_path: &Path) -> Value {
    match weighted {
        Some(weighted) => report_item(
            "Взвешенная активность",
            workforce_index_status(weighted.index),
            format!(
                "{}; роль {}; weighted {}; apps {}",
                workforce_index_text(weighted.index),
                weighted.role_label,
                human_duration(weighted.weighted_seconds),
                weighted.matched_applications
            ),
        ),
        None => report_item(
            "Взвешенная активность",
            "UNKNOWN",
            format!("policy не настроена: {}", policy_path.display()),
        ),
    }
}

fn workforce_policy_json(weighted: Option<&WeightedActivity>, policy_path: &Path) -> Value {
    match weighted {
        Some(weighted) => json!({
            "configured": true,
            "path": policy_path.display().to_string(),
            "role": weighted.role,
            "role_label": weighted.role_label,
            "index": weighted.index,
            "planned_seconds": weighted.planned_seconds,
            "app_seconds": weighted.app_seconds,
            "weighted_seconds": weighted.weighted_seconds,
            "matched_applications": weighted.matched_applications,
        }),
        None => json!({
            "configured": false,
            "path": policy_path.display().to_string(),
            "note": "weighted activity requires role/application policy",
        }),
    }
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
) -> String {
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
        "- Сотрудники за сегодня: {}\n",
        metrics.users_count
    ));
    text.push_str(&format!(
        "- Активное время: {}\n",
        human_duration(metrics.active_seconds)
    ));
    text.push_str(&format!("- Активные приложения: {}\n", metrics.apps_count));
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
    text.push_str("\nПримечание: DLP/case показатели являются derived detections/cases и требуют регламентной валидации перед подачей как подтвержденные инциденты.\n");
    text
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
            timeout_seconds: 1,
            state_dir: dir.path().join("state"),
            dlp_db_path: dir.path().join("dlp.sqlite"),
            evidence_root: dir.path().to_path_buf(),
            evidence_limit: 10,
            evidence_max_bytes: 1024,
            json_smoke: false,
            evidence_only: false,
            evidence_upload_token: None,
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
            one_c: SourceStatus {
                ok: true,
                status: "OK".to_string(),
                summary: "status=ok, companies=47".to_string(),
                error: None,
                payload: Some(json!({"status": "ok", "companies_total": 47})),
            },
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
        let missing_policy = Path::new("/tmp/detmir-missing-workforce-policy.json");
        let report = build_reports(
            &snapshot,
            &IncidentStateFile::default(),
            &evidence,
            missing_policy,
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
        assert_eq!(report["workforce_policy"]["configured"], false);
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
            one_c: SourceStatus {
                ok: true,
                status: "OK".to_string(),
                summary: "".to_string(),
                error: None,
                payload: None,
            },
        };
        let policy = WorkforcePolicy {
            default_role: "accountant".to_string(),
            roles: BTreeMap::from([(
                "accountant".to_string(),
                WorkforceRolePolicy {
                    label: Some("Бухгалтер".to_string()),
                    planned_hours_per_day: Some(8.0),
                    default_weight: Some(0.2),
                    application_weights: BTreeMap::from([
                        ("1с".to_string(), 1.0),
                        ("youtube".to_string(), 0.0),
                    ]),
                },
            )]),
        };
        let weighted = weighted_activity(&snapshot, &policy, 1).unwrap();
        assert_eq!(weighted.role, "accountant");
        assert_eq!(weighted.weighted_seconds, 3600);
        assert_eq!(weighted.app_seconds, 7200);
        assert_eq!(weighted.index, Some(13));
    }
}
