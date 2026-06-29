use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::Parser;
use reqwest::blocking::Client;
use serde_json::Value;

const DEFAULT_SERVER: &str = "http://10.10.10.13:5600";
const DEFAULT_HOST: &str = "SHARKON2025";
const DEFAULT_RDP_HOST: &str = "192.168.100.19";
const BUCKETS: &[(&str, &str)] = &[
    ("aw-watcher-afk", "AFK watcher"),
    ("aw-watcher-window", "Window watcher"),
    ("aw-worktime-sessions", "Worktime sessions"),
    ("aw-session-events", "Session events"),
    ("aw-dlp-endpoint-signals", "DLP signals"),
    ("aw-dlp-incidents", "DLP incidents"),
    ("aw-dlp-review", "DLP review"),
    ("aw-dlp-rules", "DLP rules"),
];

#[derive(Debug, Parser)]
#[command(about = "Full read-only ActivityWatch check for server, buckets, and RDP host")]
struct Cli {
    #[arg(long, default_value = DEFAULT_SERVER)]
    server: String,

    #[arg(long, default_value = DEFAULT_HOST)]
    host: String,

    #[arg(long, default_value = DEFAULT_RDP_HOST)]
    rdp_host: String,

    #[arg(long, default_value_t = 15)]
    timeout_seconds: u64,

    #[arg(long)]
    no_color: bool,

    #[arg(long, default_value_t = true)]
    dlp_enabled: bool,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BucketStatus {
    Fresh,
    Stale,
    Dead,
    Empty,
    EventDriven,
    Inactive,
    Unknown,
}

#[derive(Debug, Clone)]
struct BucketRow {
    label: &'static str,
    last_id: String,
    age: String,
    status: BucketStatus,
}

#[derive(Debug, Default)]
struct Summary {
    fresh: usize,
    stale: usize,
    dead: usize,
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
    let mut cli = Cli::parse();
    if cli.server == DEFAULT_SERVER {
        if let Some(value) =
            env_nonempty("CHECK_AW_FULL_SERVER").or_else(|| env_nonempty("AW_SMOKE_AW_SERVER"))
                .or_else(|| env_nonempty("AW_SERVER"))
        {
            cli.server = value;
        }
    }
    if cli.host == DEFAULT_HOST {
        if let Some(value) = env_nonempty("CHECK_AW_FULL_HOST")
            .or_else(|| env_nonempty("AW_SMOKE_SOURCE_HOSTNAME"))
            .or_else(|| env_nonempty("AW_LOGICAL_HOST_ID"))
            .or_else(|| env_nonempty("AW_MONITORED_WINDOWS_HOSTNAME"))
        {
            cli.host = value;
        }
    }
    if cli.rdp_host == DEFAULT_RDP_HOST {
        if let Some(value) = env_nonempty("CHECK_AW_FULL_RDP_HOST")
            .or_else(|| env_nonempty("AW_SMOKE_WINDOWS_HOST"))
            .or_else(|| env_nonempty("AW_WINDOWS_HOST"))
        {
            cli.rdp_host = value;
        }
    }
    if let Some(value) =
        env_nonempty("AW_DLP_ENABLED").or_else(|| env_nonempty("DETMIR_DLP_ENABLED"))
    {
        cli.dlp_enabled = parse_env_flag(&value);
    }
    let server = cli.server.trim_end_matches('/').to_string();
    let colors = Colors::new(!cli.no_color && std::env::var_os("NO_COLOR").is_none());
    let timeout = Duration::from_secs(cli.timeout_seconds.max(1));
    let client = Client::builder()
        .timeout(timeout)
        .no_proxy()
        .pool_max_idle_per_host(0)
        .build()
        .context("build HTTP client")?;
    let now = Utc::now();

    println!(
        "{}",
        colors.paint(
            colors.cyan,
            &format!("=== ActivityWatch Full Check: {} ===", cli.host)
        )
    );
    println!();

    println!(
        "{}",
        colors.paint(colors.cyan, &format!("--- 1. AW Server ({server}) ---"))
    );
    print!("  Connectivity... ");
    let info = match get_json(&client, &format!("{server}/api/0/info")) {
        Ok(value) => value,
        Err(_) => {
            println!("  {}", colors.paint(colors.red, "FAILED"));
            return Ok(1);
        }
    };
    let Some(version) = info.get("version").and_then(Value::as_str) else {
        println!("  {}", colors.paint(colors.red, "FAILED"));
        return Ok(1);
    };
    println!(
        "  {} (aw-server {version})",
        colors.paint(colors.green, "OK")
    );

