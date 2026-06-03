use std::{thread, time::Duration};

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, TimeDelta, Utc};
use clap::Parser;
use detmir_core::runtime_guard::ensure_influx_runtime_config;
use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::Value;

const DEFAULT_AW_API_BASE: &str = "http://127.0.0.1:5600/api/0";
const DEFAULT_CASE_API_BASE: &str = "http://127.0.0.1:5602/api/0/dlp/cases";
const DEFAULT_INFLUX_ORG: &str = "proxmox";
const DEFAULT_INFLUX_BUCKET: &str = "aw_metrics";
const DEFAULT_HOSTS: &str = "HOST-EXAMPLE";

#[derive(Debug, Parser)]
#[command(about = "AW DLP InfluxDB exporter")]
struct Cli {
    #[arg(long)]
    dry_run: bool,

    #[arg(long)]
    json: bool,

    #[arg(long, default_value_t = 30)]
    timeout_seconds: u64,
}

#[derive(Debug, Clone)]
struct Config {
    aw_api_base: String,
    case_api_base: String,
    influx_url: String,
    influx_org: String,
    influx_bucket: String,
    influx_token: String,
    influx_enabled: bool,
    hosts: Vec<String>,
    lookback_days: i64,
    event_limit: usize,
    case_limit: usize,
}

#[derive(Debug, Serialize)]
struct RunSummary {
    ok: bool,
    enabled: bool,
    dry_run: bool,
    hosts: Vec<String>,
    lookback_days: i64,
    lines: usize,
    written: usize,
    bucket: String,
    error: Option<String>,
}

#[derive(Debug, Clone)]
enum FieldValue {
    Bool(bool),
    Int(i64),
    String(String),
}

fn env(name: &str, fallback: &str) -> String {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

fn env_bool(name: &str, fallback: bool) -> bool {
    match std::env::var(name)
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
    {
        Some(value) if !value.is_empty() => matches!(value.as_str(), "1" | "true" | "yes" | "on"),
        _ => fallback,
    }
}

fn env_int(name: &str, fallback: i64) -> i64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(fallback)
}

fn env_usize(name: &str, fallback: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(fallback)
}

fn load_config() -> Config {
    Config {
        aw_api_base: env("AW_DLP_AW_API_BASE", DEFAULT_AW_API_BASE)
            .trim_end_matches('/')
            .to_string(),
        case_api_base: env("AW_DLP_CASE_API_BASE", DEFAULT_CASE_API_BASE)
            .trim_end_matches('/')
            .to_string(),
        influx_url: env("AW_DLP_INFLUX_URL", "")
            .trim_end_matches('/')
            .to_string(),
        influx_org: env("AW_DLP_INFLUX_ORG", DEFAULT_INFLUX_ORG),
        influx_bucket: env("AW_DLP_INFLUX_BUCKET", DEFAULT_INFLUX_BUCKET),
        influx_token: env("AW_DLP_INFLUX_TOKEN", ""),
        influx_enabled: env_bool("AW_DLP_INFLUX_ENABLED", false),
        hosts: env("AW_DLP_INFLUX_HOSTS", DEFAULT_HOSTS)
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(ToOwned::to_owned)
            .collect(),
        lookback_days: env_int("AW_DLP_INFLUX_LOOKBACK_DAYS", 30),
        event_limit: env_usize("AW_DLP_INFLUX_EVENT_LIMIT", 2000),
        case_limit: env_usize("AW_DLP_CASE_LIMIT", 500),
    }
}

fn validate_runtime_config(config: &Config) -> Result<()> {
    if !config.influx_enabled {
        return Ok(());
    }
    ensure_influx_runtime_config(
        "AW_DLP_INFLUX",
        &config.influx_url,
        &config.influx_org,
        &config.influx_bucket,
        &config.influx_token,
        &config.hosts,
    )
}

fn utc_now() -> DateTime<Utc> {
    Utc::now()
}

fn pts(value: Option<&str>) -> DateTime<Utc> {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return utc_now();
    };
    let normalized = value.replace('Z', "+00:00");
    DateTime::parse_from_rfc3339(&normalized)
        .map(|parsed| parsed.with_timezone(&Utc))
        .unwrap_or_else(|_| utc_now())
}

