use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, SecondsFormat, Utc};
use clap::Parser;
use detmir_core::parse_utc_rfc3339;
use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Debug, Parser)]
#[command(about = "AWatch DLP health check")]
struct Cli {
    #[arg(long, default_value = "http://127.0.0.1:5600")]
    aw_server: String,

    #[arg(long, default_value = "http://127.0.0.1:5601")]
    policy_server: String,

    #[arg(long, default_value = "http://127.0.0.1:5602")]
    case_server: String,

    #[arg(long, default_value_t = 900)]
    max_age_seconds: i64,

    #[arg(long)]
    strict_fileops: bool,

    #[arg(long, default_value_t = 100)]
    endpoint_queue_warn_depth: i64,

    #[arg(long, default_value_t = 1)]
    endpoint_send_failure_warn_count: i64,

    #[arg(long, default_value_t = 20)]
    fileops_sample_limit: i64,

    #[arg(long, default_value_t = 100)]
    fileops_queue_warn_depth: i64,

    #[arg(long, default_value_t = 1)]
    fileops_send_failure_warn_count: i64,

    #[arg(long, default_value_t = 0)]
    incident_sample_limit: i64,

    #[arg(long, default_value = "/var/lib/activitywatch/health")]
    state_dir: PathBuf,

    #[arg(long, default_value = "/opt/activitywatch/dlp-compliance/reports")]
    report_dir: PathBuf,

    #[arg(long, default_value = "152-fz,pci-dss")]
    profiles: String,

    #[arg(long)]
    json: bool,

    #[arg(long, default_value_t = 120)]
    overall_timeout_seconds: u64,

    #[arg(long, default_value = "full")]
    profile: String,

    #[arg(long, default_value_t = true)]
    enabled: bool,

    #[arg(long, default_value = "operator_disabled")]
    disabled_reason: String,

    #[arg(long, default_value = "")]
    disabled_since: String,
}

impl Cli {
    fn apply_env(mut self) -> Self {
        if !cli_arg_present("--aw-server") {
            self.aw_server = env_string("AW_HEALTH_AW_SERVER").unwrap_or(self.aw_server);
        }
        if !cli_arg_present("--policy-server") {
            self.policy_server =
                env_string("AW_HEALTH_POLICY_SERVER").unwrap_or(self.policy_server);
        }
        if !cli_arg_present("--case-server") {
            self.case_server = env_string("AW_HEALTH_CASE_SERVER").unwrap_or(self.case_server);
        }
        if !cli_arg_present("--max-age-seconds") {
            self.max_age_seconds = env_i64("AW_HEALTH_MAX_AGE_SECONDS", self.max_age_seconds);
        }
        if !cli_arg_present("--strict-fileops") {
            self.strict_fileops = env_bool("AW_HEALTH_STRICT_FILEOPS");
        }
        if !cli_arg_present("--endpoint-queue-warn-depth") {
            self.endpoint_queue_warn_depth = env_i64(
                "AW_DLP_HEALTH_ENDPOINT_QUEUE_WARN_DEPTH",
                self.endpoint_queue_warn_depth,
            );
        }
        if !cli_arg_present("--endpoint-send-failure-warn-count") {
            self.endpoint_send_failure_warn_count = env_i64(
                "AW_DLP_HEALTH_ENDPOINT_SEND_FAILURE_WARN_COUNT",
                self.endpoint_send_failure_warn_count,
            );
        }
        if !cli_arg_present("--fileops-sample-limit") {
            self.fileops_sample_limit = env_i64(
                "AW_DLP_HEALTH_FILEOPS_SAMPLE_LIMIT",
                self.fileops_sample_limit,
            );
        }
        if !cli_arg_present("--fileops-queue-warn-depth") {
            self.fileops_queue_warn_depth = env_i64(
                "AW_DLP_HEALTH_FILEOPS_QUEUE_WARN_DEPTH",
                self.fileops_queue_warn_depth,
            );
        }
        if !cli_arg_present("--fileops-send-failure-warn-count") {
            self.fileops_send_failure_warn_count = env_i64(
                "AW_DLP_HEALTH_FILEOPS_SEND_FAILURE_WARN_COUNT",
                self.fileops_send_failure_warn_count,
            );
        }
        if !cli_arg_present("--incident-sample-limit") {
            self.incident_sample_limit = env_i64(
                "AW_DLP_HEALTH_INCIDENT_SAMPLE_LIMIT",
                self.incident_sample_limit,
            );
        }
        if !cli_arg_present("--state-dir") {
            self.state_dir =
                env_path("AW_DLP_HEALTH_STATE_DIR").unwrap_or_else(|| self.state_dir.clone());
        }
        if !cli_arg_present("--report-dir") {
            self.report_dir =
                env_path("AW_DLP_COMPLIANCE_REPORT_DIR").unwrap_or_else(|| self.report_dir.clone());
        }
        if !cli_arg_present("--profiles") {
            self.profiles = env_string("AW_DLP_COMPLIANCE_PROFILES").unwrap_or(self.profiles);
        }
        if !cli_arg_present("--overall-timeout-seconds") {
            self.overall_timeout_seconds = env_u64(
                "AW_DLP_HEALTH_OVERALL_TIMEOUT_SECONDS",
                self.overall_timeout_seconds,
            );
        }
        if !cli_arg_present("--profile") {
            self.profile = env_string("AW_DLP_PROFILE")
                .or_else(|| env_string("DETMIR_PORTAL_DLP_PROFILE"))
                .unwrap_or(self.profile);
        }
        if !cli_arg_present("--enabled") {
            self.enabled = env_bool_default("AW_DLP_ENABLED", self.enabled);
        }
        if !cli_arg_present("--disabled-reason") {
            self.disabled_reason =
                env_string("AW_DLP_DISABLED_REASON").unwrap_or(self.disabled_reason);
        }
        if !cli_arg_present("--disabled-since") {
            self.disabled_since =
                env_string("AW_DLP_DISABLED_SINCE").unwrap_or(self.disabled_since);
        }
        self
    }
}

#[derive(Debug, Clone, Serialize)]
struct CheckResult {
    name: String,
    status: String,
    summary: String,
    details: Value,
}

#[derive(Debug, Serialize)]
struct Counts {
    ok: usize,
    warn: usize,
    fail: usize,
}

#[derive(Debug, Serialize)]
struct HealthPayload {
    ok: bool,
    counts: Counts,
    results: Vec<CheckResult>,
}

#[derive(Default)]
struct HealthReport {
    results: Vec<CheckResult>,
}

impl HealthReport {
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

