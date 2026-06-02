use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::{DateTime, Duration as ChronoDuration, SecondsFormat, Utc};
use clap::Parser;
use detmir_core::{exit_codes, parse_utc_rfc3339};
use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::{Value, json};

const ENV_FILE: &str = "/etc/activitywatch/aw-server.env";

#[derive(Debug, Parser)]
#[command(about = "Unified AW-RUS health orchestrator.")]
struct Cli {
    #[arg(long, default_value = "http://127.0.0.1:5600")]
    aw_server: String,

    #[arg(long, default_value = "http://127.0.0.1:5610")]
    worktime_api: String,

    #[arg(long, default_value = "192.168.100.18")]
    rdp_host: String,

    #[arg(long, default_value = "SHARKON2025")]
    rdp_hostname: String,

    #[arg(long, default_value = "/var/lib/activitywatch/health")]
    state_dir: PathBuf,

    #[arg(
        long,
        default_value = "/var/lib/activitywatch/health/windows-validation"
    )]
    validation_dir: PathBuf,

    #[arg(long, default_value_t = 900)]
    session_max_age_seconds: i64,

    #[arg(long, default_value_t = 900)]
    interactive_max_age_seconds: i64,

    #[arg(long, default_value_t = 86400)]
    session_events_max_age_seconds: i64,

    #[arg(long, default_value_t = 300)]
    guard_max_age_seconds: i64,

    #[arg(long)]
    guard_required: bool,

    #[arg(long, default_value_t = 259200)]
    validation_max_age_seconds: i64,

    #[arg(long, default_value_t = 3.0)]
    tcp_timeout_seconds: f64,

    #[arg(long)]
    json: bool,
}

impl Cli {
    fn apply_env(mut self) -> Self {
        load_env_file(Path::new(ENV_FILE));
        if !cli_arg_present("--aw-server") {
            self.aw_server = env_string("AW_SERVER_URL").unwrap_or(self.aw_server);
        }
        if !cli_arg_present("--worktime-api") {
            self.worktime_api = env_string("AW_RUS_HEALTH_WORKTIME_API")
                .or_else(|| env_string("AW_WORKTIME_REPORT_BASE"))
                .unwrap_or(self.worktime_api);
        }
        if !cli_arg_present("--rdp-host") {
            self.rdp_host = env_string("AW_MONITORED_WINDOWS_HOST").unwrap_or(self.rdp_host);
        }
        if !cli_arg_present("--rdp-hostname") {
            self.rdp_hostname =
                env_string("AW_MONITORED_WINDOWS_HOSTNAME").unwrap_or(self.rdp_hostname);
        }
        if !cli_arg_present("--state-dir") {
            self.state_dir = env_path("AW_RUS_HEALTH_STATE_DIR").unwrap_or(self.state_dir);
        }
        if !cli_arg_present("--validation-dir") {
            self.validation_dir =
                env_path("AW_RUS_HEALTH_VALIDATION_DIR").unwrap_or(self.validation_dir);
        }
        if !cli_arg_present("--session-max-age-seconds") {
            self.session_max_age_seconds = env_i64(
                "AW_RUS_HEALTH_SESSION_MAX_AGE_SECONDS",
                self.session_max_age_seconds,
            );
        }
        if !cli_arg_present("--interactive-max-age-seconds") {
            self.interactive_max_age_seconds = env_i64(
                "AW_RUS_HEALTH_INTERACTIVE_MAX_AGE_SECONDS",
                self.interactive_max_age_seconds,
            );
        }
        if !cli_arg_present("--session-events-max-age-seconds") {
            self.session_events_max_age_seconds = env_i64(
                "AW_RUS_HEALTH_SESSION_EVENTS_MAX_AGE_SECONDS",
                self.session_events_max_age_seconds,
            );
        }
        if !cli_arg_present("--guard-max-age-seconds") {
            self.guard_max_age_seconds = env_i64(
                "AW_RUS_HEALTH_GUARD_MAX_AGE_SECONDS",
                self.guard_max_age_seconds,
            );
        }
        if !cli_arg_present("--guard-required") {
            self.guard_required = env_bool("AW_RUS_HEALTH_GUARD_REQUIRED");
        }
        if !cli_arg_present("--validation-max-age-seconds") {
            self.validation_max_age_seconds = env_i64(
                "AW_RUS_HEALTH_VALIDATION_MAX_AGE_SECONDS",
                self.validation_max_age_seconds,
            );
        }
        if !cli_arg_present("--tcp-timeout-seconds") {
            self.tcp_timeout_seconds = env_f64(
                "AW_RUS_HEALTH_TCP_TIMEOUT_SECONDS",
                self.tcp_timeout_seconds,
            );
        }
        self
    }
}