fn escape_tag(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace(' ', "\\ ")
        .replace(',', "\\,")
        .replace('=', "\\=")
}

fn timestamp_ns(dt: DateTime<Utc>) -> i64 {
    dt.timestamp_nanos_opt()
        .unwrap_or_else(|| dt.timestamp() * 1_000_000_000)
}

fn line(
    measurement: &str,
    tags: Vec<(&str, String)>,
    fields: Vec<(&str, FieldValue)>,
    timestamp_ns: i64,
) -> Option<String> {
    let mut tag_items: Vec<(&str, String)> = tags
        .into_iter()
        .filter(|(_key, value)| !value.is_empty())
        .collect();
    tag_items.sort_by(|left, right| left.0.cmp(right.0));
    let tag_part = tag_items
        .iter()
        .map(|(key, value)| format!("{key}={}", escape_tag(value)))
        .collect::<Vec<_>>()
        .join(",");

    let field_parts = fields
        .into_iter()
        .map(|(key, value)| match value {
            FieldValue::Bool(value) => format!("{key}={}", if value { "true" } else { "false" }),
            FieldValue::Int(value) => format!("{key}={value}i"),
            FieldValue::String(value) => {
                let text = value.replace('\\', "\\\\").replace('"', "\\\"");
                format!("{key}=\"{text}\"")
            }
        })
        .collect::<Vec<_>>();
    if field_parts.is_empty() {
        return None;
    }
    if tag_part.is_empty() {
        Some(format!(
            "{measurement} {} {timestamp_ns}",
            field_parts.join(",")
        ))
    } else {
        Some(format!(
            "{measurement},{tag_part} {} {timestamp_ns}",
            field_parts.join(",")
        ))
    }
}

fn get_json(client: &Client, url: &str) -> Result<Value> {
    let mut last_error = None;
    for attempt in 1..=6 {
        match client
            .get(url)
            .header("Accept", "application/json")
            .send()
            .and_then(|resp| resp.error_for_status())
            .and_then(|resp| resp.json())
        {
            Ok(payload) => return Ok(payload),
            Err(err) => {
                last_error = Some(err);
                if attempt < 6 {
                    thread::sleep(Duration::from_millis(250 * attempt));
                }
            }
        }
    }
    Err(anyhow!(
        "GET {url}: {}",
        last_error
            .map(|err| err.to_string())
            .unwrap_or_else(|| "unknown error".to_string())
    ))
}

fn format_aw_time(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn fetch_bucket_events(
    client: &Client,
    config: &Config,
    bucket_id: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    limit: usize,
) -> Result<Vec<Value>> {
    let query = format!(
        "start={}&end={}&limit={}",
        urlencoding::encode(&format_aw_time(start)),
        urlencoding::encode(&format_aw_time(end)),
        limit
    );
    let url = format!(
        "{}/buckets/{}/events?{}",
        config.aw_api_base,
        urlencoding::encode(bucket_id),
        query
    );
    let payload = get_json(client, &url)?;
    Ok(payload.as_array().cloned().unwrap_or_default())
}

fn fetch_cases(client: &Client, config: &Config, host: &str) -> Result<Vec<Value>> {
    let url = format!(
        "{}?host={}&limit={}",
        config.case_api_base,
        urlencoding::encode(host),
        config.case_limit
    );
    let payload = get_json(client, &url)?;
    Ok(payload.as_array().cloned().unwrap_or_default())
}

fn data_object(item: &Value) -> &serde_json::Map<String, Value> {
    match item.get("data").and_then(Value::as_object) {
        Some(data) => data,
        None => empty_object(),
    }
}

fn empty_object() -> &'static serde_json::Map<String, Value> {
    static EMPTY: std::sync::OnceLock<serde_json::Map<String, Value>> = std::sync::OnceLock::new();
    EMPTY.get_or_init(serde_json::Map::new)
}

