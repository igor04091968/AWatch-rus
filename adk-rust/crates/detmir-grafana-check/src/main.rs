use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use chrono::{SecondsFormat, Utc};
use clap::Parser;
use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::{Value, json};

const DEFAULT_GRAFANA_URL: &str = "http://127.0.0.1:3000";
const DEFAULT_DASHBOARD_UID: &str = "detmir-aw-main";
const DEFAULT_DASHBOARD_FILE: &str = "/etc/grafana/provisioning/dashboards/aw/detmir-aw-main.json";
const DEFAULT_HOST: &str = "HOST-EXAMPLE";
const OLD_MEASUREMENTS: &[&str] = &["aw_window_event", "aw_afk_event"];
const REQUIRED_MEASUREMENTS: &[&str] = &[
    "aw_rdp_worktime_hourly",
    "aw_rdp_worktime_daily",
    "aw_rdp_worktime_summary_daily",
    "aw_true_active_app_daily",
    "aw_worktime_exporter_heartbeat",
];

#[derive(Debug, Parser)]
#[command(about = "Read-only DetMir Grafana dashboard freshness and correctness check")]
struct Cli {
    #[arg(long, env = "DETMIR_GRAFANA_URL")]
    grafana_url: Option<String>,

    #[arg(long, env = "DETMIR_GRAFANA_USER")]
    user: Option<String>,

    #[arg(long, env = "DETMIR_GRAFANA_PASSWORD")]
    password: Option<String>,

    #[arg(long, default_value = DEFAULT_DASHBOARD_UID, env = "DETMIR_GRAFANA_DASHBOARD_UID")]
    dashboard_uid: String,

    #[arg(long, default_value = DEFAULT_DASHBOARD_FILE, env = "DETMIR_GRAFANA_DASHBOARD_FILE")]
    dashboard_file: PathBuf,

    #[arg(long, default_value = DEFAULT_HOST, env = "DETMIR_GRAFANA_HOST")]
    host: String,

    #[arg(long, default_value_t = 15, env = "DETMIR_GRAFANA_TIMEOUT_SECONDS")]
    timeout_seconds: u64,

    #[arg(
        long,
        default_value_t = 360.0,
        env = "DETMIR_GRAFANA_MAX_FRESHNESS_MINUTES"
    )]
    max_freshness_minutes: f64,

    #[arg(long, default_value_t = 1, env = "DETMIR_GRAFANA_MIN_PANEL_ROWS")]
    min_panel_rows: usize,

    #[arg(long, default_value_t = 4, env = "DETMIR_GRAFANA_MIN_PANELS")]
    min_panels: usize,

    #[arg(long)]
    json: bool,

    #[arg(long, env = "DETMIR_GRAFANA_OUTPUT_JSON")]
    output_json: Option<PathBuf>,

    #[arg(long, env = "DETMIR_GRAFANA_OUTPUT_TEXT")]
    output_text: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Status {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Serialize)]
struct CheckResult {
    name: String,
    status: Status,
    summary: String,
    details: Value,
}

#[derive(Debug, Default, Serialize)]
struct Counts {
    ok: usize,
    warn: usize,
    fail: usize,
}

#[derive(Debug, Serialize)]
struct Report {
    ok: bool,
    generated_at_utc: String,
    dashboard_uid: String,
    grafana_url: String,
    counts: Counts,
    results: Vec<CheckResult>,
}

#[derive(Debug)]
struct PanelTarget {
    panel_id: i64,
    panel_title: String,
    ref_id: String,
    datasource_type: String,
    datasource_uid: String,
    query: String,
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
    let cli = Cli::parse();
    let report = build_report(&cli)?;
    let text = render_text(&report);
    if let Some(path) = &cli.output_json {
        write_report_file(path, &serde_json::to_string_pretty(&report)?)?;
    }
    if let Some(path) = &cli.output_text {
        write_report_file(path, &text)?;
    }
    if cli.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{text}");
    }
    Ok(if report.ok { 0 } else { 2 })
}