    print!("  CORS... ");
    let cors_code = check_cors(&client, &server);
    if cors_code == 200 {
        println!("{}", colors.paint(colors.green, "OK"));
    } else {
        println!("{} (HTTP {cors_code})", colors.paint(colors.red, "FAIL"));
    }
    println!();

    let context = read_context(&client, &server, &cli.host, now);
    println!("{}", colors.paint(colors.cyan, "--- 2. Data Buckets ---"));
    println!(
        "  {:<42} {:<8} {:<20} STATUS",
        "BUCKET", "EVENTS", "LAST EVENT"
    );
    println!(
        "  {:<42} {:<8} {:<20} ------",
        "------------------------------------------", "--------", "--------------------"
    );

    let mut rows = Vec::new();
    for (bucket, label) in BUCKETS
        .iter()
        .copied()
        .filter(|(bucket, _)| cli.dlp_enabled || !bucket.starts_with("aw-dlp-"))
    {
        let row = read_bucket_row(&client, &server, &cli.host, bucket, label, now, &context);
        println!(
            "  {:<42} {:<8} {:<20} {}",
            row.label,
            row.last_id,
            row.age,
            render_status(&colors, row.status)
        );
        rows.push(row);
    }
    if !cli.dlp_enabled {
        println!(
            "  {:<42} {:<8} {:<20} {}",
            "DLP buckets",
            "-",
            "disabled",
            colors.paint(colors.cyan, "SKIPPED")
        );
    }
    println!();

    println!(
        "{}",
        colors.paint(
            colors.cyan,
            &format!("--- 3. RDP Host ({}) ---", cli.rdp_host)
        )
    );
    print!("  WinRM (5985)... ");
    if tcp_open(&cli.rdp_host, 5985, Duration::from_secs(5)) {
        println!("{}", colors.paint(colors.green, "OK"));
    } else {
        println!("{}", colors.paint(colors.red, "UNREACHABLE"));
    }
    print!("  SSH (22)... ");
    if tcp_open(&cli.rdp_host, 22, Duration::from_secs(5)) {
        println!("{}", colors.paint(colors.green, "OK"));
    } else {
        println!(
            "{} (normal for Windows)",
            colors.paint(colors.yellow, "CLOSED")
        );
    }
    println!();

    let summary = summarize(&rows);
    println!("{}", colors.paint(colors.cyan, "--- 4. Summary ---"));
    println!(
        "  FRESH:  {}",
        colors.paint(colors.green, &summary.fresh.to_string())
    );
    println!(
        "  STALE:  {}",
        colors.paint(colors.yellow, &summary.stale.to_string())
    );
    println!(
        "  DEAD:   {}",
        colors.paint(colors.red, &summary.dead.to_string())
    );

    if summary.dead > 0 || summary.stale > 0 {
        println!();
        println!(
            "  {} Some collectors may need restart on RDP host",
            colors.paint(colors.red, "WARNING:")
        );
        println!(
            "  Run: {}",
            colors.paint(
                colors.cyan,
                "ansible -i ansible/inventory.ini rdp-prod -m win_shell -a 'schtasks /Run /TN \"ActivityWatch Recovery\"'"
            )
        );
    }