    fn ok(&self) -> bool {
        !self.results.iter().any(|item| item.status == "fail")
    }

    fn payload(&self) -> HealthPayload {
        let mut counts = Counts {
            ok: 0,
            warn: 0,
            fail: 0,
        };
        for item in &self.results {
            match item.status.as_str() {
                "ok" => counts.ok += 1,
                "warn" => counts.warn += 1,
                "fail" => counts.fail += 1,
                _ => {}
            }
        }
        HealthPayload {
            ok: self.ok(),
            counts,
            results: self.results.clone(),
        }
    }

    fn render_text(&self) -> String {
        let mut lines = vec![
            "=== DLP Health Check ===".to_string(),
            format!("Timestamp: {}", utc_iso()),
            String::new(),
        ];
        for item in &self.results {
            let icon = match item.status.as_str() {
                "ok" => "OK",
                "warn" => "WARN",
                "fail" => "FAIL",
                other => other,
            };
            lines.push(format!("[{icon}] {}: {}", item.name, item.summary));
            if !item.details.as_object().is_none_or(|v| v.is_empty()) {
                lines.push(format!(
                    "  details: {}",
                    serde_json::to_string(&sort_json_value(&item.details))
                        .unwrap_or_else(|_| "{}".to_string())
                ));
            }
        }
        lines.push(String::new());
        lines.push(format!(
            "Overall: {}",
            if self.ok() { "OK" } else { "FAIL" }
        ));
        lines.join("\n")
    }
}

fn utc_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true)
}

fn parse_ts(value: Option<&str>) -> Option<DateTime<Utc>> {
    value.and_then(|text| parse_utc_rfc3339(text).ok())
}

fn age_seconds(ts: Option<DateTime<Utc>>, now: DateTime<Utc>) -> Option<i64> {
    ts.map(|ts| (now - ts).num_seconds().max(0))
}

fn int_or_zero(value: Option<&Value>) -> i64 {
    match value {
        Some(Value::Number(n)) => n
            .as_i64()
            .or_else(|| n.as_u64().map(|v| v as i64))
            .unwrap_or(0),
        Some(Value::String(s)) => s.parse::<i64>().unwrap_or(0),
        Some(Value::Bool(v)) => i64::from(*v),
        _ => 0,
    }
}

fn value_str(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(v)) => v.to_string(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

fn path_tail(value: Option<&Value>, parts: usize) -> String {
    let text = value_str(value).replace('\\', "/").trim().to_string();
    if text.is_empty() {
        return String::new();
    }
    let tokens: Vec<&str> = text.split('/').filter(|item| !item.is_empty()).collect();
    if tokens.is_empty() {
        return text;
    }
    let start = tokens.len().saturating_sub(parts);
    tokens[start..].join("/")
}

fn text_excerpt(value: Option<&Value>, limit: usize) -> String {
    let text = value_str(value)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if text.chars().count() <= limit {
        return text;
    }
    let prefix: String = text.chars().take(limit.saturating_sub(1)).collect();
    format!("{}…", prefix.trim_end())
}

fn http_json(client: &Client, url: &str, timeout_seconds: u64, attempts: usize) -> Result<Value> {
    let mut last_error: Option<anyhow::Error> = None;
    for attempt in 0..attempts.max(1) {
        match client
            .get(url)
            .timeout(Duration::from_secs(timeout_seconds))
            .send()
            .and_then(|resp| resp.error_for_status())
            .with_context(|| format!("GET {url}"))
            .and_then(|resp| resp.json::<Value>().context("parse JSON"))
        {
            Ok(value) => return Ok(value),
            Err(err) => {
                last_error = Some(err);
                if attempt + 1 < attempts.max(1) {
                    sleep(Duration::from_millis(500 * (1 << attempt)));
                }
            }
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow!("HTTP request failed")))
}

fn check_http_endpoint(report: &mut HealthReport, client: &Client, name: &str, url: &str) {
    match http_json(client, url, 15, 2) {
        Ok(payload) => report.add(
            name,
            "ok",
            "HTTP endpoint responded",
            json!({"url": url, "payload": payload}),
        ),
        Err(err) => report.add(
            name,
            "fail",
            format!("HTTP endpoint failed: {err}"),
            json!({"url": url}),
        ),
    }
}

fn run_systemctl(args: &[&str]) -> (i32, String) {
    match Command::new("systemctl").args(args).output() {
        Ok(output) => {
            let mut text = String::from_utf8_lossy(&output.stdout).to_string();
            text.push_str(&String::from_utf8_lossy(&output.stderr));
            (output.status.code().unwrap_or(1), text.trim().to_string())
        }
        Err(err) => (1, err.to_string()),
    }
}

fn check_systemd_unit(report: &mut HealthReport, unit: &str, kind: &str) {
    let (active_rc, active_out) = run_systemctl(&["is-active", unit]);
    let (enabled_rc, enabled_out) = run_systemctl(&["is-enabled", unit]);
    let (exists_rc, _) = run_systemctl(&["status", unit]);
    if exists_rc != 0 && active_rc != 0 && enabled_rc != 0 {
        report.add(
            format!("systemd:{unit}"),
            "warn",
            "unit not installed",
            json!({"kind": kind}),
        );
        return;
    }
    if active_rc == 0 && enabled_rc == 0 {
        report.add(
            format!("systemd:{unit}"),
            "ok",
            "active and enabled",
            json!({"kind": kind}),
        );
        return;
    }
    report.add(
        format!("systemd:{unit}"),
        "fail",
        "unit is not active/enabled",
        json!({
            "kind": kind,
            "active": if active_out.is_empty() { active_rc.to_string() } else { active_out },
            "enabled": if enabled_out.is_empty() { enabled_rc.to_string() } else { enabled_out },
        }),
    );
}

fn latest_bucket_ts(
    client: &Client,
    api_base: &str,
    bucket_id: &str,
    bucket_meta: &Value,
) -> Option<DateTime<Utc>> {
    let meta_ts = bucket_meta
        .get("metadata")
        .and_then(|v| v.get("end"))
        .and_then(Value::as_str);
    if let Some(ts) = parse_ts(meta_ts) {
        return Some(ts);
    }
    let url = format!("{api_base}/buckets/{bucket_id}/events?limit=1");
    let events = http_json(client, &url, 15, 2).ok()?;
    events
        .as_array()
        .and_then(|items| items.first())
        .and_then(|event| event.get("timestamp"))
        .and_then(Value::as_str)
        .and_then(|text| parse_ts(Some(text)))
}

fn bucket_suffix(bucket_id: &str, prefix: &str) -> String {
    bucket_id
        .strip_prefix(prefix)
        .unwrap_or(bucket_id)
        .to_string()
}

fn bucket_keys_with_prefix(buckets: &BTreeMap<String, Value>, prefix: &str) -> Vec<String> {
    buckets
        .keys()
        .filter(|bucket_id| bucket_id.starts_with(prefix))
        .cloned()
        .collect()
}

fn check_incident_buckets(
    report: &mut HealthReport,
    client: &Client,
    api_base: &str,
    buckets: &BTreeMap<String, Value>,
    max_age_seconds: i64,
) {
    let now = Utc::now();
    let prefix = "aw-dlp-incidents_";
    let matched = bucket_keys_with_prefix(buckets, prefix);
    if matched.is_empty() {
        report.add(
            "buckets:incidents",
            "ok",
            "no incident buckets yet",
            json!({"prefix": prefix, "bucket_count": 0}),
        );
        return;
    }
    let mut ages = Vec::new();
    let mut unknown = Vec::new();
    let mut stale = Vec::new();
    for bucket_id in &matched {
        let ts = latest_bucket_ts(
            client,
            api_base,
            bucket_id,
            buckets.get(bucket_id).unwrap_or(&Value::Null),
        );
        match age_seconds(ts, now) {
            Some(age) => {
                ages.push(age);
                if age > max_age_seconds {
                    stale.push(json!({"bucket": bucket_id, "age_seconds": age}));
                }
            }
            None => unknown.push(bucket_id.clone()),
        }
    }
    if !stale.is_empty() && unknown.is_empty() {
        report.add(
            "buckets:incidents",
            "ok",
            "no recent incidents",
            json!({
                "prefix": prefix,
                "bucket_count": matched.len(),
                "max_age_seconds": max_age_seconds,
                "max_observed_age_seconds": ages.iter().max().copied(),
                "stale": stale,
                "unknown": [],
            }),
        );
        return;
    }
    let (status, summary) = if unknown.is_empty() {
        ("ok", "incident buckets healthy".to_string())
    } else {
        (
            "warn",
            format!("{} incident buckets without timestamp", unknown.len()),
        )
    };
    report.add(
        "buckets:incidents",
        status,
        summary,
        json!({
            "prefix": prefix,
            "bucket_count": matched.len(),
            "max_age_seconds": max_age_seconds,
            "max_observed_age_seconds": ages.iter().max().copied(),
            "stale": stale,
            "unknown": unknown,
        }),
    );
}

struct HostBucketCheck<'a> {
    check_name: &'a str,
    prefix: &'a str,
    max_age_seconds: i64,
    strict: bool,
}

