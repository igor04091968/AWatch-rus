use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow};
use chrono::{
    DateTime, Datelike, FixedOffset, Local, NaiveDate, NaiveTime, SecondsFormat, TimeDelta,
    TimeZone, Utc,
};
use clap::Parser;
use reqwest::{
    blocking::Client,
    header::{CONNECTION, HeaderMap, HeaderValue},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};
use url::form_urlencoded;

const DEFAULT_AW_URL: &str = "http://127.0.0.1:5600";
const DEFAULT_HOST: &str = "HOST-EXAMPLE";

#[derive(Debug, Parser)]
#[command(about = "AW Worktime Report API")]
struct Cli {
    #[arg(long)]
    once: bool,
}

#[derive(Debug, Clone)]
struct Config {
    aw_api_base: String,
    ioc_dir: PathBuf,
    default_host: String,
    default_sample_seconds: f64,
    max_sample_seconds: f64,
    listen_host: String,
    listen_port: u16,
    workday_start_hour: u32,
    workday_end_hour: u32,
    manager_target_coverage_pct: i64,
    manager_low_coverage_pct: i64,
    manager_overload_coverage_pct: i64,
    manager_trend_min_points: usize,
    manager_trend_delta_pct: f64,
    manager_off_hours_threshold_seconds: i64,
    manager_night_work_after: Option<NaiveTime>,
    manager_weekend_work_enabled: bool,
    manager_interpretation_policy_configured: bool,
    manager_late_start_grace_minutes: i64,
    manager_early_finish_grace_minutes: i64,
    manager_critical_source_max_age_seconds: i64,
    manager_web_source_max_age_seconds: i64,
    manager_session_source_max_age_seconds: i64,
    manager_infra_source_max_age_seconds: i64,
    manager_aliases_json: PathBuf,
    manager_exclude_users: BTreeSet<String>,
    events_cache_ttl_seconds: i64,
    worktime_events_limit: usize,
    aw_http_timeout_seconds: f64,
    source_http_timeout_seconds: f64,
    report_cache_ttl_seconds: i64,
    report_stale_ttl_seconds: i64,
    report_disk_cache_dir: PathBuf,
    report_disk_stale_ttl_seconds: i64,
    management_history_dir: PathBuf,
    management_history_days: usize,
    management_history_retention_days: i64,
    true_active_evidence_window_seconds: i64,
    true_active_max_event_seconds: i64,
    offset: FixedOffset,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct AwEvent {
    timestamp: Option<String>,
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    data: Value,
}

#[derive(Debug, Clone)]
struct UserBucket {
    user: String,
    user_id: String,
    samples_count: i64,
    active_samples: i64,
    session_ids: BTreeSet<String>,
    intervals: Vec<(DateTime<Utc>, DateTime<Utc>)>,
}

#[derive(Debug, Clone)]
struct AliasProfile {
    display_name: String,
    manager_owner: String,
    department: String,
    role: String,
    notes: String,
    canonical_user_id: String,
    exclude: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct InterpretationPolicy {
    #[serde(default)]
    overload_threshold: Option<f64>,
    #[serde(default)]
    underload_threshold: Option<f64>,
    #[serde(default)]
    drop_threshold_pct: Option<f64>,
    #[serde(default)]
    night_work_after: Option<String>,
    #[serde(default)]
    weekend_work: Option<bool>,
    #[serde(default)]
    min_trend_points: Option<usize>,
    #[serde(default)]
    off_hours_threshold_seconds: Option<i64>,
}

#[derive(Debug, Clone)]
struct CachedResponse {
    stored: Instant,
    data: Vec<u8>,
    content_type: String,
}

type EventsCache = Arc<Mutex<HashMap<String, (Instant, Vec<AwEvent>)>>>;
type ReportCache = Arc<Mutex<HashMap<String, CachedResponse>>>;
type IdentitySamples = BTreeMap<(String, String), Vec<(DateTime<Utc>, AwEvent)>>;
type DateBounds = (DateTime<Utc>, DateTime<Utc>);
type EventsForDate = (DateBounds, Vec<AwEvent>);

#[derive(Clone)]
struct App {
    config: Arc<Config>,
    aw: Client,
    events_cache: EventsCache,
    report_cache: ReportCache,
}

fn env(name: &str, fallback: &str) -> String {
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
        .filter(|value| *value > 0.0)
        .unwrap_or(fallback)
}

fn env_i64(name: &str, fallback: i64) -> i64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(fallback)
}

fn env_usize(name: &str, fallback: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}

fn env_bool(name: &str, fallback: bool) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_lowercase())
        .and_then(|value| match value.as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        })
        .unwrap_or(fallback)
}

fn build_aw_api_base(raw_url: &str) -> String {
    let url = raw_url.trim().trim_end_matches('/');
    if url.ends_with("/api/0") {
        url.to_string()
    } else {
        format!("{url}/api/0")
    }
}

fn load_interpretation_policy(path: &Path) -> Option<InterpretationPolicy> {
    if !path.exists() {
        return None;
    }
    serde_json::from_str::<InterpretationPolicy>(&fs::read_to_string(path).ok()?).ok()
}

fn threshold_to_pct(value: f64) -> f64 {
    if (0.0..=1.0).contains(&value) {
        value * 100.0
    } else {
        value
    }
}

fn parse_hhmm(value: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(value.trim(), "%H:%M").ok()
}

fn load_config() -> Config {
    let default_sample_seconds = env_f64("AW_WORKTIME_DEFAULT_SAMPLE_SECONDS", 30.0).max(1.0);
    let max_sample_seconds =
        env_f64("AW_WORKTIME_MAX_SAMPLE_SECONDS", 300.0).max(default_sample_seconds);
    let manager_interpretation_policy_path = PathBuf::from(env(
        "AW_WORKTIME_MANAGER_INTERPRETATION_POLICY",
        "/etc/activitywatch/worktime-interpretation-policy.json",
    ));
    let interpretation_policy = load_interpretation_policy(&manager_interpretation_policy_path);
    let manager_target_coverage_pct =
        env_i64("AW_WORKTIME_MANAGER_TARGET_COVERAGE_PCT", 75).clamp(1, 100);
    let mut manager_low_coverage_pct =
        env_i64("AW_WORKTIME_MANAGER_LOW_COVERAGE_PCT", 35).clamp(1, 100);
    let mut manager_overload_coverage_pct =
        env_i64("AW_WORKTIME_MANAGER_OVERLOAD_COVERAGE_PCT", 115).clamp(100, 300);
    let mut manager_trend_min_points =
        env_usize("AW_WORKTIME_MANAGER_TREND_MIN_POINTS", 3).clamp(2, 31);
    let mut manager_trend_delta_pct =
        env_f64("AW_WORKTIME_MANAGER_TREND_DELTA_PCT", 10.0).clamp(1.0, 100.0);
    let mut manager_off_hours_threshold_seconds =
        env_i64("AW_WORKTIME_MANAGER_OFF_HOURS_THRESHOLD_SECONDS", 1800).max(60);
    let mut manager_night_work_after =
        parse_hhmm(&env("AW_WORKTIME_MANAGER_NIGHT_WORK_AFTER", "20:00"));
    let mut manager_weekend_work_enabled =
        env_bool("AW_WORKTIME_MANAGER_WEEKEND_WORK_ENABLED", true);
    if let Some(policy) = interpretation_policy.as_ref() {
        if let Some(value) = policy.underload_threshold {
            manager_low_coverage_pct = threshold_to_pct(value).round().clamp(1.0, 100.0) as i64;
        }
        if let Some(value) = policy.overload_threshold {
            manager_overload_coverage_pct =
                threshold_to_pct(value).round().clamp(1.0, 300.0) as i64;
        }
        if let Some(value) = policy.drop_threshold_pct {
            manager_trend_delta_pct = threshold_to_pct(value).clamp(1.0, 100.0);
        }
        if let Some(value) = policy.min_trend_points {
            manager_trend_min_points = value.clamp(2, 31);
        }
        if let Some(value) = policy.off_hours_threshold_seconds {
            manager_off_hours_threshold_seconds = value.max(60);
        }
        if let Some(value) = policy.night_work_after.as_deref().and_then(parse_hhmm) {
            manager_night_work_after = Some(value);
        }
        if let Some(value) = policy.weekend_work {
            manager_weekend_work_enabled = value;
        }
    }
    let report_cache_ttl_seconds = env_i64("AW_WORKTIME_REPORT_CACHE_TTL_SECONDS", 60).max(0);
    let report_stale_ttl_seconds =
        env_i64("AW_WORKTIME_REPORT_STALE_TTL_SECONDS", 900).max(report_cache_ttl_seconds);
    Config {
        aw_api_base: build_aw_api_base(&env("AW_SERVER_URL", DEFAULT_AW_URL)),
        ioc_dir: PathBuf::from(env("AW_DLP_IOC_DIR", "/opt/activitywatch/dlp-ioc/output")),
        default_host: env("AW_WORKTIME_HOST", DEFAULT_HOST),
        default_sample_seconds,
        max_sample_seconds,
        listen_host: env("AW_WORKTIME_LISTEN_HOST", "0.0.0.0"),
        listen_port: env("AW_WORKTIME_PORT", "5610").parse().unwrap_or(5610),
        workday_start_hour: env_i64("AW_WORKTIME_MANAGER_START_HOUR", 9).clamp(0, 23) as u32,
        workday_end_hour: env_i64("AW_WORKTIME_MANAGER_END_HOUR", 18).clamp(0, 23) as u32,
        manager_target_coverage_pct,
        manager_low_coverage_pct,
        manager_overload_coverage_pct,
        manager_trend_min_points,
        manager_trend_delta_pct,
        manager_off_hours_threshold_seconds,
        manager_night_work_after,
        manager_weekend_work_enabled,
        manager_interpretation_policy_configured: interpretation_policy.is_some(),
        manager_late_start_grace_minutes: env_i64(
            "AW_WORKTIME_MANAGER_LATE_START_GRACE_MINUTES",
            60,
        )
        .max(0),
        manager_early_finish_grace_minutes: env_i64(
            "AW_WORKTIME_MANAGER_EARLY_FINISH_GRACE_MINUTES",
            90,
        )
        .max(0),
        manager_critical_source_max_age_seconds: env_i64(
            "AW_WORKTIME_MANAGER_CRITICAL_SOURCE_MAX_AGE_SECONDS",
            900,
        )
        .max(60),
        manager_web_source_max_age_seconds: env_i64(
            "AW_WORKTIME_MANAGER_WEB_SOURCE_MAX_AGE_SECONDS",
            259200,
        )
        .max(3600),
        manager_session_source_max_age_seconds: env_i64(
            "AW_WORKTIME_MANAGER_SESSION_SOURCE_MAX_AGE_SECONDS",
            604800,
        )
        .max(3600),
        manager_infra_source_max_age_seconds: env_i64(
            "AW_WORKTIME_MANAGER_INFRA_SOURCE_MAX_AGE_SECONDS",
            172800,
        )
        .max(3600),
        manager_aliases_json: PathBuf::from(env(
            "AW_WORKTIME_MANAGER_ALIASES_JSON",
            "/etc/activitywatch/worktime-manager-aliases.json",
        )),
        manager_exclude_users: env("AW_WORKTIME_MANAGER_EXCLUDE_USERS", "")
            .split(',')
            .map(|item| item.trim().to_lowercase())
            .filter(|item| !item.is_empty())
            .collect(),
        events_cache_ttl_seconds: env_i64("AW_WORKTIME_EVENTS_CACHE_TTL_SECONDS", 30).max(0),
        worktime_events_limit: env_usize("AW_WORKTIME_EVENTS_LIMIT", 50_000).clamp(100, 50_000),
        aw_http_timeout_seconds: env_f64("AW_WORKTIME_AW_HTTP_TIMEOUT_SECONDS", 8.0).max(0.5),
        source_http_timeout_seconds: env_f64("AW_WORKTIME_SOURCE_HTTP_TIMEOUT_SECONDS", 1.5)
            .max(0.25),
        report_cache_ttl_seconds,
        report_stale_ttl_seconds,
        report_disk_cache_dir: PathBuf::from(env(
            "AW_WORKTIME_REPORT_DISK_CACHE_DIR",
            "/var/lib/activitywatch/worktime-report-cache",
        )),
        report_disk_stale_ttl_seconds: env_i64("AW_WORKTIME_REPORT_DISK_STALE_TTL_SECONDS", 86400)
            .max(report_stale_ttl_seconds),
        management_history_dir: PathBuf::from(env(
            "AW_WORKTIME_MANAGEMENT_HISTORY_DIR",
            "/var/lib/activitywatch/worktime-management-history",
        )),
        management_history_days: env_usize("AW_WORKTIME_MANAGEMENT_HISTORY_DAYS", 31).clamp(1, 366),
        management_history_retention_days: env_i64(
            "AW_WORKTIME_MANAGEMENT_HISTORY_RETENTION_DAYS",
            120,
        )
        .clamp(1, 3660),
        true_active_evidence_window_seconds: env_i64(
            "AW_WORKTIME_TRUE_ACTIVE_EVIDENCE_WINDOW_SECONDS",
            180,
        )
        .max(30),
        true_active_max_event_seconds: env_i64("AW_WORKTIME_TRUE_ACTIVE_MAX_EVENT_SECONDS", 600)
            .max(30),
        offset: FixedOffset::east_opt(3 * 3600).expect("valid Moscow offset"),
    }
}

