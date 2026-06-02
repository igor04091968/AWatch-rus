use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Days, Local, NaiveDate, TimeDelta, TimeZone, Utc};
use clap::Parser;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const DEFAULT_AW_BASE_URL: &str = "http://10.10.10.13:5600/api/0";
const DEFAULT_HOST: &str = "SHARKON2025";
const DEFAULT_SAMPLE_SECONDS: f64 = 30.0;
const DEFAULT_MAX_SAMPLE_SECONDS: f64 = 300.0;
const DEFAULT_OUT_DIR: &str = "reports";

#[derive(Debug, Parser)]
#[command(about = "Build per-user RDP worktime CSV/JSON report from AW session samples")]
struct Cli {
    #[arg(long)]
    day: Option<String>,

    #[arg(long)]
    from: Option<NaiveDate>,

    #[arg(long)]
    to: Option<NaiveDate>,
}

#[derive(Debug, Clone)]
struct Config {
    aw_base_url: String,
    host: String,
    default_sample_seconds: f64,
    max_sample_seconds: f64,
    out_dir: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
struct AwEvent {
    timestamp: Option<String>,
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    data: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct ReportRow {
    user: String,
    user_id: String,
    active_seconds: i64,
    active_hhmm: String,
    first_activity: String,
    last_activity: String,
    idle_seconds: i64,
    sessions_count: usize,
    samples_count: i64,
    active_samples: i64,
}

#[derive(Debug, Serialize)]
struct JsonReport {
    host: String,
    bucket_id: String,
    from: String,
    to: String,
    generated_at_utc: String,
    rows: Vec<ReportRow>,
}

#[derive(Debug, Default)]
struct UserAggregate {
    user: String,
    user_id: String,
    sessions: BTreeSet<String>,
    samples_count: i64,
    active_samples: i64,
    intervals: Vec<(DateTime<Utc>, DateTime<Utc>)>,
}

#[derive(Debug, Clone)]
struct Sample {
    ts: DateTime<Utc>,
    duration: Option<f64>,
    data: Value,
}

fn main() {
    let code = match run() {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("{err:#}");
            1
        }
    };
    std::process::exit(code);
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let (from, to) = resolve_range(&cli)?;
    let config = load_config();
    fs::create_dir_all(&config.out_dir)
        .with_context(|| format!("create {}", config.out_dir.display()))?;

    let bucket_id = format!("aw-worktime-sessions_{}", config.host);
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(30))
        .build()
        .context("build HTTP client")?;

    get_json::<Value>(
        &client,
        &format!("{}/buckets/{bucket_id}", config.aw_base_url),
    )
    .with_context(|| format!("Bucket not found: {bucket_id}"))?;
    let events: Vec<AwEvent> = get_json(
        &client,
        &format!(
            "{}/buckets/{bucket_id}/events?limit=50000",
            config.aw_base_url
        ),
    )?;

    let rows = build_rows(
        events,
        &config.host,
        from,
        to,
        config.default_sample_seconds,
        config.max_sample_seconds,
    )?;
    let csv_out = config
        .out_dir
        .join(format!("rdp-worktime-{}_{}.csv", from, to));
    let json_out = config
        .out_dir
        .join(format!("rdp-worktime-{}_{}.json", from, to));
    write_csv(&csv_out, &rows)?;
    write_json(&json_out, &config.host, &bucket_id, from, to, &rows)?;

    println!("{}", csv_out.display());
    println!("{}", json_out.display());
    println!("CSV: {}", csv_out.display());
    println!("JSON: {}", json_out.display());
    Ok(())
}

fn load_config() -> Config {
    let default_sample_seconds =
        env_f64("AW_WORKTIME_DEFAULT_SAMPLE_SECONDS", DEFAULT_SAMPLE_SECONDS).max(1.0);
    let max_sample_seconds = env_f64("AW_WORKTIME_MAX_SAMPLE_SECONDS", DEFAULT_MAX_SAMPLE_SECONDS)
        .max(default_sample_seconds);
    Config {
        aw_base_url: normalize_aw_base(&env_string("AW_BASE_URL", DEFAULT_AW_BASE_URL)),
        host: env_string("AW_WORKTIME_HOST", DEFAULT_HOST),
        default_sample_seconds,
        max_sample_seconds,
        out_dir: PathBuf::from(env_string("OUT_DIR", DEFAULT_OUT_DIR)),
    }
}