fn s(value: Option<&Value>) -> String {
    match value {
        Some(Value::Null) | None => String::new(),
        Some(Value::String(value)) => value.trim().to_string(),
        Some(Value::Bool(value)) => value.to_string(),
        Some(Value::Number(value)) => value.to_string(),
        Some(other) => other.to_string(),
    }
}

fn first_nonempty(values: &[Option<&Value>], default: &str) -> String {
    values
        .iter()
        .map(|value| s(*value))
        .find(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn int_value(value: Option<&Value>) -> i64 {
    match value {
        Some(Value::Number(value)) => value
            .as_i64()
            .or_else(|| value.as_u64().map(|v| v as i64))
            .unwrap_or(0),
        Some(Value::String(value)) => value.parse().unwrap_or(0),
        Some(Value::Bool(value)) => i64::from(*value),
        _ => 0,
    }
}

fn bool_value(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(value)) => *value,
        Some(Value::Number(value)) => value.as_i64().unwrap_or(0) != 0,
        Some(Value::String(value)) => matches!(
            value.to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        _ => false,
    }
}

fn event_id(item: &Value, fallback: String) -> String {
    first_nonempty(&[item.get("id")], &fallback)
}

#[derive(Debug)]
struct NormalizedIncident {
    host: String,
    signal_type: String,
    username: String,
    severity: String,
    action: String,
    message: String,
    rule_id: String,
    source: String,
    document_name: String,
    printer_name: String,
    incident_status: String,
    incident_verdict: String,
    regex_matches: usize,
    dictionary_matches: usize,
    ocr_requested: bool,
}

fn normalize_incident(event: &Value, default_host: &str) -> NormalizedIncident {
    let empty = empty_object();
    let data = event
        .get("data")
        .and_then(Value::as_object)
        .unwrap_or(empty);
    let source_event = data
        .get("sourceEvent")
        .and_then(Value::as_object)
        .unwrap_or(empty);
    let source_data = source_event
        .get("data")
        .and_then(Value::as_object)
        .unwrap_or(empty);
    let nested = data
        .get("incident")
        .and_then(Value::as_object)
        .unwrap_or(empty);

    NormalizedIncident {
        host: first_nonempty(
            &[
                data.get("hostname"),
                data.get("host"),
                source_data.get("hostname"),
            ],
            default_host,
        ),
        signal_type: first_nonempty(
            &[data.get("signalType"), source_data.get("signalType")],
            "unknown",
        ),
        username: first_nonempty(
            &[
                data.get("username"),
                source_data.get("username"),
                source_data.get("owner"),
                source_data.get("host"),
            ],
            "unknown",
        ),
        severity: first_nonempty(&[data.get("severity"), nested.get("severity")], "unknown"),
        action: first_nonempty(&[data.get("action"), nested.get("verdict")], "incident"),
        message: first_nonempty(
            &[
                data.get("message"),
                source_data.get("documentName"),
                source_data.get("documentNameOriginal"),
                nested.get("comment"),
            ],
            "",
        ),
        rule_id: first_nonempty(&[data.get("ruleId")], ""),
        source: first_nonempty(
            &[
                data.get("source"),
                source_data.get("source"),
                data.get("sourceBucket"),
            ],
            "",
        ),
        document_name: first_nonempty(
            &[
                source_data.get("documentName"),
                source_data.get("documentNameOriginal"),
            ],
            "",
        ),
        printer_name: first_nonempty(&[source_data.get("printerName")], ""),
        incident_status: first_nonempty(&[nested.get("status")], ""),
        incident_verdict: first_nonempty(&[nested.get("verdict")], ""),
        regex_matches: data
            .get("regexMatches")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0),
        dictionary_matches: data
            .get("dictionaryMatches")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0),
        ocr_requested: bool_value(data.get("ocrRequested")),
    }
}

