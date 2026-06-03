use std::{collections::HashMap, thread, time::Duration};

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, TimeDelta, TimeZone, Timelike, Utc};
use clap::Parser;
use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::Value;

const DEFAULT_AW_BASE: &str = "http://127.0.0.1:5600";
const DEFAULT_WORKTIME_REPORT_BASE: &str = "http://127.0.0.1:5610";
const DEFAULT_INFLUX_ORG: &str = "proxmox";
const DEFAULT_INFLUX_BUCKET: &str = "aw_metrics";
const DEFAULT_HOST: &str = "SHARKON2025";
const DEFAULT_DAYS: &str = "today,yesterday";

#[derive(Debug, Parser)]
#[command(about = "AW Worktime InfluxDB exporter")]
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
    report_base: String,
    influx_url: String,
    influx_org: String,
    influx_bucket: String,
    influx_token: String,
    influx_enabled: bool,
    hosts: Vec<String>,
    days: Vec<String>,
    report_offset: FixedOffset,
    default_sample_seconds: f64,
    max_sample_seconds: f64,
    events_limit: usize,
}

#[derive(Debug, Serialize)]
struct RunSummary {
    ok: bool,
    enabled: bool,
    dry_run: bool,
    hosts: Vec<String>,
    days: Vec<String>,
    lines: usize,
    written: usize,
    bucket: String,
    error: Option<String>,
}

#[derive(Debug, Clone)]
enum FieldValue {
    Int(i64),
    String(String),
}

#[derive(Debug, Clone)]
struct Interval {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct UserAccumulator {
    user: String,
    user_id: String,
    samples_count: i64,
    active_samples: i64,
    session_ids: Vec<String>,
    intervals: Vec<Interval>,
}

#[derive(Debug, Clone)]
struct DailyRow {
    user: String,
    user_id: String,
    active_seconds: i64,
    idle_seconds: i64,
    sessions_count: i64,
    samples_count: i64,
    active_samples: i64,
}

#[derive(Debug, Clone)]
struct HourlyRow {
    user: String,
    user_id: String,
    bucket_start_utc: DateTime<Utc>,
    report_date: String,
    hour_local: String,
    active_seconds: i64,
}

type IdentitySamples = HashMap<(String, String), Vec<(DateTime<Utc>, Value)>>;

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

fn env_f64(name: &str, fallback: f64) -> f64 {
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

fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn build_aw_api_base(raw: &str) -> String {
    let base = raw.trim().trim_end_matches('/');
    if base.ends_with("/api/0") {
        base.to_string()
    } else {
        format!("{base}/api/0")
    }
}

fn report_offset_from_env() -> FixedOffset {
    let tz = env("AW_WORKTIME_TZ", "Europe/Moscow");
    match tz.as_str() {
        "UTC" | "Etc/UTC" => FixedOffset::east_opt(0).expect("valid UTC offset"),
        "Europe/Moscow" => FixedOffset::east_opt(3 * 3600).expect("valid Moscow offset"),
        value if value.starts_with('+') || value.starts_with('-') => parse_offset(value)
            .unwrap_or_else(|| FixedOffset::east_opt(3 * 3600).expect("valid Moscow offset")),
        _ => FixedOffset::east_opt(3 * 3600).expect("valid Moscow offset"),
    }
}

fn parse_offset(value: &str) -> Option<FixedOffset> {
    let sign = if value.starts_with('-') { -1 } else { 1 };
    let raw = value.trim_start_matches(['+', '-']);
    let mut parts = raw.split(':');
    let hours: i32 = parts.next()?.parse().ok()?;
    let minutes: i32 = parts.next().unwrap_or("0").parse().ok()?;
    FixedOffset::east_opt(sign * (hours * 3600 + minutes * 60))
}

fn load_config() -> Config {
    let aw_base = env(
        "AW_WORKTIME_AW_API_BASE",
        &env("AW_SERVER_URL", DEFAULT_AW_BASE),
    );
    let default_sample_seconds = env_f64("AW_WORKTIME_DEFAULT_SAMPLE_SECONDS", 30.0).max(1.0);
    let max_sample_seconds =
        env_f64("AW_WORKTIME_MAX_SAMPLE_SECONDS", 300.0).max(default_sample_seconds);
    Config {
        aw_api_base: build_aw_api_base(&aw_base),
        report_base: env("AW_WORKTIME_REPORT_BASE", DEFAULT_WORKTIME_REPORT_BASE)
            .trim_end_matches('/')
            .to_string(),
        influx_url: env("AW_WORKTIME_INFLUX_URL", "")
            .trim_end_matches('/')
            .to_string(),
        influx_org: env("AW_WORKTIME_INFLUX_ORG", DEFAULT_INFLUX_ORG),
        influx_bucket: env("AW_WORKTIME_INFLUX_BUCKET", DEFAULT_INFLUX_BUCKET),
        influx_token: env("AW_WORKTIME_INFLUX_TOKEN", ""),
        influx_enabled: env_bool("AW_WORKTIME_INFLUX_ENABLED", false),
        hosts: split_csv(&env(
            "AW_WORKTIME_INFLUX_HOSTS",
            &env("AW_WORKTIME_HOST", DEFAULT_HOST),
        )),
        days: split_csv(&env("AW_WORKTIME_INFLUX_DAYS", DEFAULT_DAYS)),
        report_offset: report_offset_from_env(),
        default_sample_seconds,
        max_sample_seconds,
        events_limit: env_usize("AW_WORKTIME_EVENTS_LIMIT", 50_000).max(1000),
    }
}

fn utc_now() -> DateTime<Utc> {
    Utc::now()
}

fn pts(value: Option<&str>) -> Option<DateTime<Utc>> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }
    DateTime::parse_from_rfc3339(&value.replace('Z', "+00:00"))
        .map(|parsed| parsed.with_timezone(&Utc))
        .ok()
}