impl App {
    fn new(config: Config) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(CONNECTION, HeaderValue::from_static("close"));
        let aw = Client::builder()
            .timeout(Duration::from_secs_f64(config.aw_http_timeout_seconds))
            .no_proxy()
            .pool_max_idle_per_host(0)
            .default_headers(headers)
            .build()
            .context("build AW HTTP client")?;
        Ok(Self {
            config: Arc::new(config),
            aw,
            events_cache: Arc::new(Mutex::new(HashMap::new())),
            report_cache: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    fn aw_get_json_once(&self, path: &str) -> Result<Value> {
        self.aw_get_json_with(path, 1, None)
    }

    fn aw_get_json_with(
        &self,
        path: &str,
        attempts: usize,
        timeout: Option<Duration>,
    ) -> Result<Value> {
        let url = format!("{}{}", self.config.aw_api_base, path);
        let mut last_error = None;
        for attempt in 1..=attempts.max(1) {
            let mut request = self.aw.get(&url);
            if let Some(timeout) = timeout {
                request = request.timeout(timeout);
            }
            match request.send() {
                Ok(response) => {
                    if !response.status().is_success() {
                        return Err(anyhow!("AW {path} returned HTTP {}", response.status()));
                    }
                    return response
                        .json()
                        .with_context(|| format!("decode AW JSON {path}"));
                }
                Err(error) => {
                    let is_timeout = error.is_timeout();
                    last_error = Some(error);
                    if is_timeout {
                        break;
                    }
                    if attempt < attempts {
                        std::thread::sleep(Duration::from_millis(250));
                    }
                }
            }
        }
        Err(last_error.expect("request failed")).with_context(|| format!("request AW {path}"))
    }

    fn fetch_bucket_events(
        &self,
        bucket_id: &str,
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
    ) -> Vec<AwEvent> {
        match self.fetch_bucket_events_result(bucket_id, start, end) {
            Ok(events) => events,
            Err(error) => {
                eprintln!(
                    "[aw-worktime-api-rust] events fetch failed bucket={bucket_id}: {error:#}"
                );
                Vec::new()
            }
        }
    }

    fn fetch_bucket_events_result(
        &self,
        bucket_id: &str,
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
    ) -> Result<Vec<AwEvent>> {
        let mut key = bucket_id.to_string();
        let query_limit = self.config.worktime_events_limit.min(5_000);
        let params = [format!("limit={query_limit}")];
        if let (Some(start), Some(end)) = (start, end) {
            let s = to_iso_utc(start);
            let e = to_iso_utc(end);
            key = format!("{bucket_id}|{s}|{e}");
        }
        if self.config.events_cache_ttl_seconds > 0 {
            if let Some((stored, events)) = self
                .events_cache
                .lock()
                .ok()
                .and_then(|cache| cache.get(&key).cloned())
            {
                if stored.elapsed().as_secs() <= self.config.events_cache_ttl_seconds as u64 {
                    return Ok(events);
                }
            }
        }
        let path = format!("/buckets/{bucket_id}/events?{}", params.join("&"));
        let mut events: Vec<AwEvent> = serde_json::from_value(self.aw_get_json_once(&path)?)
            .with_context(|| format!("decode AW events for {bucket_id}"))?;
        if let (Some(start), Some(end)) = (start, end) {
            events.retain(|event| {
                event
                    .timestamp
                    .as_deref()
                    .and_then(parse_iso_utc)
                    .is_some_and(|ts| ts >= start && ts <= end)
            });
        }
        if self.config.events_cache_ttl_seconds > 0 {
            if let Ok(mut cache) = self.events_cache.lock() {
                cache.insert(key, (Instant::now(), events.clone()));
            }
        }
        Ok(events)
    }

    fn latest_bucket_event(&self, bucket_id: &str) -> Option<AwEvent> {
        let bucket = self
            .aw_get_json_with(
                &format!("/buckets/{bucket_id}"),
                1,
                Some(Duration::from_secs_f64(
                    self.config.source_http_timeout_seconds,
                )),
            )
            .ok()?;
        let end = bucket.pointer("/metadata/end").and_then(Value::as_str)?;
        Some(AwEvent {
            timestamp: Some(end.to_string()),
            duration: Some(0.0),
            data: json!({"source": "bucket_metadata"}),
        })
    }

    fn report_response(
        &self,
        path: &str,
        params: &Params,
        accept: &str,
    ) -> (Vec<u8>, String, Vec<(String, String)>) {
        let fmt = resolve_report_format(params, accept);
        let host = self.resolve_report_host(params.first("host").as_deref());
        let report_date = resolve_report_date(
            &self.config,
            params.first("day").as_deref(),
            params.first("date").as_deref(),
        );
        let owner = normalize_filter(params.first("owner").as_deref().unwrap_or(""));
        let department = normalize_filter(params.first("department").as_deref().unwrap_or(""));
        let cache_key = make_report_cache_key(
            path,
            &fmt,
            &host,
            report_date,
            params.first("day").as_deref().unwrap_or(""),
            &owner,
            &department,
        );
        if let Some(cached) = self.get_report_cache(&cache_key, false) {
            return (
                cached.data,
                cached.content_type,
                vec![
                    ("X-AW-Worktime-Cache".into(), "fresh".into()),
                    ("X-AW-Worktime-Cache-Reason".into(), "ttl".into()),
                ],
            );
        }
        let built =
            self.build_report_response(path, params, &fmt, &host, report_date, &owner, &department);
        match built {
            Ok((data, content_type)) => {
                self.save_report_cache(cache_key, data.clone(), content_type.clone());
                (data, content_type, Vec::new())
            }
            Err(error) => {
                eprintln!("[aw-worktime-api-rust] report build failed path={path}: {error:#}");
                if let Some(cached) = self.get_report_cache(&cache_key, true) {
                    return (
                        cached.data,
                        cached.content_type,
                        vec![
                            ("X-AW-Worktime-Cache".into(), "stale".into()),
                            ("X-AW-Worktime-Cache-Reason".into(), "build-error".into()),
                        ],
                    );
                }
                let data = serde_json::to_vec_pretty(&json!({
                    "ok": false,
                    "error": "report_unavailable",
                    "message": "report build failed and no cached response is available",
                    "generated_at_utc": to_iso_utc(Utc::now()),
                }))
                .unwrap_or_default();
                (
                    data,
                    "application/json; charset=utf-8".to_string(),
                    Vec::new(),
                )
            }
        }
    }

    fn get_report_cache(&self, key: &str, allow_stale: bool) -> Option<CachedResponse> {
        let cached = self.report_cache.lock().ok()?.get(key).cloned();
        if let Some(cached) = cached {
            let max_age = if allow_stale {
                self.config.report_stale_ttl_seconds
            } else {
                self.config.report_cache_ttl_seconds
            };
            if max_age > 0 && cached.stored.elapsed().as_secs() <= max_age as u64 {
                return Some(cached);
            }
        }
        if allow_stale {
            return self.load_disk_cache(key);
        }
        None
    }

    fn save_report_cache(&self, key: String, data: Vec<u8>, content_type: String) {
        if self.config.report_stale_ttl_seconds <= 0 {
            return;
        }
        if let Ok(mut cache) = self.report_cache.lock() {
            cache.insert(
                key.clone(),
                CachedResponse {
                    stored: Instant::now(),
                    data: data.clone(),
                    content_type: content_type.clone(),
                },
            );
        }
        self.save_disk_cache(&key, &data, &content_type);
    }

    fn disk_cache_path(&self, key: &str) -> PathBuf {
        let mut h: u64 = 1469598103934665603;
        for b in key.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(1099511628211);
        }
        self.config
            .report_disk_cache_dir
            .join(format!("{h:016x}.json"))
    }

    fn save_disk_cache(&self, key: &str, data: &[u8], content_type: &str) {
        if self.config.report_disk_stale_ttl_seconds <= 0 {
            return;
        }
        let path = self.disk_cache_path(key);
        let _ = fs::create_dir_all(&self.config.report_disk_cache_dir);
        let payload = json!({
            "stored_epoch": SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0),
            "content_type": content_type,
            "data": String::from_utf8_lossy(data),
        });
        let _ = fs::write(path, serde_json::to_vec(&payload).unwrap_or_default());
    }