fn build_endpoint_lines(host: &str, events: &[Value]) -> Vec<String> {
    let mut lines = Vec::new();
    for item in events {
        let data = data_object(item);
        let signal_type = first_nonempty(&[data.get("signalType")], "unknown");
        let ts = timestamp_ns(pts(item.get("timestamp").and_then(Value::as_str)));
        let id = event_id(item, format!("{signal_type}-{ts}"));
        let username = first_nonempty(&[data.get("username"), data.get("owner")], "unknown");
        if signal_type == "self_test" {
            if let Some(line) = line(
                "aw_dlp_endpoint_self_test",
                vec![
                    ("host", first_nonempty(&[data.get("hostname")], host)),
                    ("event_id", id),
                    ("username", username),
                    (
                        "policy_mode",
                        first_nonempty(&[data.get("policyMode")], "unknown"),
                    ),
                    (
                        "policy_source",
                        first_nonempty(&[data.get("policySource")], "unknown"),
                    ),
                ],
                vec![
                    ("count", FieldValue::Int(1)),
                    (
                        "queue_depth",
                        FieldValue::Int(int_value(data.get("queueDepth"))),
                    ),
                    (
                        "events_enqueued",
                        FieldValue::Int(int_value(data.get("eventsEnqueued"))),
                    ),
                    (
                        "events_flushed",
                        FieldValue::Int(int_value(data.get("eventsFlushed"))),
                    ),
                    (
                        "send_failures",
                        FieldValue::Int(int_value(data.get("sendFailures"))),
                    ),
                    (
                        "policy_enabled",
                        FieldValue::Bool(bool_value(data.get("policyEnabled"))),
                    ),
                ],
                ts,
            ) {
                lines.push(line);
            }
            continue;
        }
        if let Some(line) = line(
            "aw_dlp_signal",
            vec![
                ("host", first_nonempty(&[data.get("hostname")], host)),
                ("event_id", id),
                ("signal_type", signal_type),
                ("username", username),
                ("source", first_nonempty(&[data.get("source")], "unknown")),
            ],
            vec![
                ("count", FieldValue::Int(1)),
                (
                    "document_name",
                    FieldValue::String(first_nonempty(
                        &[data.get("documentName"), data.get("documentNameOriginal")],
                        "",
                    )),
                ),
                (
                    "printer_name",
                    FieldValue::String(first_nonempty(&[data.get("printerName")], "")),
                ),
                (
                    "owner",
                    FieldValue::String(first_nonempty(&[data.get("owner")], "")),
                ),
                (
                    "session_id",
                    FieldValue::Int(int_value(data.get("sessionId"))),
                ),
            ],
            ts,
        ) {
            lines.push(line);
        }
    }
    lines
}

fn build_incident_lines(host: &str, events: &[Value]) -> Vec<String> {
    let mut lines = Vec::new();
    for item in events {
        let normalized = normalize_incident(item, host);
        let ts = timestamp_ns(pts(item.get("timestamp").and_then(Value::as_str)));
        let id = event_id(item, format!("incident-{ts}"));
        if let Some(line) = line(
            "aw_dlp_incident",
            vec![
                ("host", normalized.host),
                ("event_id", id),
                ("signal_type", normalized.signal_type),
                ("severity", normalized.severity),
                ("action", normalized.action),
                ("username", normalized.username),
                ("source", normalized.source),
            ],
            vec![
                ("count", FieldValue::Int(1)),
                ("message", FieldValue::String(normalized.message)),
                ("rule_id", FieldValue::String(normalized.rule_id)),
                (
                    "document_name",
                    FieldValue::String(normalized.document_name),
                ),
                ("printer_name", FieldValue::String(normalized.printer_name)),
                (
                    "incident_status",
                    FieldValue::String(normalized.incident_status),
                ),
                (
                    "incident_verdict",
                    FieldValue::String(normalized.incident_verdict),
                ),
                (
                    "regex_matches",
                    FieldValue::Int(normalized.regex_matches as i64),
                ),
                (
                    "dictionary_matches",
                    FieldValue::Int(normalized.dictionary_matches as i64),
                ),
                ("ocr_requested", FieldValue::Bool(normalized.ocr_requested)),
            ],
            ts,
        ) {
            lines.push(line);
        }
    }
    lines
}