fn env_string(name: &str, fallback: &str) -> String {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

fn env_f64(name: &str, fallback: f64) -> f64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(fallback)
}

fn normalize_aw_base(raw: &str) -> String {
    let base = raw.trim().trim_end_matches('/');
    if base.ends_with("/api/0") {
        base.to_string()
    } else {
        format!("{base}/api/0")
    }
}

fn resolve_range(cli: &Cli) -> Result<(NaiveDate, NaiveDate)> {
    if let Some(day) = &cli.day {
        let today = Local::now().date_naive();
        return match day.as_str() {
            "today" => Ok((today, today)),
            "yesterday" => {
                let yesterday = today
                    .checked_sub_days(Days::new(1))
                    .context("calculate yesterday")?;
                Ok((yesterday, yesterday))
            }
            _ => bail!("Invalid --day: {day}"),
        };
    }

    let Some(from) = cli.from else {
        bail!(
            "Usage: rdp-worktime-report --day today|yesterday OR --from YYYY-MM-DD --to YYYY-MM-DD"
        );
    };
    let Some(to) = cli.to else {
        bail!(
            "Usage: rdp-worktime-report --day today|yesterday OR --from YYYY-MM-DD --to YYYY-MM-DD"
        );
    };
    Ok((from, to))
}

fn get_json<T: serde::de::DeserializeOwned>(client: &Client, url: &str) -> Result<T> {
    let response = client
        .get(url)
        .send()
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("GET {url} status"))?;
    response
        .json()
        .with_context(|| format!("decode JSON from {url}"))
}

fn build_rows(
    events: Vec<AwEvent>,
    host: &str,
    from: NaiveDate,
    to: NaiveDate,
    default_sample_seconds: f64,
    max_sample_seconds: f64,
) -> Result<Vec<ReportRow>> {
    let start = day_start_utc(from)?;
    let end = day_end_utc(to)?;
    let mut by_identity: BTreeMap<(String, String), Vec<Sample>> = BTreeMap::new();

    for event in events {
        let Some(ts) = event.timestamp.as_deref().and_then(parse_ts) else {
            continue;
        };
        if ts < start || ts > end {
            continue;
        }
        let user = value_string(&event.data, "username").trim().to_string();
        if user.is_empty() {
            continue;
        }
        let session_id = value_string(&event.data, "sessionId");
        let session_id = if session_id.trim().is_empty() {
            "unknown".to_string()
        } else {
            session_id.trim().to_string()
        };
        by_identity
            .entry((user, session_id))
            .or_default()
            .push(Sample {
                ts,
                duration: event.duration,
                data: event.data,
            });
    }

    let full_range = (end - start).num_seconds() + 1;
    let mut by_user: BTreeMap<String, UserAggregate> = BTreeMap::new();
    for ((user, session_id), mut samples) in by_identity {
        samples.sort_by_key(|sample| sample.ts);
        for idx in 0..samples.len() {
            let sample = &samples[idx];
            let rec = by_user
                .entry(user.clone())
                .or_insert_with(|| UserAggregate {
                    user: user.clone(),
                    user_id: normalize_user_id(&sample.data, host, &user),
                    ..UserAggregate::default()
                });
            rec.sessions.insert(session_id.clone());
            rec.samples_count += 1;
            if !is_active(&sample.data) {
                continue;
            }
            rec.active_samples += 1;
            let sample_seconds = sample_seconds(
                sample,
                samples.get(idx + 1).map(|next| next.ts),
                default_sample_seconds,
                max_sample_seconds,
            );
            let interval_end = std::cmp::min(
                sample.ts + TimeDelta::milliseconds((sample_seconds * 1000.0).round() as i64),
                end + TimeDelta::seconds(1),
            );
            if interval_end > sample.ts {
                rec.intervals.push((sample.ts, interval_end));
            }
        }
    }

    let mut rows = Vec::new();
    for (_, rec) in by_user {
        let merged = merge_intervals(rec.intervals);
        let mut active: i64 = merged
            .iter()
            .map(|(begin, finish)| (*finish - *begin).num_seconds())
            .sum();
        active = active.min(full_range);
        let idle_seconds = (full_range - active).max(0);
        rows.push(ReportRow {
            user: rec.user,
            user_id: rec.user_id,
            active_seconds: active,
            active_hhmm: format!("{:02}:{:02}", active / 3600, (active % 3600) / 60),
            first_activity: merged
                .first()
                .map(|(begin, _)| format_ts(*begin))
                .unwrap_or_default(),
            last_activity: merged
                .last()
                .map(|(_, finish)| format_ts(*finish))
                .unwrap_or_default(),
            idle_seconds,
            sessions_count: rec.sessions.len(),
            samples_count: rec.samples_count,
            active_samples: rec.active_samples,
        });
    }
    Ok(rows)
}

