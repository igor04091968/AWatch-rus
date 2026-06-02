use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::Parser;
use reqwest::blocking::Client;
use serde_json::Value;

const DEFAULT_SERVER: &str = "http://10.10.10.13:5600";
const DEFAULT_HOST: &str = "SHARKON2025";
const BUCKETS: &[&str] = &[
    "aw-dlp-endpoint-signals",
    "aw-dlp-incidents",
    "aw-dlp-review",
    "aw-dlp-rules",
    "aw-watcher-afk",
    "aw-watcher-window",
    "aw-session-events",
    "aw-worktime-sessions",
];

#[derive(Debug, Parser)]
#[command(about = "Check ActivityWatch data collection freshness for DetMir/AW-RUS")]
struct Cli {
    #[arg(long)]
    server: Option<String>,

    #[arg(long)]
    host: Option<String>,

    #[arg(long, default_value_t = 15)]
    timeout_seconds: u64,

    #[arg(long, default_value_t = 15)]
    bucket_timeout_seconds: u64,

    #[arg(long, default_value_t = 3)]
    context_timeout_seconds: u64,

    #[arg(long, default_value_t = false)]
    with_event_ids: bool,

    #[arg(long, default_value_t = false)]
    no_color: bool,
}

#[derive(Debug, Clone)]
struct Colors {
    red: &'static str,
    green: &'static str,
    yellow: &'static str,
    cyan: &'static str,
    reset: &'static str,
}

impl Colors {
    fn new(enabled: bool) -> Self {
        if enabled {
            Self {
                red: "\u{1b}[0;31m",
                green: "\u{1b}[0;32m",
                yellow: "\u{1b}[1;33m",
                cyan: "\u{1b}[0;36m",
                reset: "\u{1b}[0m",
            }
        } else {
            Self {
                red: "",
                green: "",
                yellow: "",
                cyan: "",
                reset: "",
            }
        }
    }

    fn paint(&self, color: &str, text: &str) -> String {
        format!("{color}{text}{}", self.reset)
    }
}

#[derive(Debug, Default)]
struct ContextState {
    host_inactive: bool,
    guard_healthy: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum BucketStatus {
    Fresh,
    Stale,
    Dead,
    Empty,
    EventDriven,
    Inactive,
    Unknown,
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
    let server = cli
        .server
        .or_else(|| env_nonempty("AW_CHECK_SERVER"))
        .or_else(|| env_nonempty("AW_SERVER_URL"))
        .unwrap_or_else(|| DEFAULT_SERVER.to_string())
        .trim_end_matches('/')
        .to_string();
    let host = cli
        .host
        .or_else(|| env_nonempty("AW_CHECK_HOST"))
        .or_else(|| env_nonempty("AW_MONITORED_HOST"))
        .or_else(|| env_nonempty("AW_MONITORED_WINDOWS_HOSTNAME"))
        .unwrap_or_else(|| DEFAULT_HOST.to_string());
    let colors = Colors::new(!cli.no_color && env_nonempty("NO_COLOR").is_none());
    let client = Client::builder()
        .no_proxy()
        .build()
        .context("build HTTP client")?;
    let now = Utc::now();

    println!("=== ActivityWatch Data Check: {host} ===");
    println!();
    print!("Server connectivity... ");
    let info_url = format!("{server}/api/0/info");
    let info = match get_json(&client, &info_url, cli.timeout_seconds) {
        Ok(value) => value,
        Err(err) => {
            eprintln!("check-aw-data: {err:#}");
            println!(
                "{} (cannot reach {server})",
                colors.paint(colors.red, "FAILED")
            );
            return Ok(1);
        }
    };
    let Some(version) = info.get("version").and_then(Value::as_str) else {
        println!(
            "{} (cannot reach {server})",
            colors.paint(colors.red, "FAILED")
        );
        return Ok(1);
    };
    println!("{} (aw-server {version})", colors.paint(colors.green, "OK"));
    println!();

    let context = read_context(&server, &host, now, cli.context_timeout_seconds);
    let buckets_index = get_json(
        &client,
        &format!("{server}/api/0/buckets"),
        cli.timeout_seconds,
    )
    .ok();
    println!("--- Buckets ---");
    println!(
        "{:<45} {:<8} {:<22} STATUS",
        "BUCKET", "EVENTS", "LAST EVENT"
    );
    println!(
        "{:<45} {:<8} {:<22} ------",
        "---------------------------------------------", "--------", "----------------------"
    );

    for bucket in BUCKETS {
        let bucket_full = format!("{bucket}_{host}");
        let event = bucket_event(
            &server,
            &bucket_full,
            buckets_index.as_ref(),
            cli.with_event_ids,
            cli.bucket_timeout_seconds,
        );
        let (last_id, last_ts, age, status) = render_bucket(bucket, event.as_ref(), now, &context);
        println!(
            "{:<45} {:<8} {:<22} {}",
            bucket_full,
            last_id,
            format!("{last_ts} ({age})"),
            render_status(&colors, status)
        );
    }

    println!();
    println!("--- CORS Check ---");
    let cors_status = check_cors(&client, &server);
    if cors_status == 200 {
        println!("{} (HTTP 200)", colors.paint(colors.green, "CORS: OK"));
    } else {
        println!(
            "{} (HTTP {cors_status})",
            colors.paint(colors.red, "CORS: FAIL")
        );
    }

    println!();
    println!("=== Check Complete ===");
    println!("Timestamp: {}", now.format("%Y-%m-%d %H:%M:%S UTC"));
    Ok(0)
}

fn read_context(
    server: &str,
    host: &str,
    now: DateTime<Utc>,
    timeout_seconds: u64,
) -> ContextState {
    let mut state = ContextState::default();
    if let Ok(Some(event)) = get_latest_event(
        server,
        &format!("aw-worktime-sessions_{host}"),
        timeout_seconds,
    ) {
        if let Some(ts) = event_timestamp(&event) {
            let age = (now - ts).num_seconds();
            let active = event
                .pointer("/data/active")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if (0..900).contains(&age) && !active {
                state.host_inactive = true;
            }
        }
    }
    if let Ok(Some(event)) = get_latest_event(
        server,
        &format!("aw-rus-collector-guard_{host}"),
        timeout_seconds,
    ) {
        if let Some(ts) = event_timestamp(&event) {
            let age = (now - ts).num_seconds();
            let status = event
                .pointer("/data/status")
                .and_then(Value::as_str)
                .unwrap_or("");
            let problems = event
                .pointer("/data/problems")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0);
            if (0..300).contains(&age) && status == "ok" && problems == 0 {
                state.guard_healthy = true;
            }
        }
    }
    state
}