fn build_report(cli: &Cli) -> Result<Report> {
    let grafana_url = cli
        .grafana_url
        .clone()
        .or_else(|| env_nonempty("GRAFANA_URL"))
        .unwrap_or_else(|| DEFAULT_GRAFANA_URL.to_string())
        .trim_end_matches('/')
        .to_string();
    let user = cli.user.clone().or_else(|| env_nonempty("GRAFANA_USER"));
    let password = cli
        .password
        .clone()
        .or_else(|| env_nonempty("GRAFANA_PASSWORD"));
    let client = Client::builder()
        .timeout(Duration::from_secs(cli.timeout_seconds))
        .no_proxy()
        .build()
        .context("build HTTP client")?;
    let mut results = Vec::new();

    results.push(check_health(&client, &grafana_url));
    let dashboard_api = match get_json_auth(
        &client,
        &format!("{grafana_url}/api/dashboards/uid/{}", cli.dashboard_uid),
        user.as_deref(),
        password.as_deref(),
    ) {
        Ok(value) => {
            results.push(CheckResult {
                name: "grafana:dashboard-api".to_string(),
                status: Status::Ok,
                summary: format!("dashboard {} loaded through Grafana API", cli.dashboard_uid),
                details: json!({
                    "uid": cli.dashboard_uid,
                    "title": value.pointer("/dashboard/title").and_then(Value::as_str),
                    "version": value.pointer("/dashboard/version").and_then(Value::as_i64),
                }),
            });
            Some(value)
        }
        Err(err) => {
            results.push(CheckResult {
                name: "grafana:dashboard-api".to_string(),
                status: Status::Fail,
                summary: format!("dashboard API request failed: {err:#}"),
                details: json!({ "uid": cli.dashboard_uid }),
            });
            None
        }
    };

    if let Some(api_value) = &dashboard_api {
        let dashboard = api_value.get("dashboard").unwrap_or(api_value);
        results.push(check_dashboard_shape(dashboard, cli.min_panels));
        results.push(check_measurements(
            "grafana:dashboard-api-measurements",
            dashboard,
        ));
        let targets = collect_panel_targets(dashboard);
        results.push(CheckResult {
            name: "grafana:panel-targets".to_string(),
            status: if targets.is_empty() {
                Status::Fail
            } else {
                Status::Ok
            },
            summary: format!("{} query targets found", targets.len()),
            details: json!({
                "targets": targets.iter().map(|target| json!({
                    "panel_id": target.panel_id,
                    "panel_title": target.panel_title,
                    "ref_id": target.ref_id,
                    "datasource_uid": target.datasource_uid,
                })).collect::<Vec<_>>()
            }),
        });
        for target in &targets {
            let rendered_query = render_query_vars(&target.query, &cli.host);
            let query_result = query_panel_target(
                &client,
                &grafana_url,
                user.as_deref(),
                password.as_deref(),
                target,
                &rendered_query,
            );
            match query_result {
                Ok((rows, first_number)) => {
                    let freshness_failed = is_freshness_panel(&target.panel_title)
                        && first_number
                            .map(|value| value > cli.max_freshness_minutes)
                            .unwrap_or(true);
                    let empty_optional_panel =
                        rows < cli.min_panel_rows && is_optional_empty_panel(&target.panel_title);
                    let status = if freshness_failed
                        || (rows < cli.min_panel_rows && !empty_optional_panel)
                    {
                        Status::Fail
                    } else if empty_optional_panel {
                        Status::Warn
                    } else {
                        Status::Ok
                    };
                    let summary = if is_freshness_panel(&target.panel_title) {
                        match first_number {
                            Some(value) => format!(
                                "panel '{}' returned {rows} rows, freshness {:.1} min",
                                target.panel_title, value
                            ),
                            None => format!(
                                "panel '{}' returned {rows} rows, but freshness value was absent",
                                target.panel_title
                            ),
                        }
                    } else {
                        format!("panel '{}' returned {rows} rows", target.panel_title)
                    };
                    results.push(CheckResult {
                        name: format!("grafana:panel-query:{}:{}", target.panel_id, target.ref_id),
                        status,
                        summary,
                        details: json!({
                            "panel_id": target.panel_id,
                            "panel_title": target.panel_title,
                            "ref_id": target.ref_id,
                            "rows": rows,
                            "first_number": first_number,
                            "min_panel_rows": cli.min_panel_rows,
                            "max_freshness_minutes": cli.max_freshness_minutes,
                        }),
                    });
                }
                Err(err) => results.push(CheckResult {
                    name: format!("grafana:panel-query:{}:{}", target.panel_id, target.ref_id),
                    status: Status::Fail,
                    summary: format!("panel '{}' query failed: {err:#}", target.panel_title),
                    details: json!({
                        "panel_id": target.panel_id,
                        "panel_title": target.panel_title,
                        "ref_id": target.ref_id,
                        "datasource_uid": target.datasource_uid,
                    }),
                }),
            }
        }
    }

    if cli.dashboard_file.exists() {
        match read_json_file(&cli.dashboard_file) {
            Ok(file_dashboard) => {
                results.push(check_measurements(
                    "grafana:provisioned-file-measurements",
                    &file_dashboard,
                ));
            }
            Err(err) => results.push(CheckResult {
                name: "grafana:provisioned-file".to_string(),
                status: Status::Warn,
                summary: format!(
                    "cannot read provisioned dashboard file {}: {err:#}",
                    cli.dashboard_file.display()
                ),
                details: json!({ "path": cli.dashboard_file }),
            }),
        }
    } else {
        results.push(CheckResult {
            name: "grafana:provisioned-file".to_string(),
            status: Status::Warn,
            summary: format!("dashboard file not found: {}", cli.dashboard_file.display()),
            details: json!({ "path": cli.dashboard_file }),
        });
    }

    let counts = count_statuses(&results);
    Ok(Report {
        ok: counts.fail == 0,
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        dashboard_uid: cli.dashboard_uid.clone(),
        grafana_url,
        counts,
        results,
    })
}