fn day_start_utc(day: NaiveDate) -> Result<DateTime<Utc>> {
    Utc.from_local_datetime(
        &day.and_hms_opt(0, 0, 0)
            .with_context(|| format!("invalid start date {day}"))?,
    )
    .single()
    .context("resolve UTC start")
}

fn day_end_utc(day: NaiveDate) -> Result<DateTime<Utc>> {
    Utc.from_local_datetime(
        &day.and_hms_opt(23, 59, 59)
            .with_context(|| format!("invalid end date {day}"))?,
    )
    .single()
    .context("resolve UTC end")
}

fn parse_ts(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&value.replace('Z', "+00:00"))
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn value_string(data: &Value, key: &str) -> String {
    match data.get(key) {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Number(value)) => value.to_string(),
        Some(Value::Bool(value)) => value.to_string(),
        _ => String::new(),
    }
}

fn is_active(data: &Value) -> bool {
    if data.get("active").and_then(Value::as_bool) == Some(true) {
        return true;
    }
    let state = value_string(data, "state").trim().to_lowercase();
    state.contains("актив") || state == "active"
}

fn normalize_user_id(data: &Value, host: &str, username: &str) -> String {
    let raw = value_string(data, "userId").trim().to_string();
    if let Some((_, right)) = raw.split_once('\\') {
        return format!("{host}\\{right}");
    }
    if !raw.is_empty() {
        return raw;
    }
    format!("{host}\\{username}")
}

fn sample_seconds(
    sample: &Sample,
    next_ts: Option<DateTime<Utc>>,
    default_sample_seconds: f64,
    max_sample_seconds: f64,
) -> f64 {
    for key in ["sampleSeconds", "pollSeconds"] {
        if let Some(value) = value_f64(&sample.data, key).filter(|value| *value > 0.0) {
            return clamp_seconds(value, default_sample_seconds, max_sample_seconds);
        }
    }
    if let Some(duration) = sample.duration.filter(|value| *value > 0.0) {
        return clamp_seconds(duration, default_sample_seconds, max_sample_seconds);
    }
    if let Some(next_ts) = next_ts {
        return clamp_seconds(
            (next_ts - sample.ts).num_milliseconds() as f64 / 1000.0,
            default_sample_seconds,
            max_sample_seconds,
        );
    }
    clamp_seconds(
        default_sample_seconds,
        default_sample_seconds,
        max_sample_seconds,
    )
}

fn value_f64(data: &Value, key: &str) -> Option<f64> {
    match data.get(key) {
        Some(Value::Number(value)) => value.as_f64(),
        Some(Value::String(value)) => value.parse().ok(),
        _ => None,
    }
}

fn clamp_seconds(value: f64, fallback: f64, max_sample_seconds: f64) -> f64 {
    let seconds = if value <= 0.0 { fallback } else { value };
    seconds.min(max_sample_seconds)
}

fn merge_intervals(
    mut intervals: Vec<(DateTime<Utc>, DateTime<Utc>)>,
) -> Vec<(DateTime<Utc>, DateTime<Utc>)> {
    if intervals.is_empty() {
        return Vec::new();
    }
    intervals.sort_by_key(|(start, _)| *start);
    let mut merged = vec![intervals[0]];
    for (start, end) in intervals.into_iter().skip(1) {
        let last = merged.last_mut().expect("merged non-empty");
        if start <= last.1 {
            if end > last.1 {
                last.1 = end;
            }
        } else {
            merged.push((start, end));
        }
    }
    merged
}

