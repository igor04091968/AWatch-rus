use std::{
    collections::{BTreeMap, BTreeSet},
    process::Command,
    thread,
    time::Duration,
};

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Datelike, FixedOffset, SecondsFormat, TimeZone, Utc};
use clap::Parser;
use reqwest::header::{CONNECTION, HeaderMap, HeaderValue};
use reqwest::{
    Method, StatusCode,
    blocking::{Client, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const DEFAULT_AW_URL: &str = "http://127.0.0.1:5600";
const DEFAULT_HOST: &str = "HOST-EXAMPLE";
const AUTOHEAL_SOURCE: &str = "aw-worktime-autoheal";

#[derive(Debug, Parser)]
#[command(about = "AW Worktime autoheal")]
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
    worktime_health_url: String,
    worktime_report_timeout_seconds: f64,
    management_warm_enabled: bool,
    management_warm_url: String,
    management_warm_timeout_seconds: f64,
    today_probe_enabled: bool,
    today_probe_url: String,
    today_probe_timeout_seconds: f64,
    session_freshness_seconds: f64,
    aw_timeout_seconds: f64,
    aw_post_chunk_size: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct AwEvent {
    timestamp: Option<String>,
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    data: Value,
}

#[derive(Debug, Serialize)]
struct RunSummary {
    ok: bool,
    dry_run: bool,
    host: String,
    worktime_ok: bool,
    api_restarted: bool,
    management_warm_ok: Option<bool>,
    need_heal: bool,
    ui_bridge_started: bool,
    reset_buckets: bool,
    backfill_afk: usize,
    backfill_window: usize,
    reason: String,
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

fn env_bool_legacy(name: &str, fallback: bool) -> bool {
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
        .filter(|value| *value > 0.0)
        .unwrap_or(fallback)
}

fn env_usize(name: &str, fallback: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}

fn load_config() -> Config {
    Config {
        aw_url: env("AW_URL", DEFAULT_AW_URL)
            .trim_end_matches('/')
            .to_string(),
        host: env("AW_WORKTIME_HOST", DEFAULT_HOST),
        worktime_health_url: env("WORKTIME_HEALTH_URL", "http://127.0.0.1:5610/health"),
        worktime_report_timeout_seconds: env_f64("WORKTIME_REPORT_TIMEOUT_SECONDS", 20.0),
        management_warm_enabled: env_bool_legacy("WORKTIME_MANAGEMENT_WARM_ENABLED", true),
        management_warm_url: env(
            "WORKTIME_MANAGEMENT_WARM_URL",
            "http://127.0.0.1:5610/reports/worktime/management?day=today&format=json",
        ),
        management_warm_timeout_seconds: env_f64("WORKTIME_MANAGEMENT_WARM_TIMEOUT_SECONDS", 60.0),
        today_probe_enabled: env_bool_legacy("WORKTIME_TODAY_PROBE_ENABLED", true),
        today_probe_url: env(
            "WORKTIME_TODAY_PROBE_URL",
            "http://127.0.0.1:5610/reports/worktime/today?day=today&format=json",
        ),
        today_probe_timeout_seconds: env_f64("WORKTIME_TODAY_PROBE_TIMEOUT_SECONDS", 20.0),
        session_freshness_seconds: env_f64("WORKTIME_SESSION_FRESHNESS_SECONDS", 600.0),
        aw_timeout_seconds: env_f64("WORKTIME_AUTOHEAL_AW_TIMEOUT_SECONDS", 30.0),
        aw_post_chunk_size: env_usize("WORKTIME_AUTOHEAL_AW_POST_CHUNK_SIZE", 500),
    }
}

impl Config {
    fn window_bucket(&self) -> String {
        format!("aw-rdp-window_{}", self.host)
    }

    fn afk_bucket(&self) -> String {
        format!("aw-rdp-afk_{}", self.host)
    }

    fn session_bucket(&self) -> String {
        format!("aw-worktime-sessions_{}", self.host)
    }
}

impl AwClient {
    fn new(config: &Config) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(CONNECTION, HeaderValue::from_static("close"));
        let client = Client::builder()
            .timeout(Duration::from_secs_f64(
                config.aw_timeout_seconds.max(0.001),
            ))
            .no_proxy()
            .pool_max_idle_per_host(0)
            .default_headers(headers)
            .build()
            .context("build ActivityWatch HTTP client")?;
        Ok(Self {
            base_url: config.aw_url.clone(),
            client,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn send_retry(&self, method: Method, path: &str, payload: Option<Value>) -> Result<Response> {
        let mut last_error = None;
        for attempt in 1..=3 {
            let mut request = self.client.request(method.clone(), self.url(path));
            if let Some(payload) = payload.clone() {
                request = request.json(&payload);
            }
            match request.send() {
                Ok(response) => return Ok(response),
                Err(error) => {
                    last_error = Some(error);
                    if attempt < 3 {
                        log(&format!("warn aw request={path} retry_after_send_error"));
                        thread::sleep(Duration::from_millis(750));
                    }
                }
            }
        }
        Err(last_error.expect("error exists after failed request"))
            .with_context(|| format!("request ActivityWatch {path}"))
    }

    fn request_json(
        &self,
        method: Method,
        path: &str,
        payload: Option<Value>,
        ignore_not_found: bool,
    ) -> Result<Option<Value>> {
        let response = self.send_retry(method, path, payload)?;
        let status = response.status();
        if ignore_not_found && status == StatusCode::NOT_FOUND {
            return Ok(None);
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
        match self.request_json(Method::GET, &path, None, true)? {
            Some(value) => serde_json::from_value(value).context("decode AW events"),
            None => Ok(Vec::new()),
        }
    }

    fn delete_bucket(&self, bucket_id: &str) -> Result<()> {
        let path = format!("/api/0/buckets/{bucket_id}");
        let response = self.send_retry(Method::DELETE, &path, None)?;
        if response.status().is_success() || response.status() == StatusCode::NOT_FOUND {
            Ok(())
        } else {
            Err(anyhow!(
                "delete bucket {bucket_id} returned HTTP {}",
                response.status()
            ))
        }
    }

    fn ensure_bucket(
        &self,
        bucket_id: &str,
        event_type: &str,
        client_name: &str,
        host: &str,
    ) -> Result<()> {
        let path = format!("/api/0/buckets/{bucket_id}");
        let payload = json!({
            "client": client_name,
            "type": event_type,
            "hostname": host,
        });
        let response = self.send_retry(Method::POST, &path, Some(payload))?;
        let status = response.status();
        if status.is_success()
            || status == StatusCode::NOT_MODIFIED
            || status == StatusCode::CONFLICT
        {
            Ok(())
        } else {
            Err(anyhow!("ensure bucket {bucket_id} returned HTTP {status}"))
        }
    }

    fn post_events_chunked(
        &self,
        bucket_id: &str,
        events: &[AwEvent],
        chunk_size: usize,
    ) -> Result<()> {
        for chunk in events.chunks(chunk_size.max(1)) {
            let path = format!("/api/0/buckets/{bucket_id}/events");
            self.request_json(Method::POST, &path, Some(json!(chunk)), false)?;
        }
        Ok(())
    }
}

fn log(message: &str) {
    println!("{} {}", chrono::Local::now().format("%F %T"), message);
}

fn build_probe_client(timeout_seconds: f64) -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs_f64(timeout_seconds.max(0.001)))
        .no_proxy()
        .build()
        .context("build probe HTTP client")
}

fn probe_url(url: &str, timeout_seconds: f64) -> bool {
    let Ok(client) = build_probe_client(timeout_seconds) else {
        return false;
    };
    client
        .get(url)
        .send()
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}

fn probe_reports(config: &Config) -> bool {
    if !probe_url(
        &config.worktime_health_url,
        config.worktime_report_timeout_seconds,
    ) {
        return false;
    }
    if config.today_probe_enabled
        && !probe_url(&config.today_probe_url, config.today_probe_timeout_seconds)
    {
        return false;
    }
    true
}

fn run_systemctl(args: &[&str], dry_run: bool) -> bool {
    if dry_run {
        log(&format!("dry-run systemctl {}", args.join(" ")));
        return true;
    }
    Command::new("systemctl")
        .args(args)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn parse_iso_utc(ts: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(ts)
        .map(|parsed| parsed.with_timezone(&Utc))
        .with_context(|| format!("parse timestamp {ts}"))
}

fn to_iso_utc(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn moscow_day_start_utc() -> DateTime<Utc> {
    let msk = FixedOffset::east_opt(3 * 3600).expect("valid Moscow offset");
    let today = Utc::now().with_timezone(&msk);
    msk.with_ymd_and_hms(today.year(), today.month(), today.day(), 0, 0, 0)
        .single()
        .expect("valid day start")
        .with_timezone(&Utc)
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

fn active_users_at_latest_session(events: &[AwEvent]) -> (Option<DateTime<Utc>>, BTreeSet<String>) {
    let mut latest_ts = None;
    let mut active_users = BTreeSet::new();
    for event in events {
        let Some(ts) = event.timestamp.as_deref() else {
            continue;
        };
        let Ok(cur) = parse_iso_utc(ts) else {
            continue;
        };
        if latest_ts.is_none_or(|latest| cur > latest) {
            latest_ts = Some(cur);
            active_users.clear();
        }
        if latest_ts == Some(cur) {
            let user = value_string(&event.data, "username");
            if !user.is_empty() && is_session_active(&event.data) {
                active_users.insert(user);
            }
        }
    }
    (latest_ts, active_users)
}

fn active_window_duration_today(window_events: &[AwEvent], start: DateTime<Utc>) -> f64 {
    window_events
        .iter()
        .filter(|event| {
            event
                .timestamp
                .as_deref()
                .and_then(|ts| parse_iso_utc(ts).ok())
                .is_some_and(|ts| ts >= start)
        })
        .filter(|event| {
            value_string(&event.data, "title")
                .to_lowercase()
                .contains("rdp active")
        })
        .map(|event| event.duration.unwrap_or(0.0).max(0.0))
        .sum()
}

fn should_heal(
    aw: &AwClient,
    config: &Config,
    now: DateTime<Utc>,
    start: DateTime<Utc>,
) -> Result<(bool, String)> {
    let window_events = match aw.get_events(&config.window_bucket(), 12_000) {
        Ok(events) => events,
        Err(error) => {
            return Ok((true, format!("window_read_failed: {error:#}")));
        }
    };
    let session_events = match aw.get_events(&config.session_bucket(), 12_000) {
        Ok(events) => events,
        Err(error) => {
            return Ok((true, format!("session_read_failed: {error:#}")));
        }
    };
    let (latest_ts, active_users) = active_users_at_latest_session(&session_events);
    let Some(latest_ts) = latest_ts else {
        return Ok((false, "no_sessions".to_string()));
    };
    let age = (now - latest_ts).num_milliseconds() as f64 / 1000.0;
    if age > config.session_freshness_seconds {
        return Ok((false, format!("sessions_stale age_seconds={age:.0}")));
    }
    if active_users.is_empty() {
        return Ok((false, "no_active_users".to_string()));
    }
    let active = active_window_duration_today(&window_events, start);
    if active <= 0.0 {
        Ok((
            true,
            format!("zero_activity active_users={}", active_users.len()),
        ))
    } else {
        Ok((
            false,
            format!("activity_present active_seconds={active:.0}"),
        ))
    }
}

fn build_window_title(users: &[String]) -> String {
    if users.is_empty() {
        "RDP idle".to_string()
    } else {
        format!("RDP active ({}): {}", users.len(), users.join(", "))
    }
}

fn build_backfill(
    session_events: Vec<AwEvent>,
    start: DateTime<Utc>,
) -> (Vec<AwEvent>, Vec<AwEvent>) {
    let mut grouped: BTreeMap<DateTime<Utc>, Vec<AwEvent>> = BTreeMap::new();
    for event in session_events {
        let Some(ts) = event.timestamp.as_deref() else {
            continue;
        };
        let Ok(parsed) = parse_iso_utc(ts) else {
            continue;
        };
        if parsed >= start {
            grouped.entry(parsed).or_default().push(event);
        }
    }
    let keys: Vec<DateTime<Utc>> = grouped.keys().copied().collect();
    let mut out_afk = Vec::new();
    let mut out_win = Vec::new();
    for (idx, ts) in keys.iter().enumerate() {
        let rows = &grouped[ts];
        let mut duration = keys
            .get(idx + 1)
            .map(|next| ((*next - *ts).num_milliseconds() as f64 / 1000.0).max(0.0))
            .unwrap_or(10.0);
        if duration <= 0.0 {
            duration = 10.0;
        }
        duration = duration.min(30.0);
        let users: Vec<String> = rows
            .iter()
            .filter_map(|event| {
                let user = value_string(&event.data, "username");
                if !user.is_empty() && is_session_active(&event.data) {
                    Some(user)
                } else {
                    None
                }
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let active = !users.is_empty();
        let ts = to_iso_utc(*ts);
        out_afk.push(AwEvent {
            timestamp: Some(ts.clone()),
            duration: Some(duration),
            data: json!({
                "status": if active { "not-afk" } else { "afk" },
                "source": AUTOHEAL_SOURCE,
            }),
        });
        out_win.push(AwEvent {
            timestamp: Some(ts),
            duration: Some(duration),
            data: json!({
                "app": "RDP",
                "title": build_window_title(&users),
                "source": AUTOHEAL_SOURCE,
            }),
        });
    }
    (out_afk, out_win)
}

fn reset_bucket(
    aw: &AwClient,
    bucket_id: &str,
    event_type: &str,
    client_name: &str,
    host: &str,
) -> Result<()> {
    if let Err(error) = aw.delete_bucket(bucket_id) {
        log(&format!(
            "warn delete bucket {bucket_id} ignored: {error:#}"
        ));
    }
    aw.ensure_bucket(bucket_id, event_type, client_name, host)
}

fn run(cli: &Cli) -> Result<RunSummary> {
    let config = load_config();
    let aw = AwClient::new(&config)?;
    let mut api_restarted = false;
    let mut worktime_ok = true;

    if !probe_reports(&config) {
        log("worktime API probe failed, restarting aw-worktime-api.service");
        api_restarted = true;
        run_systemctl(&["restart", "aw-worktime-api.service"], cli.dry_run);
        thread::sleep(Duration::from_secs(2));
        if !probe_reports(&config) {
            log("worktime API still degraded after restart");
            worktime_ok = false;
        } else {
            log("worktime API recovered after restart");
        }
    }

    if !worktime_ok {
        log("skip warm/heal because worktime API is still unavailable");
        return Ok(RunSummary {
            ok: true,
            dry_run: cli.dry_run,
            host: config.host,
            worktime_ok,
            api_restarted,
            management_warm_ok: None,
            need_heal: false,
            ui_bridge_started: false,
            reset_buckets: false,
            backfill_afk: 0,
            backfill_window: 0,
            reason: "worktime_unavailable".to_string(),
        });
    }

    let management_warm_ok = if config.management_warm_enabled {
        let ok = probe_url(
            &config.management_warm_url,
            config.management_warm_timeout_seconds,
        );
        if ok {
            log("management cache warm ok");
        } else {
            log("management cache warm failed");
        }
        Some(ok)
    } else {
        None
    };

    let start = moscow_day_start_utc();
    let now = Utc::now();
    let (need_heal, reason) = should_heal(&aw, &config, now, start)?;
    if !need_heal {
        log(&format!(
            "health ok: {reason} for {}, no action",
            config.host
        ));
        return Ok(RunSummary {
            ok: true,
            dry_run: cli.dry_run,
            host: config.host,
            worktime_ok,
            api_restarted,
            management_warm_ok,
            need_heal: false,
            ui_bridge_started: false,
            reset_buckets: false,
            backfill_afk: 0,
            backfill_window: 0,
            reason,
        });
    }

    log(&format!(
        "detected zero activity for {}, running heal",
        config.host
    ));
    let mut ui_bridge_started =
        run_systemctl(&["restart", "aw-worktime-ui-bridge.timer"], cli.dry_run);
    ui_bridge_started &= run_systemctl(&["start", "aw-worktime-ui-bridge.service"], cli.dry_run);

    let session_events = aw.get_events(&config.session_bucket(), 12_000)?;
    let (afk_events, window_events) = build_backfill(session_events, start);
    if afk_events.is_empty() || window_events.is_empty() {
        log(&format!(
            "heal skipped for {}, no source sessions",
            config.host
        ));
        return Ok(RunSummary {
            ok: true,
            dry_run: cli.dry_run,
            host: config.host,
            worktime_ok,
            api_restarted,
            management_warm_ok,
            need_heal: true,
            ui_bridge_started,
            reset_buckets: false,
            backfill_afk: 0,
            backfill_window: 0,
            reason: "no_source_sessions".to_string(),
        });
    }

    if !cli.dry_run {
        reset_bucket(
            &aw,
            &config.afk_bucket(),
            "afkstatus",
            "aw-worktime-ui-bridge",
            &config.host,
        )?;
        reset_bucket(
            &aw,
            &config.window_bucket(),
            "currentwindow",
            "aw-worktime-ui-bridge",
            &config.host,
        )?;
        aw.post_events_chunked(&config.afk_bucket(), &afk_events, config.aw_post_chunk_size)?;
        aw.post_events_chunked(
            &config.window_bucket(),
            &window_events,
            config.aw_post_chunk_size,
        )?;
    }
    println!(
        "autoheal backfill posted afk={} win={}",
        afk_events.len(),
        window_events.len()
    );
    log(&format!("heal completed for {}", config.host));
    Ok(RunSummary {
        ok: true,
        dry_run: cli.dry_run,
        host: config.host,
        worktime_ok,
        api_restarted,
        management_warm_ok,
        need_heal: true,
        ui_bridge_started,
        reset_buckets: !cli.dry_run,
        backfill_afk: afk_events.len(),
        backfill_window: window_events.len(),
        reason,
    })
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let summary = run(&cli)?;
    if cli.json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
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
    fn unknown_rdp_session_counts_as_active() {
        assert!(is_session_active(&json!({
            "sessionId": 5,
            "state": "Unknown",
            "username": "user5",
            "sessionName": "rdp-tcp#0"
        })));
    }

    #[test]
    fn active_users_use_latest_sample_only() {
        let events = vec![
            event(
                "2026-05-27T07:59:25Z",
                0.0,
                json!({"sessionId": 2, "state": "Активно", "username": "admin", "sessionName": "console"}),
            ),
            event(
                "2026-05-27T07:59:30Z",
                0.0,
                json!({"sessionId": 3, "state": "Диск", "username": "old", "sessionName": ""}),
            ),
            event(
                "2026-05-27T07:59:30Z",
                0.0,
                json!({"sessionId": 4, "state": "Активно", "username": "user5", "sessionName": "rdp-tcp#0"}),
            ),
        ];
        let (latest, users) = active_users_at_latest_session(&events);
        assert_eq!(
            latest.map(to_iso_utc).as_deref(),
            Some("2026-05-27T07:59:30Z")
        );
        assert_eq!(users, BTreeSet::from(["user5".to_string()]));
    }

    #[test]
    fn active_window_duration_counts_today_rdp_active_titles() {
        let start = parse_iso_utc("2026-06-01T00:00:00Z").unwrap();
        let events = vec![
            event(
                "2026-05-31T23:59:59Z",
                30.0,
                json!({"title": "RDP active (1): user"}),
            ),
            event(
                "2026-06-01T00:00:00Z",
                10.0,
                json!({"title": "RDP active (1): user"}),
            ),
            event("2026-06-01T00:01:00Z", 20.0, json!({"title": "RDP idle"})),
        ];
        assert_eq!(active_window_duration_today(&events, start), 10.0);
    }

    #[test]
    fn build_backfill_caps_duration_and_marks_active() {
        let start = parse_iso_utc("2026-06-01T00:00:00Z").unwrap();
        let events = vec![
            event(
                "2026-06-01T00:00:00Z",
                0.0,
                json!({"sessionId": 3, "state": "Активно", "username": "user5", "sessionName": "rdp-tcp#0"}),
            ),
            event(
                "2026-06-01T00:00:45Z",
                0.0,
                json!({"sessionId": 3, "state": "Диск", "username": "user5", "sessionName": "rdp-tcp#0"}),
            ),
        ];
        let (afk, win) = build_backfill(events, start);
        assert_eq!(afk.len(), 2);
        assert_eq!(win[0].duration, Some(30.0));
        assert_eq!(value_string(&afk[0].data, "status"), "not-afk");
        assert_eq!(value_string(&win[0].data, "title"), "RDP active (1): user5");
        assert_eq!(value_string(&afk[1].data, "status"), "afk");
    }
}