fn bucket_event(
    server: &str,
    bucket: &str,
    buckets_index: Option<&Value>,
    with_event_ids: bool,
    timeout_seconds: u64,
) -> Option<Value> {
    if with_event_ids {
        get_latest_event(server, bucket, timeout_seconds)
            .ok()
            .flatten()
            .or_else(|| metadata_event(buckets_index, bucket))
    } else {
        metadata_event(buckets_index, bucket)
    }
}

fn render_bucket(
    bucket: &str,
    event: Option<&Value>,
    now: DateTime<Utc>,
    context: &ContextState,
) -> (String, String, String, BucketStatus) {
    let Some(event) = event else {
        return (
            "0".to_string(),
            "no events".to_string(),
            "none".to_string(),
            classify_bucket_no_events(bucket, context),
        );
    };
    let last_id = event
        .get("id")
        .map(json_value_to_string)
        .unwrap_or_else(|| "0".to_string());
    let Some(ts_raw) = event.get("timestamp").and_then(Value::as_str) else {
        return (
            last_id,
            "no events".to_string(),
            "none".to_string(),
            classify_bucket_no_events(bucket, context),
        );
    };
    let Some(ts) = parse_ts(ts_raw) else {
        return (
            last_id,
            ts_raw.to_string(),
            "unknown".to_string(),
            BucketStatus::Unknown,
        );
    };
    let age = (now - ts).num_seconds().max(0);
    (
        last_id,
        ts_raw.to_string(),
        format_age(age),
        classify_bucket_age(bucket, age, context),
    )
}

fn classify_bucket_age(bucket: &str, age_sec: i64, context: &ContextState) -> BucketStatus {
    match bucket {
        "aw-watcher-window" if context.host_inactive => return BucketStatus::Inactive,
        "aw-dlp-endpoint-signals" if context.host_inactive && context.guard_healthy => {
            return BucketStatus::Inactive;
        }
        _ => {}
    }

    match bucket {
        "aw-dlp-incidents" | "aw-dlp-review" | "aw-dlp-rules" | "aw-session-events" => {
            if age_sec < 86_400 {
                BucketStatus::Fresh
            } else {
                BucketStatus::EventDriven
            }
        }
        _ if age_sec < 3_600 => BucketStatus::Fresh,
        _ if age_sec < 86_400 => BucketStatus::Stale,
        _ => BucketStatus::Dead,
    }
}

fn classify_bucket_no_events(bucket: &str, context: &ContextState) -> BucketStatus {
    match bucket {
        "aw-watcher-window" if context.host_inactive => BucketStatus::Inactive,
        "aw-dlp-endpoint-signals" if context.host_inactive && context.guard_healthy => {
            BucketStatus::Inactive
        }
        "aw-dlp-incidents" | "aw-dlp-review" | "aw-dlp-rules" | "aw-session-events" => {
            BucketStatus::EventDriven
        }
        _ => BucketStatus::Empty,
    }
}

fn render_status(colors: &Colors, status: BucketStatus) -> String {
    match status {
        BucketStatus::Fresh => colors.paint(colors.green, "FRESH"),
        BucketStatus::Stale => colors.paint(colors.yellow, "STALE"),
        BucketStatus::Dead => colors.paint(colors.red, "DEAD"),
        BucketStatus::Empty => colors.paint(colors.red, "EMPTY"),
        BucketStatus::EventDriven => colors.paint(colors.cyan, "EVENT-DRIVEN"),
        BucketStatus::Inactive => colors.paint(colors.cyan, "INACTIVE"),
        BucketStatus::Unknown => colors.paint(colors.red, "?"),
    }
}