fn cli_arg_present(name: &str) -> bool {
    std::env::args_os().skip(1).any(|arg| {
        let Some(value) = arg.to_str() else {
            return false;
        };
        value == name
            || value
                .strip_prefix(name)
                .is_some_and(|rest| rest.starts_with('='))
    })
}

#[derive(Debug, Clone, Serialize)]
struct CheckResult {
    name: String,
    status: String,
    summary: String,
    details: Value,
}

#[derive(Debug, Serialize)]
struct HealthReport {
    generated_at_utc: String,
    ok: bool,
    counts: HashMap<String, usize>,
    results: Vec<CheckResult>,
}

#[derive(Default)]
struct ReportBuilder {
    results: Vec<CheckResult>,
}

impl ReportBuilder {
    fn add(
        &mut self,
        name: impl Into<String>,
        status: &str,
        summary: impl Into<String>,
        details: Value,
    ) {
        self.results.push(CheckResult {
            name: name.into(),
            status: status.to_string(),
            summary: summary.into(),
            details,
        });
    }

    fn build(&self) -> HealthReport {
        let mut counts = HashMap::from([
            ("ok".to_string(), 0),
            ("warn".to_string(), 0),
            ("fail".to_string(), 0),
        ]);
        for item in &self.results {
            *counts.entry(item.status.clone()).or_insert(0) += 1;
        }
        HealthReport {
            generated_at_utc: utc_iso(),
            ok: !self.results.iter().any(|item| item.status == "fail"),
            counts,
            results: self.results.clone(),
        }
    }
}