fn worktime_activity_map(
    client: &Client,
    api_base: &str,
    buckets: &BTreeMap<String, Value>,
    max_age_seconds: i64,
) -> BTreeMap<String, Value> {
    let now = Utc::now();
    let prefix = "aw-worktime-sessions_";
    let mut activity = BTreeMap::new();
    for bucket_id in bucket_keys_with_prefix(buckets, prefix) {
        let host = bucket_suffix(&bucket_id, prefix);
        let mut latest_ts: Option<DateTime<Utc>> = None;
        let mut latest_active = false;
        let url = format!("{api_base}/buckets/{bucket_id}/events?limit=20");
        if let Ok(events) = http_json(client, &url, 15, 2)
            && let Some(items) = events.as_array()
        {
            for event in items {
                let ts = event
                    .get("timestamp")
                    .and_then(Value::as_str)
                    .and_then(|text| parse_ts(Some(text)));
                if let Some(ts) = ts
                    && latest_ts.is_none_or(|current| ts > current)
                {
                    latest_ts = Some(ts);
                    latest_active = event
                        .get("data")
                        .and_then(|v| v.get("active"))
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                }
            }
        }
        let age = age_seconds(latest_ts, now);
        activity.insert(
            host,
            json!({
                "active": latest_ts.is_some() && latest_active && age.unwrap_or(0) <= max_age_seconds,
                "age_seconds": age,
                "bucket": bucket_id,
            }),
        );
    }
    activity
}

fn check_host_bucket_freshness(
    report: &mut HealthReport,
    client: &Client,
    api_base: &str,
    buckets: &BTreeMap<String, Value>,
    config: HostBucketCheck<'_>,
) {
    let now = Utc::now();
    let matched = bucket_keys_with_prefix(buckets, config.prefix);
    let worktime = worktime_activity_map(client, api_base, buckets, config.max_age_seconds);
    let active_hosts: Vec<String> = worktime
        .iter()
        .filter(|(_, meta)| meta.get("active").and_then(Value::as_bool).unwrap_or(false))
        .map(|(host, _)| host.clone())
        .collect();
    let matched_by_host: BTreeMap<String, String> = matched
        .iter()
        .map(|bucket_id| (bucket_suffix(bucket_id, config.prefix), bucket_id.clone()))
        .collect();
    let mut ignored_unmanaged = Vec::new();
    let mut ignored_inactive = Vec::new();
    let mut missing_active = Vec::new();
    let mut stale = Vec::new();
    let mut unknown = Vec::new();
    let mut fresh = Vec::new();

    for (host, bucket_id) in &matched_by_host {
        if !worktime.contains_key(host) {
            ignored_unmanaged.push(bucket_id.clone());
            continue;
        }
        if !active_hosts.contains(host) {
            ignored_inactive.push(bucket_id.clone());
            continue;
        }
        let ts = latest_bucket_ts(
            client,
            api_base,
            bucket_id,
            buckets.get(bucket_id).unwrap_or(&Value::Null),
        );
        match age_seconds(ts, now) {
            Some(age) if age > config.max_age_seconds => {
                stale.push(json!({"bucket": bucket_id, "age_seconds": age}));
            }
            Some(_) => fresh.push(bucket_id.clone()),
            None => unknown.push(bucket_id.clone()),
        }
    }

    for host in &active_hosts {
        if !matched_by_host.contains_key(host) {
            missing_active.push(host.clone());
        }
    }

    if active_hosts.is_empty() {
        report.add(
            config.check_name,
            "ok",
            format!(
                "no active managed hosts require {} freshness",
                config.check_name.trim_start_matches("buckets:")
            ),
            json!({
                "active_hosts": [],
                "ignored_unmanaged": ignored_unmanaged,
                "ignored_inactive": ignored_inactive,
                "worktime_hosts": worktime.keys().cloned().collect::<Vec<_>>(),
            }),
        );
        return;
    }

    let mut status = "ok";
    let mut summary = format!("{} active host buckets fresh", fresh.len());
    if !missing_active.is_empty() {
        status = if config.strict { "fail" } else { "warn" };
        summary = format!(
            "{} active hosts missing {} buckets",
            missing_active.len(),
            config.check_name.trim_start_matches("buckets:")
        );
    } else if !stale.is_empty() {
        status = if config.strict { "fail" } else { "warn" };
        summary = format!("{} active host buckets stale", stale.len());
    } else if !unknown.is_empty() {
        status = "warn";
        summary = format!("{} active host buckets without timestamp", unknown.len());
    }

    report.add(
        config.check_name,
        status,
        summary,
        json!({
            "active_hosts": active_hosts,
            "fresh": fresh,
            "stale": stale,
            "missing_active": missing_active,
            "unknown": unknown,
            "ignored_unmanaged": ignored_unmanaged,
            "ignored_inactive": ignored_inactive,
        }),
    );
}

