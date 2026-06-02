use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::net::{TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use chrono::{SecondsFormat, Utc};
use clap::Parser;
use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::{Value, json};

const DEFAULT_CONFIG: &str = "/opt/activitywatch/dlp-integrations/syslog-forwarder-config.yaml";

#[derive(Debug, Parser)]
#[command(about = "AWatch DLP syslog forwarder")]
struct Cli {
    #[arg(long, default_value = DEFAULT_CONFIG)]
    config: PathBuf,

    #[arg(long)]
    dry_run: bool,

    #[arg(long)]
    json: bool,
}

#[derive(Debug, Clone)]
struct Config {
    aw_api_base: String,
    state_path: PathBuf,
    syslog_host: String,
    syslog_port: u16,
    syslog_proto: String,
    facility: i64,
    app_name: String,
    per_bucket_limit: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            aw_api_base: default_aw_api_base(),
            state_path: default_state_path(),
            syslog_host: default_syslog_host(),
            syslog_port: default_syslog_port(),
            syslog_proto: default_syslog_proto(),
            facility: default_facility(),
            app_name: default_app_name(),
            per_bucket_limit: default_per_bucket_limit(),
        }
    }
}

#[derive(Debug, Serialize)]
struct RunSummary {
    ok: bool,
    sent: usize,
    buckets: usize,
    dry_run: bool,
    state_saved: bool,
    state_path: String,
    error: Option<String>,
}

fn default_aw_api_base() -> String {
    "http://127.0.0.1:5600/api/0".to_string()
}

fn default_state_path() -> PathBuf {
    PathBuf::from("/var/lib/activitywatch/dlp-integrations/syslog-forwarder-state.json")
}

fn default_syslog_host() -> String {
    "127.0.0.1".to_string()
}

fn default_syslog_port() -> u16 {
    514
}

fn default_syslog_proto() -> String {
    "udp".to_string()
}

fn default_facility() -> i64 {
    16
}

fn default_app_name() -> String {
    "aw-dlp".to_string()
}

fn default_per_bucket_limit() -> usize {
    300
}

fn load_config(path: &Path) -> Config {
    if !path.exists() {
        return Config::default();
    }
    let Ok(text) = fs::read_to_string(path) else {
        return Config::default();
    };
    let mut config = Config::default();
    for raw_line in text.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().trim_matches('"').trim_matches('\'');
        match key {
            "aw_api_base" if !value.is_empty() => config.aw_api_base = value.to_string(),
            "state_path" if !value.is_empty() => config.state_path = PathBuf::from(value),
            "syslog_host" if !value.is_empty() => config.syslog_host = value.to_string(),
            "syslog_port" => {
                if let Ok(port) = value.parse::<u16>() {
                    config.syslog_port = port;
                }
            }
            "syslog_proto" if !value.is_empty() => config.syslog_proto = value.to_string(),
            "facility" => {
                if let Ok(facility) = value.parse::<i64>() {
                    config.facility = facility;
                }
            }
            "app_name" if !value.is_empty() => config.app_name = value.to_string(),
            "per_bucket_limit" => {
                if let Ok(limit) = value.parse::<usize>() {
                    config.per_bucket_limit = limit;
                }
            }
            _ => {}
        }
    }
    config
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