fn env_string(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

fn env_path(name: &str) -> Option<PathBuf> {
    env_string(name).map(PathBuf::from)
}

fn env_i64(name: &str, fallback: i64) -> i64 {
    env_string(name)
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

fn env_f64(name: &str, fallback: f64) -> f64 {
    env_string(name)
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

fn env_bool(name: &str) -> bool {
    env_string(name)
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

fn load_env_file(path: &Path) {
    let Ok(raw) = fs::read_to_string(path) else {
        return;
    };
    for line in raw.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') || !line.contains('=') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if std::env::var_os(key.trim()).is_none() {
            let value = value
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();
            // SAFETY: this binary is single-threaded during configuration loading.
            unsafe { std::env::set_var(key.trim(), value) };
        }
    }
}

fn utc_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn parse_ts(value: Option<&str>) -> Option<DateTime<Utc>> {
    value.and_then(|value| parse_utc_rfc3339(value).ok())
}

fn age_seconds(ts: Option<DateTime<Utc>>) -> Option<i64> {
    ts.map(|ts| (Utc::now() - ts).num_seconds().max(0))
}

fn event_effective_ts(event: &Value) -> Option<DateTime<Utc>> {
    let timestamp = parse_ts(event.get("timestamp").and_then(Value::as_str))?;
    let duration_ms = event
        .get("duration")
        .and_then(Value::as_f64)
        .filter(|duration| duration.is_finite() && *duration > 0.0)
        .map(|duration| (duration * 1000.0).round() as i64)
        .unwrap_or(0);
    Some(timestamp + ChronoDuration::milliseconds(duration_ms))
}

fn http_json(client: &Client, url: &str, attempts: usize) -> Result<Value> {
    let attempts = attempts.max(1);
    let mut last_error = None;
    for attempt in 0..attempts {
        let result = client
            .get(url)
            .send()
            .with_context(|| format!("HTTP request failed: {url}"))
            .and_then(|response| response.error_for_status().context("HTTP status error"))
            .and_then(|response| response.json::<Value>().context("invalid JSON response"));
        match result {
            Ok(value) => return Ok(value),
            Err(err) => last_error = Some(err),
        }
        if attempt + 1 < attempts {
            std::thread::sleep(Duration::from_millis(500 * (1 << attempt)));
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("HTTP request failed: {url}")))
}

fn latest_bucket_event(client: &Client, api_base: &str, bucket_id: &str) -> Result<Option<Value>> {
    let events = http_json(
        client,
        &format!("{api_base}/buckets/{bucket_id}/events?limit=20"),
        2,
    )?;
    let Some(items) = events.as_array() else {
        return Ok(None);
    };
    let mut objects = items
        .iter()
        .filter(|item| item.is_object())
        .cloned()
        .collect::<Vec<_>>();
    objects.sort_by_key(|item| std::cmp::Reverse(event_effective_ts(item)));
    Ok(objects.into_iter().next())
}

fn bucket_metadata_ts(buckets: &Value, bucket_id: &str) -> Option<DateTime<Utc>> {
    parse_ts(
        buckets
            .get(bucket_id)?
            .get("metadata")?
            .get("end")?
            .as_str(),
    )
}

fn host_activity_from_worktime(event: Option<&Value>, max_age_seconds: i64) -> Value {
    let Some(event) = event else {
        return json!({"fresh": false, "active": false, "age_seconds": null, "timestamp": null});
    };
    let timestamp = event.get("timestamp").and_then(Value::as_str);
    let age = age_seconds(parse_ts(timestamp));
    let data = event.get("data").cloned().unwrap_or_else(|| json!({}));
    let fresh = age.is_some_and(|age| age <= max_age_seconds);
    let active = fresh && data.get("active").and_then(Value::as_bool).unwrap_or(false);
    json!({
        "fresh": fresh,
        "active": active,
        "age_seconds": age,
        "timestamp": timestamp,
        "data": data,
    })
}

fn bucket_health(
    client: &Client,
    api_base: &str,
    bucket_id: &str,
    max_age_seconds: i64,
    missing_status: &str,
    stale_status: &str,
) -> (String, String, Value) {
    let event = match latest_bucket_event(client, api_base, bucket_id) {
        Ok(event) => event,
        Err(err) => {
            return (
                "fail".to_string(),
                format!("bucket query failed: {err}"),
                json!({"bucket": bucket_id}),
            );
        }
    };
    let Some(event) = event else {
        return (
            missing_status.to_string(),
            "no events".to_string(),
            json!({"bucket": bucket_id}),
        );
    };
    let timestamp = event.get("timestamp").and_then(Value::as_str);
    let effective_ts = event_effective_ts(&event);
    let age = age_seconds(effective_ts);
    let timestamp_source = if event
        .get("duration")
        .and_then(Value::as_f64)
        .is_some_and(|duration| duration.is_finite() && duration > 0.0)
    {
        "event.timestamp+duration"
    } else {
        "event.timestamp"
    };
    let details = json!({
        "bucket": bucket_id,
        "timestamp": timestamp,
        "effective_timestamp": effective_ts.map(|ts| ts.to_rfc3339_opts(SecondsFormat::Secs, true)),
        "timestamp_source": timestamp_source,
        "age_seconds": age,
    });
    match age {
        None => (
            "warn".to_string(),
            "timestamp parse failed".to_string(),
            details,
        ),
        Some(age) if age > max_age_seconds => {
            (stale_status.to_string(), format!("stale ({age}s)"), details)
        }
        Some(age) => ("ok".to_string(), format!("fresh ({age}s)"), details),
    }
}

fn bucket_timestamp_health(
    client: &Client,
    api_base: &str,
    buckets: &Value,
    bucket_id: &str,
    max_age_seconds: i64,
    missing_status: &str,
    stale_status: &str,
) -> (String, String, Value) {
    if let Some(ts) = bucket_metadata_ts(buckets, bucket_id) {
        let age = age_seconds(Some(ts));
        let timestamp = ts.to_rfc3339_opts(SecondsFormat::Secs, true);
        let details = json!({
            "bucket": bucket_id,
            "timestamp": timestamp,
            "age_seconds": age,
            "timestamp_source": "bucket_metadata.end",
        });
        return match age {
            None => (
                "warn".to_string(),
                "timestamp parse failed".to_string(),
                details,
            ),
            Some(age) if age > max_age_seconds => {
                (stale_status.to_string(), format!("stale ({age}s)"), details)
            }
            Some(age) => ("ok".to_string(), format!("fresh ({age}s)"), details),
        };
    }
    bucket_health(
        client,
        api_base,
        bucket_id,
        max_age_seconds,
        missing_status,
        stale_status,
    )
}

fn guard_bucket_health(
    client: &Client,
    api_base: &str,
    host: &str,
    max_age_seconds: i64,
    required: bool,
) -> (String, String, Value) {
    let bucket_id = format!("aw-rus-collector-guard_{host}");
    let event = match latest_bucket_event(client, api_base, &bucket_id) {
        Ok(event) => event,
        Err(err) => {
            return (
                if required { "fail" } else { "warn" }.to_string(),
                format!("guard bucket query failed: {err}"),
                json!({"bucket": bucket_id}),
            );
        }
    };
    let Some(event) = event else {
        return (
            if required { "fail" } else { "warn" }.to_string(),
            "no guard heartbeat".to_string(),
            json!({"bucket": bucket_id, "required": required}),
        );
    };
    let timestamp = event.get("timestamp").and_then(Value::as_str);
    let age = age_seconds(parse_ts(timestamp));
    let data = event
        .get("data")
        .filter(|value| value.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let guard_status = data
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_ascii_lowercase();
    let details = json!({
        "bucket": bucket_id,
        "timestamp": timestamp,
        "age_seconds": age,
        "required": required,
        "guard_status": guard_status,
        "mode": data.get("mode").cloned().unwrap_or(Value::Null),
        "live_session_count": data.get("liveSessionCount").cloned().unwrap_or(Value::Null),
        "problems": data.get("problems").cloned().unwrap_or_else(|| json!([])),
        "actions": data.get("actions").cloned().unwrap_or_else(|| json!([])),
    });
    let stale_status = if required { "fail" } else { "warn" };
    match age {
        None => (
            "warn".to_string(),
            "guard timestamp parse failed".to_string(),
            details,
        ),
        Some(age) if age > max_age_seconds => (
            stale_status.to_string(),
            format!("guard stale ({age}s)"),
            details,
        ),
        Some(_) if matches!(guard_status.as_str(), "fail" | "error") => (
            if required { "fail" } else { "warn" }.to_string(),
            format!("guard reports {guard_status}"),
            details,
        ),
        Some(_) if guard_status == "warn" => (
            "warn".to_string(),
            "guard reports warn".to_string(),
            details,
        ),
        Some(age) => ("ok".to_string(), format!("guard fresh ({age}s)"), details),
    }
}

fn run_command(cmd: &[&str]) -> (i32, String) {
    let timeout = env_f64("AW_RUS_HEALTH_WRAPPER_TIMEOUT_SECONDS", 20.0).max(1.0);
    let mut child = match Command::new(cmd[0])
        .args(&cmd[1..])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => return (1, err.to_string()),
    };
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => match child.wait_with_output() {
                Ok(output) => {
                    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
                    if !output.stderr.is_empty() {
                        text.push_str(&String::from_utf8_lossy(&output.stderr));
                    }
                    return (output.status.code().unwrap_or(1), text.trim().to_string());
                }
                Err(err) => return (1, err.to_string()),
            },
            Ok(None) if started.elapsed() >= Duration::from_secs_f64(timeout) => {
                let _ = child.kill();
                let output = child.wait_with_output();
                let mut text = format!("timed out after {timeout:.1}s");
                if let Ok(output) = output {
                    if !output.stdout.is_empty() {
                        text.push('\n');
                        text.push_str(&String::from_utf8_lossy(&output.stdout));
                    }
                    if !output.stderr.is_empty() {
                        text.push('\n');
                        text.push_str(&String::from_utf8_lossy(&output.stderr));
                    }
                }
                return (124, text.trim().to_string());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(err) => {
                let _ = child.kill();
                return (1, err.to_string());
            }
        }
    }
}

fn check_wrapper(
    report: &mut ReportBuilder,
    name: &str,
    cmd: &[&str],
    json_mode: bool,
    failure_status: &str,
) {
    if !Path::new(cmd[0]).exists() {
        report.add(name, "warn", "binary missing", json!({"command": cmd}));
        return;
    }
    let (rc, output) = run_command(cmd);
    let mut details = json!({"command": cmd, "returncode": rc});
    if json_mode {
        match serde_json::from_str::<Value>(&output) {
            Ok(payload) => details["payload"] = payload,
            Err(_) => {
                details["raw_output"] = Value::String(output);
                report.add(name, "fail", "invalid JSON output", details);
                return;
            }
        }
    } else {
        details["output"] = Value::String(output);
    }
    report.add(
        name,
        if rc == 0 { "ok" } else { failure_status },
        if rc == 0 { "passed" } else { "failed" },
        details,
    );
}

fn tcp_connect(host: &str, port: u16, timeout_seconds: f64) -> (bool, String) {
    let addr = match format!("{host}:{port}").parse::<SocketAddr>() {
        Ok(addr) => addr,
        Err(err) => return (false, err.to_string()),
    };
    match TcpStream::connect_timeout(&addr, Duration::from_secs_f64(timeout_seconds)) {
        Ok(_) => (true, "connected".to_string()),
        Err(err) => (false, err.to_string()),
    }
}

fn latest_validation_report(dir: &Path) -> Option<PathBuf> {
    let mut entries = fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with("-aw_validate_ansible.json"))
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|path| path.metadata().and_then(|meta| meta.modified()).ok());
    entries.pop()
}