fn check_endpoint_signal_buckets(
    report: &mut HealthReport,
    client: &Client,
    api_base: &str,
    buckets: &BTreeMap<String, Value>,
    max_age_seconds: i64,
) {
    check_host_bucket_freshness(
        report,
        client,
        api_base,
        buckets,
        HostBucketCheck {
            check_name: "buckets:endpoint-signals",
            prefix: "aw-dlp-endpoint-signals_",
            max_age_seconds,
            strict: true,
        },
    );
}

fn check_file_operations_buckets(
    report: &mut HealthReport,
    client: &Client,
    api_base: &str,
    buckets: &BTreeMap<String, Value>,
    max_age_seconds: i64,
    strict: bool,
) {
    check_host_bucket_freshness(
        report,
        client,
        api_base,
        buckets,
        HostBucketCheck {
            check_name: "buckets:file-operations",
            prefix: "aw-file-operations_",
            max_age_seconds,
            strict,
        },
    );
}

fn load_counter_state(path: &Path) -> Value {
    let Ok(text) = fs::read_to_string(path) else {
        return json!({"counters": {}});
    };
    let Ok(mut payload) = serde_json::from_str::<Value>(&text) else {
        return json!({"counters": {}});
    };
    if !payload.is_object() {
        return json!({"counters": {}});
    }
    if !payload.get("counters").is_some_and(Value::is_object) {
        payload["counters"] = json!({});
    }
    payload
}

fn save_counter_state(path: &Path, state: &Value) -> Option<String> {
    let content = match serde_json::to_string_pretty(&sort_json_value(state)) {
        Ok(text) => text + "\n",
        Err(err) => return Some(err.to_string()),
    };
    if let Err(err) = write_atomic(path, &content) {
        match fs::write(path, content) {
            Ok(()) => None,
            Err(_) => Some(err.to_string()),
        }
    } else {
        None
    }
}

fn counter_delta(
    counter_state: Option<&mut Value>,
    key: &str,
    current_value: i64,
) -> (Option<i64>, i64) {
    let Some(state) = counter_state else {
        return (None, current_value);
    };
    if !state.get("counters").is_some_and(Value::is_object) {
        state["counters"] = json!({});
    }
    let counters = state
        .get_mut("counters")
        .and_then(Value::as_object_mut)
        .unwrap();
    let previous = counters.get(key).and_then(|v| match v {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    });
    counters.insert(key.to_string(), json!(current_value));
    if previous.is_none() || current_value < previous.unwrap_or(0) {
        (previous, 0)
    } else {
        (previous, current_value - previous.unwrap_or(0))
    }
}

fn transport_counter_key(prefix: &str, bucket_id: &str, data: &Value, metric: &str) -> String {
    format!(
        "{prefix}:{bucket_id}:{}:{}:{}:{metric}",
        value_str(data.get("hostname")),
        int_or_zero(data.get("sessionId")),
        value_str(data.get("username"))
    )
}

#[derive(Clone, Copy)]
struct RuntimeThresholds {
    sample_limit: i64,
    queue_warn_depth: i64,
    send_failure_warn_count: i64,
}