fn check_health(client: &Client, grafana_url: &str) -> CheckResult {
    match get_json_auth(client, &format!("{grafana_url}/api/health"), None, None) {
        Ok(value) => {
            let database = value
                .get("database")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            CheckResult {
                name: "grafana:health".to_string(),
                status: if database.eq_ignore_ascii_case("ok") {
                    Status::Ok
                } else {
                    Status::Warn
                },
                summary: format!("Grafana health database={database}"),
                details: value,
            }
        }
        Err(err) => CheckResult {
            name: "grafana:health".to_string(),
            status: Status::Fail,
            summary: format!("Grafana health request failed: {err:#}"),
            details: json!({ "url": grafana_url }),
        },
    }
}

fn check_dashboard_shape(dashboard: &Value, min_panels: usize) -> CheckResult {
    let title = dashboard
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let panels = collect_panels(dashboard);
    CheckResult {
        name: "grafana:dashboard-shape".to_string(),
        status: if panels.len() >= min_panels {
            Status::Ok
        } else {
            Status::Fail
        },
        summary: format!("dashboard '{title}' has {} panels", panels.len()),
        details: json!({
            "title": title,
            "panel_count": panels.len(),
            "min_panels": min_panels,
        }),
    }
}

fn check_measurements(name: &str, dashboard: &Value) -> CheckResult {
    let text = dashboard.to_string();
    let old = contains_any(&text, OLD_MEASUREMENTS);
    let missing_required = REQUIRED_MEASUREMENTS
        .iter()
        .filter(|measurement| !text.contains(**measurement))
        .copied()
        .collect::<Vec<_>>();
    let status = if old || !missing_required.is_empty() {
        Status::Fail
    } else {
        Status::Ok
    };
    CheckResult {
        name: name.to_string(),
        status,
        summary: if status == Status::Ok {
            "dashboard uses current DetMir worktime measurements".to_string()
        } else {
            "dashboard measurement set is stale or incomplete".to_string()
        },
        details: json!({
            "old_measurements_present": OLD_MEASUREMENTS
                .iter()
                .filter(|measurement| text.contains(**measurement))
                .copied()
                .collect::<Vec<_>>(),
            "required_measurements_missing": missing_required,
            "required_measurements": REQUIRED_MEASUREMENTS,
        }),
    }
}