fn build_review_lines(host: &str, events: &[Value]) -> Vec<String> {
    let mut lines = Vec::new();
    for item in events {
        let empty = empty_object();
        let data = item.get("data").and_then(Value::as_object).unwrap_or(empty);
        let review = data
            .get("review")
            .and_then(Value::as_object)
            .unwrap_or(empty);
        let source_data = data
            .get("sourceEvent")
            .and_then(Value::as_object)
            .and_then(|value| value.get("data"))
            .and_then(Value::as_object)
            .unwrap_or(empty);
        let ts = timestamp_ns(pts(item.get("timestamp").and_then(Value::as_str)));
        let review_id = first_nonempty(&[review.get("reviewId")], &format!("review-{ts}"));
        if let Some(line) = line(
            "aw_dlp_review",
            vec![
                (
                    "host",
                    first_nonempty(&[data.get("host"), source_data.get("hostname")], host),
                ),
                ("review_id", review_id),
                (
                    "verdict",
                    first_nonempty(&[review.get("verdict")], "unknown"),
                ),
                (
                    "signal_type",
                    first_nonempty(&[source_data.get("signalType")], "unknown"),
                ),
                (
                    "username",
                    first_nonempty(
                        &[source_data.get("username"), source_data.get("owner")],
                        "unknown",
                    ),
                ),
            ],
            vec![
                ("count", FieldValue::Int(1)),
                (
                    "archived",
                    FieldValue::Bool(bool_value(review.get("archived"))),
                ),
                (
                    "comment",
                    FieldValue::String(first_nonempty(&[review.get("comment")], "")),
                ),
                (
                    "category",
                    FieldValue::String(first_nonempty(&[review.get("category")], "")),
                ),
                (
                    "document_name",
                    FieldValue::String(first_nonempty(
                        &[
                            source_data.get("documentName"),
                            source_data.get("documentNameOriginal"),
                        ],
                        "",
                    )),
                ),
                (
                    "printer_name",
                    FieldValue::String(first_nonempty(&[source_data.get("printerName")], "")),
                ),
            ],
            ts,
        ) {
            lines.push(line);
        }
    }
    lines
}

fn build_rule_lines(host: &str, events: &[Value]) -> Vec<String> {
    let mut lines = Vec::new();
    for item in events {
        let empty = empty_object();
        let data = item.get("data").and_then(Value::as_object).unwrap_or(empty);
        let match_data = data
            .get("match")
            .and_then(Value::as_object)
            .unwrap_or(empty);
        let ts = timestamp_ns(pts(item.get("timestamp").and_then(Value::as_str)));
        let rule_id = first_nonempty(&[data.get("ruleId")], &format!("rule-{ts}"));
        let enabled = data
            .get("enabled")
            .is_none_or(|value| bool_value(Some(value)));
        if let Some(line) = line(
            "aw_dlp_rule",
            vec![
                (
                    "host",
                    first_nonempty(&[data.get("host"), match_data.get("hostname")], host),
                ),
                ("rule_id", rule_id),
                ("action", first_nonempty(&[data.get("action")], "unknown")),
                (
                    "signal_type",
                    first_nonempty(&[match_data.get("signalType")], "unknown"),
                ),
                (
                    "username",
                    first_nonempty(
                        &[match_data.get("username"), match_data.get("owner")],
                        "unknown",
                    ),
                ),
                (
                    "enabled",
                    if enabled {
                        "true".to_string()
                    } else {
                        "false".to_string()
                    },
                ),
            ],
            vec![
                ("count", FieldValue::Int(1)),
                (
                    "category",
                    FieldValue::String(first_nonempty(&[data.get("category")], "")),
                ),
                (
                    "comment",
                    FieldValue::String(first_nonempty(&[data.get("comment")], "")),
                ),
                (
                    "document_name",
                    FieldValue::String(first_nonempty(&[match_data.get("documentName")], "")),
                ),
                (
                    "printer_name",
                    FieldValue::String(first_nonempty(&[match_data.get("printerName")], "")),
                ),
            ],
            ts,
        ) {
            lines.push(line);
        }
    }
    lines
}