fn http_json(client: &Client, url: &str) -> Result<Value> {
    client
        .get(url)
        .timeout(Duration::from_secs(15))
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
) -> Result<(Vec<Value>, BTreeMap<String, i64>)> {
    let buckets = http_json(
        client,
        &format!("{}/buckets/", aw_base.trim_end_matches('/')),
    )?;
    let bucket_ids = incident_bucket_ids(&buckets);
    let last_ids = state.get("last_ids").and_then(Value::as_object);
    let mut max_ids = BTreeMap::new();
    let mut out = Vec::new();
    for bucket_id in bucket_ids {
        let url = format!(
            "{}/buckets/{bucket_id}/events?limit={}",
            aw_base.trim_end_matches('/'),
            per_bucket_limit
        );
        let events = match http_json(client, &url) {
            Ok(Value::Array(items)) => items,
            Ok(_) => {
                eprintln!("skip bucket {bucket_id}: events response is not a list");
                continue;
            }
            Err(err) => {
                eprintln!("skip bucket {bucket_id}: {err}");
                continue;
            }
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

fn build_message(event: &Value, app_name: &str, facility: i64) -> String {
    let pri = facility * 8 + 6;
    let ts = Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true);
    let data = event
        .get("data")
        .filter(|value| value.is_object())
        .unwrap_or(&Value::Null);
    let host = data
        .get("hostname")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let payload = json!({
        "event_id": event.get("id").cloned().unwrap_or(Value::Null),
        "timestamp": event.get("timestamp").cloned().unwrap_or(Value::Null),
        "host": host,
        "severity": data.get("severity").cloned().unwrap_or(Value::Null),
        "signalType": data.get("signalType").cloned().unwrap_or(Value::Null),
        "username": data.get("username").cloned().unwrap_or(Value::Null),
        "action": data.get("action").cloned().unwrap_or(Value::Null),
        "message": data.get("message").cloned().unwrap_or(Value::Null),
        "data": data,
    });
    let payload_text = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string());
    format!("<{pri}>1 {ts} {host} {app_name} - - - {payload_text}")
}

fn send_syslog(line: &str, host: &str, port: u16, proto: &str) -> Result<()> {
    if proto.eq_ignore_ascii_case("tcp") {
        let mut stream = TcpStream::connect((host, port))
            .with_context(|| format!("connect TCP syslog {host}:{port}"))?;
        stream
            .set_write_timeout(Some(Duration::from_secs(10)))
            .context("set TCP write timeout")?;
        stream
            .write_all(format!("{line}\n").as_bytes())
            .context("write TCP syslog")?;
        return Ok(());
    }
    let socket = UdpSocket::bind("0.0.0.0:0").context("bind UDP syslog socket")?;
    socket
        .send_to(line.as_bytes(), (host, port))
        .with_context(|| format!("send UDP syslog {host}:{port}"))?;
    Ok(())
}

fn run(cli: &Cli, client: &Client) -> RunSummary {
    let cfg = load_config(&cli.config);
    let state = load_json(&cfg.state_path);
    let (incidents, max_ids) =
        match iter_new_incidents(client, &cfg.aw_api_base, &state, cfg.per_bucket_limit) {
            Ok(result) => result,
            Err(err) => {
                eprintln!("skip syslog forwarder run: AW API unavailable: {err}");
                return RunSummary {
                    ok: true,
                    sent: 0,
                    buckets: 0,
                    dry_run: cli.dry_run,
                    state_saved: false,
                    state_path: cfg.state_path.to_string_lossy().to_string(),
                    error: Some(err.to_string()),
                };
            }
        };

    let mut sent = 0;
    for event in &incidents {
        let line = build_message(event, &cfg.app_name, cfg.facility);
        if !cli.dry_run
            && let Err(err) =
                send_syslog(&line, &cfg.syslog_host, cfg.syslog_port, &cfg.syslog_proto)
        {
            return RunSummary {
                ok: false,
                sent,
                buckets: max_ids.len(),
                dry_run: cli.dry_run,
                state_saved: false,
                state_path: cfg.state_path.to_string_lossy().to_string(),
                error: Some(err.to_string()),
            };
        }
        sent += 1;
    }

    let mut payload = json!({"last_ids": max_ids});
    let mut state_saved = false;
    if !cli.dry_run {
        if let Err(err) = save_json(&cfg.state_path, &payload) {
            return RunSummary {
                ok: false,
                sent,
                buckets: payload["last_ids"]
                    .as_object()
                    .map_or(0, serde_json::Map::len),
                dry_run: cli.dry_run,
                state_saved: false,
                state_path: cfg.state_path.to_string_lossy().to_string(),
                error: Some(err.to_string()),
            };
        }
        state_saved = true;
    } else {
        payload["dry_run"] = json!(true);
    }

    RunSummary {
        ok: true,
        sent,
        buckets: payload["last_ids"]
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
            "syslog forwarder sent={} buckets={}",
            summary.sent, summary.buckets
        );
    }
    if summary.ok {
        Ok(())
    } else {
        Err(anyhow!(
            summary
                .error
                .unwrap_or_else(|| "syslog forwarder failed".to_string())
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_only_new_incidents_by_id() {
        let state = json!({"last_ids": {"aw-dlp-incidents_HOST": 10}});
        let last_ids = state.get("last_ids").and_then(Value::as_object).unwrap();
        assert_eq!(last_ids["aw-dlp-incidents_HOST"], 10);
    }

    #[test]
    fn builds_rfc5424_like_message() {
        let event = json!({
            "id": 42,
            "timestamp": "2026-05-31T12:00:00Z",
            "data": {
                "hostname": "HOST1",
                "severity": "high",
                "signalType": "dlp_incident",
                "username": "user",
                "action": "alert",
                "message": "test"
            }
        });
        let msg = build_message(&event, "aw-dlp", 16);
        assert!(msg.starts_with("<134>1 "));
        assert!(msg.contains(" HOST1 aw-dlp - - - "));
        assert!(msg.contains("\"event_id\":42"));
    }

    #[test]
    fn incident_bucket_filter_is_sorted() {
        let buckets = json!({
            "other": {},
            "aw-dlp-incidents_B": {},
            "aw-dlp-incidents_A": {}
        });
        assert_eq!(
            incident_bucket_ids(&buckets),
            vec!["aw-dlp-incidents_A", "aw-dlp-incidents_B"]
        );
    }
}