fn write_atomic(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(content.as_bytes())?;
    tmp.persist(path)
        .map_err(|err| anyhow::anyhow!("failed to persist {}: {}", path.display(), err.error))?;
    Ok(())
}

fn chmod_if_possible(path: &Path, mode: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = path.metadata() {
            let mut perms = meta.permissions();
            perms.set_mode(mode);
            let _ = fs::set_permissions(path, perms);
        }
    }
}

fn render_text(report: &HealthReport) -> String {
    let mut lines = vec![
        "=== AW-RUS Health ===".to_string(),
        format!("Timestamp: {}", utc_iso()),
        String::new(),
    ];
    for item in &report.results {
        lines.push(format!(
            "[{}] {}: {}",
            item.status.to_ascii_uppercase(),
            item.name,
            item.summary
        ));
    }
    lines.push(String::new());
    lines.push(format!(
        "Counts: ok={} warn={} fail={}",
        report.counts.get("ok").copied().unwrap_or(0),
        report.counts.get("warn").copied().unwrap_or(0),
        report.counts.get("fail").copied().unwrap_or(0)
    ));
    lines.push(format!(
        "Overall: {}",
        if report.ok { "OK" } else { "FAIL" }
    ));
    lines.join("\n")
}

fn normalize_aw_api_base(aw_server: &str) -> String {
    let trimmed = aw_server.trim_end_matches('/');
    if trimmed.ends_with("/api/0") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/api/0")
    }
}