fn get_latest_event(server: &str, bucket: &str, timeout_seconds: u64) -> Result<Option<Value>> {
    let url = format!("{server}/api/0/buckets/{bucket}/events?limit=1");
    let client = Client::builder()
        .timeout(Duration::from_secs(timeout_seconds.max(1)))
        .no_proxy()
        .pool_max_idle_per_host(0)
        .build()
        .context("build timed HTTP client")?;
    let value = client
        .get(&url)
        .header("Connection", "close")
        .send()
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("GET {url} status"))?
        .json::<Value>()
        .with_context(|| format!("decode JSON from {url}"))?;
    Ok(value.as_array().and_then(|items| items.first()).cloned())
}

fn metadata_event(buckets_index: Option<&Value>, bucket: &str) -> Option<Value> {
    let bucket_info = buckets_index?.get(bucket)?;
    let timestamp = bucket_info
        .pointer("/metadata/end")
        .or_else(|| bucket_info.get("end"))
        .and_then(Value::as_str)?;
    Some(serde_json::json!({
        "id": 0,
        "timestamp": timestamp,
        "data": {},
        "_source": "bucket_metadata",
    }))
}

fn get_json(client: &Client, url: &str, _timeout_seconds: u64) -> Result<Value> {
    client
        .get(url)
        .send()
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("GET {url} status"))?
        .json::<Value>()
        .with_context(|| format!("decode JSON from {url}"))
}

fn check_cors(_client: &Client, server: &str) -> u16 {
    let origin = "http://10.10.10.13:5600";
    let first = curl_status(&format!("{server}/api/0/settings/"), origin);
    if first == 200 {
        return first;
    }
    if !server.contains("127.0.0.1") && !server.contains("localhost") {
        let fallback = curl_status("http://127.0.0.1:5600/api/0/settings/", origin);
        if fallback != 0 {
            return fallback;
        }
    }
    first
}

fn curl_status(url: &str, origin: &str) -> u16 {
    let origin_header = format!("Origin: {origin}");
    let args = [
        "-s",
        "--connect-timeout",
        "3",
        "--max-time",
        "5",
        "-o",
        "/dev/null",
        "-w",
        "%{http_code}",
        "-H",
        origin_header.as_str(),
        url,
    ];
    let output = Command::new("/usr/bin/curl")
        .args(args)
        .output()
        .or_else(|_| Command::new("curl").args(args).output());
    let Ok(output) = output else {
        return 0;
    };
    if !output.status.success() {
        return 0;
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u16>()
        .unwrap_or(0)
}

fn event_timestamp(event: &Value) -> Option<DateTime<Utc>> {
    event
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(parse_ts)
}

fn parse_ts(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&value.replace('Z', "+00:00"))
        .map(|ts| ts.with_timezone(&Utc))
        .ok()
}

fn format_age(age_sec: i64) -> String {
    if age_sec < 3_600 {
        format!("{}m ago", age_sec / 60)
    } else if age_sec < 86_400 {
        format!("{}h ago", age_sec / 3_600)
    } else {
        format!("{}d ago", age_sec / 86_400)
    }
}

fn json_value_to_string(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        other => other.to_string(),
    }
}

fn env_nonempty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_driven_buckets_do_not_become_dead_when_old() {
        let context = ContextState::default();
        assert_eq!(
            classify_bucket_age("aw-dlp-incidents", 100_000, &context),
            BucketStatus::EventDriven
        );
    }

    #[test]
    fn inactive_host_suppresses_window_stale() {
        let context = ContextState {
            host_inactive: true,
            guard_healthy: true,
        };
        assert_eq!(
            classify_bucket_age("aw-watcher-window", 100_000, &context),
            BucketStatus::Inactive
        );
        assert_eq!(
            classify_bucket_no_events("aw-dlp-endpoint-signals", &context),
            BucketStatus::Inactive
        );
    }

    #[test]
    fn formats_age_like_legacy_script() {
        assert_eq!(format_age(59), "0m ago");
        assert_eq!(format_age(3_600), "1h ago");
        assert_eq!(format_age(86_400), "1d ago");
    }

    #[test]
    fn metadata_only_bucket_event_skips_missing_deep_event_read() {
        let index = serde_json::json!({
            "aw-watcher-window_SHARKON2025": {
                "metadata": {
                    "end": "2026-06-02T00:00:00Z"
                }
            }
        });
        let event = bucket_event(
            "http://127.0.0.1:1",
            "aw-watcher-window_SHARKON2025",
            Some(&index),
            false,
            1,
        )
        .expect("metadata event");
        assert_eq!(
            event.get("timestamp").and_then(Value::as_str),
            Some("2026-06-02T00:00:00Z")
        );
        assert_eq!(event.get("id").and_then(Value::as_i64), Some(0));
    }
}
