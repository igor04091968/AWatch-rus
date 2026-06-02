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

const DEFAULT_CONFIG: &str = "/opt/activitywatch/dlp-integrations/cef-config.yaml";

#[derive(Debug, Parser)]
#[command(about = "AWatch DLP CEF exporter")]
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
    per_bucket_limit: usize,
    severity_mapping: BTreeMap<String, i64>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            aw_api_base: "http://127.0.0.1:5600/api/0".to_string(),
            state_path: PathBuf::from("/var/lib/activitywatch/dlp-integrations/cef-state.json"),
            syslog_host: "127.0.0.1".to_string(),
            syslog_port: 514,
            syslog_proto: "udp".to_string(),
            per_bucket_limit: 300,
            severity_mapping: BTreeMap::from([
                ("low".to_string(), 3),
                ("medium".to_string(), 6),
                ("high".to_string(), 10),
            ]),
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
    target: String,
    error: Option<String>,
}

fn load_config(path: &Path) -> Config {
    let mut config = Config::default();
    let Ok(text) = fs::read_to_string(path) else {
        return config;
    };
    let mut in_severity_mapping = false;
    for raw_line in text.lines() {
        let line_without_comment = raw_line.split('#').next().unwrap_or("");
        let line = line_without_comment.trim();
        if line.is_empty() {
            continue;
        }
        if line == "severity_mapping:" {
            in_severity_mapping = true;
            config.severity_mapping.clear();
            continue;
        }
        if in_severity_mapping && (raw_line.starts_with(' ') || raw_line.starts_with('\t')) {
            if let Some((key, value)) = line.split_once(':')
                && let Ok(score) = clean_scalar(value).parse::<i64>()
            {
                config
                    .severity_mapping
                    .insert(key.trim().to_ascii_lowercase(), score);
            }
            continue;
        }
        in_severity_mapping = false;
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = clean_scalar(value);
        match key {
            "aw_api_base" if !value.is_empty() => config.aw_api_base = value,
            "state_path" if !value.is_empty() => config.state_path = PathBuf::from(value),
            "syslog_host" if !value.is_empty() => config.syslog_host = value,
            "syslog_port" => config.syslog_port = value.parse().unwrap_or(config.syslog_port),
            "syslog_proto" if !value.is_empty() => config.syslog_proto = value.to_ascii_lowercase(),
            "per_bucket_limit" => {
                config.per_bucket_limit = value.parse().unwrap_or(config.per_bucket_limit);
            }
            _ => {}
        }
    }
    if config.severity_mapping.is_empty() {
        config.severity_mapping = Config::default().severity_mapping;
    }
    config
}

