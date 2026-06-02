use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use chrono::{SecondsFormat, Utc};
use clap::Parser;
use reqwest::blocking::Client;
use reqwest::header::CONTENT_TYPE;
use serde::Serialize;
use serde_json::{Value, json};

const DEFAULT_CONFIG: &str = "/opt/activitywatch/dlp-integrations/webhook-config.yaml";

#[derive(Debug, Parser)]
#[command(about = "AWatch DLP webhook sender")]
struct Cli {
    #[arg(long, default_value = DEFAULT_CONFIG)]
    config: PathBuf,

    #[arg(long)]
    dry_run: bool,

    #[arg(long)]
    json: bool,
}

#[derive(Debug, Clone)]
struct HookConfig {
    url: String,
    severity: Vec<String>,
}

#[derive(Debug, Clone)]
struct Config {
    aw_api_base: String,
    state_path: PathBuf,
    retries: usize,
    timeout_sec: u64,
    backoff_base: f64,
    per_bucket_limit: usize,
    critical_webhooks: Vec<HookConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            aw_api_base: "http://127.0.0.1:5600/api/0".to_string(),
            state_path: PathBuf::from("/var/lib/activitywatch/dlp-integrations/webhook-state.json"),
            retries: 4,
            timeout_sec: 15,
            backoff_base: 2.0,
            per_bucket_limit: 300,
            critical_webhooks: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize)]
struct RunSummary {
    ok: bool,
    delivered: usize,
    incidents_seen: usize,
    buckets: usize,
    dry_run: bool,
    state_saved: bool,
    state_path: String,
    error: Option<String>,
}

fn load_config(path: &Path) -> Config {
    let mut config = Config::default();
    let Ok(text) = fs::read_to_string(path) else {
        return config;
    };
    let mut current_hook: Option<HookConfig> = None;
    for raw_line in text.lines() {
        let line_without_comment = raw_line.split('#').next().unwrap_or("");
        let line = line_without_comment.trim();
        if line.is_empty() || line == "critical_webhooks:" {
            continue;
        }
        if line.starts_with("- ") {
            if let Some(hook) = current_hook.take() {
                config.critical_webhooks.push(hook);
            }
            let mut hook = HookConfig {
                url: String::new(),
                severity: vec!["high".to_string()],
            };
            parse_hook_field(line.trim_start_matches("- ").trim(), &mut hook);
            current_hook = Some(hook);
            continue;
        }
        if raw_line.starts_with(' ') || raw_line.starts_with('\t') {
            if let Some(hook) = current_hook.as_mut() {
                parse_hook_field(line, hook);
            }
            continue;
        }
        if let Some(hook) = current_hook.take() {
            config.critical_webhooks.push(hook);
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = clean_scalar(value);
        match key {
            "aw_api_base" if !value.is_empty() => config.aw_api_base = value,
            "state_path" if !value.is_empty() => config.state_path = PathBuf::from(value),
            "retries" => config.retries = value.parse().unwrap_or(config.retries),
            "timeout_sec" => config.timeout_sec = value.parse().unwrap_or(config.timeout_sec),
            "backoff_base" => config.backoff_base = value.parse().unwrap_or(config.backoff_base),
            "per_bucket_limit" => {
                config.per_bucket_limit = value.parse().unwrap_or(config.per_bucket_limit);
            }
            _ => {}
        }
    }
    if let Some(hook) = current_hook {
        config.critical_webhooks.push(hook);
    }
    config
}

fn parse_hook_field(line: &str, hook: &mut HookConfig) {
    let Some((key, value)) = line.split_once(':') else {
        return;
    };
    let key = key.trim();
    let value = clean_scalar(value);
    match key {
        "url" => hook.url = value,
        "severity" => hook.severity = parse_list_or_scalar(&value),
        _ => {}
    }
}

fn clean_scalar(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string()
}

fn parse_list_or_scalar(value: &str) -> Vec<String> {
    let text = value.trim();
    if text.starts_with('[') && text.ends_with(']') {
        return text
            .trim_start_matches('[')
            .trim_end_matches(']')
            .split(',')
            .map(clean_scalar)
            .filter(|item| !item.is_empty())
            .collect();
    }
    if text.is_empty() {
        Vec::new()
    } else {
        vec![clean_scalar(text)]
    }
}

fn load_json(path: &Path) -> Value {
    let Ok(text) = fs::read_to_string(path) else {
        return json!({});
    };
    serde_json::from_str::<Value>(&text)
        .ok()
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}))
}

fn save_json(path: &Path, payload: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    fs::write(path, serde_json::to_string_pretty(payload)?)
        .with_context(|| format!("write {}", path.display()))
}