fn validation_check(report: &mut ReportBuilder, validation_dir: &Path, max_age_seconds: i64) {
    let Some(path) = latest_validation_report(validation_dir) else {
        report.add(
            "validation:windows",
            "warn",
            "no validation report snapshot",
            json!({"directory": validation_dir}),
        );
        return;
    };
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw.trim_start_matches('\u{feff}').to_string(),
        Err(err) => {
            report.add(
                "validation:windows",
                "fail",
                format!("invalid validation snapshot: {err}"),
                json!({"path": path}),
            );
            return;
        }
    };
    let payload = match serde_json::from_str::<Value>(&raw) {
        Ok(payload) => payload,
        Err(err) => {
            report.add(
                "validation:windows",
                "fail",
                format!("invalid validation snapshot: {err}"),
                json!({"path": path}),
            );
            return;
        }
    };
    let age = path
        .metadata()
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .map(|elapsed| elapsed.as_secs() as i64);
    if age.is_some_and(|age| age > max_age_seconds) {
        report.add(
            "validation:windows",
            "warn",
            format!("validation snapshot is stale ({}s)", age.unwrap_or(0)),
            json!({
                "path": path,
                "overall_ok": payload.get("overallOk").cloned().unwrap_or(Value::Null),
                "failed_sections": payload.pointer("/summary/failedSections").cloned().unwrap_or_else(|| json!([])),
            }),
        );
    } else if payload.get("overallOk").and_then(Value::as_bool) == Some(true) {
        report.add(
            "validation:windows",
            "ok",
            "validation snapshot OK",
            json!({"path": path, "age_seconds": age}),
        );
    } else {
        report.add(
            "validation:windows",
            "fail",
            "validation snapshot reports failure",
            json!({
                "path": path,
                "age_seconds": age,
                "failed_sections": payload.pointer("/summary/failedSections").cloned().unwrap_or_else(|| json!([])),
            }),
        );
    }
}