fn resolve_report_date(day: &str, offset: FixedOffset) -> Result<NaiveDate> {
    if let Ok(date) = NaiveDate::parse_from_str(day, "%Y-%m-%d") {
        return Ok(date);
    }
    let today = utc_now().with_timezone(&offset).date_naive();
    if day == "yesterday" {
        Ok(today - TimeDelta::days(1))
    } else {
        Ok(today)
    }
}

fn report_bounds(
    report_date: NaiveDate,
    offset: FixedOffset,
) -> Result<(DateTime<Utc>, DateTime<Utc>, DateTime<Utc>)> {
    let start_local = offset
        .with_ymd_and_hms(
            report_date.year(),
            report_date.month(),
            report_date.day(),
            0,
            0,
            0,
        )
        .single()
        .ok_or_else(|| anyhow!("invalid local report date"))?;
    let start = start_local.with_timezone(&Utc);
    let end_exclusive = (start_local + TimeDelta::days(1)).with_timezone(&Utc);
    let end_inclusive = end_exclusive - TimeDelta::seconds(1);
    Ok((start, end_inclusive, end_exclusive))
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

fn fetch_events_for_date(
    client: &Client,
    config: &Config,
    host: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Vec<Value> {
    let bucket_id = format!("aw-worktime-sessions_{host}");
    let bucket_url = format!(
        "{}/buckets/{}",
        config.aw_api_base,
        urlencoding::encode(&bucket_id)
    );
    if let Err(err) = get_json(client, &bucket_url) {
        eprintln!(
            "[aw-worktime-influx-exporter] bucket lookup failed for host={host} bucket={bucket_id}: {err}"
        );
        return Vec::new();
    }
    let query = format!(
        "start={}&end={}&limit={}",
        urlencoding::encode(&format_aw_time(start)),
        urlencoding::encode(&format_aw_time(end)),
        config.events_limit
    );
    let events_url = format!(
        "{}/buckets/{}/events?{}",
        config.aw_api_base,
        urlencoding::encode(&bucket_id),
        query
    );
    match get_json(client, &events_url) {
        Ok(payload) => payload.as_array().cloned().unwrap_or_default(),
        Err(err) => {
            eprintln!(
                "[aw-worktime-influx-exporter] events fetch failed for host={host} bucket={bucket_id}: {err}"
            );
            Vec::new()
        }
    }
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

fn is_machine_user(user: &str) -> bool {
    let user = user.trim().to_ascii_lowercase();
    user.ends_with('$') || matches!(user.as_str(), "system" | "localservice" | "networkservice")
}

fn is_active_sample(data: &serde_json::Map<String, Value>) -> bool {
    let state = s(data.get("state")).to_ascii_lowercase();
    if bool_value(data.get("active")) {
        return true;
    }
    if state.contains("актив") || state == "active" {
        return true;
    }
    if state == "unknown" {
        let session_id = int_value(data.get("sessionId"));
        let user = s(data.get("username"));
        let session_name = s(data.get("sessionName")).to_ascii_lowercase();
        return session_id > 0
            && !user.is_empty()
            && !is_machine_user(&user)
            && (session_name.starts_with("rdp-") || session_name == "console");
    }
    false
}

fn normalize_user_id(data: &serde_json::Map<String, Value>, host: &str, username: &str) -> String {
    let user_id = s(data.get("userId"));
    if !user_id.is_empty() {
        if let Some((_left, right)) = user_id.split_once('\\') {
            if !right.is_empty() {
                return format!("{host}\\{right}");
            }
        }
        return user_id;
    }
    format!("{host}\\{username}")
}

fn clamp_seconds(value: f64, config: &Config) -> f64 {
    let mut seconds = value;
    if seconds <= 0.0 || !seconds.is_finite() {
        seconds = config.default_sample_seconds;
    }
    seconds.min(config.max_sample_seconds)
}

fn event_sample_seconds(
    event: &Value,
    next_same_session_ts: Option<DateTime<Utc>>,
    event_ts: DateTime<Utc>,
    data: &serde_json::Map<String, Value>,
    config: &Config,
) -> f64 {
    for key in ["sampleSeconds", "pollSeconds"] {
        let value = data.get(key);
        let parsed = match value {
            Some(Value::Number(number)) => number.as_f64(),
            Some(Value::String(text)) => text.parse().ok(),
            _ => None,
        };
        if let Some(seconds) = parsed.filter(|seconds| *seconds > 0.0) {
            return clamp_seconds(seconds, config);
        }
    }
    let duration = match event.get("duration") {
        Some(Value::Number(number)) => number.as_f64().unwrap_or(0.0),
        Some(Value::String(text)) => text.parse().unwrap_or(0.0),
        _ => 0.0,
    };
    if duration > 0.0 {
        return clamp_seconds(duration, config);
    }
    if let Some(next_ts) = next_same_session_ts {
        let delta = (next_ts - event_ts).num_milliseconds() as f64 / 1000.0;
        if delta > 0.0 {
            return clamp_seconds(delta, config);
        }
    }
    clamp_seconds(config.default_sample_seconds, config)
}

fn merge_intervals(intervals: &[Interval]) -> Vec<Interval> {
    if intervals.is_empty() {
        return Vec::new();
    }
    let mut ordered = intervals.to_vec();
    ordered.sort_by_key(|item| item.start);
    let mut merged = vec![ordered[0].clone()];
    for interval in ordered.into_iter().skip(1) {
        let last = merged.last_mut().expect("merged interval exists");
        if interval.start <= last.end {
            if interval.end > last.end {
                last.end = interval.end;
            }
        } else {
            merged.push(interval);
        }
    }
    merged
}

fn collect_user_rows(
    events: &[Value],
    end_exclusive: DateTime<Utc>,
    host: &str,
    config: &Config,
) -> HashMap<String, UserAccumulator> {
    let mut by_identity: IdentitySamples = HashMap::new();
    for event in events {
        let Some(ts) = pts(event.get("timestamp").and_then(Value::as_str)) else {
            continue;
        };
        let Some(data) = event.get("data").and_then(Value::as_object) else {
            continue;
        };
        let username = s(data.get("username"));
        if username.is_empty() {
            continue;
        }
        let session_id = s(data.get("sessionId"));
        by_identity
            .entry((
                username,
                if session_id.is_empty() {
                    "unknown".to_string()
                } else {
                    session_id
                },
            ))
            .or_default()
            .push((ts, event.clone()));
    }

    let mut by_user: HashMap<String, UserAccumulator> = HashMap::new();
    for ((username, session_id), mut samples) in by_identity {
        samples.sort_by_key(|item| item.0);
        for idx in 0..samples.len() {
            let (event_ts, event) = &samples[idx];
            let Some(data) = event.get("data").and_then(Value::as_object) else {
                continue;
            };
            let next_ts = samples.get(idx + 1).map(|item| item.0);
            let active = is_active_sample(data);
            let sample_seconds = event_sample_seconds(event, next_ts, *event_ts, data, config);
            let row = by_user
                .entry(username.clone())
                .or_insert_with(|| UserAccumulator {
                    user: username.clone(),
                    user_id: normalize_user_id(data, host, &username),
                    samples_count: 0,
                    active_samples: 0,
                    session_ids: Vec::new(),
                    intervals: Vec::new(),
                });
            row.samples_count += 1;
            if !row.session_ids.contains(&session_id) {
                row.session_ids.push(session_id.clone());
            }
            if active {
                row.active_samples += 1;
                let interval_start = *event_ts;
                let interval_end = (*event_ts
                    + TimeDelta::milliseconds((sample_seconds * 1000.0) as i64))
                .min(end_exclusive);
                if interval_end > interval_start {
                    row.intervals.push(Interval {
                        start: interval_start,
                        end: interval_end,
                    });
                }
            }
        }
    }
    by_user
}

fn aggregate_daily_rows(
    events: &[Value],
    start: DateTime<Utc>,
    end_exclusive: DateTime<Utc>,
    host: &str,
    config: &Config,
) -> Vec<DailyRow> {
    let by_user = collect_user_rows(events, end_exclusive, host, config);
    let mut users: Vec<_> = by_user.keys().cloned().collect();
    users.sort();
    let full_range = (end_exclusive - start).num_seconds();
    let mut rows = Vec::new();
    for username in users {
        let row = &by_user[&username];
        let active_seconds: i64 = merge_intervals(&row.intervals)
            .iter()
            .map(|interval| (interval.end - interval.start).num_seconds())
            .sum::<i64>()
            .min(full_range);
        rows.push(DailyRow {
            user: row.user.clone(),
            user_id: row.user_id.clone(),
            active_seconds,
            idle_seconds: (full_range - active_seconds).max(0),
            sessions_count: row.session_ids.len() as i64,
            samples_count: row.samples_count,
            active_samples: row.active_samples,
        });
    }
    rows
}

fn aggregate_hourly_rows(
    events: &[Value],
    end_exclusive: DateTime<Utc>,
    host: &str,
    config: &Config,
) -> Vec<HourlyRow> {
    let by_user = collect_user_rows(events, end_exclusive, host, config);
    let mut users: Vec<_> = by_user.keys().cloned().collect();
    users.sort();
    let mut rows = Vec::new();
    for username in users {
        let row = &by_user[&username];
        let mut per_bucket: HashMap<DateTime<Utc>, i64> = HashMap::new();
        for interval in merge_intervals(&row.intervals) {
            let mut cursor = interval.start;
            while cursor < interval.end {
                let local = cursor.with_timezone(&config.report_offset);
                let bucket_local = config
                    .report_offset
                    .with_ymd_and_hms(local.year(), local.month(), local.day(), local.hour(), 0, 0)
                    .single()
                    .expect("valid hourly bucket");
                let bucket_start = bucket_local.with_timezone(&Utc);
                let bucket_end = (bucket_local + TimeDelta::hours(1)).with_timezone(&Utc);
                let overlap_start = interval.start.max(bucket_start);
                let overlap_end = interval.end.min(bucket_end);
                if overlap_end > overlap_start {
                    *per_bucket.entry(bucket_start).or_default() +=
                        (overlap_end - overlap_start).num_seconds();
                }
                cursor = bucket_end;
            }
        }
        let mut buckets: Vec<_> = per_bucket.into_iter().collect();
        buckets.sort_by_key(|item| item.0);
        for (bucket_start_utc, active_seconds) in buckets {
            if active_seconds <= 0 {
                continue;
            }
            let bucket_local = bucket_start_utc.with_timezone(&config.report_offset);
            rows.push(HourlyRow {
                user: row.user.clone(),
                user_id: row.user_id.clone(),
                bucket_start_utc,
                report_date: bucket_local.date_naive().to_string(),
                hour_local: format!("{:02}:00", bucket_local.hour()),
                active_seconds,
            });
        }
    }
    rows
}

fn fetch_true_active_apps(
    client: &Client,
    config: &Config,
    host: &str,
    day: &str,
    report_date: NaiveDate,
) -> Vec<Value> {
    let date_param = if matches!(day, "today" | "yesterday") {
        format!("day={}", urlencoding::encode(day))
    } else {
        format!("date={report_date}")
    };
    let url = format!(
        "{}/reports/worktime/today?host={}&{}&allow_stale=1",
        config.report_base,
        urlencoding::encode(host),
        date_param
    );
    match get_json(client, &url) {
        Ok(payload) => payload
            .get("true_active_apps")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        Err(err) => {
            eprintln!(
                "[aw-worktime-influx-exporter] true-active fetch failed for host={host} day={day}: {err}"
            );
            Vec::new()
        }
    }
}

fn build_report_summary(rows: &[DailyRow]) -> (i64, i64, String) {
    if rows.is_empty() {
        return (0, 0, String::new());
    }
    let total_active_seconds = rows.iter().map(|row| row.active_seconds).sum();
    let top_user = rows
        .iter()
        .max_by_key(|row| row.active_seconds)
        .map(|row| row.user.clone())
        .unwrap_or_default();
    (rows.len() as i64, total_active_seconds, top_user)
}

fn build_lines_for_day(
    client: &Client,
    config: &Config,
    host: &str,
    day: &str,
) -> Result<Vec<String>> {
    let report_date = resolve_report_date(day, config.report_offset)?;
    let (start, end_inclusive, end_exclusive) = report_bounds(report_date, config.report_offset)?;
    let daily_ts = timestamp_ns(start);
    let events = fetch_events_for_date(client, config, host, start, end_inclusive);
    let daily_rows = aggregate_daily_rows(&events, start, end_exclusive, host, config);
    let hourly_rows = aggregate_hourly_rows(&events, end_exclusive, host, config);
    let true_active_apps = fetch_true_active_apps(client, config, host, day, report_date);
    let mut lines = Vec::new();

    for row in &daily_rows {
        if let Some(line) = line(
            "aw_rdp_worktime_daily",
            vec![
                ("host", host.to_string()),
                ("user", row.user.clone()),
                ("user_id", row.user_id.clone()),
                ("report_date", report_date.to_string()),
            ],
            vec![
                ("active_seconds", FieldValue::Int(row.active_seconds)),
                ("idle_seconds", FieldValue::Int(row.idle_seconds)),
                ("sessions_count", FieldValue::Int(row.sessions_count)),
                ("samples_count", FieldValue::Int(row.samples_count)),
                ("active_samples", FieldValue::Int(row.active_samples)),
            ],
            daily_ts,
        ) {
            lines.push(line);
        }
    }

    for row in &hourly_rows {
        if let Some(line) = line(
            "aw_rdp_worktime_hourly",
            vec![
                ("host", host.to_string()),
                ("user", row.user.clone()),
                ("user_id", row.user_id.clone()),
                ("report_date", row.report_date.clone()),
                ("hour_local", row.hour_local.clone()),
            ],
            vec![("active_seconds", FieldValue::Int(row.active_seconds))],
            timestamp_ns(row.bucket_start_utc),
        ) {
            lines.push(line);
        }
    }

    let (users_count, total_active_seconds, top_user) = build_report_summary(&daily_rows);
    if let Some(line) = line(
        "aw_rdp_worktime_summary_daily",
        vec![
            ("host", host.to_string()),
            ("report_date", report_date.to_string()),
        ],
        vec![
            ("users_count", FieldValue::Int(users_count)),
            (
                "total_active_seconds",
                FieldValue::Int(total_active_seconds),
            ),
            ("top_user", FieldValue::String(top_user)),
        ],
        daily_ts,
    ) {
        lines.push(line);
    }

    for app in &true_active_apps {
        if let Some(line) = line(
            "aw_true_active_app_daily",
            vec![
                ("host", host.to_string()),
                ("application", s(app.get("application"))),
                ("report_date", report_date.to_string()),
            ],
            vec![
                (
                    "proved_work_seconds",
                    FieldValue::Int(int_value(app.get("proved_work_seconds"))),
                ),
                (
                    "evidence_events",
                    FieldValue::Int(int_value(app.get("evidence_events"))),
                ),
                (
                    "proved_work_human",
                    FieldValue::String(s(app.get("proved_work_human"))),
                ),
                (
                    "proved_work_hhmm",
                    FieldValue::String(s(app.get("proved_work_hhmm"))),
                ),
                ("last_action", FieldValue::String(s(app.get("last_action")))),
                (
                    "last_action_local",
                    FieldValue::String(s(app.get("last_action_local"))),
                ),
                (
                    "last_action_utc",
                    FieldValue::String(s(app.get("last_action_utc"))),
                ),
            ],
            daily_ts,
        ) {
            lines.push(line);
        }
    }

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
            days: config.days,
            lines: 0,
            written: 0,
            bucket: config.influx_bucket,
            error: None,
        });
    }
    let client = Client::builder()
        .timeout(Duration::from_secs(cli.timeout_seconds))
        .no_proxy()
        .build()
        .context("build HTTP client")?;
    let mut lines = Vec::new();
    for host in &config.hosts {
        for day in &config.days {
            lines.extend(build_lines_for_day(&client, &config, host, day)?);
        }
        if let Some(line) = line(
            "aw_worktime_exporter_heartbeat",
            vec![("host", host.to_string())],
            vec![("run", FieldValue::Int(1))],
            timestamp_ns(utc_now()),
        ) {
            lines.push(line);
        }
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
        days: config.days,
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
                eprintln!("[aw-worktime-influx-exporter] disabled by AW_WORKTIME_INFLUX_ENABLED");
            } else {
                eprintln!(
                    "[aw-worktime-influx-exporter] wrote {} points to {}",
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
                        days: Vec::new(),
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
            aw_api_base: "http://127.0.0.1:5600/api/0".to_string(),
            report_base: "http://127.0.0.1:5610".to_string(),
            influx_url: String::new(),
            influx_org: DEFAULT_INFLUX_ORG.to_string(),
            influx_bucket: DEFAULT_INFLUX_BUCKET.to_string(),
            influx_token: String::new(),
            influx_enabled: false,
            hosts: vec![DEFAULT_HOST.to_string()],
            days: vec!["today".to_string()],
            report_offset: FixedOffset::east_opt(3 * 3600).unwrap(),
            default_sample_seconds: 30.0,
            max_sample_seconds: 300.0,
            events_limit: 50_000,
        }
    }

    #[test]
    fn line_escapes_tags_and_string_fields() {
        let out = line(
            "m",
            vec![("host", "A B,C=D".to_string())],
            vec![("text", FieldValue::String("a\"b".to_string()))],
            10,
        )
        .unwrap();
        assert_eq!(out, "m,host=A\\ B\\,C\\=D text=\"a\\\"b\" 10");
    }

    #[test]
    fn heartbeat_line_uses_current_exporter_timestamp() {
        let out = line(
            "aw_worktime_exporter_heartbeat",
            vec![("host", "SHARKON2025".to_string())],
            vec![("run", FieldValue::Int(1))],
            42,
        )
        .unwrap();
        assert_eq!(
            out,
            "aw_worktime_exporter_heartbeat,host=SHARKON2025 run=1i 42"
        );
    }

    #[test]
    fn aggregates_daily_and_hourly_active_samples() {
        let config = test_config();
        let report_date = NaiveDate::from_ymd_opt(2026, 5, 14).unwrap();
        let (start, _end_inclusive, end_exclusive) =
            report_bounds(report_date, config.report_offset).unwrap();
        let events = vec![json!({
            "timestamp": "2026-05-14T06:00:00Z",
            "duration": 0.0,
            "data": {
                "username": "user5",
                "userId": "WORKGROUP\\user5",
                "sessionId": 4,
                "state": "Активно",
                "active": true,
                "sampleSeconds": 1800
            }
        })];

        let daily = aggregate_daily_rows(&events, start, end_exclusive, "SHARKON2025", &config);
        let hourly = aggregate_hourly_rows(&events, end_exclusive, "SHARKON2025", &config);

        assert_eq!(daily.len(), 1);
        assert_eq!(daily[0].user_id, "SHARKON2025\\user5");
        assert_eq!(daily[0].active_seconds, 300);
        assert_eq!(daily[0].active_samples, 1);
        assert_eq!(hourly.len(), 1);
        assert_eq!(hourly[0].report_date, "2026-05-14");
        assert_eq!(hourly[0].hour_local, "09:00");
        assert_eq!(hourly[0].active_seconds, 300);
    }

    #[test]
    fn resolves_moscow_report_bounds() {
        let offset = FixedOffset::east_opt(3 * 3600).unwrap();
        let report_date = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        let (start, end_inclusive, end_exclusive) = report_bounds(report_date, offset).unwrap();
        assert_eq!(format_aw_time(start), "2026-05-31T21:00:00Z");
        assert_eq!(format_aw_time(end_inclusive), "2026-06-01T20:59:59Z");
        assert_eq!(format_aw_time(end_exclusive), "2026-06-01T21:00:00Z");
    }
}