fn check_file_operations_runtime(
    report: &mut HealthReport,
    client: &Client,
    api_base: &str,
    buckets: &BTreeMap<String, Value>,
    thresholds: RuntimeThresholds,
    mut counter_state: Option<&mut Value>,
) {
    let now = Utc::now();
    let prefix = "aw-file-operations_";
    let matched = bucket_keys_with_prefix(buckets, prefix);
    if matched.is_empty() {
        report.add(
            "file-operations-runtime",
            "warn",
            "no file-operations buckets to sample",
            json!({"bucket_count": 0}),
        );
        return;
    }

    let mut sampled = Vec::new();
    let mut latest_operations = Vec::new();
    let mut latest_health = Vec::new();
    let mut warnings = Vec::new();
    let mut read_failed = Vec::new();

    for bucket_id in &matched {
        let url = format!(
            "{api_base}/buckets/{bucket_id}/events?limit={}",
            thresholds.sample_limit
        );
        let events = match http_json(client, &url, 15, 2) {
            Ok(Value::Array(items)) => items,
            Ok(_) => {
                read_failed
                    .push(json!({"bucket": bucket_id, "error": "events response is not a list"}));
                continue;
            }
            Err(err) => {
                read_failed.push(json!({"bucket": bucket_id, "error": err.to_string()}));
                continue;
            }
        };
        let mut operation_counts: BTreeMap<String, i64> = BTreeMap::new();
        let mut latest_health_event: Option<Value> = None;
        let mut latest_health_ts: Option<DateTime<Utc>> = None;
        for event in &events {
            let data = event
                .get("data")
                .filter(|v| v.is_object())
                .unwrap_or(&Value::Null);
            let ts = event
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(|text| parse_ts(Some(text)));
            let signal_type = value_str(data.get("signalType"));
            let operation = value_str(data.get("operation"));
            if signal_type == "collector_health" {
                if latest_health_event.is_none()
                    || ts.is_some_and(|ts| latest_health_ts.is_none_or(|current| ts > current))
                {
                    latest_health_event = Some(event.clone());
                    latest_health_ts = ts;
                }
                continue;
            }
            if !operation.is_empty() {
                *operation_counts.entry(operation.clone()).or_insert(0) += 1;
                latest_operations.push(json!({
                    "bucket": bucket_id,
                    "timestamp": event.get("timestamp").cloned().unwrap_or(Value::Null),
                    "age_seconds": age_seconds(ts, now),
                    "operation": operation,
                    "username": value_str(data.get("username")),
                    "hostname": value_str(data.get("hostname")),
                    "extension": value_str(data.get("extension")),
                    "archiveHint": data.get("archiveHint").and_then(Value::as_bool).unwrap_or(false),
                    "path_tail": path_tail(data.get("path"), 2),
                    "size": int_or_zero(data.get("size")),
                }));
            }
        }
        sampled.push(json!({
            "bucket": bucket_id,
            "sampled_events": events.len(),
            "operation_counts": operation_counts,
        }));
        let Some(health_event) = latest_health_event else {
            warnings.push(json!({"bucket": bucket_id, "metric": "collector_health", "value": "missing_in_sample"}));
            continue;
        };
        let health_data = health_event.get("data").unwrap_or(&Value::Null);
        let send_failures = int_or_zero(health_data.get("sendFailures"));
        let (previous, delta) = counter_delta(
            counter_state.as_deref_mut(),
            &transport_counter_key("file-operations", bucket_id, health_data, "sendFailures"),
            send_failures,
        );
        let health_item = json!({
            "bucket": bucket_id,
            "timestamp": health_event.get("timestamp").cloned().unwrap_or(Value::Null),
            "age_seconds": age_seconds(latest_health_ts, now),
            "queueDepth": int_or_zero(health_data.get("queueDepth")),
            "eventsEnqueued": int_or_zero(health_data.get("eventsEnqueued")),
            "eventsFlushed": int_or_zero(health_data.get("eventsFlushed")),
            "sendFailures": send_failures,
            "sendFailuresPrevious": previous,
            "sendFailuresDelta": delta,
            "username": value_str(health_data.get("username")),
            "hostname": value_str(health_data.get("hostname")),
            "sessionId": int_or_zero(health_data.get("sessionId")),
        });
        if health_item["queueDepth"].as_i64().unwrap_or(0) > thresholds.queue_warn_depth {
            warnings.push(json!({"bucket": bucket_id, "metric": "queueDepth", "value": health_item["queueDepth"], "threshold": thresholds.queue_warn_depth}));
        }
        if thresholds.send_failure_warn_count > 0 && delta >= thresholds.send_failure_warn_count {
            warnings.push(json!({
                "bucket": bucket_id,
                "metric": "sendFailuresDelta",
                "value": delta,
                "current": send_failures,
                "previous": previous,
                "threshold": thresholds.send_failure_warn_count,
            }));
        }
        latest_health.push(health_item);
    }

    latest_operations.sort_by_key(|item| Reverse(value_str(item.get("timestamp"))));
    let mut status = "ok";
    let mut summary = format!("{} file-operations buckets sampled", matched.len());
    if !read_failed.is_empty() {
        status = "warn";
        summary = format!(
            "{} file-operations buckets failed to sample",
            read_failed.len()
        );
    } else if !warnings.is_empty() {
        status = "warn";
        summary = "file-operations runtime counters outside expectations".to_string();
    }

    report.add(
        "file-operations-runtime",
        status,
        summary,
        json!({
            "bucket_count": matched.len(),
            "sample_limit": thresholds.sample_limit,
            "sampled": sampled,
            "latest_health": latest_health,
            "latest_operations": latest_operations.into_iter().take(5).collect::<Vec<_>>(),
            "warnings": warnings,
            "read_failed": read_failed,
            "thresholds": {"queueDepth": thresholds.queue_warn_depth, "sendFailures": thresholds.send_failure_warn_count},
        }),
    );
}

fn check_endpoint_self_test_metrics(
    report: &mut HealthReport,
    client: &Client,
    api_base: &str,
    buckets: &BTreeMap<String, Value>,
    queue_warn_depth: i64,
    send_failure_warn_count: i64,
    mut counter_state: Option<&mut Value>,
) {
    let now = Utc::now();
    let mut missing = Vec::new();
    let mut latest_self_tests = Vec::new();
    let mut warnings = Vec::new();
    for bucket_id in bucket_keys_with_prefix(buckets, "aw-dlp-endpoint-signals_") {
        let url = format!("{api_base}/buckets/{bucket_id}/events?limit=20");
        let events = match http_json(client, &url, 15, 2) {
            Ok(Value::Array(items)) => items,
            Ok(_) => {
                report.add(
                    format!("endpoint-self-test:{bucket_id}"),
                    "warn",
                    "failed to read events: events response is not a list",
                    json!({"bucket": bucket_id}),
                );
                continue;
            }
            Err(err) => {
                report.add(
                    format!("endpoint-self-test:{bucket_id}"),
                    "warn",
                    format!("failed to read events: {err}"),
                    json!({"bucket": bucket_id}),
                );
                continue;
            }
        };
        let mut latest_event: Option<Value> = None;
        let mut latest_ts: Option<DateTime<Utc>> = None;
        for event in &events {
            let data = event
                .get("data")
                .filter(|v| v.is_object())
                .unwrap_or(&Value::Null);
            let is_self_test = data.get("signalType").and_then(Value::as_str) == Some("self_test");
            let has_expected = [
                "queueDepth",
                "eventsEnqueued",
                "eventsFlushed",
                "sendFailures",
            ]
            .iter()
            .all(|key| data.get(*key).is_some());
            if !is_self_test || !has_expected {
                continue;
            }
            let ts = event
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(|text| parse_ts(Some(text)));
            if latest_event.is_none()
                || ts.is_some_and(|ts| latest_ts.is_none_or(|current| ts > current))
            {
                latest_event = Some(event.clone());
                latest_ts = ts;
            }
        }
        let Some(latest_event) = latest_event else {
            missing.push(bucket_id);
            continue;
        };
        let data = latest_event.get("data").unwrap_or(&Value::Null);
        let send_failures = int_or_zero(data.get("sendFailures"));
        let (previous, delta) = counter_delta(
            counter_state.as_deref_mut(),
            &transport_counter_key("endpoint-self-test", &bucket_id, data, "sendFailures"),
            send_failures,
        );
        let item = json!({
            "bucket": bucket_id,
            "timestamp": latest_event.get("timestamp").cloned().unwrap_or(Value::Null),
            "age_seconds": age_seconds(latest_ts, now),
            "queueDepth": int_or_zero(data.get("queueDepth")),
            "eventsEnqueued": int_or_zero(data.get("eventsEnqueued")),
            "eventsFlushed": int_or_zero(data.get("eventsFlushed")),
            "sendFailures": send_failures,
            "sendFailuresPrevious": previous,
            "sendFailuresDelta": delta,
        });
        if item["queueDepth"].as_i64().unwrap_or(0) > queue_warn_depth {
            warnings.push(json!({"bucket": item["bucket"], "metric": "queueDepth", "value": item["queueDepth"], "threshold": queue_warn_depth}));
        }
        if send_failure_warn_count > 0 && delta >= send_failure_warn_count {
            warnings.push(json!({
                "bucket": item["bucket"],
                "metric": "sendFailuresDelta",
                "value": delta,
                "current": send_failures,
                "previous": previous,
                "threshold": send_failure_warn_count,
            }));
        }
        latest_self_tests.push(item);
    }
    let thresholds =
        json!({"queueDepth": queue_warn_depth, "sendFailures": send_failure_warn_count});
    if !missing.is_empty() {
        report.add(
            "endpoint-self-test-metrics",
            "warn",
            "missing transport metrics in sampled self_test events",
            json!({"buckets": missing, "latest_self_tests": latest_self_tests, "thresholds": thresholds}),
        );
    } else if !warnings.is_empty() {
        report.add(
            "endpoint-self-test-metrics",
            "warn",
            "endpoint transport counters outside thresholds",
            json!({"latest_self_tests": latest_self_tests, "warnings": warnings, "thresholds": thresholds}),
        );
    } else {
        report.add(
            "endpoint-self-test-metrics",
            "ok",
            "self_test transport metrics present",
            json!({"latest_self_tests": latest_self_tests, "thresholds": thresholds}),
        );
    }
}