    fn load_disk_cache(&self, key: &str) -> Option<CachedResponse> {
        let path = self.disk_cache_path(key);
        let payload: Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
        let stored_epoch = payload.get("stored_epoch")?.as_f64()?;
        let now_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_secs_f64();
        if now_epoch - stored_epoch > self.config.report_disk_stale_ttl_seconds as f64 {
            return None;
        }
        Some(CachedResponse {
            stored: Instant::now() - Duration::from_secs_f64((now_epoch - stored_epoch).max(0.0)),
            data: payload.get("data")?.as_str()?.as_bytes().to_vec(),
            content_type: payload.get("content_type")?.as_str()?.to_string(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn build_report_response(
        &self,
        path: &str,
        params: &Params,
        fmt: &str,
        host: &str,
        report_date: NaiveDate,
        owner: &str,
        department: &str,
    ) -> Result<(Vec<u8>, String)> {
        let is_management = path == "/reports/worktime/management";
        if is_management {
            let payload = self.management_report_for_date(host, report_date, owner, department)?;
            return Ok(match fmt {
                "csv" => (
                    management_csv(&payload).into_bytes(),
                    "text/csv; charset=utf-8".to_string(),
                ),
                "html" => (
                    render_management_html(&payload).into_bytes(),
                    "text/html; charset=utf-8".to_string(),
                ),
                _ => (
                    serde_json::to_vec_pretty(&payload)?,
                    "application/json; charset=utf-8".to_string(),
                ),
            });
        }

        let (bounds, events) = self.fetch_events_for_date(host, report_date)?;
        let rows = aggregate_rows(&self.config, &events, bounds.0, bounds.1, host, false);
        let total_active_seconds = rows
            .iter()
            .map(|row| {
                row.get("active_seconds")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
            })
            .sum::<i64>();
        let true_active_apps = if total_active_seconds > 0 {
            self.build_true_active_apps(host, report_date)
        } else {
            Vec::new()
        };
        let day = params.first("day");
        Ok(match fmt {
            "csv" => (
                today_csv(&rows).into_bytes(),
                "text/csv; charset=utf-8".to_string(),
            ),
            "html" => (
                render_today_html(
                    &self.config,
                    &rows,
                    host,
                    report_date,
                    day.as_deref(),
                    &true_active_apps,
                )
                .into_bytes(),
                "text/html; charset=utf-8".to_string(),
            ),
            _ => {
                let payload = json!({
                    "generated_at_utc": to_iso_utc(Utc::now()),
                    "report_timezone": "Europe/Moscow",
                    "host": host,
                    "report_date": report_date.to_string(),
                    "bucket_id": sessions_bucket(host),
                    "rows": rows,
                    "true_active_apps": true_active_apps,
                });
                (
                    serde_json::to_vec_pretty(&payload)?,
                    "application/json; charset=utf-8".to_string(),
                )
            }
        })
    }

    fn fetch_events_for_date(&self, host: &str, report_date: NaiveDate) -> Result<EventsForDate> {
        let bounds = report_bounds(&self.config, report_date);
        let events = self.fetch_bucket_events_result(
            &sessions_bucket(host),
            Some(bounds.0),
            Some(bounds.1),
        )?;
        Ok((bounds, events))
    }

    fn resolve_report_host(&self, host: Option<&str>) -> String {
        let configured = resolve_host(&self.config, host);
        if !configured.is_empty() && configured != DEFAULT_HOST {
            return configured;
        }
        self.detect_worktime_host()
            .unwrap_or_else(|| configured_if_nonempty(&configured))
    }

    fn detect_worktime_host(&self) -> Option<String> {
        let payload = self.aw_get_json_once("/buckets").ok()?;
        host_from_buckets_payload(&payload)
    }

    fn management_report_for_date(
        &self,
        host: &str,
        report_date: NaiveDate,
        owner: &str,
        department: &str,
    ) -> Result<Value> {
        let (bounds, events) = self.fetch_events_for_date(host, report_date)?;
        let rows = aggregate_rows(&self.config, &events, bounds.0, bounds.1, host, true);
        Ok(self.build_management_payload(rows, host, report_date, owner, department))
    }

    fn build_true_active_apps(&self, host: &str, report_date: NaiveDate) -> Vec<Value> {
        let bounds = report_bounds(&self.config, report_date);
        let window_events = {
            let primary = self.fetch_bucket_events(
                &format!("aw-watcher-window_{host}"),
                Some(bounds.0),
                Some(bounds.1),
            );
            if primary.is_empty() {
                self.fetch_bucket_events(
                    &format!("aw-rdp-window_{host}"),
                    Some(bounds.0),
                    Some(bounds.1),
                )
            } else {
                primary
            }
        };
        let afk_events = {
            let primary = self.fetch_bucket_events(
                &format!("aw-watcher-afk_{host}"),
                Some(bounds.0),
                Some(bounds.1),
            );
            if primary.is_empty() {
                self.fetch_bucket_events(
                    &format!("aw-rdp-afk_{host}"),
                    Some(bounds.0),
                    Some(bounds.1),
                )
            } else {
                primary
            }
        };
        let mut evidence = HashMap::new();
        for bucket in [
            format!("aw-file-operations_{host}"),
            format!("aw-dlp-endpoint-signals_{host}"),
            format!("aw-watcher-web-chrome_{host}"),
            format!("aw-watcher-web-edge_{host}"),
            format!("aw-detmir-web-category_{host}"),
        ] {
            evidence.insert(
                bucket.clone(),
                self.fetch_bucket_events(&bucket, Some(bounds.0), Some(bounds.1)),
            );
        }
        build_true_active_apps_from_events(
            &self.config,
            &window_events,
            &afk_events,
            &evidence,
            bounds.0,
            bounds.1,
        )
    }
}

#[derive(Debug, Clone, Default)]
struct Params(HashMap<String, Vec<String>>);

impl Params {
    fn parse(query: &str) -> Self {
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        for (k, v) in form_urlencoded::parse(query.as_bytes()) {
            if !v.is_empty() {
                map.entry(k.into_owned()).or_default().push(v.into_owned());
            }
        }
        Self(map)
    }
    fn first(&self, name: &str) -> Option<String> {
        self.0.get(name).and_then(|v| v.first()).cloned()
    }
}

fn parse_iso_utc(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
}

fn to_iso_utc(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn hhmm(seconds: i64) -> String {
    let seconds = seconds.max(0);
    format!("{:02}:{:02}", seconds / 3600, (seconds % 3600) / 60)
}

fn human_duration_ru(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let h = seconds / 3600;
    let m = (seconds % 3600) / 60;
    if h > 0 && m > 0 {
        format!("{h} ч {m} мин")
    } else if h > 0 {
        format!("{h} ч")
    } else if m > 0 {
        format!("{m} мин")
    } else {
        format!("{seconds} сек")
    }
}

fn resolve_host(config: &Config, host: Option<&str>) -> String {
    let h = host.unwrap_or(&config.default_host).trim();
    if h.is_empty() {
        config.default_host.clone()
    } else {
        h.to_string()
    }
}

fn configured_if_nonempty(value: &str) -> String {
    if value.trim().is_empty() {
        DEFAULT_HOST.to_string()
    } else {
        value.to_string()
    }
}

fn host_from_buckets_payload(payload: &Value) -> Option<String> {
    let mut hosts = BTreeSet::new();
    if let Some(map) = payload.as_object() {
        for key in map.keys() {
            collect_worktime_host_from_bucket_id(key, &mut hosts);
        }
    }
    if let Some(items) = payload.as_array() {
        for item in items {
            if let Some(id) = item.as_str() {
                collect_worktime_host_from_bucket_id(id, &mut hosts);
            }
            if let Some(id) = item.get("id").and_then(Value::as_str) {
                collect_worktime_host_from_bucket_id(id, &mut hosts);
            }
            if let Some(id) = item.get("name").and_then(Value::as_str) {
                collect_worktime_host_from_bucket_id(id, &mut hosts);
            }
        }
    }
    hosts.into_iter().find(|host| host != DEFAULT_HOST)
}

fn collect_worktime_host_from_bucket_id(bucket_id: &str, hosts: &mut BTreeSet<String>) {
    let Some(host) = bucket_id.strip_prefix("aw-worktime-sessions_") else {
        return;
    };
    let host = host.trim();
    if !host.is_empty() {
        hosts.insert(host.to_string());
    }
}

fn sessions_bucket(host: &str) -> String {
    format!("aw-worktime-sessions_{host}")
}

fn resolve_report_date(config: &Config, day: Option<&str>, date: Option<&str>) -> NaiveDate {
    if let Some(date) = date.and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()) {
        return date;
    }
    let today = Utc::now().with_timezone(&config.offset).date_naive();
    if day == Some("yesterday") {
        today - TimeDelta::days(1)
    } else {
        today
    }
}

fn report_bounds(config: &Config, report_date: NaiveDate) -> (DateTime<Utc>, DateTime<Utc>) {
    let start_local = config
        .offset
        .with_ymd_and_hms(
            report_date.year(),
            report_date.month(),
            report_date.day(),
            0,
            0,
            0,
        )
        .single()
        .unwrap();
    let start = start_local.with_timezone(&Utc);
    let end = (start_local + TimeDelta::days(1) - TimeDelta::seconds(1)).with_timezone(&Utc);
    (start, end)
}

fn workday_bounds(
    config: &Config,
    report_date: NaiveDate,
) -> (DateTime<FixedOffset>, DateTime<FixedOffset>, i64) {
    let start = config
        .offset
        .with_ymd_and_hms(
            report_date.year(),
            report_date.month(),
            report_date.day(),
            config.workday_start_hour,
            0,
            0,
        )
        .single()
        .unwrap();
    let mut end = config
        .offset
        .with_ymd_and_hms(
            report_date.year(),
            report_date.month(),
            report_date.day(),
            config.workday_end_hour,
            0,
            0,
        )
        .single()
        .unwrap();
    if end <= start {
        end = start + TimeDelta::hours(8);
    }
    let duration = (end - start).num_seconds();
    (start, end, duration)
}

fn value_string(data: &Value, key: &str) -> String {
    match data.get(key) {
        Some(Value::String(value)) => value.trim().to_string(),
        Some(Value::Number(value)) => value.to_string(),
        Some(Value::Bool(value)) => value.to_string(),
        Some(Value::Null) | None => String::new(),
        Some(value) => value.to_string().trim_matches('"').trim().to_string(),
    }
}

fn is_machine_user(user: &str) -> bool {
    let u = user.trim().to_lowercase();
    u.ends_with('$') || matches!(u.as_str(), "system" | "localservice" | "networkservice")
}

fn is_active_sample(data: &Value) -> bool {
    if data.get("active").and_then(Value::as_bool).unwrap_or(false) {
        return true;
    }
    let state = value_string(data, "state").to_lowercase();
    if state.contains("актив") || state == "active" {
        return true;
    }
    if state == "unknown" {
        let sid = value_string(data, "sessionId").parse::<i64>().unwrap_or(-1);
        let user = value_string(data, "username");
        let session_name = value_string(data, "sessionName").to_lowercase();
        return sid > 0
            && !user.is_empty()
            && !is_machine_user(&user)
            && (session_name.starts_with("rdp-") || session_name == "console");
    }
    false
}

fn normalize_user_id(data: &Value, host: &str, username: &str) -> String {
    let user_id = value_string(data, "userId");
    if !user_id.is_empty() {
        if let Some((_, right)) = user_id.split_once('\\') {
            return format!("{host}\\{right}");
        }
        return user_id;
    }
    format!("{host}\\{username}")
}

fn clamp_seconds(config: &Config, value: f64, fallback: f64) -> f64 {
    let seconds = if value > 0.0 { value } else { fallback };
    seconds.min(config.max_sample_seconds).max(1.0)
}

fn event_sample_seconds(
    config: &Config,
    event: &AwEvent,
    ts: DateTime<Utc>,
    next_ts: Option<DateTime<Utc>>,
) -> f64 {
    for key in ["sampleSeconds", "pollSeconds"] {
        let value = event.data.get(key).and_then(|v| {
            if let Some(n) = v.as_f64() {
                Some(n)
            } else {
                v.as_str().and_then(|s| s.parse().ok())
            }
        });
        if let Some(value) = value.filter(|v| *v > 0.0) {
            return clamp_seconds(config, value, config.default_sample_seconds);
        }
    }
    if let Some(duration) = event.duration.filter(|v| *v > 0.0) {
        return clamp_seconds(config, duration, config.default_sample_seconds);
    }
    if let Some(next_ts) = next_ts {
        let delta = (next_ts - ts).num_milliseconds() as f64 / 1000.0;
        if delta > 0.0 {
            return clamp_seconds(config, delta, config.default_sample_seconds);
        }
    }
    clamp_seconds(
        config,
        config.default_sample_seconds,
        config.default_sample_seconds,
    )
}

fn merge_intervals(
    mut intervals: Vec<(DateTime<Utc>, DateTime<Utc>)>,
) -> Vec<(DateTime<Utc>, DateTime<Utc>)> {
    intervals.sort_by_key(|i| i.0);
    let mut merged: Vec<(DateTime<Utc>, DateTime<Utc>)> = Vec::new();
    for (start, end) in intervals {
        if let Some(last) = merged.last_mut() {
            if start <= last.1 {
                if end > last.1 {
                    last.1 = end;
                }
                continue;
            }
        }
        merged.push((start, end));
    }
    merged
}

fn aggregate_rows(
    config: &Config,
    events: &[AwEvent],
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    host: &str,
    intervals: bool,
) -> Vec<Value> {
    let mut by_identity: IdentitySamples = BTreeMap::new();
    for event in events {
        let Some(ts) = event.timestamp.as_deref().and_then(parse_iso_utc) else {
            continue;
        };
        if ts < start || ts > end {
            continue;
        }
        let username = value_string(&event.data, "username");
        if username.is_empty() {
            continue;
        }
        let session_id = value_string(&event.data, "sessionId");
        by_identity
            .entry((
                username,
                if session_id.is_empty() {
                    "unknown".into()
                } else {
                    session_id
                },
            ))
            .or_default()
            .push((ts, event.clone()));
    }
    let end_exclusive = end + TimeDelta::seconds(1);
    let mut by_user: BTreeMap<String, UserBucket> = BTreeMap::new();
    for ((username, session_id), mut samples) in by_identity {
        samples.sort_by_key(|item| item.0);
        for idx in 0..samples.len() {
            let (ts, event) = &samples[idx];
            let next_ts = samples.get(idx + 1).map(|item| item.0);
            let active = is_active_sample(&event.data);
            let row = by_user
                .entry(username.clone())
                .or_insert_with(|| UserBucket {
                    user: username.clone(),
                    user_id: normalize_user_id(&event.data, host, &username),
                    samples_count: 0,
                    active_samples: 0,
                    session_ids: BTreeSet::new(),
                    intervals: Vec::new(),
                });
            row.samples_count += 1;
            row.session_ids.insert(session_id.clone());
            if active {
                row.active_samples += 1;
                let sample_seconds = event_sample_seconds(config, event, *ts, next_ts);
                let interval_end = (*ts
                    + TimeDelta::milliseconds((sample_seconds * 1000.0) as i64))
                .min(end_exclusive);
                if interval_end > *ts {
                    row.intervals.push((*ts, interval_end));
                }
            }
        }
    }
    let full_range = (end - start).num_seconds() + 1;
    let mut rows = Vec::new();
    for (_, row) in by_user {
        let merged = merge_intervals(row.intervals);
        let active_seconds = merged
            .iter()
            .map(|(s, e)| (*e - *s).num_seconds())
            .sum::<i64>()
            .min(full_range);
        let first = merged.first().map(|i| to_iso_utc(i.0)).unwrap_or_default();
        let last = merged.last().map(|i| to_iso_utc(i.1)).unwrap_or_default();
        let mut obj = Map::new();
        obj.insert("user".into(), json!(row.user));
        obj.insert("user_id".into(), json!(row.user_id));
        obj.insert("active_seconds".into(), json!(active_seconds));
        obj.insert("active_hhmm".into(), json!(hhmm(active_seconds)));
        obj.insert("first_activity".into(), json!(first));
        obj.insert("last_activity".into(), json!(last));
        obj.insert(
            "idle_seconds".into(),
            json!((full_range - active_seconds).max(0)),
        );
        obj.insert("sessions_count".into(), json!(row.session_ids.len()));
        obj.insert("samples_count".into(), json!(row.samples_count));
        obj.insert("active_samples".into(), json!(row.active_samples));
        if intervals {
            obj.insert(
                "_intervals".into(),
                json!(
                    merged
                        .iter()
                        .map(|(s, e)| json!([to_iso_utc(*s), to_iso_utc(*e)]))
                        .collect::<Vec<_>>()
                ),
            );
        }
        rows.push(Value::Object(obj));
    }
    rows
}

fn build_report_summary(rows: &[Value]) -> Value {
    let total_active = rows
        .iter()
        .map(|r| r.get("active_seconds").and_then(Value::as_i64).unwrap_or(0))
        .sum::<i64>();
    let first = rows
        .iter()
        .filter_map(|r| r.get("first_activity").and_then(Value::as_str))
        .filter(|s| !s.is_empty())
        .min()
        .unwrap_or("");
    let last = rows
        .iter()
        .filter_map(|r| r.get("last_activity").and_then(Value::as_str))
        .filter(|s| !s.is_empty())
        .max()
        .unwrap_or("");
    let top = rows
        .iter()
        .max_by_key(|r| r.get("active_seconds").and_then(Value::as_i64).unwrap_or(0));
    json!({
        "users_count": rows.len(),
        "total_active_seconds": total_active,
        "total_active_hhmm": hhmm(total_active),
        "first_activity": first,
        "last_activity": last,
        "top_user": top.and_then(|r| r.get("user")).and_then(Value::as_str).unwrap_or(""),
        "top_user_active_hhmm": top.and_then(|r| r.get("active_hhmm")).and_then(Value::as_str).unwrap_or("00:00"),
    })
}

fn normalize_key(value: &str) -> String {
    value.trim().to_lowercase()
}

fn default_display_name(user: &str, user_id: &str) -> String {
    let mut base = if !user_id.trim().is_empty() {
        user_id.trim().to_string()
    } else {
        user.trim().to_string()
    };
    if let Some((_, right)) = base.split_once('\\') {
        base = right.to_string();
    }
    if base.is_ascii() && base.to_lowercase() == base && base.chars().any(|c| c.is_alphabetic()) {
        base.to_uppercase()
    } else {
        base
    }
}

fn load_aliases(config: &Config) -> Value {
    fs::read_to_string(&config.manager_aliases_json)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| json!({}))
}

fn resolve_user_alias(config: &Config, user: &str, user_id: &str, host: &str) -> AliasProfile {
    let raw = load_aliases(config);
    let users = raw.get("users").unwrap_or(&raw);
    let candidates = [
        normalize_key(user_id),
        normalize_key(&format!("{host}\\{user}")),
        normalize_key(user),
    ];
    let mut alias = json!({});
    if let Some(map) = users.as_object() {
        for candidate in candidates {
            if let Some(value) = map.get(&candidate) {
                alias = if value.is_string() {
                    json!({"display_name": value.as_str().unwrap_or("")})
                } else {
                    value.clone()
                };
                break;
            }
        }
    }
    let display_name = value_string(&alias, "display_name")
        .if_empty(value_string(&alias, "name"))
        .if_empty(default_display_name(user, user_id));
    let manager_owner = value_string(&alias, "manager")
        .if_empty(value_string(&alias, "owner"))
        .if_empty(display_name.clone());
    let exclude = alias
        .get("exclude")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || config.manager_exclude_users.contains(&normalize_key(user))
        || config
            .manager_exclude_users
            .contains(&normalize_key(&display_name));
    AliasProfile {
        display_name,
        manager_owner,
        department: value_string(&alias, "department"),
        role: value_string(&alias, "role"),
        notes: value_string(&alias, "notes"),
        canonical_user_id: value_string(&alias, "canonical_user_id").if_empty(user_id.to_string()),
        exclude,
    }
}

trait IfEmpty {
    fn if_empty(self, fallback: String) -> String;
}
impl IfEmpty for String {
    fn if_empty(self, fallback: String) -> String {
        if self.trim().is_empty() {
            fallback
        } else {
            self
        }
    }
}

fn normalize_filter(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn filter_key(value: &str) -> String {
    normalize_filter(value).to_lowercase()
}

fn interval_overlap_seconds(
    intervals: &[Value],
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> (i64, Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
    let mut total = 0;
    let mut first = None;
    let mut last = None;
    for item in intervals {
        let Some(arr) = item.as_array() else {
            continue;
        };
        if arr.len() != 2 {
            continue;
        }
        let Some(s) = arr[0].as_str().and_then(parse_iso_utc) else {
            continue;
        };
        let Some(e) = arr[1].as_str().and_then(parse_iso_utc) else {
            continue;
        };
        let os = s.max(start);
        let oe = e.min(end);
        if oe <= os {
            continue;
        }
        total += (oe - os).num_seconds();
        if first.is_none_or(|f| os < f) {
            first = Some(os);
        }
        if last.is_none_or(|l| oe > l) {
            last = Some(oe);
        }
    }
    (total, first, last)
}

impl App {
    fn build_management_payload(
        &self,
        rows: Vec<Value>,
        host: &str,
        report_date: NaiveDate,
        owner_filter: &str,
        department_filter: &str,
    ) -> Value {
        let (day_start, day_end) = report_bounds(&self.config, report_date);
        let (work_start_local, work_end_local, work_duration) =
            workday_bounds(&self.config, report_date);
        let now_local = Utc::now().with_timezone(&self.config.offset);
        let is_today = report_date == now_local.date_naive();
        let effective_end_local = if is_today && now_local < work_end_local {
            now_local
        } else {
            work_end_local
        };
        let expected_seconds = if is_today {
            (effective_end_local - work_start_local)
                .num_seconds()
                .max(0)
                .min(work_duration)
        } else {
            work_duration
        };
        let target_seconds = expected_seconds * self.config.manager_target_coverage_pct / 100;
        let low_seconds = expected_seconds * self.config.manager_low_coverage_pct / 100;
        let late_start =
            work_start_local + TimeDelta::minutes(self.config.manager_late_start_grace_minutes);
        let early_finish =
            work_end_local - TimeDelta::minutes(self.config.manager_early_finish_grace_minutes);
        let owner_filter = normalize_filter(owner_filter);
        let department_filter = normalize_filter(department_filter);

        let mut roster = Vec::new();
        let mut actions = Vec::new();
        for row in rows {
            let user = row.get("user").and_then(Value::as_str).unwrap_or("");
            let user_id = row.get("user_id").and_then(Value::as_str).unwrap_or("");
            let alias = resolve_user_alias(&self.config, user, user_id, host);
            if alias.exclude {
                continue;
            }
            if !owner_filter.is_empty()
                && filter_key(&alias.manager_owner) != filter_key(&owner_filter)
            {
                continue;
            }
            if !department_filter.is_empty()
                && filter_key(&alias.department) != filter_key(&department_filter)
            {
                continue;
            }
            let intervals = row
                .get("_intervals")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let (work_secs, work_first, work_last) = interval_overlap_seconds(
                &intervals,
                work_start_local.with_timezone(&Utc),
                effective_end_local.with_timezone(&Utc),
            );
            let calendar_secs = row
                .get("active_seconds")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let coverage = if expected_seconds > 0 {
                clamp_pct(work_secs as f64 / expected_seconds as f64 * 100.0)
            } else {
                0.0
            };
            let status = if work_secs <= 0 {
                "inactive"
            } else if work_secs < target_seconds {
                "below_target"
            } else {
                "ok"
            };
            let first_local = row
                .get("first_activity")
                .and_then(Value::as_str)
                .and_then(parse_iso_utc)
                .map(|dt| dt.with_timezone(&self.config.offset).to_rfc3339())
                .unwrap_or_default();
            let last_local = row
                .get("last_activity")
                .and_then(Value::as_str)
                .and_then(parse_iso_utc)
                .map(|dt| dt.with_timezone(&self.config.offset).to_rfc3339())
                .unwrap_or_default();
            let work_first_local = work_first
                .map(|dt| dt.with_timezone(&self.config.offset))
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_default();
            let work_last_local = work_last
                .map(|dt| dt.with_timezone(&self.config.offset))
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_default();
            let mut public = row.as_object().cloned().unwrap_or_default();
            public.remove("_intervals");
            public.insert("user".into(), json!(alias.display_name));
            public.insert("user_original".into(), json!(user));
            public.insert("manager_owner".into(), json!(alias.manager_owner));
            public.insert("department".into(), json!(alias.department));
            public.insert("role".into(), json!(alias.role));
            public.insert("notes".into(), json!(alias.notes));
            public.insert("canonical_user_id".into(), json!(alias.canonical_user_id));
            public.insert("calendar_active_seconds".into(), json!(calendar_secs));
            public.insert("calendar_active_hhmm".into(), json!(hhmm(calendar_secs)));
            public.insert("workday_active_seconds".into(), json!(work_secs));
            public.insert("workday_active_hhmm".into(), json!(hhmm(work_secs)));
            public.insert("coverage_pct".into(), json!(coverage));
            public.insert("status".into(), json!(status));
            public.insert("first_activity_local".into(), json!(first_local));
            public.insert("last_activity_local".into(), json!(last_local));
            public.insert(
                "workday_first_activity_local".into(),
                json!(work_first_local),
            );
            public.insert("workday_last_activity_local".into(), json!(work_last_local));
            let public_value = Value::Object(public.clone());
            let evidence = json!({
                "calendar_active_hhmm": hhmm(calendar_secs),
                "workday_active_hhmm": hhmm(work_secs),
                "coverage_pct": coverage,
                "first_activity": row.get("first_activity").cloned().unwrap_or(json!("")),
                "last_activity": row.get("last_activity").cloned().unwrap_or(json!("")),
                "sessions_count": row.get("sessions_count").cloned().unwrap_or(json!(0)),
                "manager_owner": public.get("manager_owner").cloned().unwrap_or(json!("")),
                "department": public.get("department").cloned().unwrap_or(json!("")),
                "role": public.get("role").cloned().unwrap_or(json!("")),
            });
            let owner = public
                .get("manager_owner")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let display = public
                .get("user")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let canonical = public
                .get("canonical_user_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if work_secs <= 0 {
                actions.push(action("missing_activity", "critical", &owner, "today", &format!("За {} у сотрудника {} нет подтверждённой активности в рабочем окне RDP.", report_date, display), &format!("Проверить сотрудника {display}: работал ли он в рабочее время, была ли потеря сбора данных или отсутствие входа в систему."), &canonical, evidence));
            } else {
                if expected_seconds > 0 && work_secs < low_seconds {
                    actions.push(action("low_activity_review", "high", &owner, "24h", &format!("У сотрудника {display} активное время в рабочем окне {} ниже {}% от ожидаемого окна.", hhmm(work_secs), self.config.manager_low_coverage_pct), &format!("Проверить загрузку сотрудника {display}, задачи и фактическое присутствие в рабочем процессе."), &canonical, evidence.clone()));
                } else if expected_seconds > 0 && work_secs < target_seconds {
                    actions.push(action("target_gap_review", "medium", &owner, "24h", &format!("У сотрудника {display} активное время в рабочем окне {} ниже управленческого целевого порога {}%.", hhmm(work_secs), self.config.manager_target_coverage_pct), &format!("Уточнить причину отклонения по сотруднику {display} и подтвердить план работ."), &canonical, evidence.clone()));
                }
                if work_first.is_some_and(|dt| dt.with_timezone(&self.config.offset) > late_start) {
                    actions.push(action("late_start_review", "medium", &owner, "24h", &format!("У сотрудника {display} первая активность в рабочем окне зафиксирована поздно."), &format!("Проверить причину позднего старта сотрудника {display} и подтвердить, что это не проблема доступа или дисциплины."), &canonical, evidence.clone()));
                }
                if !is_today
                    && work_last
                        .is_some_and(|dt| dt.with_timezone(&self.config.offset) < early_finish)
                {
                    actions.push(action("early_finish_review", "medium", &owner, "24h", &format!("У сотрудника {display} последняя активность в рабочем окне завершилась рано."), &format!("Проверить, было ли досрочное завершение рабочего дня сотрудника {display} согласовано и чем оно объясняется."), &canonical, evidence));
                }
            }
            roster.push(public_value);
        }
        actions.sort_by_key(|a| {
            (
                priority_rank(a.get("priority").and_then(Value::as_str).unwrap_or("")),
                a.get("owner")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_lowercase(),
                a.get("action_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            )
        });
        let sources_actions = self.build_source_freshness(
            host,
            roster
                .iter()
                .any(|r| r.get("status").and_then(Value::as_str) != Some("inactive")),
        );
        let sources = sources_actions.0;
        actions.extend(sources_actions.1);
        actions.sort_by_key(|a| {
            (
                priority_rank(a.get("priority").and_then(Value::as_str).unwrap_or("")),
                a.get("owner")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_lowercase(),
                a.get("action_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            )
        });
        let summary = summarize_management_rows(&roster, &actions, expected_seconds);
        let owner_rollups = build_rollups(&roster, &actions, "manager_owner");
        let department_rollups = build_rollups(&roster, &actions, "department");
        let owner_roster = owner_rollups.clone();
        let executive = build_executive_summary(&summary, &actions, &sources);
        let current_trend_point = json!({
            "report_date": report_date.to_string(),
            "users_count": summary["users_count"],
            "active_users": summary["active_users"],
            "inactive_users": summary["inactive_users"],
            "workday_total_active_seconds": summary["workday_total_active_seconds"],
            "workday_total_active_hhmm": summary["workday_total_active_hhmm"],
            "portfolio_coverage_pct": summary["portfolio_coverage_pct"],
            "actions_count": summary["actions_count"],
            "critical_actions_count": summary["critical_actions_count"],
            "department_rollups": compact_rollup_points(&department_rollups),
            "owner_rollups": compact_rollup_points(&owner_rollups),
        });
        let history_saved = self.save_management_history_point(
            host,
            &owner_filter,
            &department_filter,
            report_date,
            &current_trend_point,
        );
        self.prune_management_history(host, &owner_filter, &department_filter, report_date);
        let trend = self.load_management_trend(
            host,
            &owner_filter,
            &department_filter,
            report_date,
            &current_trend_point,
        );
        let trend_points = trend.len();
        let insights = build_management_insights(
            &self.config,
            &summary,
            &roster,
            &owner_rollups,
            &department_rollups,
            &trend,
            report_date,
        );
        json!({
            "generated_at_utc": to_iso_utc(Utc::now()),
            "host": host,
            "report_date": report_date.to_string(),
            "report_timezone": "Europe/Moscow",
            "filters": {"owner": owner_filter, "department": department_filter},
            "workday": {
                "start_local": work_start_local.to_rfc3339(),
                "end_local": work_end_local.to_rfc3339(),
                "expected_seconds_per_user": expected_seconds,
                "expected_hhmm_per_user": hhmm(expected_seconds),
                "target_coverage_pct": self.config.manager_target_coverage_pct,
                "low_coverage_pct": self.config.manager_low_coverage_pct,
            },
            "summary": summary,
            "actions": actions,
            "rows": roster,
            "sources": sources,
            "trend": trend,
            "trend_scope": "portfolio",
            "trend_insights": insights,
            "interpretation_policy": {
                "configured": self.config.manager_interpretation_policy_configured,
                "overload_threshold": self.config.manager_overload_coverage_pct as f64 / 100.0,
                "underload_threshold": self.config.manager_low_coverage_pct as f64 / 100.0,
                "drop_threshold_pct": self.config.manager_trend_delta_pct,
                "night_work_after": self.config.manager_night_work_after.map(|time| time.format("%H:%M").to_string()),
                "weekend_work": self.config.manager_weekend_work_enabled,
                "min_trend_points": self.config.manager_trend_min_points,
                "off_hours_threshold_seconds": self.config.manager_off_hours_threshold_seconds
            },
            "history": {
                "enabled": true,
                "saved_current_point": history_saved,
                "points_count": trend_points,
                "days_configured": self.config.management_history_days,
                "retention_days": self.config.management_history_retention_days,
                "storage": "daily_aggregate_points"
            },
            "executive": executive,
            "owner_rollups": owner_rollups,
            "department_rollups": department_rollups,
            "owner_roster": owner_roster,
            "bucket_id": sessions_bucket(host),
            "report_bounds": {"start_utc": to_iso_utc(day_start), "end_utc": to_iso_utc(day_end)},
        })
    }

    fn management_history_filter_dir(&self, host: &str, owner: &str, department: &str) -> PathBuf {
        let host = history_component(host);
        let filter = stable_hash_hex(&format!(
            "owner={}|department={}",
            normalize_filter(owner),
            normalize_filter(department)
        ));
        self.config.management_history_dir.join(host).join(filter)
    }

    fn management_history_path(
        &self,
        host: &str,
        owner: &str,
        department: &str,
        report_date: NaiveDate,
    ) -> PathBuf {
        self.management_history_filter_dir(host, owner, department)
            .join(format!("{report_date}.json"))
    }

    fn save_management_history_point(
        &self,
        host: &str,
        owner: &str,
        department: &str,
        report_date: NaiveDate,
        point: &Value,
    ) -> bool {
        let path = self.management_history_path(host, owner, department, report_date);
        let Some(dir) = path.parent() else {
            return false;
        };
        if fs::create_dir_all(dir).is_err() {
            return false;
        }
        let payload = json!({
            "stored_at_utc": to_iso_utc(Utc::now()),
            "report_date": report_date.to_string(),
            "host": host,
            "filters": {
                "owner": normalize_filter(owner),
                "department": normalize_filter(department)
            },
            "point": point
        });
        let tmp = path.with_extension("json.tmp");
        if fs::write(
            &tmp,
            serde_json::to_vec_pretty(&payload).unwrap_or_default(),
        )
        .is_err()
        {
            return false;
        }
        fs::rename(tmp, path).is_ok()
    }

    fn load_management_trend(
        &self,
        host: &str,
        owner: &str,
        department: &str,
        report_date: NaiveDate,
        current_point: &Value,
    ) -> Vec<Value> {
        let days = self.config.management_history_days as i64;
        let start_date = report_date - TimeDelta::days(days.saturating_sub(1));
        let mut points = BTreeMap::new();
        for offset in 0..days {
            let date = start_date + TimeDelta::days(offset);
            let path = self.management_history_path(host, owner, department, date);
            let Some(point) = load_management_history_point(&path) else {
                continue;
            };
            points.insert(date, point);
        }
        points
            .entry(report_date)
            .or_insert_with(|| current_point.clone());
        points.into_values().collect()
    }

    fn prune_management_history(
        &self,
        host: &str,
        owner: &str,
        department: &str,
        report_date: NaiveDate,
    ) {
        let dir = self.management_history_filter_dir(host, owner, department);
        let cutoff = report_date - TimeDelta::days(self.config.management_history_retention_days);
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            let Ok(date) = NaiveDate::parse_from_str(stem, "%Y-%m-%d") else {
                continue;
            };
            if date < cutoff {
                let _ = fs::remove_file(path);
            }
        }
    }

    fn build_source_freshness(
        &self,
        host: &str,
        interactive_required: bool,
    ) -> (Vec<Value>, Vec<Value>) {
        let critical_max_age = self.config.manager_critical_source_max_age_seconds;
        let rust_sessions_bucket = sessions_bucket(host);
        let watcher_afk_bucket = format!("aw-watcher-afk_{host}");
        let rust_sessions_fresh = self
            .latest_bucket_event(&rust_sessions_bucket)
            .and_then(|event| {
                event
                    .timestamp
                    .as_deref()
                    .and_then(parse_iso_utc)
                    .map(|dt| (Utc::now() - dt).num_seconds().max(0) <= critical_max_age)
            })
            .unwrap_or(false);
        let watcher_afk_fresh = self
            .latest_bucket_event(&watcher_afk_bucket)
            .and_then(|event| {
                event
                    .timestamp
                    .as_deref()
                    .and_then(parse_iso_utc)
                    .map(|dt| (Utc::now() - dt).num_seconds().max(0) <= critical_max_age)
            })
            .unwrap_or(false);
        let specs = vec![
            (
                "worktime_sessions",
                "RDP worktime sessions",
                rust_sessions_bucket,
                self.config.manager_critical_source_max_age_seconds,
                true,
                false,
            ),
            (
                "rdp_window",
                "RDP current window",
                format!("aw-rdp-window_{host}"),
                self.config.manager_critical_source_max_age_seconds,
                true,
                false,
            ),
            (
                "rdp_afk",
                "RDP AFK",
                format!("aw-rdp-afk_{host}"),
                self.config.manager_critical_source_max_age_seconds,
                true,
                false,
            ),
            (
                "watcher_window",
                "Local watcher window",
                format!("aw-watcher-window_{host}"),
                self.config.manager_critical_source_max_age_seconds,
                true,
                true,
            ),
            (
                "watcher_afk",
                "Local watcher AFK",
                format!("aw-watcher-afk_{host}"),
                self.config.manager_critical_source_max_age_seconds,
                true,
                false,
            ),
            (
                "file_operations",
                "File operations collector",
                format!("aw-file-operations_{host}"),
                self.config.manager_critical_source_max_age_seconds,
                true,
                true,
            ),
            (
                "web_categories",
                "Browser/web categories",
                format!("aw-detmir-web-category_{host}"),
                self.config.manager_web_source_max_age_seconds,
                false,
                false,
            ),
            (
                "session_events",
                "Windows session events",
                format!("aw-session-events_{host}"),
                self.config.manager_session_source_max_age_seconds,
                false,
                false,
            ),
            (
                "pve_tasks",
                "PVE task feed",
                "aw-pve-task-events_pve-detmir".to_string(),
                self.config.manager_infra_source_max_age_seconds,
                false,
                false,
            ),
        ];
        let now = Utc::now();
        let mut sources = Vec::new();
        let mut actions = Vec::new();
        for (source_id, label, bucket, max_age, required, interactive_only) in specs {
            if interactive_only && !interactive_required {
                sources.push(json!({
                    "source_id": source_id,
                    "label": label,
                    "status": "inactive",
                    "status_label": "inactive",
                    "bucket_id": bucket,
                    "timestamp": "",
                    "age_seconds": null,
                    "required": required,
                    "interactive_only": interactive_only,
                    "interactive_required": interactive_required,
                    "max_age_seconds": max_age,
                    "summary": "inactive: no active interactive users",
                    "event_summary": "",
                }));
                continue;
            }
            let event = self.latest_bucket_event(&bucket);
            let ts = event
                .as_ref()
                .and_then(|e| e.timestamp.as_deref())
                .and_then(parse_iso_utc);
            let age = ts.map(|dt| (now - dt).num_seconds().max(0));
            let mut status = if event.is_none() {
                if required { "fail" } else { "warn" }
            } else if age.is_none() {
                "warn"
            } else if age.unwrap() > max_age {
                if required { "fail" } else { "warn" }
            } else {
                "ok"
            };
            let mut summary = if event.is_none() {
                "bucket missing or empty".to_string()
            } else if age.is_none() {
                "timestamp parse failed".to_string()
            } else if status == "ok" {
                format!("fresh ({}s)", age.unwrap())
            } else {
                format!("stale ({}s)", age.unwrap())
            };
            let legacy_rdp_covered = legacy_rdp_covered_by_rust_sources(
                source_id,
                status,
                rust_sessions_fresh,
                watcher_afk_fresh,
            );
            let effective_required = required && !legacy_rdp_covered;
            if legacy_rdp_covered {
                status = "inactive";
                summary =
                    "legacy RDP source covered by fresh Rust worktime/watcher sources".to_string();
            }
            if interactive_only && !interactive_required && status != "ok" {
                status = "inactive";
                summary = "inactive: no active interactive users".to_string();
            }
            let source = json!({
                "source_id": source_id,
                "label": label,
                "status": status,
                "status_label": source_status_label(status),
                "bucket_id": bucket,
                "timestamp": event.as_ref().and_then(|e| e.timestamp.clone()).unwrap_or_default(),
                "age_seconds": age,
                "required": effective_required,
                "interactive_only": interactive_only,
                "interactive_required": interactive_required,
                "max_age_seconds": max_age,
                "summary": summary,
                "event_summary": event.as_ref().map(source_summary).unwrap_or_default(),
            });
            if !matches!(status, "ok" | "inactive") {
                actions.push(action(
                    "source_freshness_review",
                    if effective_required { "critical" } else { "medium" },
                    "ops",
                    if effective_required { "today" } else { "3d" },
                    &format!("Источник '{label}' в состоянии {}: {}.", source_status_label(status), summary),
                    "Проверить collector/service, причину отставания и подтвердить, что управленческие выводы по данным ещё надёжны.",
                    "",
                    json!({"source_id": source_id, "bucket_id": bucket, "age_seconds": age, "required": effective_required}),
                ));
            }
            sources.push(source);
        }
        (sources, actions)
    }
}

#[allow(clippy::too_many_arguments)]
fn action(
    id: &str,
    priority: &str,
    owner: &str,
    deadline: &str,
    reason: &str,
    recommended: &str,
    user_id: &str,
    evidence: Value,
) -> Value {
    json!({
        "action_id": id,
        "priority": priority,
        "owner": owner,
        "user_id": user_id,
        "deadline_hint": deadline,
        "reason": reason,
        "recommended_action": recommended,
        "evidence": evidence,
    })
}

fn priority_rank(priority: &str) -> i32 {
    match priority {
        "critical" => 0,
        "high" => 1,
        "medium" => 2,
        "low" => 3,
        _ => 9,
    }
}

fn clamp_pct(value: f64) -> f64 {
    (value.clamp(0.0, 100.0) * 100.0).round() / 100.0
}

fn source_status_label(status: &str) -> &str {
    match status {
        "ok" => "fresh",
        "warn" => "stale",
        "fail" => "missing",
        "inactive" => "inactive",
        _ => status,
    }
}

fn legacy_rdp_covered_by_rust_sources(
    source_id: &str,
    status: &str,
    rust_sessions_fresh: bool,
    watcher_afk_fresh: bool,
) -> bool {
    matches!(source_id, "rdp_window" | "rdp_afk")
        && status != "ok"
        && rust_sessions_fresh
        && watcher_afk_fresh
}

fn source_summary(event: &AwEvent) -> String {
    let d = &event.data;
    let signal = value_string(d, "signalType");
    if signal == "collector_health" {
        return format!(
            "queue={} failures={} flushed={}",
            d.get("queueDepth").unwrap_or(&json!(0)),
            d.get("sendFailures").unwrap_or(&json!(0)),
            d.get("eventsFlushed").unwrap_or(&json!(0))
        );
    }
    for key in ["domain", "eventType", "action", "title", "status", "app"] {
        let value = value_string(d, key);
        if !value.is_empty() {
            return value.chars().take(120).collect();
        }
    }
    String::new()
}

fn summarize_management_rows(rows: &[Value], actions: &[Value], expected_seconds: i64) -> Value {
    let users_count = rows.len() as i64;
    let active_users = rows
        .iter()
        .filter(|r| r.get("status").and_then(Value::as_str) != Some("inactive"))
        .count() as i64;
    let inactive_users = rows
        .iter()
        .filter(|r| r.get("status").and_then(Value::as_str) == Some("inactive"))
        .count() as i64;
    let below_target_users = rows
        .iter()
        .filter(|r| r.get("status").and_then(Value::as_str) == Some("below_target"))
        .count() as i64;
    let on_target_users = rows
        .iter()
        .filter(|r| r.get("status").and_then(Value::as_str) == Some("ok"))
        .count() as i64;
    let work_total = rows
        .iter()
        .map(|r| {
            r.get("workday_active_seconds")
                .and_then(Value::as_i64)
                .unwrap_or(0)
        })
        .sum::<i64>();
    let cal_total = rows
        .iter()
        .map(|r| {
            r.get("calendar_active_seconds")
                .and_then(Value::as_i64)
                .unwrap_or(0)
        })
        .sum::<i64>();
    let coverage = if users_count > 0 && expected_seconds > 0 {
        clamp_pct(work_total as f64 / (expected_seconds * users_count) as f64 * 100.0)
    } else {
        0.0
    };
    let top = rows.iter().max_by_key(|r| {
        r.get("workday_active_seconds")
            .and_then(Value::as_i64)
            .unwrap_or(0)
    });
    json!({
        "users_count": users_count,
        "active_users": active_users,
        "inactive_users": inactive_users,
        "on_target_users": on_target_users,
        "below_target_users": below_target_users,
        "portfolio_coverage_pct": coverage,
        "actions_count": actions.len(),
        "critical_actions_count": actions.iter().filter(|a| a.get("priority").and_then(Value::as_str) == Some("critical")).count(),
        "high_actions_count": actions.iter().filter(|a| a.get("priority").and_then(Value::as_str) == Some("high")).count(),
        "calendar_total_active_seconds": cal_total,
        "calendar_total_active_hhmm": hhmm(cal_total),
        "workday_total_active_seconds": work_total,
        "workday_total_active_hhmm": hhmm(work_total),
        "total_active_seconds": work_total,
        "total_active_hhmm": hhmm(work_total),
        "first_activity": rows.iter().filter_map(|r| r.get("workday_first_activity_local").and_then(Value::as_str)).filter(|s| !s.is_empty()).min().unwrap_or(""),
        "last_activity": rows.iter().filter_map(|r| r.get("workday_last_activity_local").and_then(Value::as_str)).filter(|s| !s.is_empty()).max().unwrap_or(""),
        "top_user": top.and_then(|r| r.get("user")).and_then(Value::as_str).unwrap_or(""),
        "top_user_active_hhmm": top.and_then(|r| r.get("workday_active_hhmm")).and_then(Value::as_str).unwrap_or("00:00"),
    })
}

fn build_rollups(rows: &[Value], actions: &[Value], field: &str) -> Vec<Value> {
    let mut groups: BTreeMap<String, Map<String, Value>> = BTreeMap::new();
    for row in rows {
        let name = row
            .get(field)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("Без подразделения")
            .to_string();
        let group = groups.entry(name.clone()).or_insert_with(|| {
            let mut g = Map::new();
            g.insert("name".into(), json!(name));
            for key in [
                "users_count",
                "active_users",
                "inactive_users",
                "below_target_users",
                "workday_total_active_seconds",
                "actions_count",
                "critical_actions_count",
                "high_actions_count",
                "medium_actions_count",
                "low_actions_count",
            ] {
                g.insert(key.into(), json!(0));
            }
            g.insert("users".into(), json!([]));
            g
        });
        inc(group, "users_count", 1);
        if row.get("status").and_then(Value::as_str) == Some("inactive") {
            inc(group, "inactive_users", 1);
        } else {
            inc(group, "active_users", 1);
        }
        if row.get("status").and_then(Value::as_str) == Some("below_target") {
            inc(group, "below_target_users", 1);
        }
        inc(
            group,
            "workday_total_active_seconds",
            row.get("workday_active_seconds")
                .and_then(Value::as_i64)
                .unwrap_or(0),
        );
        if let Some(arr) = group.get_mut("users").and_then(Value::as_array_mut) {
            arr.push(row.get("user").cloned().unwrap_or(json!("")));
        }
    }
    for action in actions {
        let name = action
            .get("owner")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("unassigned")
            .to_string();
        let Some(group) = groups.get_mut(&name) else {
            continue;
        };
        inc(group, "actions_count", 1);
        let key = match action.get("priority").and_then(Value::as_str).unwrap_or("") {
            "critical" => "critical_actions_count",
            "high" => "high_actions_count",
            "medium" => "medium_actions_count",
            "low" => "low_actions_count",
            _ => "low_actions_count",
        };
        inc(group, key, 1);
    }
    groups
        .into_values()
        .map(|mut g| {
            let secs = g
                .get("workday_total_active_seconds")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let users = g.get("users_count").and_then(Value::as_i64).unwrap_or(0);
            g.insert("workday_total_active_hhmm".into(), json!(hhmm(secs)));
            g.insert(
                "portfolio_coverage_pct".into(),
                json!(if users > 0 {
                    clamp_pct(secs as f64 / (users * 9 * 3600) as f64 * 100.0)
                } else {
                    0.0
                }),
            );
            Value::Object(g)
        })
        .collect()
}

fn inc(map: &mut Map<String, Value>, key: &str, amount: i64) {
    let old = map.get(key).and_then(Value::as_i64).unwrap_or(0);
    map.insert(key.into(), json!(old + amount));
}

fn compact_rollup_points(rollups: &[Value]) -> Vec<Value> {
    rollups
        .iter()
        .filter(|item| {
            item.get("users_count")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                > 0
        })
        .map(|item| {
            json!({
                "name": item.get("name").cloned().unwrap_or(json!("")),
                "users_count": item.get("users_count").cloned().unwrap_or(json!(0)),
                "active_users": item.get("active_users").cloned().unwrap_or(json!(0)),
                "inactive_users": item.get("inactive_users").cloned().unwrap_or(json!(0)),
                "below_target_users": item.get("below_target_users").cloned().unwrap_or(json!(0)),
                "workday_total_active_seconds": item.get("workday_total_active_seconds").cloned().unwrap_or(json!(0)),
                "workday_total_active_hhmm": item.get("workday_total_active_hhmm").cloned().unwrap_or(json!("00:00")),
                "portfolio_coverage_pct": item.get("portfolio_coverage_pct").cloned().unwrap_or(json!(0.0)),
                "actions_count": item.get("actions_count").cloned().unwrap_or(json!(0)),
                "critical_actions_count": item.get("critical_actions_count").cloned().unwrap_or(json!(0)),
                "high_actions_count": item.get("high_actions_count").cloned().unwrap_or(json!(0)),
            })
        })
        .collect()
}

fn build_management_insights(
    config: &Config,
    summary: &Value,
    rows: &[Value],
    owner_rollups: &[Value],
    department_rollups: &[Value],
    trend: &[Value],
    report_date: NaiveDate,
) -> Vec<Value> {
    let mut insights = Vec::new();
    let min_points = config.manager_trend_min_points;
    let portfolio_values = trend_values(trend, "portfolio_coverage_pct");
    if portfolio_values.len() < min_points {
        insights.push(insight(
            "history_insufficient",
            "INFO",
            "portfolio",
            "Workforce",
            "История еще накапливается",
            &format!(
                "Накоплено {} daily point(s), для устойчивой интерпретации нужно минимум {}.",
                portfolio_values.len(),
                min_points
            ),
            "Использовать текущий дневной срез; недельные и месячные выводы включатся после накопления истории.",
        ));
    } else if monotonic_delta(
        &portfolio_values,
        min_points,
        config.manager_trend_delta_pct,
        true,
    ) {
        insights.push(insight(
            "portfolio_activity_growing",
            "OK",
            "portfolio",
            "Workforce",
            "Активность растет несколько дней подряд",
            &format!(
                "Portfolio coverage вырос за последние {} точек минимум на {:.0} п.п.",
                min_points, config.manager_trend_delta_pct
            ),
            "Проверить, связано ли улучшение с реальным ростом загрузки или с изменением состава сотрудников.",
        ));
    } else if monotonic_delta(
        &portfolio_values,
        min_points,
        config.manager_trend_delta_pct,
        false,
    ) {
        insights.push(insight(
            "portfolio_activity_falling",
            "WARN",
            "portfolio",
            "Workforce",
            "Активность падает несколько дней подряд",
            &format!(
                "Portfolio coverage снизился за последние {} точек минимум на {:.0} п.п.",
                min_points, config.manager_trend_delta_pct
            ),
            "Разобрать причины снижения: простой, отсутствие задач, сбой сбора или изменение рабочего процесса.",
        ));
    }

    add_rollup_current_insights(
        &mut insights,
        department_rollups,
        "department",
        config.manager_low_coverage_pct as f64,
        config.manager_overload_coverage_pct as f64,
    );
    add_rollup_current_insights(
        &mut insights,
        owner_rollups,
        "owner",
        config.manager_low_coverage_pct as f64,
        config.manager_overload_coverage_pct as f64,
    );
    add_rollup_history_insights(
        &mut insights,
        trend,
        "department_rollups",
        "department",
        config.manager_low_coverage_pct as f64,
        config.manager_trend_min_points,
        config.manager_trend_delta_pct,
    );
    add_rollup_history_insights(
        &mut insights,
        trend,
        "owner_rollups",
        "owner",
        config.manager_low_coverage_pct as f64,
        config.manager_trend_min_points,
        config.manager_trend_delta_pct,
    );
    add_off_hours_insights(&mut insights, config, summary, rows, report_date);

    insights.sort_by_key(|item| {
        (
            insight_rank(item.get("severity").and_then(Value::as_str).unwrap_or("")),
            item.get("scope")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            item.get("subject")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        )
    });
    insights.truncate(12);
    insights
}

fn trend_values(trend: &[Value], field: &str) -> Vec<f64> {
    trend
        .iter()
        .filter_map(|item| item.get(field).and_then(Value::as_f64))
        .collect()
}

fn monotonic_delta(values: &[f64], min_points: usize, min_delta: f64, increasing: bool) -> bool {
    if values.len() < min_points {
        return false;
    }
    let tail = &values[values.len() - min_points..];
    let monotonic = tail.windows(2).all(|pair| {
        if increasing {
            pair[1] >= pair[0]
        } else {
            pair[1] <= pair[0]
        }
    });
    monotonic
        && if increasing {
            tail[tail.len() - 1] - tail[0] >= min_delta
        } else {
            tail[0] - tail[tail.len() - 1] >= min_delta
        }
}

fn add_rollup_current_insights(
    insights: &mut Vec<Value>,
    rollups: &[Value],
    scope: &str,
    low_pct: f64,
    overload_pct: f64,
) {
    for item in rollups {
        let users = item.get("users_count").and_then(Value::as_i64).unwrap_or(0);
        if users <= 0 {
            continue;
        }
        let subject = item
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Без группы");
        let coverage = item
            .get("portfolio_coverage_pct")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let active = item
            .get("active_users")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let hhmm = item
            .get("workday_total_active_hhmm")
            .and_then(Value::as_str)
            .unwrap_or("00:00");
        if coverage < low_pct {
            insights.push(insight(
                "current_underload",
                "WARN",
                scope,
                subject,
                "Текущая недогрузка",
                &format!("Coverage {coverage:.0}% ниже порога {low_pct:.0}%; active {active}/{users}; workday total {hhmm}."),
                "Проверить план задач, отсутствие входа в систему и возможный сбой сбора данных.",
            ));
        } else if coverage >= overload_pct {
            insights.push(insight(
                "current_overload",
                "WARN",
                scope,
                subject,
                "Возможная перегрузка",
                &format!("Coverage {coverage:.0}% выше порога {overload_pct:.0}%; active {active}/{users}; workday total {hhmm}."),
                "Проверить переработку, распределение задач и риск выгорания.",
            ));
        }
    }
}

fn add_rollup_history_insights(
    insights: &mut Vec<Value>,
    trend: &[Value],
    trend_key: &str,
    scope: &str,
    low_pct: f64,
    min_points: usize,
    min_delta: f64,
) {
    let mut series: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for point in trend {
        let Some(items) = point.get(trend_key).and_then(Value::as_array) else {
            continue;
        };
        for item in items {
            let users = item.get("users_count").and_then(Value::as_i64).unwrap_or(0);
            if users <= 0 {
                continue;
            }
            let Some(name) = item.get("name").and_then(Value::as_str) else {
                continue;
            };
            let Some(coverage) = item.get("portfolio_coverage_pct").and_then(Value::as_f64) else {
                continue;
            };
            series.entry(name.to_string()).or_default().push(coverage);
        }
    }
    for (subject, values) in series {
        if values.len() < min_points {
            continue;
        }
        let tail = &values[values.len() - min_points..];
        if tail.iter().all(|value| *value < low_pct) {
            insights.push(insight(
                "stable_underload",
                "WARN",
                scope,
                &subject,
                "Стабильная недогрузка",
                &format!("Coverage ниже {low_pct:.0}% последние {} daily points.", tail.len()),
                "Проверить устойчивую нехватку задач, неверный профиль роли или постоянную проблему сбора.",
            ));
        }
        let avg_previous = if tail.len() > 1 {
            tail[..tail.len() - 1].iter().sum::<f64>() / (tail.len() - 1) as f64
        } else {
            tail[0]
        };
        let current = tail[tail.len() - 1];
        if avg_previous - current >= min_delta {
            insights.push(insight(
                "drop_vs_norm",
                "WARN",
                scope,
                &subject,
                "Резкая просадка относительно своей нормы",
                &format!(
                    "Текущий coverage {current:.0}% ниже среднего последних точек на {:.0} п.п.",
                    avg_previous - current
                ),
                "Проверить изменение задач, простой, отпуск/отсутствие и корректность сбора.",
            ));
        }
    }
}

fn add_off_hours_insights(
    insights: &mut Vec<Value>,
    config: &Config,
    summary: &Value,
    rows: &[Value],
    report_date: NaiveDate,
) {
    let calendar_total = summary
        .get("calendar_total_active_seconds")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let workday_total = summary
        .get("workday_total_active_seconds")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let off_hours = (calendar_total - workday_total).max(0);
    let is_weekend = matches!(
        report_date.weekday(),
        chrono::Weekday::Sat | chrono::Weekday::Sun
    );
    if is_weekend && !config.manager_weekend_work_enabled {
        return;
    }
    if is_weekend && calendar_total >= config.manager_off_hours_threshold_seconds {
        insights.push(insight(
            "weekend_work",
            "WARN",
            "portfolio",
            "Workforce",
            "Есть работа в выходной день",
            &format!(
                "Зафиксировано {} активности в календарный выходной.",
                hhmm(calendar_total)
            ),
            "Проверить, была ли работа согласована и не маскирует ли она аврал или сбой графика.",
        ));
        return;
    }
    if let Some(night_after) = config.manager_night_work_after {
        let night_users = rows
            .iter()
            .filter(|row| {
                row_local_time(row, "last_activity_local").is_some_and(|time| time >= night_after)
            })
            .count();
        if night_users > 0 && off_hours >= config.manager_off_hours_threshold_seconds {
            insights.push(insight(
                "night_work",
                "WARN",
                "portfolio",
                "Workforce",
                "Есть ночная работа",
                &format!(
                    "После {} зафиксирована активность; сотрудников: {}; вне рабочего окна: {}.",
                    night_after.format("%H:%M"),
                    night_users,
                    hhmm(off_hours)
                ),
                "Проверить согласование ночной работы, авралы и корректность графика.",
            ));
        }
    }
    if off_hours >= config.manager_off_hours_threshold_seconds {
        let users = rows
            .iter()
            .filter(|row| {
                let calendar = row
                    .get("calendar_active_seconds")
                    .and_then(Value::as_i64)
                    .unwrap_or(0);
                let workday = row
                    .get("workday_active_seconds")
                    .and_then(Value::as_i64)
                    .unwrap_or(0);
                calendar > workday
            })
            .count();
        insights.push(insight(
            "off_hours_work",
            "WARN",
            "portfolio",
            "Workforce",
            "Есть активность вне рабочего окна",
            &format!(
                "Вне рабочего окна зафиксировано {}; сотрудников: {}.",
                hhmm(off_hours),
                users
            ),
            "Проверить ночную/раннюю работу, регламент смен и корректность рабочего окна.",
        ));
    }
}

fn row_local_time(row: &Value, key: &str) -> Option<NaiveTime> {
    row.get(key)
        .and_then(Value::as_str)
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|dt| dt.time())
}

fn insight(
    code: &str,
    severity: &str,
    scope: &str,
    subject: &str,
    title: &str,
    evidence: &str,
    recommendation: &str,
) -> Value {
    json!({
        "code": code,
        "severity": severity,
        "scope": scope,
        "subject": subject,
        "title": title,
        "evidence": evidence,
        "recommendation": recommendation,
    })
}

fn insight_rank(severity: &str) -> i32 {
    match severity {
        "FAIL" => 0,
        "WARN" => 1,
        "OK" => 2,
        "INFO" => 3,
        _ => 4,
    }
}

fn build_executive_summary(summary: &Value, actions: &[Value], sources: &[Value]) -> Value {
    let critical = actions
        .iter()
        .filter(|a| a.get("priority").and_then(Value::as_str) == Some("critical"))
        .count();
    let high = actions
        .iter()
        .filter(|a| a.get("priority").and_then(Value::as_str) == Some("high"))
        .count();
    let portfolio_state = if critical > 0 {
        "critical"
    } else if high > 0 {
        "attention"
    } else {
        "stable"
    };
    let headline = if critical > 0 {
        format!("Есть {critical} критичных вопроса, требующих решения сегодня.")
    } else if high > 0 {
        format!("Критичных провалов нет, но есть {high} вопроса повышенного внимания.")
    } else {
        "Критичных отклонений не найдено, рабочий день идёт в пределах нормы.".to_string()
    };
    let stale = sources
        .iter()
        .filter(|s| s.get("status").and_then(Value::as_str) != Some("ok"))
        .count();
    json!({
        "portfolio_state": portfolio_state,
        "headline": headline,
        "message": format!("Активны {} из {} сотрудников. Покрытие рабочего окна {}%.", summary["active_users"], summary["users_count"], summary["portfolio_coverage_pct"]),
        "focus_items": actions.iter().take(5).map(|a| json!({"priority": a["priority"], "owner": a["owner"], "title": a["action_id"], "reason": a["reason"], "recommended_action": a["recommended_action"]})).collect::<Vec<_>>(),
        "stale_sources": sources.iter().filter(|s| s.get("status").and_then(Value::as_str) != Some("ok")).take(3).cloned().collect::<Vec<_>>(),
        "stale_sources_count": stale,
    })
}

fn normalize_app_name(app: &str, title: &str) -> String {
    let app_l = app.trim().to_lowercase();
    let title_l = title.to_lowercase();
    if app_l.starts_with("1cv8") || app_l.starts_with("1cestart") {
        "1С".into()
    } else if app_l == "chrome.exe" || title_l.contains("google chrome") {
        "Chrome".into()
    } else if app_l == "msedge.exe" || title_l.contains("microsoft edge") {
        "Edge".into()
    } else if app_l == "browser.exe" || title_l.contains("яндекс") {
        "Яндекс Браузер".into()
    } else if app_l == "excel.exe" {
        "Excel".into()
    } else if app_l == "winword.exe" {
        "Word".into()
    } else if app_l == "explorer.exe" {
        "Проводник".into()
    } else if app_l == "totalcmd.exe" || app_l == "totalcmd64.exe" {
        "Total Commander".into()
    } else if !app.trim().is_empty() {
        app.trim().trim_end_matches(".exe").to_string()
    } else if !title.trim().is_empty() {
        title.trim().to_string()
    } else {
        "Неизвестное приложение".into()
    }
}

fn event_duration(config: &Config, event: &AwEvent, fallback: f64) -> f64 {
    event
        .duration
        .unwrap_or(0.0)
        .max(fallback)
        .min(config.true_active_max_event_seconds as f64)
        .max(1.0)
}

fn event_context(event: &AwEvent) -> String {
    for key in [
        "title",
        "url",
        "path",
        "filePath",
        "targetPath",
        "windowTitle",
        "foregroundTitle",
        "signalType",
    ] {
        let value = value_string(&event.data, key);
        if !value.is_empty() {
            return value;
        }
    }
    "активность".into()
}

fn is_real_evidence(event: &AwEvent) -> bool {
    let signal = value_string(&event.data, "signalType").to_lowercase();
    if matches!(
        signal.as_str(),
        "collector_health" | "self_test" | "heartbeat" | "health"
    ) {
        return false;
    }
    ["url", "title", "path", "filePath", "targetPath"]
        .iter()
        .any(|key| !value_string(&event.data, key).is_empty())
        || !signal.is_empty()
}

fn build_true_active_apps_from_events(
    config: &Config,
    window_events: &[AwEvent],
    afk_events: &[AwEvent],
    evidence_by_bucket: &HashMap<String, Vec<AwEvent>>,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Vec<Value> {
    let mut windows = Vec::new();
    let mut prev_key = None;
    for event in sorted_events(window_events, start, end) {
        let app = value_string(&event.data, "app")
            .if_empty(value_string(&event.data, "process"))
            .if_empty(value_string(&event.data, "processName"));
        let title =
            value_string(&event.data, "title").if_empty(value_string(&event.data, "windowTitle"));
        if app.is_empty() && title.is_empty() {
            continue;
        }
        let ts = event.timestamp.as_deref().and_then(parse_iso_utc).unwrap();
        let dur = event_duration(config, event, config.default_sample_seconds);
        let interval = (
            ts.max(start),
            (ts + TimeDelta::milliseconds((dur * 1000.0) as i64)).min(end + TimeDelta::seconds(1)),
        );
        if interval.1 <= interval.0 {
            continue;
        }
        let app_name = normalize_app_name(&app, &title);
        let key = (app_name.clone(), title.clone());
        let changed = prev_key.as_ref().is_some_and(|p| p != &key);
        prev_key = Some(key);
        windows.push((app_name, app, title, interval.0, interval.1, changed, ts));
    }
    let mut afk_intervals = Vec::new();
    for event in sorted_events(afk_events, start, end) {
        let status = value_string(&event.data, "status").to_lowercase();
        if !matches!(
            status.as_str(),
            "not-afk" | "not_afk" | "active" | "активно"
        ) {
            continue;
        }
        let ts = event.timestamp.as_deref().and_then(parse_iso_utc).unwrap();
        let dur = event_duration(config, event, 5.0);
        let interval = (
            ts.max(start),
            (ts + TimeDelta::milliseconds((dur * 1000.0) as i64)).min(end + TimeDelta::seconds(1)),
        );
        if interval.1 > interval.0 {
            afk_intervals.push(interval);
        }
    }
    afk_intervals = merge_intervals(afk_intervals);
    let mut evidence: HashMap<String, Vec<(DateTime<Utc>, String)>> = HashMap::new();
    for (app_name, _raw, title, _s, _e, changed, ts) in &windows {
        if *changed {
            evidence
                .entry(app_name.clone())
                .or_default()
                .push((*ts, title.clone()));
        }
    }
    for events in evidence_by_bucket.values() {
        for event in sorted_events(events, start, end) {
            if !is_real_evidence(event) {
                continue;
            }
            let ts = event.timestamp.as_deref().and_then(parse_iso_utc).unwrap();
            if let Some((app_name, _, _, _, _, _, _)) = windows
                .iter()
                .find(|(_, _, _, s, e, _, _)| *s <= ts && ts < *e)
            {
                evidence
                    .entry(app_name.clone())
                    .or_default()
                    .push((ts, event_context(event)));
            }
        }
    }
    let mut rows = Vec::new();
    let delta = TimeDelta::seconds(config.true_active_evidence_window_seconds);
    let apps: BTreeSet<String> = windows
        .iter()
        .map(|w| w.0.clone())
        .chain(evidence.keys().cloned())
        .collect();
    for app in apps {
        let app_evidence = evidence.get(&app).cloned().unwrap_or_default();
        if app_evidence.is_empty() {
            continue;
        }
        let evidence_windows = merge_intervals(
            app_evidence
                .iter()
                .map(|(ts, _)| (*ts - delta, *ts + delta))
                .collect(),
        );
        let mut proved = Vec::new();
        for window in windows.iter().filter(|w| w.0 == app) {
            for afk in &afk_intervals {
                if let Some(active) = overlap((window.3, window.4), *afk) {
                    for ev in &evidence_windows {
                        if let Some(p) = overlap(active, *ev) {
                            proved.push(p);
                        }
                    }
                }
            }
        }
        let proved = merge_intervals(proved);
        let seconds = proved
            .iter()
            .map(|(s, e)| (*e - *s).num_seconds())
            .sum::<i64>();
        if seconds <= 0 {
            continue;
        }
        let (last_ts, last_context) = app_evidence.last().cloned().unwrap();
        rows.push(json!({
            "application": app,
            "proved_work_seconds": seconds,
            "proved_work_hhmm": hhmm(seconds),
            "proved_work_human": human_duration_ru(seconds),
            "last_action_utc": to_iso_utc(last_ts),
            "last_action_local": last_ts.with_timezone(&config.offset).format("%H:%M").to_string(),
            "last_action": last_context,
            "evidence_events": app_evidence.len(),
        }));
    }
    rows.sort_by_key(|r| {
        (
            -r.get("proved_work_seconds")
                .and_then(Value::as_i64)
                .unwrap_or(0),
            r.get("application")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_lowercase(),
        )
    });
    rows
}

fn sorted_events(events: &[AwEvent], start: DateTime<Utc>, end: DateTime<Utc>) -> Vec<&AwEvent> {
    let mut out: Vec<&AwEvent> = events
        .iter()
        .filter(|e| {
            e.timestamp
                .as_deref()
                .and_then(parse_iso_utc)
                .is_some_and(|ts| ts >= start && ts <= end)
        })
        .collect();
    out.sort_by_key(|e| e.timestamp.clone());
    out
}

fn overlap(
    a: (DateTime<Utc>, DateTime<Utc>),
    b: (DateTime<Utc>, DateTime<Utc>),
) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let s = a.0.max(b.0);
    let e = a.1.min(b.1);
    if e > s { Some((s, e)) } else { None }
}

fn resolve_report_format(params: &Params, accept: &str) -> String {
    let requested = params.first("format").unwrap_or_default().to_lowercase();
    if matches!(requested.as_str(), "csv" | "html" | "json") {
        return requested;
    }
    let accept = accept.to_lowercase();
    if accept.contains("text/html") && !accept.contains("application/json") {
        "html".into()
    } else {
        "json".into()
    }
}

fn make_report_cache_key(
    path: &str,
    fmt: &str,
    host: &str,
    report_date: NaiveDate,
    day: &str,
    owner: &str,
    department: &str,
) -> String {
    format!("{path}|{fmt}|{host}|{report_date}|{day}|{owner}|{department}")
}

fn stable_hash_hex(value: &str) -> String {
    let mut h: u64 = 1469598103934665603;
    for b in value.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    format!("{h:016x}")
}

fn history_component(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "default".to_string()
    } else {
        out.chars().take(80).collect()
    }
}

fn load_management_history_point(path: &Path) -> Option<Value> {
    let payload: Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    payload.get("point").cloned().map(sanitize_trend_point)
}

fn sanitize_trend_point(mut point: Value) -> Value {
    let Some(object) = point.as_object_mut() else {
        return point;
    };
    for key in ["department_rollups", "owner_rollups"] {
        let Some(items) = object.get(key).and_then(Value::as_array) else {
            continue;
        };
        object.insert(key.to_string(), json!(compact_rollup_points(items)));
    }
    point
}

fn today_csv(rows: &[Value]) -> String {
    let mut out = "user,user_id,active_seconds,active_hhmm,first_activity,last_activity,idle_seconds,sessions_count,samples_count,active_samples\n".to_string();
    for row in rows {
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{}\n",
            csv_cell(row["user"].as_str().unwrap_or("")),
            csv_cell(row["user_id"].as_str().unwrap_or("")),
            row["active_seconds"].as_i64().unwrap_or(0),
            row["active_hhmm"].as_str().unwrap_or(""),
            row["first_activity"].as_str().unwrap_or(""),
            row["last_activity"].as_str().unwrap_or(""),
            row["idle_seconds"].as_i64().unwrap_or(0),
            row["sessions_count"].as_i64().unwrap_or(0),
            row["samples_count"].as_i64().unwrap_or(0),
            row["active_samples"].as_i64().unwrap_or(0)
        ));
    }
    out
}

fn management_csv(payload: &Value) -> String {
    let mut out =
        "priority,owner,user_id,action_id,deadline_hint,reason,recommended_action\n".to_string();
    for action in payload
        .get("actions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        out.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            csv_cell(action["priority"].as_str().unwrap_or("")),
            csv_cell(action["owner"].as_str().unwrap_or("")),
            csv_cell(action["user_id"].as_str().unwrap_or("")),
            csv_cell(action["action_id"].as_str().unwrap_or("")),
            csv_cell(action["deadline_hint"].as_str().unwrap_or("")),
            csv_cell(action["reason"].as_str().unwrap_or("")),
            csv_cell(action["recommended_action"].as_str().unwrap_or(""))
        ));
    }
    out
}

fn csv_cell(value: &str) -> String {
    if value.contains([',', '"', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn esc(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn render_today_html(
    _config: &Config,
    rows: &[Value],
    host: &str,
    report_date: NaiveDate,
    selected_day: Option<&str>,
    true_apps: &[Value],
) -> String {
    let summary = build_report_summary(rows);
    let cards = format!(
        "<div class='card'><b>Пользователи</b><strong>{}</strong></div><div class='card'><b>Активное время</b><strong>{}</strong></div><div class='card'><b>Лидер дня</b><strong>{}</strong></div>",
        summary["users_count"],
        summary["total_active_hhmm"].as_str().unwrap_or("00:00"),
        esc(summary["top_user"].as_str().unwrap_or(""))
    );
    let app_rows = if true_apps.is_empty() {
        "<tr><td colspan='3'>Пока нет доказанной активной работы по приложениям за выбранную дату.</td></tr>".to_string()
    } else {
        true_apps
            .iter()
            .map(|r| {
                format!(
                    "<tr><td>{}</td><td class='good'>{}</td><td>{} · {}</td></tr>",
                    esc(r["application"].as_str().unwrap_or("-")),
                    esc(r["proved_work_human"].as_str().unwrap_or("0 сек")),
                    esc(r["last_action_local"].as_str().unwrap_or("-")),
                    esc(r["last_action"].as_str().unwrap_or("-"))
                )
            })
            .collect::<String>()
    };
    let user_rows = if rows.is_empty() {
        "<tr><td colspan='9'>За выбранную дату данных пока нет.</td></tr>".to_string()
    } else {
        rows.iter().map(|r| format!("<tr><td>{}</td><td>{}</td><td class='good'>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>", esc(r["user"].as_str().unwrap_or("")), esc(r["user_id"].as_str().unwrap_or("")), esc(r["active_hhmm"].as_str().unwrap_or("")), r["active_seconds"], esc(r["first_activity"].as_str().unwrap_or("")), esc(r["last_activity"].as_str().unwrap_or("")), r["idle_seconds"], r["sessions_count"], r["samples_count"])).collect::<String>()
    };
    let day_query = selected_day
        .filter(|d| matches!(*d, "today" | "yesterday"))
        .map(|d| format!("day={d}"))
        .unwrap_or_else(|| format!("date={report_date}"));
    format!(
        r#"<!doctype html><html lang="ru"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>AW-rus Отчёт по работе в RDP</title><style>{}</style></head><body><main><section class="hero"><h1>Отчёт по работе в RDP</h1><p>Хост: {} · Дата: {} · Часовой пояс: Europe/Moscow · Сформировано: {}</p><nav><a href="/reports/worktime/today?format=html&host={}&day=today">Сегодня</a><a href="/reports/worktime/today?format=html&host={}&day=yesterday">Вчера</a><a href="/reports/worktime/today?format=csv&host={}&{}">CSV</a><a href="/reports/worktime/today?format=json&host={}&{}">JSON</a><a href="/reports/worktime/management?format=html&host={}&{}">Управленческий отчёт</a></nav><div class="grid">{}</div></section><section><h2>Доказанная работа по приложениям</h2><table><thead><tr><th>Приложение</th><th>Доказанная работа</th><th>Последнее действие</th></tr></thead><tbody>{}</tbody></table></section><section><h2>Таблица по пользователям</h2><table><thead><tr><th>Пользователь</th><th>Учётная запись</th><th>Активно</th><th>Активно, сек</th><th>Начало</th><th>Конец</th><th>Простой</th><th>Сессии</th><th>Сэмплы</th></tr></thead><tbody>{}</tbody></table></section></main></body></html>"#,
        base_css(),
        esc(host),
        report_date,
        Local::now().format("%F %T"),
        esc(host),
        esc(host),
        esc(host),
        day_query,
        esc(host),
        day_query,
        esc(host),
        day_query,
        cards,
        app_rows,
        user_rows
    )
}

fn render_management_html(payload: &Value) -> String {
    let rows = payload["rows"].as_array().cloned().unwrap_or_default();
    let actions = payload["actions"].as_array().cloned().unwrap_or_default();
    let sources = payload["sources"].as_array().cloned().unwrap_or_default();
    let action_rows = if actions.is_empty() {
        "<tr><td colspan='6'>Нет действий.</td></tr>".to_string()
    } else {
        actions
            .iter()
            .map(|a| {
                format!(
                    "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                    esc(a["priority"].as_str().unwrap_or("")),
                    esc(a["owner"].as_str().unwrap_or("")),
                    esc(a["action_id"].as_str().unwrap_or("")),
                    esc(a["deadline_hint"].as_str().unwrap_or("")),
                    esc(a["reason"].as_str().unwrap_or("")),
                    esc(a["recommended_action"].as_str().unwrap_or(""))
                )
            })
            .collect()
    };
    let user_rows = if rows.is_empty() {
        "<tr><td colspan='8'>Нет сотрудников в выборке.</td></tr>".to_string()
    } else {
        rows.iter().map(|r| format!("<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td class='good'>{}</td><td>{}%</td><td>{}</td><td>{}</td></tr>", esc(r["user"].as_str().unwrap_or("")), esc(r["manager_owner"].as_str().unwrap_or("")), esc(r["department"].as_str().unwrap_or("")), esc(r["status"].as_str().unwrap_or("")), esc(r["workday_active_hhmm"].as_str().unwrap_or("")), r["coverage_pct"].as_f64().unwrap_or(0.0), esc(r["workday_first_activity_local"].as_str().unwrap_or("")), esc(r["workday_last_activity_local"].as_str().unwrap_or("")))).collect()
    };
    let source_rows = sources
        .iter()
        .map(|s| {
            format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                esc(s["label"].as_str().unwrap_or("")),
                esc(s["status_label"].as_str().unwrap_or("")),
                esc(s["bucket_id"].as_str().unwrap_or("")),
                esc(s["timestamp"].as_str().unwrap_or("")),
                s["age_seconds"],
                esc(s["summary"].as_str().unwrap_or(""))
            )
        })
        .collect::<String>();
    format!(
        r#"<!doctype html><html lang="ru"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>AW-rus Управленческий отчёт по работе в RDP</title><style>{}</style></head><body><main><section class="hero"><h1>AW-rus Управленческий отчёт по работе в RDP</h1><p>{} · {} · {}</p><h2>Что делать сегодня</h2><p>{}</p><nav><a href="/reports/worktime/management?format=json&host={}">JSON</a><a href="/reports/worktime/management?format=csv&host={}">CSV</a><a href="/reports/worktime/today?format=html&host={}">Классический отчёт</a><a href="/reports/worktime/management?format=html&host={}">Сбросить</a></nav></section><section><h2>Очередь действий руководителя</h2><table><tbody>{}</tbody></table></section><section><h2>Сотрудники</h2><table><tbody>{}</tbody></table></section><section><h2>Тренд за период</h2><p>Тренд за {} дней</p></section><section><h2>По ответственным</h2><pre>{}</pre></section><section><h2>Ответственные и эскалация</h2><pre>{}</pre></section><section><h2>По подразделениям</h2><pre>{}</pre></section><section><h2>Свежесть источников данных</h2><table><tbody>{}</tbody></table></section><p>Фильтр: {}</p></main></body></html>"#,
        base_css(),
        esc(payload["host"].as_str().unwrap_or("")),
        esc(payload["report_date"].as_str().unwrap_or("")),
        esc(payload["report_timezone"].as_str().unwrap_or("")),
        esc(payload
            .pointer("/executive/headline")
            .and_then(Value::as_str)
            .unwrap_or("")),
        esc(payload["host"].as_str().unwrap_or("")),
        esc(payload["host"].as_str().unwrap_or("")),
        esc(payload["host"].as_str().unwrap_or("")),
        esc(payload["host"].as_str().unwrap_or("")),
        action_rows,
        user_rows,
        payload["trend"].as_array().map(|a| a.len()).unwrap_or(0),
        esc(&payload["owner_rollups"].to_string()),
        esc(&payload["owner_roster"].to_string()),
        esc(&payload["department_rollups"].to_string()),
        source_rows,
        esc(&payload["filters"].to_string())
    )
}

fn base_css() -> &'static str {
    "body{margin:0;background:#f5f7fb;color:#172033;font:14px/1.45 'Segoe UI',Arial,sans-serif}main{max-width:1360px;margin:0 auto;padding:20px}.hero{background:#12324a;color:white;border-radius:8px;padding:18px 20px;margin-bottom:16px}.hero a{color:white;margin-right:12px}.grid{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:12px}.card{background:rgba(255,255,255,.12);border:1px solid rgba(255,255,255,.18);border-radius:8px;padding:12px}.card b{display:block;color:#dbeafe}.card strong{font-size:22px}section{background:white;border:1px solid #dbe3ee;border-radius:8px;margin:14px 0;padding:14px;overflow:auto}table{border-collapse:collapse;width:100%}th,td{border-bottom:1px solid #e5eaf2;padding:9px 10px;text-align:left;vertical-align:top}.good{color:#0f766e;font-weight:700}@media(max-width:800px){main{padding:10px}.grid{grid-template-columns:1fr}table{min-width:900px}}"
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("valid header")
}

fn respond(
    request: Request,
    status: u16,
    data: Vec<u8>,
    content_type: &str,
    extra_headers: Vec<(String, String)>,
) {
    let mut response = Response::new(
        StatusCode(status),
        vec![
            header("Content-Type", content_type),
            header("Content-Length", &data.len().to_string()),
        ],
        Cursor::new(data),
        None,
        None,
    );
    for (key, value) in extra_headers {
        response.add_header(header(&key, &value));
    }
    let _ = request.respond(response);
}

fn handle(app: &App, request: Request) {
    if request.method() != &Method::Get {
        respond(
            request,
            405,
            Vec::new(),
            "text/plain; charset=utf-8",
            Vec::new(),
        );
        return;
    }
    let url = request.url().to_string();
    let (path, query) = url.split_once('?').unwrap_or((&url, ""));
    if path == "/health" || path == "/api/health" {
        let payload = json!({
            "ok": true,
            "generated_at_utc": to_iso_utc(Utc::now()),
            "report_timezone": "Europe/Moscow",
            "default_host": app.config.default_host,
            "aw_api_base": app.config.aw_api_base,
        });
        respond(
            request,
            200,
            serde_json::to_vec_pretty(&payload).unwrap(),
            "application/json; charset=utf-8",
            Vec::new(),
        );
        return;
    }
    if path.starts_with("/dlp-ioc/") {
        let name = path.rsplit('/').next().unwrap_or("");
        if !matches!(
            name,
            "ioc_blacklist.json" | "ioc_blacklist.csv" | "ioc_blacklist.sql"
        ) {
            respond(
                request,
                404,
                Vec::new(),
                "text/plain; charset=utf-8",
                Vec::new(),
            );
            return;
        }
        let file = app.config.ioc_dir.join(name);
        match fs::read(file) {
            Ok(data) => {
                let ctype = if name.ends_with(".json") {
                    "application/json; charset=utf-8"
                } else if name.ends_with(".csv") {
                    "text/csv; charset=utf-8"
                } else {
                    "text/plain; charset=utf-8"
                };
                respond(request, 200, data, ctype, Vec::new());
            }
            Err(_) => respond(
                request,
                404,
                Vec::new(),
                "text/plain; charset=utf-8",
                Vec::new(),
            ),
        }
        return;
    }
    if path != "/reports/worktime/today" && path != "/reports/worktime/management" {
        respond(
            request,
            404,
            Vec::new(),
            "text/plain; charset=utf-8",
            Vec::new(),
        );
        return;
    }
    let params = Params::parse(query);
    let accept = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Accept"))
        .map(|h| h.value.as_str())
        .unwrap_or("");
    let (data, content_type, headers) = app.report_response(path, &params, accept);
    let status = if content_type.starts_with("application/json")
        && serde_json::from_slice::<Value>(&data)
            .ok()
            .and_then(|v| v.get("error").cloned())
            .is_some()
    {
        503
    } else {
        200
    };
    respond(request, status, data, &content_type, headers);
}

fn main() -> Result<()> {
    let _cli = Cli::parse();
    let config = load_config();
    let addr = format!("{}:{}", config.listen_host, config.listen_port);
    let app = App::new(config)?;
    let server = Server::http(&addr).map_err(|error| anyhow!("bind {addr}: {error}"))?;
    println!("{} listening on {}", Local::now().format("%F %T"), addr);
    for request in server.incoming_requests() {
        let app = app.clone();
        std::thread::spawn(move || handle(&app, request));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> Config {
        load_config()
    }

    fn event(ts: &str, username: &str, sid: i64, active: bool, extra: Value) -> AwEvent {
        let mut data = Map::new();
        data.insert("username".into(), json!(username));
        data.insert("userId".into(), json!(format!("WORKGROUP\\{username}")));
        data.insert("sessionId".into(), json!(sid));
        data.insert(
            "state".into(),
            json!(if active { "Активно" } else { "Диск" }),
        );
        data.insert("active".into(), json!(active));
        if let Some(extra) = extra.as_object() {
            for (k, v) in extra {
                data.insert(k.clone(), v.clone());
            }
        }
        AwEvent {
            timestamp: Some(ts.into()),
            duration: Some(0.0),
            data: Value::Object(data),
        }
    }

    #[test]
    fn aggregate_rows_merges_overlap() {
        let cfg = test_config();
        let start = parse_iso_utc("2026-05-14T06:00:00Z").unwrap();
        let end = parse_iso_utc("2026-05-14T06:59:59Z").unwrap();
        let rows = aggregate_rows(
            &cfg,
            &[
                event(
                    "2026-05-14T06:00:00Z",
                    "user5",
                    4,
                    true,
                    json!({"sampleSeconds":30}),
                ),
                event(
                    "2026-05-14T06:00:30Z",
                    "user5",
                    4,
                    true,
                    json!({"sampleSeconds":30}),
                ),
                event(
                    "2026-05-14T06:00:15Z",
                    "user5",
                    5,
                    true,
                    json!({"sampleSeconds":30}),
                ),
            ],
            start,
            end,
            "HOST-EXAMPLE",
            false,
        );
        assert_eq!(rows[0]["active_seconds"], 60);
        assert_eq!(rows[0]["sessions_count"], 2);
    }

    #[test]
    fn report_format_prefers_html_for_browser() {
        assert_eq!(
            resolve_report_format(&Params::default(), "text/html,application/xhtml+xml"),
            "html"
        );
        assert_eq!(
            resolve_report_format(&Params::parse("format=json"), "text/html"),
            "json"
        );
    }

    #[test]
    fn host_detection_uses_worktime_session_bucket() {
        let payload = json!({
            "aw-watcher-window_HOST-EXAMPLE": {},
            "aw-worktime-sessions_HOST-EXAMPLE": {},
            "aw-worktime-sessions_WORKSTATION-01": {}
        });
        assert_eq!(
            host_from_buckets_payload(&payload),
            Some("WORKSTATION-01".to_string())
        );
    }

    #[test]
    fn management_insights_detect_falling_portfolio_trend() {
        let cfg = test_config();
        let trend = vec![
            json!({"report_date": "2026-05-12", "portfolio_coverage_pct": 80.0}),
            json!({"report_date": "2026-05-13", "portfolio_coverage_pct": 65.0}),
            json!({"report_date": "2026-05-14", "portfolio_coverage_pct": 50.0}),
        ];
        let insights = build_management_insights(
            &cfg,
            &json!({
                "calendar_total_active_seconds": 0,
                "workday_total_active_seconds": 0
            }),
            &[],
            &[],
            &[],
            &trend,
            NaiveDate::from_ymd_opt(2026, 5, 14).unwrap(),
        );
        assert!(
            insights
                .iter()
                .any(|item| item["code"] == "portfolio_activity_falling")
        );
    }

    #[test]
    fn management_history_ignores_action_only_rollups() {
        let cfg = test_config();
        let trend = ["2026-05-12", "2026-05-13", "2026-05-14"]
            .into_iter()
            .map(|date| {
                json!({
                    "report_date": date,
                    "portfolio_coverage_pct": 50.0,
                    "department_rollups": [
                        {"name": "ops", "users_count": 0, "portfolio_coverage_pct": 0.0},
                        {"name": "Отдел продаж", "users_count": 2, "portfolio_coverage_pct": 10.0}
                    ],
                    "owner_rollups": [
                        {"name": "ops", "users_count": 0, "portfolio_coverage_pct": 0.0},
                        {"name": "Руководитель", "users_count": 1, "portfolio_coverage_pct": 10.0}
                    ]
                })
            })
            .collect::<Vec<_>>();
        let insights = build_management_insights(
            &cfg,
            &json!({
                "calendar_total_active_seconds": 0,
                "workday_total_active_seconds": 0
            }),
            &[],
            &[],
            &[],
            &trend,
            NaiveDate::from_ymd_opt(2026, 5, 14).unwrap(),
        );
        assert!(!insights.iter().any(|item| item["subject"] == "ops"));
        assert!(
            insights
                .iter()
                .any(|item| item["subject"] == "Отдел продаж")
        );
        assert!(
            insights
                .iter()
                .any(|item| item["subject"] == "Руководитель")
        );
    }

    #[test]
    fn compact_rollup_points_hides_action_only_groups() {
        let points = compact_rollup_points(&[
            json!({"name": "ops", "users_count": 0, "portfolio_coverage_pct": 0.0, "actions_count": 2}),
            json!({"name": "Отдел продаж", "users_count": 2, "portfolio_coverage_pct": 50.0, "actions_count": 1}),
        ]);
        assert_eq!(points.len(), 1);
        assert_eq!(points[0]["name"], "Отдел продаж");
    }

    #[test]
    fn build_rollups_does_not_create_action_only_groups() {
        let rows = vec![json!({
            "user": "user1",
            "department": "Отдел продаж",
            "manager_owner": "Руководитель",
            "status": "below_target",
            "workday_active_seconds": 3600
        })];
        let actions = vec![
            action(
                "source_freshness_review",
                "critical",
                "ops",
                "today",
                "техническая проверка источника",
                "проверить сервис",
                "",
                json!({}),
            ),
            action(
                "manager_review",
                "high",
                "Руководитель",
                "today",
                "проверка руководителя",
                "разобрать нагрузку",
                "",
                json!({}),
            ),
        ];
        let rollups = build_rollups(&rows, &actions, "manager_owner");
        assert_eq!(rollups.len(), 1);
        assert_eq!(rollups[0]["name"], "Руководитель");
        assert_eq!(rollups[0]["actions_count"], 1);
    }

    #[test]
    fn stale_legacy_rdp_sources_are_covered_by_fresh_rust_sources() {
        assert!(legacy_rdp_covered_by_rust_sources(
            "rdp_window",
            "fail",
            true,
            true
        ));
        assert!(legacy_rdp_covered_by_rust_sources(
            "rdp_afk", "warn", true, true
        ));
        assert!(!legacy_rdp_covered_by_rust_sources(
            "rdp_window",
            "fail",
            true,
            false
        ));
        assert!(!legacy_rdp_covered_by_rust_sources(
            "worktime_sessions",
            "fail",
            true,
            true
        ));
    }

    #[test]
    fn sanitize_trend_point_removes_legacy_action_only_rollups() {
        let point = sanitize_trend_point(json!({
            "department_rollups": [
                {"name": "ops", "users_count": 0, "portfolio_coverage_pct": 0.0},
                {"name": "Отдел продаж", "users_count": 2, "portfolio_coverage_pct": 50.0}
            ],
            "owner_rollups": [
                {"name": "ops", "users_count": 0, "portfolio_coverage_pct": 0.0},
                {"name": "Руководитель", "users_count": 1, "portfolio_coverage_pct": 50.0}
            ]
        }));
        assert_eq!(point["department_rollups"].as_array().unwrap().len(), 1);
        assert_eq!(point["owner_rollups"].as_array().unwrap().len(), 1);
        assert_eq!(point["department_rollups"][0]["name"], "Отдел продаж");
        assert_eq!(point["owner_rollups"][0]["name"], "Руководитель");
    }

    #[test]
    fn interpretation_policy_accepts_fraction_thresholds() {
        let policy: InterpretationPolicy = serde_json::from_value(json!({
            "overload_threshold": 0.92,
            "underload_threshold": 0.45,
            "drop_threshold_pct": 20,
            "night_work_after": "20:00",
            "weekend_work": true
        }))
        .unwrap();
        assert_eq!(threshold_to_pct(policy.overload_threshold.unwrap()), 92.0);
        assert_eq!(threshold_to_pct(policy.underload_threshold.unwrap()), 45.0);
        assert_eq!(threshold_to_pct(policy.drop_threshold_pct.unwrap()), 20.0);
        assert_eq!(
            parse_hhmm(policy.night_work_after.as_deref().unwrap()).unwrap(),
            NaiveTime::from_hms_opt(20, 0, 0).unwrap()
        );
        assert_eq!(policy.weekend_work, Some(true));
    }

    #[test]
    fn weekend_policy_can_disable_weekend_work_insight() {
        let mut cfg = test_config();
        cfg.manager_weekend_work_enabled = false;
        let insights = build_management_insights(
            &cfg,
            &json!({
                "calendar_total_active_seconds": 7200,
                "workday_total_active_seconds": 0
            }),
            &[],
            &[],
            &[],
            &[],
            NaiveDate::from_ymd_opt(2026, 5, 16).unwrap(),
        );
        assert!(!insights.iter().any(|item| item["code"] == "weekend_work"));
    }

    #[test]
    fn true_active_apps_require_evidence() {
        let cfg = test_config();
        let start = parse_iso_utc("2026-05-14T06:00:00Z").unwrap();
        let end = parse_iso_utc("2026-05-14T07:59:59Z").unwrap();
        let windows = vec![
            AwEvent {
                timestamp: Some("2026-05-14T06:00:00Z".into()),
                duration: Some(120.0),
                data: json!({"app":"1cv8.exe","title":"ИНФОВЕСТ"}),
            },
            AwEvent {
                timestamp: Some("2026-05-14T06:02:00Z".into()),
                duration: Some(180.0),
                data: json!({"app":"1cv8.exe","title":"Счета учета: Материалы"}),
            },
        ];
        let afk = vec![AwEvent {
            timestamp: Some("2026-05-14T06:00:00Z".into()),
            duration: Some(600.0),
            data: json!({"status":"not-afk"}),
        }];
        let rows =
            build_true_active_apps_from_events(&cfg, &windows, &afk, &HashMap::new(), start, end);
        assert_eq!(rows[0]["application"], "1С");
        assert_eq!(rows[0]["proved_work_seconds"], 300);
    }

    #[test]
    fn management_history_accumulates_daily_trend_points() {
        let mut cfg = test_config();
        let dir = std::env::temp_dir().join(format!(
            "aw-worktime-history-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        cfg.management_history_dir = dir.clone();
        cfg.management_history_days = 31;
        cfg.management_history_retention_days = 120;
        let app = App::new(cfg).unwrap();
        for day in ["2026-05-12", "2026-05-13", "2026-05-14"] {
            let date = NaiveDate::parse_from_str(day, "%Y-%m-%d").unwrap();
            let payload = app.build_management_payload(Vec::new(), "HOST-EXAMPLE", date, "", "");
            assert_eq!(payload["history"]["saved_current_point"], true);
        }
        let payload = app.build_management_payload(
            Vec::new(),
            "HOST-EXAMPLE",
            NaiveDate::from_ymd_opt(2026, 5, 14).unwrap(),
            "",
            "",
        );
        let trend = payload["trend"].as_array().unwrap();
        assert_eq!(trend.len(), 3);
        assert_eq!(payload["history"]["points_count"], 3);
        assert_eq!(trend[0]["report_date"], "2026-05-12");
        assert_eq!(trend[2]["report_date"], "2026-05-14");
        let _ = fs::remove_dir_all(dir);
    }
}