fn run(cli: &Cli) -> Result<HealthReport> {
    let client = Client::builder()
        .timeout(Duration::from_secs(25))
        .no_proxy()
        .build()?;
    let mut report = ReportBuilder::default();
    let aw_api_base = normalize_aw_api_base(&cli.aw_server);

    check_wrapper(
        &mut report,
        "wrapper:aw-health-check",
        &["/usr/local/bin/aw-health-check"],
        false,
        "warn",
    );
    check_wrapper(
        &mut report,
        "wrapper:dlp-health-check",
        &["/usr/local/bin/dlp-health-check", "--json"],
        true,
        "warn",
    );

    match http_json(&client, &format!("{aw_api_base}/info"), 2) {
        Ok(info) => report.add(
            "http:aw-server",
            "ok",
            "activitywatch API responded",
            json!({"version": info.get("version").cloned().unwrap_or(Value::Null)}),
        ),
        Err(err) => report.add(
            "http:aw-server",
            "fail",
            format!("activitywatch API failed: {err}"),
            json!({"url": format!("{aw_api_base}/info")}),
        ),
    }

    match http_json(
        &client,
        &format!("{}/health", cli.worktime_api.trim_end_matches('/')),
        2,
    ) {
        Ok(payload) => report.add(
            "http:worktime-api",
            "ok",
            "worktime API responded",
            json!({"payload": if payload.is_object() { payload } else { json!({}) }}),
        ),
        Err(err) => report.add(
            "http:worktime-api",
            "fail",
            format!("worktime API failed: {err}"),
            json!({"url": cli.worktime_api}),
        ),
    }

    for (port, label) in [(5985_u16, "winrm"), (3389_u16, "rdp")] {
        let (ok, message) = tcp_connect(&cli.rdp_host, port, cli.tcp_timeout_seconds);
        report.add(
            format!("tcp:{label}"),
            if ok { "ok" } else { "fail" },
            if ok {
                message
            } else {
                format!("unreachable: {message}")
            },
            json!({"host": cli.rdp_host, "port": port}),
        );
    }

    let buckets = match http_json(&client, &format!("{aw_api_base}/buckets"), 2) {
        Ok(buckets) if buckets.is_object() => {
            report.add(
                "aw:buckets-index",
                "ok",
                "bucket index loaded",
                json!({"total": buckets.as_object().map(|value| value.len()).unwrap_or(0)}),
            );
            buckets
        }
        Ok(_) => {
            report.add(
                "aw:buckets-index",
                "fail",
                "failed to load bucket index: bucket index is not a dict",
                json!({}),
            );
            json!({})
        }
        Err(err) => {
            report.add(
                "aw:buckets-index",
                "fail",
                format!("failed to load bucket index: {err}"),
                json!({}),
            );
            json!({})
        }
    };

    let host = &cli.rdp_hostname;
    let (status, summary, details) = guard_bucket_health(
        &client,
        &aw_api_base,
        host,
        cli.guard_max_age_seconds,
        cli.guard_required,
    );
    report.add("bucket:collector-guard", &status, summary, details);

    let worktime_bucket = format!("aw-worktime-sessions_{host}");
    let worktime_event = if buckets.is_object() {
        latest_bucket_event(&client, &aw_api_base, &worktime_bucket)
            .ok()
            .flatten()
    } else {
        None
    };
    let activity =
        host_activity_from_worktime(worktime_event.as_ref(), cli.session_max_age_seconds);
    if worktime_event.is_some() {
        let (status, summary, mut details) = bucket_health(
            &client,
            &aw_api_base,
            &worktime_bucket,
            cli.session_max_age_seconds,
            "fail",
            "fail",
        );
        details["host_activity"] = activity.clone();
        report.add("bucket:worktime-sessions", &status, summary, details);
    } else {
        report.add(
            "bucket:worktime-sessions",
            "fail",
            "no events",
            json!({"bucket": worktime_bucket, "host_activity": activity}),
        );
    }

    let interactive_required = activity
        .get("active")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    for (bucket_name, label) in [
        ("aw-watcher-afk", "bucket:afk"),
        ("aw-watcher-window", "bucket:window"),
        ("aw-dlp-endpoint-signals", "bucket:endpoint-signals"),
    ] {
        let (mut status, mut summary, mut details) = bucket_timestamp_health(
            &client,
            &aw_api_base,
            &buckets,
            &format!("{bucket_name}_{host}"),
            cli.interactive_max_age_seconds,
            if interactive_required { "fail" } else { "warn" },
            if interactive_required { "fail" } else { "warn" },
        );
        details["interactive_required"] = Value::Bool(interactive_required);
        details["host_activity"] = activity.clone();
        if !interactive_required && status != "ok" {
            details["inactive_summary"] = Value::String(summary);
            status = "ok".to_string();
            summary = "inactive: no active interactive users".to_string();
        }
        report.add(label, &status, summary, details);
    }

    let (mut status, mut summary, details) = bucket_timestamp_health(
        &client,
        &aw_api_base,
        &buckets,
        &format!("aw-session-events_{host}"),
        cli.session_events_max_age_seconds,
        "fail",
        "warn",
    );
    if status == "warn" {
        if let Some(age) = details.get("age_seconds").and_then(Value::as_i64) {
            status = "ok".to_string();
            summary = format!("event-driven ({age}s since last logon marker)");
        }
    }
    report.add("bucket:session-events", &status, summary, details);

    validation_check(
        &mut report,
        &cli.validation_dir,
        cli.validation_max_age_seconds,
    );
    Ok(report.build())
}