fn query_panel_target(
    client: &Client,
    grafana_url: &str,
    user: Option<&str>,
    password: Option<&str>,
    target: &PanelTarget,
    query: &str,
) -> Result<(usize, Option<f64>)> {
    let body = json!({
        "from": "now-48h",
        "to": "now",
        "queries": [{
            "refId": target.ref_id,
            "datasource": {
                "type": target.datasource_type,
                "uid": target.datasource_uid,
            },
            "query": query,
            "rawQuery": true,
            "format": "table",
            "intervalMs": 60000,
            "maxDataPoints": 1000,
        }]
    });
    let response = post_json_auth(
        client,
        &format!("{grafana_url}/api/ds/query"),
        user,
        password,
        &body,
    )?;
    let result = response
        .pointer(&format!("/results/{}", target.ref_id))
        .ok_or_else(|| anyhow!("missing result for refId {}", target.ref_id))?;
    if let Some(error) = result.get("error").and_then(Value::as_str) {
        return Err(anyhow!("{error}"));
    }
    if let Some(status) = result.get("status").and_then(Value::as_i64) {
        if status >= 400 {
            return Err(anyhow!("Grafana datasource status {status}"));
        }
    }
    Ok((frame_row_count(result), first_numeric_value(result)))
}

fn get_json_auth(
    client: &Client,
    url: &str,
    user: Option<&str>,
    password: Option<&str>,
) -> Result<Value> {
    let mut request = client.get(url);
    if let Some(user) = user {
        request = request.basic_auth(user, password);
    }
    let response = request
        .send()
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("GET {url} returned non-success status"))?;
    response
        .json::<Value>()
        .with_context(|| format!("decode JSON from {url}"))
}

fn post_json_auth(
    client: &Client,
    url: &str,
    user: Option<&str>,
    password: Option<&str>,
    body: &Value,
) -> Result<Value> {
    let mut request = client.post(url).json(body);
    if let Some(user) = user {
        request = request.basic_auth(user, password);
    }
    let response = request
        .send()
        .with_context(|| format!("POST {url}"))?
        .error_for_status()
        .with_context(|| format!("POST {url} returned non-success status"))?;
    response
        .json::<Value>()
        .with_context(|| format!("decode JSON from {url}"))
}

fn read_json_file(path: &PathBuf) -> Result<Value> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))
}

fn collect_panel_targets(dashboard: &Value) -> Vec<PanelTarget> {
    let mut targets = Vec::new();
    for panel in collect_panels(dashboard) {
        let panel_id = panel.get("id").and_then(Value::as_i64).unwrap_or(0);
        let panel_title = panel
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("untitled")
            .to_string();
        let datasource_type = panel
            .pointer("/datasource/type")
            .and_then(Value::as_str)
            .unwrap_or("influxdb")
            .to_string();
        let datasource_uid = panel
            .pointer("/datasource/uid")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let Some(items) = panel.get("targets").and_then(Value::as_array) else {
            continue;
        };
        for item in items {
            let Some(query) = item.get("query").and_then(Value::as_str) else {
                continue;
            };
            targets.push(PanelTarget {
                panel_id,
                panel_title: panel_title.clone(),
                ref_id: item
                    .get("refId")
                    .and_then(Value::as_str)
                    .unwrap_or("A")
                    .to_string(),
                datasource_type: item
                    .pointer("/datasource/type")
                    .and_then(Value::as_str)
                    .unwrap_or(&datasource_type)
                    .to_string(),
                datasource_uid: item
                    .pointer("/datasource/uid")
                    .and_then(Value::as_str)
                    .unwrap_or(&datasource_uid)
                    .to_string(),
                query: query.to_string(),
            });
        }
    }
    targets
}

fn collect_panels(dashboard: &Value) -> Vec<&Value> {
    let mut panels = Vec::new();
    collect_panels_inner(dashboard, &mut panels);
    panels
}

fn collect_panels_inner<'a>(value: &'a Value, panels: &mut Vec<&'a Value>) {
    if value.get("targets").is_some() && value.get("type").is_some() {
        panels.push(value);
    }
    if let Some(children) = value.get("panels").and_then(Value::as_array) {
        for child in children {
            collect_panels_inner(child, panels);
        }
    }
}