fn build_fileops_lines(host: &str, events: &[Value]) -> Vec<String> {
    let mut lines = Vec::new();
    for item in events {
        let data = data_object(item);
        let signal_type = first_nonempty(&[data.get("signalType")], "unknown");
        if signal_type != "collector_health" {
            continue;
        }
        let ts = timestamp_ns(pts(item.get("timestamp").and_then(Value::as_str)));
        let id = event_id(item, format!("fileops-{ts}"));
        if let Some(line) = line(
            "aw_dlp_fileops_health",
            vec![
                ("host", first_nonempty(&[data.get("hostname")], host)),
                ("event_id", id),
                (
                    "username",
                    first_nonempty(&[data.get("username")], "unknown"),
                ),
            ],
            vec![
                ("count", FieldValue::Int(1)),
                (
                    "queue_depth",
                    FieldValue::Int(int_value(data.get("queueDepth"))),
                ),
                (
                    "events_enqueued",
                    FieldValue::Int(int_value(data.get("eventsEnqueued"))),
                ),
                (
                    "events_flushed",
                    FieldValue::Int(int_value(data.get("eventsFlushed"))),
                ),
                (
                    "send_failures",
                    FieldValue::Int(int_value(data.get("sendFailures"))),
                ),
                (
                    "session_id",
                    FieldValue::Int(int_value(data.get("sessionId"))),
                ),
            ],
            ts,
        ) {
            lines.push(line);
        }
    }
    lines
}

fn build_case_lines(host: &str, cases: &[Value]) -> Vec<String> {
    let mut lines = Vec::new();
    for item in cases {
        let empty = empty_object();
        let object = item.as_object().unwrap_or(empty);
        let evidence = object
            .get("evidence")
            .and_then(Value::as_object)
            .unwrap_or(empty);
        let evidence_items = evidence
            .get("items")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0);
        let ts = timestamp_ns(pts(object
            .get("updated_at")
            .or_else(|| object.get("created_at"))
            .and_then(Value::as_str)));
        if let Some(line) = line(
            "aw_dlp_case",
            vec![
                ("host", first_nonempty(&[object.get("host")], host)),
                ("case_id", first_nonempty(&[object.get("id")], "")),
                ("status", first_nonempty(&[object.get("status")], "unknown")),
                (
                    "severity",
                    first_nonempty(&[object.get("severity")], "unknown"),
                ),
                (
                    "assignee",
                    first_nonempty(&[object.get("assignee")], "unassigned"),
                ),
            ],
            vec![
                ("count", FieldValue::Int(1)),
                (
                    "title",
                    FieldValue::String(first_nonempty(&[object.get("title")], "")),
                ),
                (
                    "incident_id",
                    FieldValue::String(first_nonempty(&[object.get("incident_id")], "")),
                ),
                (
                    "has_forensics",
                    FieldValue::Bool(!matches!(object.get("forensics"), None | Some(Value::Null))),
                ),
                ("evidence_items", FieldValue::Int(evidence_items as i64)),
                (
                    "chain_length",
                    FieldValue::Int(int_value(evidence.get("chain_length"))),
                ),
            ],
            ts,
        ) {
            lines.push(line);
        }
    }
    lines
}

