use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, SecondsFormat, TimeDelta, Utc};
use clap::Parser;
use reqwest::header::{CONNECTION, HeaderMap, HeaderValue};
use reqwest::{Method, StatusCode, blocking::Client};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const DEFAULT_AW_URL: &str = "http://127.0.0.1:5600";
const DEFAULT_HOST: &str = "SHARKON2025";
const DEFAULT_STATE_PATH: &str = "/var/lib/activitywatch/aw-worktime-ui-bridge-state.json";
const BRIDGE_SOURCE: &str = "aw-worktime-ui-bridge";

#[derive(Debug, Parser)]
#[command(about = "AW Worktime UI bridge (sessions -> afk/window)")]
struct Cli {
    #[arg(long)]
    dry_run: bool,

    #[arg(long)]
    json: bool,
}

#[derive(Debug, Clone)]
struct Config {
    aw_url: String,
    host: String,
    state_path: PathBuf,
    timeout_seconds: f64,
    watcher_fallback_enabled: bool,
    watcher_fallback_stale_seconds: f64,
    collector_health_max_age_seconds: f64,
    collector_health_query_limit: usize,
    foreground_context_cache_seconds: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct AwEvent {
    timestamp: Option<String>,
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    data: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct State {
    last_ts: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_foreground_context: Option<ForegroundContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ForegroundContext {
    app: String,
    title: String,
    timestamp: Option<String>,
}

#[derive(Debug, Serialize)]
struct RunSummary {
    ok: bool,
    dry_run: bool,
    host: String,
    input_events: usize,
    posted_afk: usize,
    posted_win: usize,
    watcher_afk_posted: usize,
    watcher_win_posted: usize,
    last_ts: Option<String>,
    state_saved: bool,
}

struct AwClient {
    base_url: String,
    client: Client,
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

fn load_config() -> Config {
    Config {
        aw_url: env("AW_SERVER_URL", DEFAULT_AW_URL)
            .trim_end_matches('/')
            .to_string(),
        host: env("AW_WORKTIME_HOST", DEFAULT_HOST),
        state_path: PathBuf::from(env("AW_WORKTIME_UI_BRIDGE_STATE", DEFAULT_STATE_PATH)),
        timeout_seconds: env_f64("AW_WORKTIME_UI_BRIDGE_TIMEOUT", 60.0),
        watcher_fallback_enabled: env_bool("AW_WORKTIME_UI_BRIDGE_WATCHER_FALLBACK", true),
        watcher_fallback_stale_seconds: env_f64(
            "AW_WORKTIME_UI_BRIDGE_WATCHER_STALE_SECONDS",
            600.0,
        ),
        collector_health_max_age_seconds: env_f64(
            "AW_WORKTIME_UI_BRIDGE_COLLECTOR_HEALTH_MAX_AGE_SECONDS",
            300.0,
        ),
        collector_health_query_limit: env_usize(
            "AW_WORKTIME_UI_BRIDGE_COLLECTOR_HEALTH_QUERY_LIMIT",
            200,
        ),
        foreground_context_cache_seconds: env_f64(
            "AW_WORKTIME_UI_BRIDGE_FOREGROUND_CACHE_SECONDS",
            900.0,
        ),
    }
}

impl Config {
    fn sessions_bucket(&self) -> String {
        format!("aw-worktime-sessions_{}", self.host)
    }
    fn afk_bucket(&self) -> String {
        format!("aw-rdp-afk_{}", self.host)
    }
    fn window_bucket(&self) -> String {
        format!("aw-rdp-window_{}", self.host)
    }
    fn watcher_afk_bucket(&self) -> String {
        format!("aw-watcher-afk_{}", self.host)
    }
    fn watcher_window_bucket(&self) -> String {
        format!("aw-watcher-window_{}", self.host)
    }
    fn web_category_bucket(&self) -> String {
        format!("aw-detmir-web-category_{}", self.host)
    }
}

impl AwClient {
    fn new(config: &Config) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(CONNECTION, HeaderValue::from_static("close"));
        let client = Client::builder()
            .timeout(Duration::from_secs_f64(config.timeout_seconds.max(0.001)))
            .no_proxy()
            .pool_max_idle_per_host(0)
            .default_headers(headers)
            .build()
            .context("build HTTP client")?;
        Ok(Self {
            base_url: config.aw_url.clone(),
            client,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn request_json(
        &self,
        method: Method,
        path: &str,
        payload: Option<Value>,
    ) -> Result<Option<Value>> {
        let mut last_error = None;
        let mut response = None;
        for attempt in 1..=3 {
            let mut request = self.client.request(method.clone(), self.url(path));
            if let Some(payload) = payload.clone() {
                request = request.json(&payload);
            }
            match request.send() {
                Ok(ok_response) => {
                    response = Some(ok_response);
                    break;
                }
                Err(error) => {
                    last_error = Some(error);
                    if attempt < 3 {
                        eprintln!("warn request={path} retry_after_send_error");
                        thread::sleep(Duration::from_millis(750));
                    }
                }
            }
        }
        let response = match response {
            Some(response) => response,
            None => {
                let error = last_error.expect("error exists after failed request");
                return Err(error).with_context(|| format!("request ActivityWatch {path}"));
            }
        };
        let status = response.status();
        if status == StatusCode::NOT_FOUND {
            return Err(anyhow!("not found: {path}"));
        }
        if !status.is_success() {
            return Err(anyhow!("ActivityWatch {path} returned HTTP {status}"));
        }
        let bytes = response.bytes().context("read ActivityWatch response")?;
        if bytes.is_empty() {
            Ok(None)
        } else {
            serde_json::from_slice(&bytes)
                .map(Some)
                .with_context(|| format!("decode ActivityWatch JSON from {path}"))
        }
    }

    fn get_events(&self, bucket_id: &str, limit: usize) -> Result<Vec<AwEvent>> {
        let path = format!("/api/0/buckets/{bucket_id}/events?limit={limit}");
        match self.request_json(Method::GET, &path, None) {
            Ok(Some(value)) => serde_json::from_value(value).context("decode AW events"),
            Ok(None) => Ok(Vec::new()),
            Err(error) if error.to_string().starts_with("not found:") => Ok(Vec::new()),
            Err(error) => Err(error),
        }
    }

    fn get_latest_bucket_event(&self, bucket_id: &str) -> Result<Option<AwEvent>> {
        Ok(self.get_events(bucket_id, 1)?.into_iter().next())
    }

    fn ensure_bucket(
        &self,
        bucket_id: &str,
        event_type: &str,
        client_name: &str,
        host: &str,
    ) -> Result<()> {
        let payload = json!({
            "client": client_name,
            "type": event_type,
            "hostname": host,
        });
        let path = format!("/api/0/buckets/{bucket_id}");
        let response = match self.client.post(self.url(&path)).json(&payload).send() {
            Ok(response) => response,
            Err(error) => {
                eprintln!("warn ensure_bucket={bucket_id} skipped: {error}");
                return Ok(());
            }
        };
        let status = response.status();
        if status.is_success()
            || status == StatusCode::NOT_MODIFIED
            || status == StatusCode::CONFLICT
        {
            Ok(())
        } else {
            eprintln!("warn ensure_bucket={bucket_id} returned HTTP {status}; continuing");
            Ok(())
        }
    }

    fn post_events(&self, bucket_id: &str, events: &[AwEvent]) -> Result<()> {
        let path = format!("/api/0/buckets/{bucket_id}/events");
        self.request_json(Method::POST, &path, Some(json!(events)))
            .map(|_| ())
    }
}

fn load_state(path: &Path) -> State {
    let fallback = State {
        last_ts: "1970-01-01T00:00:00Z".to_string(),
        last_foreground_context: None,
    };
    let Ok(raw) = fs::read_to_string(path) else {
        return fallback;
    };
    let Ok(state) = serde_json::from_str::<State>(&raw) else {
        return fallback;
    };
    if state.last_ts.trim().is_empty() {
        fallback
    } else {
        state
    }
}

fn save_state(path: &Path, state: &State) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let tmp = PathBuf::from(format!("{}.tmp", path.display()));
    let raw = serde_json::to_vec(state).context("encode state")?;
    fs::write(&tmp, raw).with_context(|| format!("write {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("replace {}", path.display()))?;
    Ok(())
}

fn parse_iso_utc(ts: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(ts)
        .map(|parsed| parsed.with_timezone(&Utc))
        .with_context(|| format!("parse timestamp {ts}"))
}

fn to_iso_utc(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn normalize_event_ts(ts: &str) -> String {
    parse_iso_utc(ts)
        .map(|dt| to_iso_utc(dt - TimeDelta::nanoseconds(dt.timestamp_subsec_nanos().into())))
        .unwrap_or_else(|_| ts.to_string())
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

fn value_i64(data: &Value, key: &str) -> Option<i64> {
    match data.get(key) {
        Some(Value::Number(value)) => value.as_i64(),
        Some(Value::String(value)) => value.trim().parse().ok(),
        _ => None,
    }
}

fn is_session_active(data: &Value) -> bool {
    if data.get("active").and_then(Value::as_bool).unwrap_or(false) {
        return true;
    }
    let state = value_string(data, "state").to_lowercase();
    if state == "active" || state == "активно" {
        return true;
    }
    if state == "unknown" {
        let sid = value_i64(data, "sessionId").unwrap_or(-1);
        let user = value_string(data, "username").to_lowercase();
        let session_name = value_string(data, "sessionName").to_lowercase();
        return sid > 0
            && !user.is_empty()
            && !user.ends_with('$')
            && (session_name.starts_with("rdp-") || session_name == "console");
    }
    false
}

fn build_window_title(users: &[String], active_count: usize) -> String {
    if users.is_empty() {
        "RDP idle".to_string()
    } else {
        format!("RDP active ({active_count}): {}", users.join(", "))
    }
}

fn get_latest_active_session_ids(events: &[AwEvent]) -> BTreeSet<i64> {
    let mut grouped: BTreeMap<DateTime<Utc>, Vec<&AwEvent>> = BTreeMap::new();
    for event in events {
        let Some(ts) = event.timestamp.as_deref() else {
            continue;
        };
        let Ok(parsed) = parse_iso_utc(ts) else {
            continue;
        };
        grouped.entry(parsed).or_default().push(event);
    }
    let Some((_, latest_events)) = grouped.iter().next_back() else {
        return BTreeSet::new();
    };
    latest_events
        .iter()
        .filter(|event| is_session_active(&event.data))
        .filter_map(|event| value_i64(&event.data, "sessionId"))
        .collect()
}

fn normalize_foreground_context(data: &Value) -> Option<ForegroundContext> {
    let foreground_process = value_string(data, "foregroundProcess");
    let foreground_title = value_string(data, "foregroundTitle");
    if foreground_process.is_empty() && foreground_title.is_empty() {
        return None;
    }
    let app = if foreground_process.ends_with(".exe") {
        foreground_process.clone()
    } else {
        format!("{foreground_process}.exe")
    };
    Some(ForegroundContext {
        app,
        title: if foreground_title.is_empty() {
            foreground_process
        } else {
            foreground_title
        },
        timestamp: None,
    })
}

fn get_latest_foreground_context(
    aw: &AwClient,
    config: &Config,
    now_utc: DateTime<Utc>,
    active_session_ids: &BTreeSet<i64>,
    state: &State,
) -> Result<Option<ForegroundContext>> {
    let mut recent_candidates: Vec<(Option<i64>, DateTime<Utc>, ForegroundContext)> = Vec::new();
    for event in aw
        .get_events(
            &config.web_category_bucket(),
            config.collector_health_query_limit,
        )?
        .into_iter()
        .rev()
    {
        let data = &event.data;
        if value_string(data, "signalType").to_lowercase() != "collector_health" {
            continue;
        }
        let Some(ts) = event.timestamp.as_deref() else {
            continue;
        };
        let Ok(event_dt) = parse_iso_utc(ts) else {
            continue;
        };
        if (now_utc - event_dt).num_milliseconds() as f64 / 1000.0
            > config.collector_health_max_age_seconds
        {
            continue;
        }
        let Some(normalized) = normalize_foreground_context(data) else {
            continue;
        };
        recent_candidates.push((value_i64(data, "sessionId"), event_dt, normalized));
    }

    for (session_id, event_dt, mut normalized) in recent_candidates.iter().cloned() {
        if !active_session_ids.is_empty()
            && session_id.is_some_and(|sid| active_session_ids.contains(&sid))
        {
            normalized.timestamp = Some(to_iso_utc(event_dt));
            return Ok(Some(normalized));
        }
    }

    if let Some((_, event_dt, mut normalized)) = recent_candidates.into_iter().next() {
        normalized.timestamp = Some(to_iso_utc(event_dt));
        return Ok(Some(normalized));
    }

    if let Some(cached) = &state.last_foreground_context {
        if let Some(cached_ts) = cached
            .timestamp
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            if let Ok(cached_dt) = parse_iso_utc(cached_ts) {
                let age = (now_utc - cached_dt).num_milliseconds() as f64 / 1000.0;
                if age <= config.foreground_context_cache_seconds
                    && (!cached.app.trim().is_empty() || !cached.title.trim().is_empty())
                {
                    return Ok(Some(ForegroundContext {
                        app: if cached.app.trim().is_empty() {
                            "RDP".to_string()
                        } else {
                            cached.app.trim().to_string()
                        },
                        title: if cached.title.trim().is_empty() {
                            cached.app.trim().to_string()
                        } else {
                            cached.title.trim().to_string()
                        },
                        timestamp: Some(cached_ts.to_string()),
                    }));
                }
            }
        }
    }
    Ok(None)
}

fn transform(
    events: &[AwEvent],
    foreground_context: Option<&ForegroundContext>,
) -> (Vec<AwEvent>, Vec<AwEvent>, Option<String>) {
    let mut out_afk = Vec::new();
    let mut out_win = Vec::new();
    let mut last_ts = None;
    let mut grouped: BTreeMap<String, Vec<&AwEvent>> = BTreeMap::new();
    for event in events {
        let Some(ts) = event.timestamp.as_deref() else {
            continue;
        };
        grouped
            .entry(normalize_event_ts(ts))
            .or_default()
            .push(event);
    }
    let ordered_ts: Vec<String> = grouped.keys().cloned().collect();
    let parsed_ts: BTreeMap<String, Option<DateTime<Utc>>> = ordered_ts
        .iter()
        .map(|ts| (ts.clone(), parse_iso_utc(ts).ok()))
        .collect();

    for (idx, ts) in ordered_ts.iter().enumerate() {
        let rows = &grouped[ts];
        let src_duration = rows
            .iter()
            .map(|event| event.duration.unwrap_or(0.0))
            .fold(0.0, f64::max);
        let mut duration = src_duration;
        let cur_dt = parsed_ts.get(ts).and_then(|dt| *dt);
        let next_dt = ordered_ts
            .get(idx + 1)
            .and_then(|next_ts| parsed_ts.get(next_ts))
            .and_then(|dt| *dt);
        let next_gap = match (cur_dt, next_dt) {
            (Some(cur_dt), Some(next_dt)) => {
                Some(((next_dt - cur_dt).num_milliseconds() as f64 / 1000.0).max(0.0))
            }
            _ => None,
        };
        if duration <= 0.0 {
            if let Some(next_gap) = next_gap {
                duration = next_gap;
            }
            if duration <= 0.0 {
                duration = 10.0;
            }
        } else if let Some(next_gap) = next_gap.filter(|value| *value > 0.0) {
            duration = duration.min(next_gap);
        }
        duration = duration.min(30.0);

        let mut active_users: BTreeSet<String> = BTreeSet::new();
        for row in rows {
            let user = value_string(&row.data, "username");
            if !user.is_empty() && is_session_active(&row.data) {
                active_users.insert(user);
            }
        }
        let active_users: Vec<String> = active_users.into_iter().collect();
        let active_count = active_users.len();
        let is_active = active_count > 0;

        out_afk.push(AwEvent {
            timestamp: Some(ts.clone()),
            duration: Some(duration),
            data: json!({
                "status": if is_active { "not-afk" } else { "afk" },
                "source": BRIDGE_SOURCE,
            }),
        });

        let win_data = if is_active {
            if let Some(context) = foreground_context {
                let mut title = context.title.trim().to_string();
                if active_count > 1 {
                    let rdp_title = build_window_title(&active_users, active_count);
                    title = if title.is_empty() {
                        rdp_title
                    } else {
                        format!("{title} | {rdp_title}")
                    };
                }
                json!({
                    "app": if context.app.trim().is_empty() { "RDP" } else { context.app.trim() },
                    "title": if title.is_empty() { build_window_title(&active_users, active_count) } else { title },
                    "source": BRIDGE_SOURCE,
                })
            } else {
                json!({
                    "app": "RDP",
                    "title": build_window_title(&active_users, active_count),
                    "source": BRIDGE_SOURCE,
                })
            }
        } else {
            json!({
                "app": "RDP",
                "title": build_window_title(&active_users, active_count),
                "source": BRIDGE_SOURCE,
            })
        };
        out_win.push(AwEvent {
            timestamp: Some(ts.clone()),
            duration: Some(duration),
            data: win_data,
        });
        last_ts = Some(ts.clone());
    }
    (out_afk, out_win, last_ts)
}

fn normalize_watcher_window_events(win_events: &[AwEvent]) -> Vec<AwEvent> {
    let mut normalized = Vec::new();
    for event in win_events {
        let app = value_string(&event.data, "app");
        if app.is_empty() || app.to_uppercase() == "RDP" {
            continue;
        }
        let mut data = event.data.clone();
        let title = value_string(&data, "title");
        if let Some((prefix, _)) = title.split_once(" | RDP active (") {
            if let Some(map) = data.as_object_mut() {
                map.insert(
                    "title".to_string(),
                    Value::String(prefix.trim().to_string()),
                );
            }
        }
        let mut cloned = event.clone();
        cloned.data = data;
        normalized.push(cloned);
    }
    normalized
}

fn get_latest_bucket_event_ts(aw: &AwClient, bucket_id: &str) -> Result<Option<DateTime<Utc>>> {
    let Some(event) = aw.get_latest_bucket_event(bucket_id)? else {
        return Ok(None);
    };
    let Some(ts) = event.timestamp.as_deref() else {
        return Ok(None);
    };
    Ok(parse_iso_utc(ts).ok())
}

fn bucket_needs_fallback(
    aw: &AwClient,
    bucket_id: &str,
    now_utc: DateTime<Utc>,
    stale_after_seconds: f64,
) -> Result<bool> {
    let Some(latest_dt) = get_latest_bucket_event_ts(aw, bucket_id)? else {
        return Ok(true);
    };
    Ok((now_utc - latest_dt).num_milliseconds() as f64 / 1000.0 >= stale_after_seconds)
}

fn watcher_window_needs_bridge_sync(
    aw: &AwClient,
    config: &Config,
    now_utc: DateTime<Utc>,
) -> Result<bool> {
    let bucket_id = config.watcher_window_bucket();
    let Some(latest_event) = aw.get_latest_bucket_event(&bucket_id)? else {
        return Ok(true);
    };
    let Some(latest_dt) = get_latest_bucket_event_ts(aw, &bucket_id)? else {
        return Ok(true);
    };
    if (now_utc - latest_dt).num_milliseconds() as f64 / 1000.0
        >= config.watcher_fallback_stale_seconds
    {
        return Ok(true);
    }

    let source = value_string(&latest_event.data, "source").to_lowercase();
    Ok(source == BRIDGE_SOURCE)
}

fn run(cli: &Cli) -> Result<RunSummary> {
    let config = load_config();
    let aw = AwClient::new(&config)?;
    let state = load_state(&config.state_path);
    let last_dt = parse_iso_utc(&state.last_ts)
        .unwrap_or_else(|_| DateTime::from_timestamp(0, 0).expect("valid unix epoch"));

    if !cli.dry_run {
        aw.ensure_bucket(
            &config.afk_bucket(),
            "afkstatus",
            BRIDGE_SOURCE,
            &config.host,
        )?;
        aw.ensure_bucket(
            &config.window_bucket(),
            "currentwindow",
            BRIDGE_SOURCE,
            &config.host,
        )?;
    }

    let now_utc = Utc::now();
    let recent = aw.get_events(&config.sessions_bucket(), 5000)?;
    if recent.is_empty() {
        return Ok(RunSummary {
            ok: true,
            dry_run: cli.dry_run,
            host: config.host,
            input_events: 0,
            posted_afk: 0,
            posted_win: 0,
            watcher_afk_posted: 0,
            watcher_win_posted: 0,
            last_ts: None,
            state_saved: false,
        });
    }

    let events: Vec<AwEvent> = recent
        .into_iter()
        .filter(|event| {
            event
                .timestamp
                .as_deref()
                .and_then(|ts| parse_iso_utc(ts).ok())
                .is_some_and(|ts| ts > last_dt)
        })
        .collect();
    if events.is_empty() {
        return Ok(RunSummary {
            ok: true,
            dry_run: cli.dry_run,
            host: config.host,
            input_events: 0,
            posted_afk: 0,
            posted_win: 0,
            watcher_afk_posted: 0,
            watcher_win_posted: 0,
            last_ts: None,
            state_saved: false,
        });
    }

    let active_session_ids = get_latest_active_session_ids(&events);
    let foreground_context =
        get_latest_foreground_context(&aw, &config, now_utc, &active_session_ids, &state)?;
    let (afk_events, win_events, new_last_ts) = transform(&events, foreground_context.as_ref());
    let Some(new_last_ts) = new_last_ts else {
        return Ok(RunSummary {
            ok: true,
            dry_run: cli.dry_run,
            host: config.host,
            input_events: events.len(),
            posted_afk: 0,
            posted_win: 0,
            watcher_afk_posted: 0,
            watcher_win_posted: 0,
            last_ts: None,
            state_saved: false,
        });
    };
    if afk_events.is_empty() || win_events.is_empty() {
        return Ok(RunSummary {
            ok: true,
            dry_run: cli.dry_run,
            host: config.host,
            input_events: events.len(),
            posted_afk: 0,
            posted_win: 0,
            watcher_afk_posted: 0,
            watcher_win_posted: 0,
            last_ts: Some(new_last_ts),
            state_saved: false,
        });
    }

    let watcher_win_events = normalize_watcher_window_events(&win_events);
    let mut watcher_afk_posted = 0;
    let mut watcher_win_posted = 0;

    if !cli.dry_run {
        aw.post_events(&config.afk_bucket(), &afk_events)?;
        aw.post_events(&config.window_bucket(), &win_events)?;
    }

    if config.watcher_fallback_enabled {
        if bucket_needs_fallback(
            &aw,
            &config.watcher_afk_bucket(),
            now_utc,
            config.watcher_fallback_stale_seconds,
        )? {
            watcher_afk_posted = afk_events.len();
            if !cli.dry_run {
                aw.ensure_bucket(
                    &config.watcher_afk_bucket(),
                    "afkstatus",
                    "aw-watcher-afk",
                    &config.host,
                )?;
                aw.post_events(&config.watcher_afk_bucket(), &afk_events)?;
            }
        }
        if !watcher_win_events.is_empty()
            && watcher_window_needs_bridge_sync(&aw, &config, now_utc)?
        {
            watcher_win_posted = watcher_win_events.len();
            if !cli.dry_run {
                aw.ensure_bucket(
                    &config.watcher_window_bucket(),
                    "currentwindow",
                    "aw-watcher-window",
                    &config.host,
                )?;
                aw.post_events(&config.watcher_window_bucket(), &watcher_win_events)?;
            }
        }
    }

    let next_context = foreground_context.or(state.last_foreground_context);
    let mut next_state = State {
        last_ts: new_last_ts.clone(),
        last_foreground_context: None,
    };
    if let Some(context) = next_context {
        next_state.last_foreground_context = Some(ForegroundContext {
            app: context.app,
            title: context.title,
            timestamp: context.timestamp.or_else(|| Some(to_iso_utc(now_utc))),
        });
    }
    if !cli.dry_run {
        save_state(&config.state_path, &next_state)?;
    }

    Ok(RunSummary {
        ok: true,
        dry_run: cli.dry_run,
        host: config.host,
        input_events: events.len(),
        posted_afk: afk_events.len(),
        posted_win: win_events.len(),
        watcher_afk_posted,
        watcher_win_posted,
        last_ts: Some(new_last_ts),
        state_saved: !cli.dry_run,
    })
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let summary = run(&cli)?;
    if cli.json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else if summary.posted_afk > 0 || summary.posted_win > 0 {
        println!(
            "posted_afk={} posted_win={} last_ts={}",
            summary.posted_afk,
            summary.posted_win,
            summary.last_ts.as_deref().unwrap_or("")
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(ts: &str, duration: f64, data: Value) -> AwEvent {
        AwEvent {
            timestamp: Some(ts.to_string()),
            duration: Some(duration),
            data,
        }
    }

    #[test]
    fn latest_active_session_ids_uses_latest_timestamp_group() {
        let events = vec![
            event(
                "2026-05-27T07:59:25Z",
                0.0,
                json!({"sessionId": 2, "state": "Активно", "username": "администратор", "sessionName": "console"}),
            ),
            event(
                "2026-05-27T07:59:30Z",
                0.0,
                json!({"sessionId": 2, "state": "Активно", "username": "администратор", "sessionName": "console"}),
            ),
            event(
                "2026-05-27T07:59:30Z",
                0.0,
                json!({"sessionId": 3, "state": "Активно", "username": "user5", "sessionName": "rdp-tcp#0"}),
            ),
            event(
                "2026-05-27T07:59:30Z",
                0.0,
                json!({"sessionId": 4, "state": "Диск", "username": "user1", "sessionName": ""}),
            ),
        ];
        assert_eq!(
            get_latest_active_session_ids(&events),
            BTreeSet::from([2, 3])
        );
    }

    #[test]
    fn transform_uses_foreground_context_for_active_sessions() {
        let events = vec![event(
            "2026-05-27T07:59:30Z",
            0.0,
            json!({"username": "user5", "state": "Активно", "sessionId": 3, "sessionName": "rdp-tcp#0"}),
        )];
        let ctx = ForegroundContext {
            app: "totalcmd.exe".to_string(),
            title: "Total Commander 6.01 - HARVEST".to_string(),
            timestamp: None,
        };
        let (afk_events, win_events, last_ts) = transform(&events, Some(&ctx));
        assert_eq!(value_string(&afk_events[0].data, "status"), "not-afk");
        assert_eq!(value_string(&win_events[0].data, "app"), "totalcmd.exe");
        assert_eq!(
            value_string(&win_events[0].data, "title"),
            "Total Commander 6.01 - HARVEST"
        );
        assert_eq!(last_ts.as_deref(), Some("2026-05-27T07:59:30Z"));
    }

    #[test]
    fn transform_caps_duration_at_next_timestamp_gap() {
        let events = vec![
            event(
                "2026-05-27T07:59:30Z",
                5.0,
                json!({"username": "user5", "state": "Активно", "sessionId": 3, "sessionName": "rdp-tcp#0"}),
            ),
            event(
                "2026-05-27T07:59:31Z",
                5.0,
                json!({"username": "user5", "state": "Активно", "sessionId": 3, "sessionName": "rdp-tcp#0"}),
            ),
        ];
        let ctx = ForegroundContext {
            app: "totalcmd.exe".to_string(),
            title: "Total Commander 6.01 - HARVEST".to_string(),
            timestamp: None,
        };
        let (_, win_events, _) = transform(&events, Some(&ctx));
        assert_eq!(win_events[0].duration, Some(1.0));
        assert_eq!(win_events[1].duration, Some(5.0));
    }

    #[test]
    fn normalize_watcher_window_events_strips_rdp_suffix_for_real_apps() {
        let events = vec![
            event(
                "2026-05-27T08:10:00Z",
                5.0,
                json!({"app": "totalcmd.exe", "title": "Total Commander 6.01 - HARVEST | RDP active (2): user5, администратор", "source": BRIDGE_SOURCE}),
            ),
            event(
                "2026-05-27T08:10:05Z",
                5.0,
                json!({"app": "RDP", "title": "RDP active (2): user5, администратор", "source": BRIDGE_SOURCE}),
            ),
        ];
        let normalized = normalize_watcher_window_events(&events);
        assert_eq!(
            value_string(&normalized[0].data, "title"),
            "Total Commander 6.01 - HARVEST"
        );
        assert_eq!(normalized.len(), 1);
    }

    #[test]
    fn unknown_rdp_session_counts_as_active() {
        let data = json!({"sessionId": 5, "state": "Unknown", "username": "user5", "sessionName": "rdp-tcp#0"});
        assert!(is_session_active(&data));
    }

    #[test]
    fn machine_unknown_session_is_not_active() {
        let data = json!({"sessionId": 5, "state": "Unknown", "username": "HOST$", "sessionName": "rdp-tcp#0"});
        assert!(!is_session_active(&data));
    }
}