fn check_incident_runtime(
    report: &mut HealthReport,
    client: &Client,
    api_base: &str,
    buckets: &BTreeMap<String, Value>,
    sample_limit: i64,
) {
    let now = Utc::now();
    let prefix = "aw-dlp-incidents_";
    let matched = bucket_keys_with_prefix(buckets, prefix);
    if matched.is_empty() {
        report.add(
            "incident-runtime",
            "ok",
            "no incident buckets to sample",
            json!({"bucket_count": 0}),
        );
        return;
    }
    if sample_limit <= 0 {
        let metadata: Vec<Value> = matched
            .iter()
            .map(|bucket_id| {
                let ts = latest_bucket_ts(
                    client,
                    api_base,
                    bucket_id,
                    buckets.get(bucket_id).unwrap_or(&Value::Null),
                );
                json!({
                    "bucket": bucket_id,
                    "end": ts.map(|ts| ts.to_rfc3339_opts(SecondsFormat::Secs, true)),
                    "age_seconds": age_seconds(ts, now),
                })
            })
            .collect();
        report.add(
            "incident-runtime",
            "ok",
            "incident event sampling disabled",
            json!({"bucket_count": matched.len(), "sample_limit": sample_limit, "metadata": metadata}),
        );
        return;
    }

    let mut sampled = Vec::new();
    let mut latest_incidents = Vec::new();
    let mut read_failed = Vec::new();
    let mut totals = BTreeMap::from([
        ("sampled_events".to_string(), 0_i64),
        ("real_incidents".to_string(), 0_i64),
        ("self_tests".to_string(), 0_i64),
    ]);
    let mut severity_counts: BTreeMap<String, i64> = BTreeMap::new();
    let mut action_counts: BTreeMap<String, i64> = BTreeMap::new();
    let mut rule_counts: HashMap<String, i64> = HashMap::new();

    for bucket_id in &matched {
        let url = format!("{api_base}/buckets/{bucket_id}/events?limit={sample_limit}");
        let events = match http_json(client, &url, 5, 1) {
            Ok(Value::Array(items)) => items,
            Ok(_) => {
                read_failed
                    .push(json!({"bucket": bucket_id, "error": "events response is not a list"}));
                continue;
            }
            Err(err) => {
                read_failed.push(json!({"bucket": bucket_id, "error": err.to_string()}));
                continue;
            }
        };
        let mut bucket_summary = BTreeMap::from([
            ("bucket".to_string(), json!(bucket_id)),
            ("sampled_events".to_string(), json!(events.len())),
            ("real_incidents".to_string(), json!(0)),
            ("self_tests".to_string(), json!(0)),
        ]);
        *totals.get_mut("sampled_events").unwrap() += events.len() as i64;
        for event in &events {
            let data = event
                .get("data")
                .filter(|v| v.is_object())
                .unwrap_or(&Value::Null);
            let signal_type = value_str(data.get("signalType"));
            let source = value_str(data.get("source"));
            let rule_id = value_str(data.get("ruleId")).trim().to_string();
            let rule_id = if rule_id.is_empty() {
                value_str(data.get("rule_id")).trim().to_string()
            } else {
                rule_id
            };
            let is_self_test = signal_type == "self_test"
                || source == "self-test"
                || rule_id.starts_with("selftest-");
            if is_self_test {
                *bucket_summary.get_mut("self_tests").unwrap() =
                    json!(bucket_summary["self_tests"].as_i64().unwrap_or(0) + 1);
                *totals.get_mut("self_tests").unwrap() += 1;
                continue;
            }
            let ts = event
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(|text| parse_ts(Some(text)));
            let severity = nonempty_lower(data.get("severity"), "unknown");
            let action = nonempty_lower(data.get("action"), "unknown");
            let rule_key = if rule_id.is_empty() {
                "unknown".to_string()
            } else {
                rule_id.clone()
            };
            *severity_counts.entry(severity.clone()).or_insert(0) += 1;
            *action_counts.entry(action.clone()).or_insert(0) += 1;
            *rule_counts.entry(rule_key).or_insert(0) += 1;
            *bucket_summary.get_mut("real_incidents").unwrap() =
                json!(bucket_summary["real_incidents"].as_i64().unwrap_or(0) + 1);
            *totals.get_mut("real_incidents").unwrap() += 1;
            latest_incidents.push(json!({
                "bucket": bucket_id,
                "timestamp": event.get("timestamp").cloned().unwrap_or(Value::Null),
                "age_seconds": age_seconds(ts, now),
                "ruleId": rule_id,
                "severity": severity,
                "action": action,
                "username": value_str(data.get("username")),
                "hostname": value_str(data.get("hostname")),
                "source": source,
                "message_excerpt": text_excerpt(data.get("message"), 120),
            }));
        }
        sampled.push(json!(bucket_summary));
    }

    latest_incidents.sort_by_key(|item| Reverse(value_str(item.get("timestamp"))));
    let mut sorted_rules: Vec<(String, i64)> = rule_counts.into_iter().collect();
    sorted_rules.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let rule_counts_top: BTreeMap<String, i64> = sorted_rules.into_iter().take(10).collect();
    let mut status = "ok";
    let mut summary = format!(
        "{} real incidents in sampled events",
        totals["real_incidents"]
    );
    if !read_failed.is_empty() {
        status = "warn";
        summary = format!("{} incident buckets failed to sample", read_failed.len());
    } else if totals["real_incidents"] == 0 {
        summary = "no real incidents in sampled events".to_string();
    }
    report.add(
        "incident-runtime",
        status,
        summary,
        json!({
            "bucket_count": matched.len(),
            "sample_limit": sample_limit,
            "totals": totals,
            "sampled": sampled,
            "severity_counts": severity_counts,
            "action_counts": action_counts,
            "rule_counts": rule_counts_top,
            "latest_incidents": latest_incidents.into_iter().take(5).collect::<Vec<_>>(),
            "read_failed": read_failed,
        }),
    );
}

