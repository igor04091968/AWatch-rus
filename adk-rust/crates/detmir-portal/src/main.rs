use std::collections::BTreeMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use chrono::{SecondsFormat, Utc};
use clap::Parser;
use reqwest::blocking::Client;
use reqwest::header::{CONNECTION, HeaderValue};
use serde::Serialize;
use serde_json::{Value, json};
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
        default_value = "http://10.10.10.13:5610",
        env = "DETMIR_PORTAL_WORKTIME_URL"
    )]
    worktime_url: String,

    #[arg(
        long,
        default_value = "http://10.10.10.2:8710",
        env = "DETMIR_PORTAL_ONE_C_URL"
    )]
    one_c_url: String,

    #[arg(long, default_value_t = 10, env = "DETMIR_PORTAL_TIMEOUT_SECONDS")]
    timeout_seconds: u64,

    #[arg(long)]
    json_smoke: bool,
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
    status: String,
    kind: String,
    source: String,
    summary: String,
    generated_at_utc: String,
    link: String,
}

#[derive(Debug, Serialize)]
struct PortalLinks {
    portal: String,
    grafana_dashboards: String,
    detmir_activitywatch: String,
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
        let smoke = json!({
            "health": build_health(&snapshot),
            "summary": build_summary(&snapshot),
            "incidents": build_incidents(&snapshot),
        });
        println!("{}", serde_json::to_string_pretty(&smoke)?);
        return Ok(if build_health(&snapshot).ok { 0 } else { 2 });
    }

    let server = Server::http(&args.bind).map_err(|err| anyhow!("bind {}: {err}", args.bind))?;
    eprintln!("detmir-portal listening on http://{}", args.bind);
    for request in server.incoming_requests() {
        if let Err(err) = handle_request(request, &args) {
            eprintln!("detmir-portal request failed: {err:#}");
        }
    }
    Ok(0)
}

fn handle_request(request: Request, args: &Cli) -> Result<()> {
    if request.method() != &Method::Get {
        return respond_text(request, StatusCode(405), "Method Not Allowed", "text/plain");
    }
    let path = normalize_path(request.url());
    match path.as_str() {
        "/" | "/operator" | "/manager" | "/owner" | "/incidents" => respond_text(
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
            respond_json(request, &build_operator(&snapshot))
        }
        "/api/manager" => {
            let snapshot = build_snapshot(args);
            respond_json(request, &build_manager(&snapshot))
        }
        "/api/owner" => {
            let snapshot = build_snapshot(args);
            respond_json(request, &build_owner(&snapshot))
        }
        "/api/incidents" => {
            let snapshot = build_snapshot(args);
            respond_json(request, &build_incidents(&snapshot))
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

fn build_operator(snapshot: &Snapshot) -> Value {
    json!({
        "generated_at_utc": snapshot.generated_at_utc,
        "summary": build_summary(snapshot),
        "detmir_status": snapshot.detmir_status,
        "detmir_check": snapshot.detmir_check,
        "failed_units": snapshot.failed_units,
        "grafana_data": grafana_service(snapshot),
        "links": links(),
        "incidents": build_incidents(snapshot),
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

fn build_incidents(snapshot: &Snapshot) -> Vec<IncidentItem> {
    let mut incidents = Vec::new();
    for source in [
        ("detmir_status", &snapshot.detmir_status),
        ("detmir_check", &snapshot.detmir_check),
        ("failed_units", &snapshot.failed_units),
        ("worktime", &snapshot.worktime),
        ("one_c", &snapshot.one_c),
    ] {
        if !source.1.ok {
            incidents.push(IncidentItem {
                status: source.1.status.clone(),
                kind: "health".to_string(),
                source: source.0.to_string(),
                summary: source.1.summary.clone(),
                generated_at_utc: snapshot.generated_at_utc.clone(),
                link: "/portal/operator".to_string(),
            });
        }
    }
    if let Some(check) = snapshot.detmir_check.payload.as_ref() {
        if let Some(services) = check.get("services").and_then(Value::as_array) {
            for service in services {
                if service.get("ok").and_then(Value::as_bool) == Some(false) {
                    incidents.push(IncidentItem {
                        status: if service.get("required").and_then(Value::as_bool) == Some(true) {
                            "FAIL"
                        } else {
                            "WARN"
                        }
                        .to_string(),
                        kind: "service".to_string(),
                        source: service
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("service")
                            .to_string(),
                        summary: service
                            .get("error")
                            .and_then(Value::as_str)
                            .unwrap_or("service check failed")
                            .to_string(),
                        generated_at_utc: snapshot.generated_at_utc.clone(),
                        link: "/portal/operator".to_string(),
                    });
                }
            }
        }
        if let Some(buckets) = check.get("buckets").and_then(Value::as_array) {
            for bucket in buckets {
                if bucket.get("ok").and_then(Value::as_bool) == Some(false) {
                    incidents.push(IncidentItem {
                        status: bucket
                            .get("status")
                            .and_then(Value::as_str)
                            .unwrap_or("WARN")
                            .to_string(),
                        kind: "collector".to_string(),
                        source: bucket
                            .get("bucket")
                            .and_then(Value::as_str)
                            .unwrap_or("bucket")
                            .to_string(),
                        summary: format!(
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
                        generated_at_utc: snapshot.generated_at_utc.clone(),
                        link: "/portal/operator".to_string(),
                    });
                }
            }
        }
    }
    incidents
}

fn links() -> PortalLinks {
    PortalLinks {
        portal: "/portal/".to_string(),
        grafana_dashboards: "/dashboards".to_string(),
        detmir_activitywatch:
            "/d/detmir-aw-main/detmir-activitywatch?orgId=1&from=now-48h&to=now&timezone=browser&var-host=SHARKON2025&refresh=5m"
                .to_string(),
        aw_ui: "/r/aw/".to_string(),
        worktime_report: "/r/aw-worktime".to_string(),
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
        assert_eq!(links.worktime_report, "/r/aw-worktime");
    }

    #[test]
    fn collection_block_detects_green_summary() {
        let value = json!({"summary":{"bucket_stale":0,"bucket_dead":0,"service_failures":0}});
        let block = collection_block(Some(&value));
        assert_eq!(block.status, "OK");
    }
}