fn clean_scalar(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string()
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

fn value_str(value: Option<&Value>, default: &str) -> String {
    value.and_then(Value::as_str).unwrap_or(default).to_string()
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
    let base = aw_base.trim_end_matches('/');
    let buckets = http_json(client, &format!("{base}/buckets/"))?;
    let bucket_ids = incident_bucket_ids(&buckets);
    let last_ids = state.get("last_ids").and_then(Value::as_object);
    let mut max_ids = BTreeMap::new();
    let mut out = Vec::new();
    for bucket_id in bucket_ids {
        let events = match http_json(
            client,
            &format!("{base}/buckets/{bucket_id}/events?limit={per_bucket_limit}"),
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

fn escape_cef(value: Option<&Value>) -> String {
    let text = match value {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(v)) => v.to_string(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    };
    text.replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('=', "\\=")
        .replace('\n', "\\n")
        .replace('\r', "")
}

fn map_severity(name: &str, mapping: &BTreeMap<String, i64>) -> i64 {
    mapping
        .get(&name.to_ascii_lowercase())
        .copied()
        .unwrap_or(3)
}

fn build_cef(event: &Value, mapping: &BTreeMap<String, i64>) -> String {
    let data = event
        .get("data")
        .filter(|value| value.is_object())
        .unwrap_or(&Value::Null);
    let sev_name = value_str(data.get("severity"), "low").to_ascii_lowercase();
    let sev_num = map_severity(&sev_name, mapping);
    let rt = event
        .get("timestamp")
        .cloned()
        .unwrap_or_else(|| json!(Utc::now().to_rfc3339_opts(SecondsFormat::Micros, false)));
    let rule = data
        .get("ruleId")
        .cloned()
        .unwrap_or_else(|| json!("dlp-incident"));
    let msg = data
        .get("message")
        .cloned()
        .unwrap_or_else(|| json!("AWatch DLP incident"));
    let sig = data
        .get("signalType")
        .cloned()
        .unwrap_or_else(|| json!("unknown"));
    let host = data
        .get("hostname")
        .cloned()
        .unwrap_or_else(|| json!("unknown"));
    let user = data
        .get("username")
        .cloned()
        .unwrap_or_else(|| json!("unknown"));
    let action = data
        .get("action")
        .cloned()
        .unwrap_or_else(|| json!("alert"));
    let ext = format!(
        "rt={} shost={} suser={} cs1Label=signalType cs1={} cs2Label=action cs2={} cs3Label=ruleId cs3={}",
        escape_cef(Some(&rt)),
        escape_cef(Some(&host)),
        escape_cef(Some(&user)),
        escape_cef(Some(&sig)),
        escape_cef(Some(&action)),
        escape_cef(Some(&rule)),
    );
    format!(
        "CEF:0|AWatch-rus|DLP|1.0|{}|{}|{}|{}",
        escape_cef(Some(&rule)),
        escape_cef(Some(&msg)),
        sev_num,
        ext
    )
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
                eprintln!("skip CEF exporter run: AW API unavailable: {err}");
                return RunSummary {
                    ok: true,
                    sent: 0,
                    buckets: 0,
                    dry_run: cli.dry_run,
                    state_saved: false,
                    state_path: cfg.state_path.to_string_lossy().to_string(),
                    target: format!(
                        "{}:{}/{}",
                        cfg.syslog_host, cfg.syslog_port, cfg.syslog_proto
                    ),
                    error: Some(err.to_string()),
                };
            }
        };

    let mut sent = 0;
    for event in &incidents {
        let line = build_cef(event, &cfg.severity_mapping);
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
                target: format!(
                    "{}:{}/{}",
                    cfg.syslog_host, cfg.syslog_port, cfg.syslog_proto
                ),
                error: Some(err.to_string()),
            };
        }
        sent += 1;
    }

    let mut next_state = state;
    next_state["last_ids"] = json!(max_ids);
    next_state["updated_at"] = json!(Utc::now().to_rfc3339_opts(SecondsFormat::Micros, false));
    let mut state_saved = false;
    if !cli.dry_run {
        if let Err(err) = save_json(&cfg.state_path, &next_state) {
            return RunSummary {
                ok: false,
                sent,
                buckets: next_state["last_ids"]
                    .as_object()
                    .map_or(0, serde_json::Map::len),
                dry_run: false,
                state_saved: false,
                state_path: cfg.state_path.to_string_lossy().to_string(),
                target: format!(
                    "{}:{}/{}",
                    cfg.syslog_host, cfg.syslog_port, cfg.syslog_proto
                ),
                error: Some(err.to_string()),
            };
        }
        state_saved = true;
    }

    RunSummary {
        ok: true,
        sent,
        buckets: next_state["last_ids"]
            .as_object()
            .map_or(0, serde_json::Map::len),
        dry_run: cli.dry_run,
        state_saved,
        state_path: cfg.state_path.to_string_lossy().to_string(),
        target: format!(
            "{}:{}/{}",
            cfg.syslog_host, cfg.syslog_port, cfg.syslog_proto
        ),
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
            "CEF exporter done: sent={} buckets={} target={}",
            summary.sent, summary.buckets, summary.target
        );
    }
    if summary.ok {
        Ok(())
    } else {
        Err(anyhow!(
            summary
                .error
                .unwrap_or_else(|| "CEF exporter failed".to_string())
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_cef_fields_like_python_contract() {
        assert_eq!(
            escape_cef(Some(&json!("a\\b|c=d\nx\r"))),
            "a\\\\b\\|c\\=d\\nx"
        );
        assert_eq!(escape_cef(None), "");
    }

    #[test]
    fn builds_cef_line() {
        let event = json!({
            "timestamp": "2026-06-01T10:00:00Z",
            "data": {
                "severity": "high",
                "ruleId": "usb|copy",
                "message": "a=b",
                "signalType": "dlp",
                "hostname": "host",
                "username": "user",
                "action": "alert"
            }
        });
        let line = build_cef(&event, &Config::default().severity_mapping);
        assert!(line.starts_with("CEF:0|AWatch-rus|DLP|1.0|usb\\|copy|a\\=b|10|"));
        assert!(line.contains("shost=host"));
        assert!(line.contains("cs3Label=ruleId cs3=usb\\|copy"));
    }

    #[test]
    fn parses_config_mapping() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cef-config.yaml");
        fs::write(
            &path,
            r#"
aw_api_base: "http://127.0.0.1:5600/api/0"
state_path: "/tmp/cef-state.json"
syslog_host: "127.0.0.1"
syslog_port: 5514
syslog_proto: "tcp"
per_bucket_limit: 10
severity_mapping:
  low: 1
  medium: 5
  high: 9
"#,
        )
        .unwrap();
        let cfg = load_config(&path);
        assert_eq!(cfg.syslog_port, 5514);
        assert_eq!(cfg.syslog_proto, "tcp");
        assert_eq!(cfg.per_bucket_limit, 10);
        assert_eq!(cfg.severity_mapping["high"], 9);
    }
}