fn nonempty_lower(value: Option<&Value>, default: &str) -> String {
    let text = value_str(value).trim().to_ascii_lowercase();
    if text.is_empty() {
        default.to_string()
    } else {
        text
    }
}

fn check_compliance_reports(
    report: &mut HealthReport,
    report_dir: &Path,
    profiles: &[String],
    month: &str,
) {
    let mut missing = Vec::new();
    let mut present = Vec::new();
    for profile in profiles {
        for suffix in ["html", "json"] {
            let path = report_dir.join(format!("{profile}-{month}.{suffix}"));
            if path.exists() {
                present.push(path.to_string_lossy().to_string());
            } else {
                missing.push(path.to_string_lossy().to_string());
            }
        }
    }
    if missing.is_empty() {
        report.add(
            "compliance-reports",
            "ok",
            "all expected compliance artifacts exist",
            json!({"present": present}),
        );
    } else {
        report.add(
            "compliance-reports",
            "fail",
            "missing expected compliance report artifacts",
            json!({"present": present, "missing": missing}),
        );
    }
}

fn normalized_profile(profile: &str) -> String {
    match profile.trim().to_ascii_lowercase().as_str() {
        "disabled" | "off" => "core_only".to_string(),
        "core-only" | "core_only" => "core_only".to_string(),
        "light" | "lite" => "light".to_string(),
        "on-demand" | "on_demand" => "on_demand".to_string(),
        "enabled" | "on" | "full" => "full".to_string(),
        "" => "full".to_string(),
        other => other.to_string(),
    }
}

fn profile_is_disabled(profile: &str) -> bool {
    matches!(normalized_profile(profile).as_str(), "core_only")
}

fn profile_checks_heavy_services(profile: &str) -> bool {
    matches!(normalized_profile(profile).as_str(), "full" | "on_demand")
}

fn build_report(cli: &Cli, client: &Client) -> HealthReport {
    let mut report = HealthReport::default();
    let profile = normalized_profile(&cli.profile);
    if !cli.enabled || profile_is_disabled(&profile) {
        report.add(
            "dlp:mode",
            "ok",
            "DLP runtime disabled by production profile",
            json!({
                "mode": "disabled",
                "profile": profile,
                "reason": &cli.disabled_reason,
                "disabled_since": empty_string_as_null(&cli.disabled_since),
                "checks_skipped": [
                    "policy API",
                    "case API",
                    "DLP systemd units",
                    "DLP ActivityWatch buckets",
                    "DLP compliance reports"
                ],
                "load_reduction": [
                    "no DLP bucket freshness reads",
                    "no DLP case/policy HTTP checks",
                    "no DLP compliance filesystem scan"
                ]
            }),
        );
        return report;
    }
    report.add(
        "dlp:mode",
        "ok",
        format!("DLP runtime profile {profile}"),
        json!({
            "mode": if profile == "light" { "light" } else { "enabled" },
            "profile": profile,
            "heavy_services_checked": profile_checks_heavy_services(&cli.profile)
        }),
    );

    let aw_api_base = format!("{}/api/0", cli.aw_server.trim_end_matches('/'));
    let counter_state_path = cli.state_dir.join("dlp-health-check-counters.json");
    let mut counter_state = load_counter_state(&counter_state_path);

    check_http_endpoint(
        &mut report,
        client,
        "http:aw",
        &format!("{aw_api_base}/info"),
    );
    if profile_checks_heavy_services(&cli.profile) {
        check_http_endpoint(
            &mut report,
            client,
            "http:policy",
            &format!("{}/healthz", cli.policy_server.trim_end_matches('/')),
        );
        check_http_endpoint(
            &mut report,
            client,
            "http:cases",
            &format!("{}/health", cli.case_server.trim_end_matches('/')),
        );
    } else {
        report.add(
            "http:heavy-dlp",
            "ok",
            "heavy DLP policy/case HTTP checks skipped for lightweight profile",
            json!({
                "profile": profile,
                "skipped": ["policy API", "case API"]
            }),
        );
    }

    for unit in ["activitywatch-server", "aw-worktime-api.service"] {
        check_systemd_unit(&mut report, unit, "service");
    }
    if profile_checks_heavy_services(&cli.profile) {
        for unit in [
            "aw-dlp-policy-engine.service",
            "aw-dlp-case-management.service",
        ] {
            check_systemd_unit(&mut report, unit, "service");
        }
        for unit in [
            "aw-dlp-report-scheduler.timer",
            "aw-dlp-syslog-forwarder.timer",
            "aw-dlp-webhook-sender.timer",
            "aw-dlp-cef-exporter.timer",
            "activitywatch-dlp-aggregator.timer",
            "aw-dlp-ioc-refresh.timer",
        ] {
            check_systemd_unit(&mut report, unit, "timer");
        }
    } else {
        report.add(
            "systemd:heavy-dlp",
            "ok",
            "heavy DLP systemd checks skipped for lightweight profile",
            json!({
                "profile": profile,
                "skipped": [
                    "aw-dlp-policy-engine.service",
                    "aw-dlp-case-management.service",
                    "aw-dlp-report-scheduler.timer",
                    "aw-dlp-syslog-forwarder.timer",
                    "aw-dlp-webhook-sender.timer",
                    "aw-dlp-cef-exporter.timer"
                ]
            }),
        );
    }
    for unit in ["aw-worktime-ui-bridge.timer"] {
        check_systemd_unit(&mut report, unit, "timer");
    }

    match http_json(client, &format!("{aw_api_base}/buckets"), 15, 2) {
        Ok(Value::Object(map)) => {
            let buckets: BTreeMap<String, Value> = map.into_iter().collect();
            report.add(
                "aw:buckets-index",
                "ok",
                "bucket index loaded",
                json!({"total": buckets.len()}),
            );
            check_endpoint_signal_buckets(
                &mut report,
                client,
                &aw_api_base,
                &buckets,
                cli.max_age_seconds,
            );
            check_file_operations_buckets(
                &mut report,
                client,
                &aw_api_base,
                &buckets,
                cli.max_age_seconds,
                cli.strict_fileops,
            );
            check_file_operations_runtime(
                &mut report,
                client,
                &aw_api_base,
                &buckets,
                RuntimeThresholds {
                    sample_limit: cli.fileops_sample_limit,
                    queue_warn_depth: cli.fileops_queue_warn_depth,
                    send_failure_warn_count: cli.fileops_send_failure_warn_count,
                },
                Some(&mut counter_state),
            );
            check_incident_buckets(
                &mut report,
                client,
                &aw_api_base,
                &buckets,
                cli.max_age_seconds * 24,
            );
            check_incident_runtime(
                &mut report,
                client,
                &aw_api_base,
                &buckets,
                cli.incident_sample_limit,
            );
            check_endpoint_self_test_metrics(
                &mut report,
                client,
                &aw_api_base,
                &buckets,
                cli.endpoint_queue_warn_depth,
                cli.endpoint_send_failure_warn_count,
                Some(&mut counter_state),
            );
        }
        Ok(_) => report.add(
            "aw:buckets-index",
            "fail",
            "failed to inspect bucket index: bucket list is not a dict",
            json!({}),
        ),
        Err(err) => report.add(
            "aw:buckets-index",
            "fail",
            format!("failed to inspect bucket index: {err}"),
            json!({}),
        ),
    }

    if let Some(state_error) = save_counter_state(&counter_state_path, &counter_state) {
        report.add(
            "state:counters",
            "warn",
            format!("failed to save counter baseline: {state_error}"),
            json!({"path": counter_state_path}),
        );
    }
    let month = Utc::now().format("%Y-%m").to_string();
    let profiles: Vec<String> = cli
        .profiles
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect();
    check_compliance_reports(&mut report, &cli.report_dir, &profiles, &month);
    report
}