fn write_csv(path: &Path, rows: &[ReportRow]) -> Result<()> {
    let fields = [
        "user",
        "user_id",
        "active_seconds",
        "active_hhmm",
        "first_activity",
        "last_activity",
        "idle_seconds",
        "sessions_count",
        "samples_count",
        "active_samples",
    ];
    let mut out = String::new();
    out.push_str(&fields.join(","));
    out.push('\n');
    for row in rows {
        out.push_str(&csv_escape(&row.user));
        out.push(',');
        out.push_str(&csv_escape(&row.user_id));
        out.push(',');
        out.push_str(&row.active_seconds.to_string());
        out.push(',');
        out.push_str(&csv_escape(&row.active_hhmm));
        out.push(',');
        out.push_str(&csv_escape(&row.first_activity));
        out.push(',');
        out.push_str(&csv_escape(&row.last_activity));
        out.push(',');
        out.push_str(&row.idle_seconds.to_string());
        out.push(',');
        out.push_str(&row.sessions_count.to_string());
        out.push(',');
        out.push_str(&row.samples_count.to_string());
        out.push(',');
        out.push_str(&row.active_samples.to_string());
        out.push('\n');
    }
    fs::write(path, out).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn write_json(
    path: &Path,
    host: &str,
    bucket_id: &str,
    from: NaiveDate,
    to: NaiveDate,
    rows: &[ReportRow],
) -> Result<()> {
    let report = JsonReport {
        host: host.to_string(),
        bucket_id: bucket_id.to_string(),
        from: from.to_string(),
        to: to.to_string(),
        generated_at_utc: format_ts(Utc::now()),
        rows: rows.to_vec(),
    };
    fs::write(path, serde_json::to_string_pretty(&report)?)
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn format_ts(value: DateTime<Utc>) -> String {
    value.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use serde_json::json;

    use super::*;

    #[test]
    fn computes_active_intervals_and_merges_overlap() {
        let events = vec![
            event(
                "2026-06-01T10:00:00Z",
                None,
                json!({"username":"user5","userId":"HOST\\user5","sessionId":2,"active":true,"sampleSeconds":60}),
            ),
            event(
                "2026-06-01T10:00:30Z",
                None,
                json!({"username":"user5","userId":"HOST\\user5","sessionId":2,"state":"Активно","sampleSeconds":60}),
            ),
            event(
                "2026-06-01T11:00:00Z",
                None,
                json!({"username":"user5","sessionId":3,"state":"Disc","sampleSeconds":60}),
            ),
        ];
        let rows = build_rows(
            events,
            "SHARKON2025",
            NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
            30.0,
            300.0,
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].active_seconds, 90);
        assert_eq!(rows[0].active_hhmm, "00:01");
        assert_eq!(rows[0].sessions_count, 2);
        assert_eq!(rows[0].samples_count, 3);
        assert_eq!(rows[0].active_samples, 2);
        assert_eq!(rows[0].user_id, "SHARKON2025\\user5");
    }

    #[test]
    fn uses_next_timestamp_when_duration_missing() {
        let events = vec![
            event(
                "2026-06-01T10:00:00Z",
                None,
                json!({"username":"admin","sessionId":1,"state":"active"}),
            ),
            event(
                "2026-06-01T10:02:00Z",
                None,
                json!({"username":"admin","sessionId":1,"state":"active"}),
            ),
        ];
        let rows = build_rows(
            events,
            "HOST",
            NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
            30.0,
            300.0,
        )
        .unwrap();
        assert_eq!(rows[0].active_seconds, 150);
        assert_eq!(rows[0].active_samples, 2);
    }

    #[test]
    fn normalizes_base_url() {
        assert_eq!(
            normalize_aw_base("http://127.0.0.1:5600"),
            "http://127.0.0.1:5600/api/0"
        );
        assert_eq!(
            normalize_aw_base("http://127.0.0.1:5600/api/0/"),
            "http://127.0.0.1:5600/api/0"
        );
    }

    fn event(timestamp: &str, duration: Option<f64>, data: Value) -> AwEvent {
        AwEvent {
            timestamp: Some(timestamp.to_string()),
            duration,
            data,
        }
    }
}