fn http_json(client: &Client, url: &str, timeout_sec: u64) -> Result<Value> {
    client
        .get(url)
        .timeout(Duration::from_secs(timeout_sec))
        .send()
        .and_then(|resp| resp.error_for_status())
        .with_context(|| format!("GET {url}"))?
        .json::<Value>()
        .with_context(|| format!("parse JSON from {url}"))
}

fn int_value(value: Option<&Value>) -> i64 {
    match value {
        Some(Value::Number(n)) => n
            .as_i64()
            .or_else(|| n.as_u64().map(|v| v as i64))
            .unwrap_or(0),
        Some(Value::String(s)) => s.parse::<i64>().unwrap_or(0),
        _ => 0,
    }
}

fn incident_bucket_ids(buckets: &Value) -> Vec<String> {
    let mut ids: Vec<String> = buckets
        .as_object()
        .map(|map| {
            map.keys()
                .filter(|bucket_id| bucket_id.starts_with("aw-dlp-incidents_"))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    ids.sort();
    ids
}

fn iter_new_incidents(
    client: &Client,
    aw_base: &str,
    state: &Value,
    per_bucket_limit: usize,
    timeout_sec: u64,
) -> Result<(Vec<Value>, BTreeMap<String, i64>)> {
    let base = aw_base.trim_end_matches('/');
    let buckets = http_json(client, &format!("{base}/buckets/"), timeout_sec)?;
    let bucket_ids = incident_bucket_ids(&buckets);
    let last_ids = state.get("last_ids").and_then(Value::as_object);
    let mut max_ids = BTreeMap::new();
    let mut out = Vec::new();
    for bucket_id in bucket_ids {
        let events = match http_json(
            client,
            &format!("{base}/buckets/{bucket_id}/events?limit={per_bucket_limit}"),
            timeout_sec,
        ) {
            Ok(Value::Array(items)) => items,
            Ok(_) => continue,
            Err(err) => return Err(err),
        };
        let prev = last_ids
            .and_then(|ids| ids.get(&bucket_id))
            .and_then(|v| match v {
                Value::Number(n) => n.as_i64(),
                Value::String(s) => s.parse().ok(),
                _ => None,
            })
            .unwrap_or(0);
        let mut bucket_max = prev;
        for event in events {
            let event_id = int_value(event.get("id"));
            if event_id <= prev {
                continue;
            }
            if event_id > bucket_max {
                bucket_max = event_id;
            }
            out.push(event);
        }
        max_ids.insert(bucket_id, bucket_max);
    }
    out.sort_by_key(|event| int_value(event.get("id")));
    Ok((out, max_ids))
}

fn should_send(severity: &str, allowed: &[String]) -> bool {
    let severity = severity.to_ascii_lowercase();
    allowed
        .iter()
        .any(|item| item.to_ascii_lowercase() == severity)
}

fn value_str(value: Option<&Value>, default: &str) -> String {
    value.and_then(Value::as_str).unwrap_or(default).to_string()
}

fn build_payload(event: &Value) -> Value {
    let data = event
        .get("data")
        .filter(|value| value.is_object())
        .unwrap_or(&Value::Null);
    let severity = value_str(data.get("severity"), "low");
    json!({
        "source": "AWatch-rus DLP",
        "timestamp": event.get("timestamp").cloned().unwrap_or(Value::Null),
        "event_id": event.get("id").cloned().unwrap_or(Value::Null),
        "severity": severity,
        "message": data.get("message").cloned().unwrap_or(Value::Null),
        "ruleId": data.get("ruleId").cloned().unwrap_or(Value::Null),
        "signalType": data.get("signalType").cloned().unwrap_or(Value::Null),
        "hostname": data.get("hostname").cloned().unwrap_or(Value::Null),
        "username": data.get("username").cloned().unwrap_or(Value::Null),
        "action": data.get("action").cloned().unwrap_or(Value::Null),
        "raw": data,
    })
}

fn post_with_retry(client: &Client, url: &str, payload: &Value, cfg: &Config) -> bool {
    let body = match serde_json::to_vec(payload) {
        Ok(body) => body,
        Err(err) => {
            eprintln!("webhook payload serialization error url={url}: {err}");
            return false;
        }
    };
    for attempt in 1..=cfg.retries.max(1) {
        let result = client
            .post(url)
            .timeout(Duration::from_secs(cfg.timeout_sec))
            .header(CONTENT_TYPE, "application/json; charset=utf-8")
            .body(body.clone())
            .send();
        match result {
            Ok(resp) if resp.status().is_success() => return true,
            Ok(resp) => eprintln!(
                "webhook http error url={url} code={} attempt={attempt}/{}",
                resp.status(),
                cfg.retries.max(1)
            ),
            Err(err) => eprintln!(
                "webhook transport error url={url} err={err} attempt={attempt}/{}",
                cfg.retries.max(1)
            ),
        }
        if attempt < cfg.retries.max(1) {
            sleep(Duration::from_secs_f64(
                cfg.backoff_base.powi((attempt - 1) as i32),
            ));
        }
    }
    false
}

fn run(cli: &Cli, client: &Client) -> RunSummary {
    let cfg = load_config(&cli.config);
    let state = load_json(&cfg.state_path);
    let (incidents, max_ids) = match iter_new_incidents(
        client,
        &cfg.aw_api_base,
        &state,
        cfg.per_bucket_limit,
        cfg.timeout_sec,
    ) {
        Ok(result) => result,
        Err(err) => {
            eprintln!("skip webhook sender run: AW API unavailable: {err}");
            return RunSummary {
                ok: true,
                delivered: 0,
                incidents_seen: 0,
                buckets: 0,
                dry_run: cli.dry_run,
                state_saved: false,
                state_path: cfg.state_path.to_string_lossy().to_string(),
                error: Some(err.to_string()),
            };
        }
    };

    let mut delivered = 0;
    for event in &incidents {
        let data = event
            .get("data")
            .filter(|value| value.is_object())
            .unwrap_or(&Value::Null);
        let severity = value_str(data.get("severity"), "low");
        for hook in &cfg.critical_webhooks {
            if hook.url.trim().is_empty() || !should_send(&severity, &hook.severity) {
                continue;
            }
            if cli.dry_run || post_with_retry(client, &hook.url, &build_payload(event), &cfg) {
                delivered += 1;
            }
        }
    }

    let mut next_state = state;
    next_state["last_ids"] = json!(max_ids);
    next_state["updated_at"] = json!(Utc::now().to_rfc3339_opts(SecondsFormat::Micros, false));
    let mut state_saved = false;
    if !cli.dry_run {
        if let Err(err) = save_json(&cfg.state_path, &next_state) {
            return RunSummary {
                ok: false,
                delivered,
                incidents_seen: incidents.len(),
                buckets: next_state["last_ids"]
                    .as_object()
                    .map_or(0, serde_json::Map::len),
                dry_run: false,
                state_saved: false,
                state_path: cfg.state_path.to_string_lossy().to_string(),
                error: Some(err.to_string()),
            };
        }
        state_saved = true;
    }

    RunSummary {
        ok: true,
        delivered,
        incidents_seen: incidents.len(),
        buckets: next_state["last_ids"]
            .as_object()
            .map_or(0, serde_json::Map::len),
        dry_run: cli.dry_run,
        state_saved,
        state_path: cfg.state_path.to_string_lossy().to_string(),
        error: None,
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = Client::builder()
        .no_proxy()
        .build()
        .context("build HTTP client")?;
    let summary = run(&cli, &client);
    if cli.json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else if let Some(err) = &summary.error {
        eprintln!("{err}");
    } else {
        println!(
            "Webhook sender done: delivered={} incidents_seen={}",
            summary.delivered, summary.incidents_seen
        );
    }
    if summary.ok {
        Ok(())
    } else {
        Err(anyhow!(
            summary
                .error
                .unwrap_or_else(|| "webhook sender failed".to_string())
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_config_with_hook() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("webhook-config.yaml");
        fs::write(
            &path,
            r#"
aw_api_base: "http://127.0.0.1:5600/api/0"
state_path: "/tmp/webhook-state.json"
retries: 2
timeout_sec: 3
backoff_base: 1.5
per_bucket_limit: 10
critical_webhooks:
  - url: "https://example.invalid/hook"
    severity: ["high", "medium"]
"#,
        )
        .unwrap();
        let cfg = load_config(&path);
        assert_eq!(cfg.retries, 2);
        assert_eq!(cfg.timeout_sec, 3);
        assert_eq!(cfg.per_bucket_limit, 10);
        assert_eq!(cfg.critical_webhooks.len(), 1);
        assert_eq!(cfg.critical_webhooks[0].url, "https://example.invalid/hook");
        assert_eq!(cfg.critical_webhooks[0].severity, ["high", "medium"]);
    }

    #[test]
    fn severity_matching_is_case_insensitive() {
        assert!(should_send("HIGH", &[String::from("high")]));
        assert!(!should_send("low", &[String::from("high")]));
    }

    #[test]
    fn payload_shape_matches_python_contract() {
        let event = json!({
            "id": 7,
            "timestamp": "2026-06-01T10:00:00Z",
            "data": {
                "severity": "high",
                "message": "m",
                "ruleId": "r",
                "signalType": "dlp_incident",
                "hostname": "h",
                "username": "u",
                "action": "alert"
            }
        });
        let payload = build_payload(&event);
        assert_eq!(payload["source"], "AWatch-rus DLP");
        assert_eq!(payload["event_id"], 7);
        assert_eq!(payload["severity"], "high");
        assert_eq!(payload["raw"]["ruleId"], "r");
    }
}