fn write_atomic(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let mut tmp = tempfile::NamedTempFile::new_in(path.parent().unwrap_or_else(|| Path::new(".")))?;
    tmp.write_all(content.as_bytes())?;
    tmp.flush()?;
    tmp.persist(path)
        .map(|_| ())
        .map_err(|err| anyhow!(err.error))
        .with_context(|| format!("persist {}", path.display()))
}

fn sort_json_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let sorted: serde_json::Map<String, Value> = map
                .iter()
                .map(|(key, value)| (key.clone(), sort_json_value(value)))
                .collect();
            Value::Object(sorted)
        }
        Value::Array(items) => Value::Array(items.iter().map(sort_json_value).collect()),
        other => other.clone(),
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

fn env_string(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

fn env_path(name: &str) -> Option<PathBuf> {
    env_string(name).map(PathBuf::from)
}

fn env_i64(name: &str, default: i64) -> i64 {
    env_string(name)
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    env_string(name)
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default)
}

fn env_bool(name: &str) -> bool {
    env_string(name)
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn env_bool_default(name: &str, default: bool) -> bool {
    env_string(name)
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(default)
}

fn empty_string_as_null(value: &str) -> Value {
    if value.trim().is_empty() {
        Value::Null
    } else {
        json!(value)
    }
}

fn start_overall_timeout_watchdog(seconds: u64) {
    if seconds == 0 {
        return;
    }
    std::thread::spawn(move || {
        sleep(Duration::from_secs(seconds));
        eprintln!("dlp-health-check timed out after {seconds} seconds");
        std::process::exit(124);
    });
}

fn main() -> Result<()> {
    let cli = Cli::parse().apply_env();
    start_overall_timeout_watchdog(cli.overall_timeout_seconds);
    let client = Client::builder()
        .no_proxy()
        .build()
        .context("build HTTP client")?;
    let report = build_report(&cli, &client);
    let payload = report.payload();
    if cli.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("{}", report.render_text());
    }
    std::process::exit(if payload.ok { 0 } else { 1 });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_tail_keeps_last_two_parts() {
        assert_eq!(
            path_tail(Some(&json!("C:\\Users\\USER1\\Downloads\\report.zip")), 2),
            "Downloads/report.zip"
        );
    }

    #[test]
    fn counter_delta_uses_baseline() {
        let mut state = json!({"counters": {}});
        assert_eq!(counter_delta(Some(&mut state), "k", 12), (None, 0));
        assert_eq!(counter_delta(Some(&mut state), "k", 12), (Some(12), 0));
        assert_eq!(counter_delta(Some(&mut state), "k", 13), (Some(12), 1));
        assert_eq!(counter_delta(Some(&mut state), "k", 1), (Some(13), 0));
    }

    #[test]
    fn transport_counter_key_separates_sessions() {
        let data = json!({
            "hostname": "HOST-EXAMPLE",
            "sessionId": 4,
            "username": "USER4"
        });
        assert_eq!(
            transport_counter_key(
                "file-operations",
                "aw-file-operations_HOST-EXAMPLE",
                &data,
                "sendFailures"
            ),
            "file-operations:aw-file-operations_HOST-EXAMPLE:HOST-EXAMPLE:4:USER4:sendFailures"
        );
    }

    #[test]
    fn text_excerpt_truncates_like_python() {
        let text = json!("one   two   three");
        assert_eq!(text_excerpt(Some(&text), 20), "one two three");
        let long = json!("abcdef");
        assert_eq!(text_excerpt(Some(&long), 4), "abc…");
    }

    #[test]
    fn report_ok_ignores_warnings() {
        let mut report = HealthReport::default();
        report.add("a", "ok", "ok", json!({}));
        report.add("b", "warn", "warn", json!({}));
        let payload = report.payload();
        assert!(payload.ok);
        assert_eq!(payload.counts.ok, 1);
        assert_eq!(payload.counts.warn, 1);
        assert_eq!(payload.counts.fail, 0);
    }

    #[test]
    fn dlp_profile_normalization_matches_runtime_control_names() {
        assert_eq!(normalized_profile("disabled"), "core_only");
        assert_eq!(normalized_profile("core-only"), "core_only");
        assert_eq!(normalized_profile("light"), "light");
        assert_eq!(normalized_profile("lite"), "light");
        assert_eq!(normalized_profile("on-demand"), "on_demand");
        assert_eq!(normalized_profile("enabled"), "full");
    }

    #[test]
    fn light_profile_does_not_require_heavy_services() {
        assert!(!profile_checks_heavy_services("light"));
        assert!(!profile_checks_heavy_services("core_only"));
        assert!(profile_checks_heavy_services("on_demand"));
        assert!(profile_checks_heavy_services("full"));
    }
}