fn main() -> Result<()> {
    let cli = Cli::parse().apply_env();
    let report = run(&cli)?;
    let json_text = serde_json::to_string_pretty(&report)? + "\n";
    let text = render_text(&report) + "\n";
    let json_path = cli.state_dir.join("aw-rus-health.json");
    let text_path = cli.state_dir.join("aw-rus-health.txt");
    write_atomic(&json_path, &json_text)?;
    write_atomic(&text_path, &text)?;
    chmod_if_possible(&json_path, 0o644);
    chmod_if_possible(&text_path, 0o644);
    if cli.json {
        print!("{json_text}");
    } else {
        print!("{text}");
    }
    std::process::exit(if report.ok {
        exit_codes::OK
    } else {
        exit_codes::ERROR
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_from_worktime_marks_recent_active_session() {
        let event = json!({"timestamp": "2026-05-18T10:00:00Z", "data": {"active": true}});
        let activity = host_activity_from_worktime(Some(&event), i64::MAX);
        assert_eq!(activity["active"], true);
        assert_eq!(activity["fresh"], true);
    }

    #[test]
    fn heartbeat_missing_activity_is_inactive() {
        let activity = host_activity_from_worktime(None, 900);
        assert_eq!(activity["active"], false);
        assert_eq!(activity["fresh"], false);
    }

    #[test]
    fn bucket_metadata_ts_reads_activitywatch_end_timestamp() {
        let buckets = json!({
            "aw-watcher-window_HOST": {
                "metadata": {
                    "end": "2026-06-01T10:54:37.976Z"
                }
            }
        });
        let ts = bucket_metadata_ts(&buckets, "aw-watcher-window_HOST").unwrap();
        assert_eq!(
            ts.to_rfc3339_opts(SecondsFormat::Millis, true),
            "2026-06-01T10:54:37.976Z"
        );
    }

    #[test]
    fn event_effective_ts_adds_duration_to_long_activitywatch_event() {
        let event = json!({
            "timestamp": "2026-06-01T10:30:29.573Z",
            "duration": 1448.403,
        });
        let ts = event_effective_ts(&event).unwrap();
        assert_eq!(
            ts.to_rfc3339_opts(SecondsFormat::Millis, true),
            "2026-06-01T10:54:37.976Z"
        );
    }

    #[test]
    fn normalizes_aw_api_base() {
        assert_eq!(
            normalize_aw_api_base("http://127.0.0.1:5600"),
            "http://127.0.0.1:5600/api/0"
        );
        assert_eq!(
            normalize_aw_api_base("http://127.0.0.1:5600/api/0"),
            "http://127.0.0.1:5600/api/0"
        );
    }
}