fn build_lines_for_host(
    client: &Client,
    config: &Config,
    host: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<String>> {
    let mut lines = Vec::new();
    lines.extend(build_endpoint_lines(
        host,
        &fetch_bucket_events(
            client,
            config,
            &format!("aw-dlp-endpoint-signals_{host}"),
            start,
            end,
            config.event_limit,
        )?,
    ));
    lines.extend(build_incident_lines(
        host,
        &fetch_bucket_events(
            client,
            config,
            &format!("aw-dlp-incidents_{host}"),
            start,
            end,
            config.event_limit,
        )?,
    ));
    lines.extend(build_review_lines(
        host,
        &fetch_bucket_events(
            client,
            config,
            &format!("aw-dlp-review_{host}"),
            start,
            end,
            config.event_limit,
        )?,
    ));
    lines.extend(build_rule_lines(
        host,
        &fetch_bucket_events(
            client,
            config,
            &format!("aw-dlp-rules_{host}"),
            start,
            end,
            config.event_limit,
        )?,
    ));
    lines.extend(build_fileops_lines(
        host,
        &fetch_bucket_events(
            client,
            config,
            &format!("aw-file-operations_{host}"),
            start,
            end,
            config.event_limit,
        )?,
    ));
    lines.extend(build_case_lines(host, &fetch_cases(client, config, host)?));
    Ok(lines)
}

fn write_lines(client: &Client, config: &Config, lines: &[String]) -> Result<usize> {
    if lines.is_empty() {
        return Ok(0);
    }
    if config.influx_url.is_empty() || config.influx_token.is_empty() {
        bail!("InfluxDB destination is not configured");
    }
    let url = format!(
        "{}/api/v2/write?org={}&bucket={}&precision=ns",
        config.influx_url,
        urlencoding::encode(&config.influx_org),
        urlencoding::encode(&config.influx_bucket)
    );
    let payload = format!("{}\n", lines.join("\n"));
    client
        .post(url)
        .header("Authorization", format!("Token {}", config.influx_token))
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(payload)
        .send()
        .and_then(|resp| resp.error_for_status())
        .context("InfluxDB write failed")?;
    Ok(lines.len())
}

fn run(cli: &Cli) -> Result<RunSummary> {
    let config = load_config();
    if !config.influx_enabled && !cli.dry_run {
        return Ok(RunSummary {
            ok: true,
            enabled: false,
            dry_run: false,
            hosts: config.hosts,
            lookback_days: config.lookback_days,
            lines: 0,
            written: 0,
            bucket: config.influx_bucket,
            error: None,
        });
    }
    validate_runtime_config(&config)?;
    let client = Client::builder()
        .timeout(Duration::from_secs(cli.timeout_seconds))
        .no_proxy()
        .build()
        .context("build HTTP client")?;
    let end = utc_now();
    let start = end - TimeDelta::days(config.lookback_days);
    let mut lines = Vec::new();
    for host in &config.hosts {
        lines.extend(build_lines_for_host(&client, &config, host, start, end)?);
    }
    let written = if cli.dry_run {
        0
    } else {
        write_lines(&client, &config, &lines)?
    };
    Ok(RunSummary {
        ok: true,
        enabled: config.influx_enabled,
        dry_run: cli.dry_run,
        hosts: config.hosts,
        lookback_days: config.lookback_days,
        lines: lines.len(),
        written,
        bucket: config.influx_bucket,
        error: None,
    })
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(summary) => {
            if cli.json || cli.dry_run {
                println!("{}", serde_json::to_string_pretty(&summary)?);
            } else if !summary.enabled {
                eprintln!("[aw-dlp-influx-exporter] disabled by AW_DLP_INFLUX_ENABLED");
            } else {
                eprintln!(
                    "[aw-dlp-influx-exporter] wrote {} points to {}",
                    summary.written, summary.bucket
                );
            }
            Ok(())
        }
        Err(err) => {
            if cli.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&RunSummary {
                        ok: false,
                        enabled: false,
                        dry_run: cli.dry_run,
                        hosts: Vec::new(),
                        lookback_days: 0,
                        lines: 0,
                        written: 0,
                        bucket: String::new(),
                        error: Some(err.to_string()),
                    })?
                );
            }
            Err(err)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_config() -> Config {
        Config {
            aw_api_base: DEFAULT_AW_API_BASE.to_string(),
            case_api_base: DEFAULT_CASE_API_BASE.to_string(),
            influx_url: String::new(),
            influx_org: DEFAULT_INFLUX_ORG.to_string(),
            influx_bucket: DEFAULT_INFLUX_BUCKET.to_string(),
            influx_token: String::new(),
            influx_enabled: false,
            hosts: vec![DEFAULT_HOSTS.to_string()],
            lookback_days: 30,
            event_limit: 2000,
            case_limit: 500,
        }
    }

    #[test]
    fn runtime_validation_rejects_placeholder_influx_destination() {
        let mut config = test_config();
        config.influx_enabled = true;
        config.influx_url = "http://192.0.2.10:8086".to_string();
        config.hosts = vec!["HOST-EXAMPLE".to_string()];

        let err = validate_runtime_config(&config).unwrap_err().to_string();
        assert!(err.contains("AW_DLP_INFLUX_URL"));

        config.influx_url = "http://influxdb.internal:8086".to_string();
        config.influx_token = "prod-write-token-value".to_string();
        let err = validate_runtime_config(&config).unwrap_err().to_string();
        assert!(err.contains("AW_DLP_INFLUX_HOSTS"));

        config.hosts = vec!["WINDOWS-HOST".to_string()];
        config.influx_bucket = "BUCKET-EXAMPLE".to_string();
        let err = validate_runtime_config(&config).unwrap_err().to_string();
        assert!(err.contains("AW_DLP_INFLUX_BUCKET"));

        config.influx_bucket = DEFAULT_INFLUX_BUCKET.to_string();
        config.influx_token = "CHANGE_ME".to_string();
        let err = validate_runtime_config(&config).unwrap_err().to_string();
        assert!(err.contains("AW_DLP_INFLUX_TOKEN"));
    }

    #[test]
    fn endpoint_lines_emit_self_test_and_signal() {
        let events = vec![
            json!({
                "id": 10,
                "timestamp": "2026-05-15T10:00:00Z",
                "data": {
                    "hostname": "HOST-EXAMPLE",
                    "username": "Администратор",
                    "signalType": "self_test",
                    "policyMode": "server",
                    "policySource": "local-fallback",
                    "queueDepth": 2,
                    "eventsEnqueued": 100,
                    "eventsFlushed": 99,
                    "sendFailures": 1,
                    "policyEnabled": true
                }
            }),
            json!({
                "id": 11,
                "timestamp": "2026-05-15T10:01:00Z",
                "data": {
                    "hostname": "HOST-EXAMPLE",
                    "username": "Администратор",
                    "signalType": "print_job",
                    "source": "endpoint-signals-phase2",
                    "documentName": "Документ.docx",
                    "printerName": "HP LaserJet"
                }
            }),
        ];
        let lines = build_endpoint_lines("HOST-EXAMPLE", &events);
        assert!(
            lines
                .iter()
                .any(|line| line.starts_with("aw_dlp_endpoint_self_test,"))
        );
        assert!(lines.iter().any(|line| line.starts_with("aw_dlp_signal,")));
    }

    #[test]
    fn normalize_incident_handles_nested_source_event() {
        let item = json!({
            "timestamp": "2026-05-15T10:02:00Z",
            "data": {
                "host": "HOST-EXAMPLE",
                "incident": {"status": "open", "verdict": "incident"},
                "sourceBucket": "aw-dlp-endpoint-signals_HOST-EXAMPLE",
                "sourceEvent": {
                    "data": {
                        "signalType": "print_job",
                        "hostname": "HOST-EXAMPLE",
                        "username": "Администратор",
                        "documentName": "Письмо",
                        "printerName": "HP",
                        "source": "endpoint-signals-phase2"
                    }
                }
            }
        });
        let normalized = normalize_incident(&item, "HOST-EXAMPLE");
        assert_eq!(normalized.signal_type, "print_job");
        assert_eq!(normalized.username, "Администратор");
        assert_eq!(normalized.action, "incident");
        assert_eq!(normalized.incident_status, "open");
    }

    #[test]
    fn case_lines_emit_case_state() {
        let cases = vec![json!({
            "id": 28,
            "host": "HOST-EXAMPLE",
            "status": "open",
            "severity": "medium",
            "assignee": null,
            "title": "DLP print_job · Администратор",
            "incident_id": "case-1",
            "evidence": {"items": [1], "chain_length": 1},
            "forensics": null,
            "updated_at": "2026-05-15T10:03:00+00:00"
        })];
        let lines = build_case_lines("HOST-EXAMPLE", &cases);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("aw_dlp_case,"));
    }

    #[test]
    fn timestamp_parser_accepts_zulu() {
        assert_eq!(pts(Some("2026-05-15T10:00:00Z")).timestamp(), 1_778_839_200);
    }
}