fn render_query_vars(query: &str, host: &str) -> String {
    query.replace("${host}", host).replace("$host", host)
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

fn is_freshness_panel(title: &str) -> bool {
    title.to_lowercase().contains("свеж")
}

fn is_optional_empty_panel(title: &str) -> bool {
    let title = title.to_lowercase();
    title.contains("прилож") || title.contains("сотрудник")
}

fn frame_row_count(value: &Value) -> usize {
    value
        .get("frames")
        .and_then(Value::as_array)
        .map(|frames| {
            frames
                .iter()
                .filter_map(|frame| frame.pointer("/data/values").and_then(Value::as_array))
                .map(|columns| {
                    columns
                        .iter()
                        .filter_map(Value::as_array)
                        .map(Vec::len)
                        .max()
                        .unwrap_or(0)
                })
                .sum()
        })
        .unwrap_or(0)
}

fn first_numeric_value(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::Array(items) => items.iter().find_map(first_numeric_value),
        Value::Object(map) => map.values().find_map(first_numeric_value),
        _ => None,
    }
}

fn count_statuses(results: &[CheckResult]) -> Counts {
    let mut counts = Counts::default();
    for result in results {
        match result.status {
            Status::Ok => counts.ok += 1,
            Status::Warn => counts.warn += 1,
            Status::Fail => counts.fail += 1,
        }
    }
    counts
}

fn env_nonempty(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

fn write_report_file(path: &PathBuf, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    fs::write(path, content).with_context(|| format!("write {}", path.display()))
}

fn render_text(report: &Report) -> String {
    let mut out = String::new();
    out.push_str("=== DetMir Grafana Check ===\n");
    out.push_str(&format!("dashboard_uid={}\n", report.dashboard_uid));
    out.push_str(&format!("grafana_url={}\n", report.grafana_url));
    out.push_str(&format!("generated_at_utc={}\n\n", report.generated_at_utc));
    for result in &report.results {
        out.push_str(&format!(
            "{:<5} {:<42} {}\n",
            format!("{:?}", result.status).to_uppercase(),
            result.name,
            result.summary
        ));
    }
    out.push('\n');
    out.push_str(&format!(
        "counts: ok={} warn={} fail={}",
        report.counts.ok, report.counts.warn, report.counts.fail
    ));
    out.push('\n');
    out.push_str(&format!("ok={}\n", report.ok));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_vars_render_host_forms() {
        assert_eq!(
            render_query_vars(
                "r.host == \"${host}\" or r.host == \"$host\"",
                "HOST-EXAMPLE"
            ),
            "r.host == \"HOST-EXAMPLE\" or r.host == \"HOST-EXAMPLE\""
        );
    }

    #[test]
    fn frame_row_count_reads_grafana_frames() {
        let value = json!({
            "frames": [{
                "data": {
                    "values": [
                        ["2026-06-02T10:00:00Z", "2026-06-02T11:00:00Z"],
                        [1.0, 2.0]
                    ]
                }
            }]
        });
        assert_eq!(frame_row_count(&value), 2);
        assert_eq!(first_numeric_value(&value), Some(1.0));
    }

    #[test]
    fn measurement_check_catches_old_and_missing() {
        let dashboard = json!({
            "panels": [{
                "type": "stat",
                "targets": [{"query": "from(bucket:\"aw_metrics\") |> filter(fn:(r)=>r._measurement == \"aw_window_event\")"}]
            }]
        });
        let result = check_measurements("test", &dashboard);
        assert_eq!(result.status, Status::Fail);
    }

    #[test]
    fn collect_targets_from_dashboard_panels() {
        let dashboard = json!({
            "panels": [{
                "id": 5,
                "type": "stat",
                "title": "Свежесть worktime данных",
                "datasource": {"type": "influxdb", "uid": "influxdb_aw"},
                "targets": [{"refId": "A", "query": "from(bucket:\"aw_metrics\")"}]
            }]
        });
        let targets = collect_panel_targets(&dashboard);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].panel_id, 5);
        assert_eq!(targets[0].datasource_uid, "influxdb_aw");
    }

    #[test]
    fn application_panels_are_optional_when_empty() {
        assert!(is_optional_empty_panel(
            "Сегодня: доказанная работа по приложениям"
        ));
        assert!(is_optional_empty_panel(
            "Сегодня: приложения и подтверждения"
        ));
        assert!(is_optional_empty_panel(
            "Сегодня: активность по сотрудникам"
        ));
        assert!(!is_optional_empty_panel("Сегодня: активное время"));
    }
}