    println!();
    println!("{}", colors.paint(colors.cyan, "=== Check Complete ==="));
    println!("  Timestamp: {}", now.format("%Y-%m-%d %H:%M:%S UTC"));
    Ok(0)
}

fn read_context(client: &Client, server: &str, host: &str, now: DateTime<Utc>) -> ContextState {
    let mut state = ContextState::default();
    if let Ok(Some(event)) = latest_event(client, server, &format!("aw-worktime-sessions_{host}")) {
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
    if let Ok(Some(event)) = latest_event(client, server, &format!("aw-rus-collector-guard_{host}"))
    {
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

fn read_bucket_row(
    client: &Client,
    server: &str,
    host: &str,
    bucket: &str,
    label: &'static str,
    now: DateTime<Utc>,
    context: &ContextState,
) -> BucketRow {
    let bucket_full = format!("{bucket}_{host}");
    let event = latest_event(client, server, &bucket_full).ok().flatten();
    let Some(event) = event else {
        return BucketRow {
            label,
            last_id: "0".to_string(),
            age: "none".to_string(),
            status: classify_bucket_no_events(bucket, context),
        };
    };

    let last_id = event
        .get("id")
        .map(json_value_to_string)
        .unwrap_or_else(|| "0".to_string());
    let Some(ts_raw) = event.get("timestamp").and_then(Value::as_str) else {
        return BucketRow {
            label,
            last_id,
            age: "?".to_string(),
            status: BucketStatus::Unknown,
        };
    };
    let Some(ts) = parse_ts(ts_raw) else {
        return BucketRow {
            label,
            last_id,
            age: "?".to_string(),
            status: BucketStatus::Unknown,
        };
    };
    let effective_ts = if bucket == "aw-watcher-afk" {
        bucket_metadata_end(client, server, &bucket_full).unwrap_or(ts)
    } else {
        ts
    };
    let age_sec = (now - effective_ts).num_seconds().max(0);
    BucketRow {
        label,
        last_id,
        age: format_age(age_sec),
        status: classify_bucket_age(bucket, age_sec, context),
    }
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

fn summarize(rows: &[BucketRow]) -> Summary {
    let mut summary = Summary::default();
    for row in rows {
        match row.status {
            BucketStatus::Fresh | BucketStatus::EventDriven | BucketStatus::Inactive => {
                summary.fresh += 1
            }
            BucketStatus::Stale => summary.stale += 1,
            BucketStatus::Dead | BucketStatus::Empty | BucketStatus::Unknown => summary.dead += 1,
        }
    }
    summary
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

fn latest_event(client: &Client, server: &str, bucket: &str) -> Result<Option<Value>> {
    let url = format!("{server}/api/0/buckets/{bucket}/events?limit=1");
    let value = get_json(client, &url)?;
    Ok(value.as_array().and_then(|items| items.first()).cloned())
}

fn bucket_metadata_end(client: &Client, server: &str, bucket: &str) -> Option<DateTime<Utc>> {
    let url = format!("{server}/api/0/buckets/{bucket}");
    let value = get_json(client, &url).ok()?;
    value
        .pointer("/metadata/end")
        .and_then(Value::as_str)
        .and_then(parse_ts)
}

fn get_json(client: &Client, url: &str) -> Result<Value> {
    client
        .get(url)
        .header("Connection", "close")
        .send()
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("GET {url} status"))?
        .json::<Value>()
        .with_context(|| format!("decode JSON from {url}"))
}

fn check_cors(client: &Client, server: &str) -> u16 {
    let url = format!("{server}/api/0/settings/");
    client
        .get(&url)
        .header("Origin", server)
        .send()
        .map(|response| response.status().as_u16())
        .unwrap_or(0)
}

fn tcp_open(host: &str, port: u16, timeout: Duration) -> bool {
    let Ok(mut addrs) = (host, port).to_socket_addrs() else {
        return false;
    };
    addrs.any(|addr| TcpStream::connect_timeout(&addr, timeout).is_ok())
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
        format!("{}m", age_sec / 60)
    } else if age_sec < 86_400 {
        format!("{}h", age_sec / 3_600)
    } else {
        format!("{}d", age_sec / 86_400)
    }
}

fn json_value_to_string(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        other => other.to_string(),
    }
}

fn env_nonempty(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

fn parse_env_flag(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_driven_bucket_is_not_dead_when_old() {
        assert_eq!(
            classify_bucket_age("aw-session-events", 100_000, &ContextState::default()),
            BucketStatus::EventDriven
        );
    }

    #[test]
    fn inactive_context_suppresses_expected_idle_buckets() {
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
    fn summary_treats_event_and_inactive_as_fresh_class() {
        let rows = vec![
            BucketRow {
                label: "a",
                last_id: "0".to_string(),
                age: "1m".to_string(),
                status: BucketStatus::Fresh,
            },
            BucketRow {
                label: "b",
                last_id: "0".to_string(),
                age: "none".to_string(),
                status: BucketStatus::EventDriven,
            },
            BucketRow {
                label: "c",
                last_id: "0".to_string(),
                age: "none".to_string(),
                status: BucketStatus::Inactive,
            },
            BucketRow {
                label: "d",
                last_id: "0".to_string(),
                age: "none".to_string(),
                status: BucketStatus::Empty,
            },
        ];
        let summary = summarize(&rows);
        assert_eq!(summary.fresh, 3);
        assert_eq!(summary.stale, 0);
        assert_eq!(summary.dead, 1);
    }

    #[test]
    fn formats_age_like_legacy_full_check() {
        assert_eq!(format_age(59), "0m");
        assert_eq!(format_age(3_600), "1h");
        assert_eq!(format_age(86_400), "1d");
    }
}
