use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, SecondsFormat, TimeDelta, Utc};
use clap::Parser;
use detmir_aw_client::ActivityWatchClient;
use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const DEFAULT_AW_URL: &str = "http://127.0.0.1:5600/api/0";
const DEFAULT_CLICKHOUSE_URL: &str = "http://127.0.0.1:8123";
const DEFAULT_CLICKHOUSE_DATABASE: &str = "aw_workforce";
const DEFAULT_STATE_PATH: &str = "/var/lib/aw-workforce-ingest/state.json";
const WINDOW_TABLE: &str = "aw_window_events";
const BROWSER_TABLE: &str = "aw_browser_events";

const WINDOW_COLUMNS: &[&str] = &[
    "event_time",
    "host_name",
    "user_login",
    "process_name",
    "window_title",
    "duration_sec",
    "source_bucket",
    "source_event_id",
];

const BROWSER_COLUMNS: &[&str] = &[
    "event_time",
    "host_name",
    "user_login",
    "browser_name",
    "url",
    "title",
    "duration_sec",
    "source_bucket",
    "source_event_id",
];

#[derive(Debug, Parser)]
#[command(
    about = "Ingest ActivityWatch window/browser events into aw_workforce ClickHouse tables."
)]
pub struct Cli {
    #[arg(long, default_value = DEFAULT_AW_URL, env = "AW_WORKFORCE_AW_URL")]
    aw_url: String,

    #[arg(
        long,
        default_value = DEFAULT_CLICKHOUSE_URL,
        env = "AW_WORKFORCE_CLICKHOUSE_URL"
    )]
    clickhouse_url: String,

    #[arg(
        long,
        default_value = DEFAULT_CLICKHOUSE_DATABASE,
        env = "AW_WORKFORCE_CLICKHOUSE_DATABASE"
    )]
    clickhouse_database: String,

    #[arg(long, default_value = "", env = "AW_WORKFORCE_CLICKHOUSE_USER")]
    clickhouse_user: String,

    #[arg(long, default_value = "", env = "AW_WORKFORCE_CLICKHOUSE_PASSWORD")]
    clickhouse_password: String,

    #[arg(long, env = "AW_WORKFORCE_HOST")]
    host: String,

    #[arg(long = "window-bucket")]
    window_buckets: Vec<String>,

    #[arg(long = "browser-bucket")]
    browser_buckets: Vec<String>,

    #[arg(long, default_value = DEFAULT_STATE_PATH, env = "AW_WORKFORCE_STATE_PATH")]
    state_path: PathBuf,

    #[arg(long, default_value_t = 300, env = "AW_WORKFORCE_OVERLAP_SECONDS")]
    overlap_seconds: i64,

    #[arg(long, env = "AW_WORKFORCE_NO_STATE")]
    no_state: bool,

    #[arg(long)]
    since: Option<String>,

    #[arg(long)]
    until: Option<String>,

    #[arg(long, default_value_t = 24)]
    hours: i64,

    #[arg(long, default_value_t = 10_000)]
    limit: usize,

    #[arg(long, default_value_t = 30)]
    timeout_seconds: u64,

    #[arg(long, default_value_t = 3, env = "AW_WORKFORCE_RETRY_ATTEMPTS")]
    retry_attempts: usize,

    #[arg(long, default_value_t = 1000, env = "AW_WORKFORCE_RETRY_BACKOFF_MS")]
    retry_backoff_ms: u64,

    #[arg(long, env = "AW_WORKFORCE_FAIL_ON_EMPTY")]
    fail_on_empty: bool,

    #[arg(long)]
    dry_run: bool,

    #[arg(long)]
    json: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct Bucket {
    #[serde(default)]
    hostname: String,
}

#[derive(Debug, Deserialize)]
struct RawAwEvent {
    #[serde(default)]
    id: Option<Value>,
    timestamp: String,
    #[serde(default)]
    duration: f64,
    #[serde(default)]
    data: Value,
}

#[derive(Debug)]
struct AwEvent {
    bucket_id: String,
    bucket_hostname: String,
    event_id: String,
    timestamp: String,
    duration: f64,
    data: Value,
}

#[derive(Debug, Clone, Serialize)]
struct WindowRow {
    event_time: String,
    host_name: String,
    user_login: String,
    process_name: String,
    window_title: String,
    duration_sec: u32,
    source_bucket: String,
    source_event_id: String,
}

#[derive(Debug, Clone, Serialize)]
struct BrowserRow {
    event_time: String,
    host_name: String,
    user_login: String,
    browser_name: String,
    url: String,
    title: String,
    duration_sec: u32,
    source_bucket: String,
    source_event_id: String,
}

trait SourceKey {
    fn source_bucket(&self) -> &str;
    fn source_event_id(&self) -> &str;
}

impl SourceKey for WindowRow {
    fn source_bucket(&self) -> &str {
        &self.source_bucket
    }

    fn source_event_id(&self) -> &str {
        &self.source_event_id
    }
}

impl SourceKey for BrowserRow {
    fn source_bucket(&self) -> &str {
        &self.source_bucket
    }

    fn source_event_id(&self) -> &str {
        &self.source_event_id
    }
}

#[derive(Debug, Serialize)]
struct RunSummary {
    ok: bool,
    dry_run: bool,
    aw_url: String,
    clickhouse_url: String,
    clickhouse_database: String,
    host: String,
    state_path: String,
    state_enabled: bool,
    state_loaded_last_end: Option<String>,
    state_saved_last_end: Option<String>,
    overlap_seconds: i64,
    start: String,
    end: String,
    window_buckets: Vec<String>,
    browser_buckets: Vec<String>,
    missing_buckets: Vec<String>,
    window_events_fetched: usize,
    browser_events_fetched: usize,
    window_rows_filtered_invalid: usize,
    window_rows_prepared: usize,
    browser_rows_prepared: usize,
    window_rows_inserted: usize,
    browser_rows_inserted: usize,
    window_rows_skipped_existing: usize,
    browser_rows_skipped_existing: usize,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct State {
    #[serde(default)]
    last_end: Option<String>,
}

#[derive(Debug)]
struct ClickHouseClient {
    client: Client,
    base_url: String,
    database: String,
    username: String,
    password: String,
}

impl ClickHouseClient {
    fn new(
        base_url: &str,
        database: &str,
        username: &str,
        password: &str,
        timeout_seconds: u64,
    ) -> Result<Self> {
        let database = normalize_identifier(database, "ClickHouse database")?;
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_seconds))
            .no_proxy()
            .build()
            .context("build ClickHouse HTTP client")?;
        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            database,
            username: username.to_string(),
            password: password.to_string(),
        })
    }

    fn execute(&self, sql: &str) -> Result<String> {
        if sql.trim().is_empty() {
            return Ok(String::new());
        }
        let mut req = self
            .client
            .post(&self.base_url)
            .query(&[("database", self.database.as_str())])
            .body(sql.to_string());
        if !self.username.trim().is_empty() {
            req = req.basic_auth(self.username.clone(), Some(self.password.clone()));
        }
        let response = req.send().context("ClickHouse request")?;
        let status = response.status();
        let body = response.text().unwrap_or_default();
        if !status.is_success() {
            bail!("ClickHouse HTTP {status}: {}", body.trim());
        }
        Ok(body)
    }

    fn query_json_each_row<T: DeserializeOwned>(&self, sql: &str) -> Result<Vec<T>> {
        let body = self.execute(sql)?;
        body.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).with_context(|| "parse ClickHouse JSONEachRow"))
            .collect()
    }

    fn insert_json_each_row<T: Serialize>(
        &self,
        table: &str,
        columns: &[&str],
        rows: &[T],
    ) -> Result<usize> {
        if rows.is_empty() {
            return Ok(0);
        }
        let table_name = self.table_name(table)?;
        let mut payload = String::new();
        payload.push_str(&format!(
            "INSERT INTO {table_name} ({}) FORMAT JSONEachRow\n",
            columns.join(", ")
        ));
        for row in rows {
            payload.push_str(&serde_json::to_string(row)?);
            payload.push('\n');
        }
        self.execute(&payload)?;
        Ok(rows.len())
    }

    fn table_name(&self, table: &str) -> Result<String> {
        let table = normalize_identifier(table, "ClickHouse table")?;
        Ok(format!("{}.{}", self.database, table))
    }
}

#[derive(Debug, Deserialize)]
struct ExistingId {
    source_event_id: String,
}

pub fn run_from_args() -> Result<()> {
    let cli = Cli::parse();
    let json_output = cli.json
        || std::env::var("AW_WORKFORCE_JSON")
            .map(|value| matches!(value.as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);
    let summary = run_with_retries(cli)?;
    if summary.dry_run || summary.ok {
        if json_output || summary.dry_run {
            println!("{}", serde_json::to_string_pretty(&summary)?);
        } else {
            println!(
                "aw-workforce-ingest ok dry_run={} window_inserted={} browser_inserted={} window_existing={} browser_existing={} window_filtered_invalid={} missing_buckets={}",
                summary.dry_run,
                summary.window_rows_inserted,
                summary.browser_rows_inserted,
                summary.window_rows_skipped_existing,
                summary.browser_rows_skipped_existing,
                summary.window_rows_filtered_invalid,
                summary.missing_buckets.len()
            );
        }
    }
    Ok(())
}

fn run_with_retries(cli: Cli) -> Result<RunSummary> {
    let retry_attempts = cli.retry_attempts.max(1);
    let retry_backoff_ms = cli.retry_backoff_ms;
    let mut last_error = None;
    for attempt in 1..=retry_attempts {
        match run_once(&cli) {
            Ok(summary) => return Ok(summary),
            Err(err) => {
                last_error = Some(err);
                if attempt < retry_attempts {
                    std::thread::sleep(Duration::from_millis(
                        retry_backoff_ms.saturating_mul(attempt as u64),
                    ));
                }
            }
        }
    }
    Err(last_error.expect("at least one retry attempt"))
}

fn run_once(cli: &Cli) -> Result<RunSummary> {
    validate_cli(cli)?;
    let host = cli.host.trim().to_string();
    let state_enabled = !cli.no_state;
    let state = if state_enabled {
        load_state(&cli.state_path)?
    } else {
        State::default()
    };
    let state_loaded_last_end = state.last_end.clone();
    let end = match &cli.until {
        Some(value) => parse_time(value)?,
        None => Utc::now(),
    };
    let start = match &cli.since {
        Some(value) => parse_time(value)?,
        None if state_enabled => match &state.last_end {
            Some(last_end) => parse_time(last_end)? - TimeDelta::seconds(cli.overlap_seconds),
            None => end - TimeDelta::hours(cli.hours),
        },
        None => end - TimeDelta::hours(cli.hours),
    };
    if start >= end {
        bail!("--since must be earlier than --until");
    }

    let window_bucket_ids = default_window_buckets(&host, &cli.window_buckets);
    let browser_bucket_ids = default_browser_buckets(&host, &cli.browser_buckets);

    let aw_client =
        ActivityWatchClient::new(&cli.aw_url, Duration::from_secs(cli.timeout_seconds))?;
    let buckets: BTreeMap<String, Bucket> = aw_client.get_json("/buckets")?;
    let (window_buckets, mut missing_buckets) = resolve_buckets(&buckets, &window_bucket_ids);
    let (browser_buckets, browser_missing) = resolve_buckets(&buckets, &browser_bucket_ids);
    missing_buckets.extend(browser_missing);
    if window_buckets.is_empty() && browser_buckets.is_empty() {
        bail!("none of the requested ActivityWatch buckets exist");
    }

    let window_events = fetch_events(&aw_client, &window_buckets, start, end, cli.limit, "window")?;
    let browser_events = fetch_events(
        &aw_client,
        &browser_buckets,
        start,
        end,
        cli.limit,
        "browser",
    )?;

    let mut window_rows_filtered_invalid = 0usize;
    let window_rows = window_events
        .iter()
        .filter_map(|event| match map_window_row(event, &host) {
            Ok(row) if is_invalid_synthetic_window_event(event, &row) => {
                window_rows_filtered_invalid = window_rows_filtered_invalid.saturating_add(1);
                None
            }
            Ok(row) => Some(Ok(row)),
            Err(err) => Some(Err(err)),
        })
        .collect::<Result<Vec<_>>>()?;
    let browser_rows = browser_events
        .iter()
        .map(|event| map_browser_row(event, &host))
        .collect::<Result<Vec<_>>>()?;

    let mut window_rows_inserted = 0;
    let mut browser_rows_inserted = 0;
    let mut window_rows_skipped_existing = 0;
    let mut browser_rows_skipped_existing = 0;

    if !cli.dry_run {
        let clickhouse = ClickHouseClient::new(
            &cli.clickhouse_url,
            &cli.clickhouse_database,
            &cli.clickhouse_user,
            &cli.clickhouse_password,
            cli.timeout_seconds,
        )?;
        let (new_window_rows, skipped_window) =
            filter_existing_rows(&clickhouse, WINDOW_TABLE, &window_rows)?;
        let (new_browser_rows, skipped_browser) =
            filter_existing_rows(&clickhouse, BROWSER_TABLE, &browser_rows)?;
        window_rows_inserted =
            clickhouse.insert_json_each_row(WINDOW_TABLE, WINDOW_COLUMNS, &new_window_rows)?;
        browser_rows_inserted =
            clickhouse.insert_json_each_row(BROWSER_TABLE, BROWSER_COLUMNS, &new_browser_rows)?;
        window_rows_skipped_existing = skipped_window;
        browser_rows_skipped_existing = skipped_browser;
    }
    if cli.fail_on_empty && window_rows.is_empty() && browser_rows.is_empty() {
        bail!("no ActivityWatch events fetched for requested buckets and time range");
    }

    let mut state_saved_last_end = None;
    if state_enabled && !cli.dry_run {
        let next_state = State {
            last_end: Some(format_aw_timestamp(end)),
        };
        save_state(&cli.state_path, &next_state)?;
        state_saved_last_end = next_state.last_end;
    }

    Ok(RunSummary {
        ok: true,
        dry_run: cli.dry_run,
        aw_url: cli.aw_url.clone(),
        clickhouse_url: cli.clickhouse_url.clone(),
        clickhouse_database: cli.clickhouse_database.clone(),
        host,
        state_path: cli.state_path.display().to_string(),
        state_enabled,
        state_loaded_last_end,
        state_saved_last_end,
        overlap_seconds: cli.overlap_seconds,
        start: format_aw_timestamp(start),
        end: format_aw_timestamp(end),
        window_buckets: window_bucket_ids,
        browser_buckets: browser_bucket_ids,
        missing_buckets,
        window_events_fetched: window_events.len(),
        browser_events_fetched: browser_events.len(),
        window_rows_filtered_invalid,
        window_rows_prepared: window_rows.len(),
        browser_rows_prepared: browser_rows.len(),
        window_rows_inserted,
        browser_rows_inserted,
        window_rows_skipped_existing,
        browser_rows_skipped_existing,
    })
}

fn validate_cli(cli: &Cli) -> Result<()> {
    if cli.host.trim().is_empty() {
        bail!("--host or AW_WORKFORCE_HOST is required");
    }
    if cli.hours <= 0 {
        bail!("--hours must be positive");
    }
    if cli.overlap_seconds < 0 {
        bail!("--overlap-seconds must be zero or positive");
    }
    if cli.limit == 0 {
        bail!("--limit must be positive");
    }
    if cli.timeout_seconds == 0 {
        bail!("--timeout-seconds must be positive");
    }
    if cli.retry_attempts == 0 {
        bail!("--retry-attempts must be positive");
    }
    Ok(())
}

fn load_state(path: &Path) -> Result<State> {
    if !path.exists() {
        return Ok(State::default());
    }
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))
}

fn save_state(path: &Path, state: &State) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)
            .with_context(|| format!("create temp state in {}", parent.display()))?;
        serde_json::to_writer_pretty(&mut temp, state)?;
        std::io::Write::write_all(&mut temp, b"\n")?;
        temp.persist(path)
            .map_err(|err| anyhow::anyhow!("persist {}: {}", path.display(), err))?;
        return Ok(());
    }
    fs::write(path, serde_json::to_string_pretty(state)? + "\n")
        .with_context(|| format!("write {}", path.display()))
}

fn default_window_buckets(host: &str, requested: &[String]) -> Vec<String> {
    if requested.is_empty() {
        return vec![format!("aw-watcher-window_{host}")];
    }
    requested
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn default_browser_buckets(host: &str, requested: &[String]) -> Vec<String> {
    if requested.is_empty() {
        return vec![
            format!("aw-watcher-web-edge_{host}"),
            format!("aw-watcher-web-chrome_{host}"),
        ];
    }
    requested
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn resolve_buckets(
    buckets: &BTreeMap<String, Bucket>,
    requested: &[String],
) -> (Vec<(String, Bucket)>, Vec<String>) {
    let mut selected = Vec::new();
    let mut missing = Vec::new();
    for bucket_id in requested {
        match buckets.get(bucket_id) {
            Some(bucket) => selected.push((bucket_id.clone(), bucket.clone())),
            None => missing.push(bucket_id.clone()),
        }
    }
    (selected, missing)
}

fn fetch_events(
    client: &ActivityWatchClient,
    buckets: &[(String, Bucket)],
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    limit: usize,
    label: &str,
) -> Result<Vec<AwEvent>> {
    let mut events = Vec::new();
    for (bucket_id, bucket) in buckets {
        let path = build_events_path(bucket_id, start, end, limit);
        let raw_events: Vec<RawAwEvent> = client
            .get_json(&path)
            .with_context(|| format!("fetch {label} events from {bucket_id}"))?;
        events.extend(raw_events.into_iter().map(|raw| {
            let data = normalized_data(raw.data);
            let event_id = event_id(bucket_id, &raw.id, &raw.timestamp, raw.duration, &data);
            AwEvent {
                bucket_id: bucket_id.clone(),
                bucket_hostname: bucket.hostname.clone(),
                event_id,
                timestamp: raw.timestamp,
                duration: raw.duration,
                data,
            }
        }));
    }
    Ok(events)
}

fn build_events_path(
    bucket_id: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    limit: usize,
) -> String {
    format!(
        "/buckets/{}/events?start={}&end={}&limit={}",
        urlencoding::encode(bucket_id),
        urlencoding::encode(&format_aw_timestamp(start)),
        urlencoding::encode(&format_aw_timestamp(end)),
        limit
    )
}

fn map_window_row(event: &AwEvent, fallback_host: &str) -> Result<WindowRow> {
    Ok(WindowRow {
        event_time: format_clickhouse_datetime(parse_time(&event.timestamp)?),
        host_name: event_host(event, fallback_host),
        user_login: event_user(event),
        process_name: normalize_process_name(
            &first_string(
                &event.data,
                &["app", "process", "process_name", "foregroundProcess"],
            )
            .unwrap_or_else(|| "unknown".to_string()),
        ),
        window_title: first_string(&event.data, &["title", "window_title", "foregroundTitle"])
            .unwrap_or_default(),
        duration_sec: duration_to_u32(event.duration),
        source_bucket: event.bucket_id.clone(),
        source_event_id: event.event_id.clone(),
    })
}

fn is_invalid_synthetic_window_event(_event: &AwEvent, row: &WindowRow) -> bool {
    row.user_login == "unknown"
        || (row.process_name == "unknown" && row.window_title.trim().is_empty())
}

fn map_browser_row(event: &AwEvent, fallback_host: &str) -> Result<BrowserRow> {
    Ok(BrowserRow {
        event_time: format_clickhouse_datetime(parse_time(&event.timestamp)?),
        host_name: event_host(event, fallback_host),
        user_login: event_user(event),
        browser_name: normalize_process_name(
            &first_string(&event.data, &["browser", "app"])
                .unwrap_or_else(|| "unknown".to_string()),
        ),
        url: first_string(&event.data, &["url", "uri"]).unwrap_or_default(),
        title: first_string(&event.data, &["title"]).unwrap_or_default(),
        duration_sec: duration_to_u32(event.duration),
        source_bucket: event.bucket_id.clone(),
        source_event_id: event.event_id.clone(),
    })
}

fn event_host(event: &AwEvent, fallback_host: &str) -> String {
    first_string(&event.data, &["hostname", "host"])
        .or_else(|| {
            let trimmed = event.bucket_hostname.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
        .unwrap_or_else(|| fallback_host.to_string())
}

fn event_user(event: &AwEvent) -> String {
    let raw = first_string(
        &event.data,
        &["username", "user", "userName", "login", "userId"],
    );
    normalize_user_login(raw.as_deref())
}

fn normalized_data(value: Value) -> Value {
    if value.is_object() {
        value
    } else {
        json!({ "raw": value })
    }
}

fn first_string(data: &Value, keys: &[&str]) -> Option<String> {
    let object = data.as_object()?;
    for key in keys {
        if let Some(value) = object.get(*key) {
            match value {
                Value::Null => {}
                Value::String(value) => {
                    let trimmed = value.trim();
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    }
                }
                other => {
                    let rendered = other.to_string();
                    if !rendered.is_empty() {
                        return Some(rendered);
                    }
                }
            }
        }
    }
    None
}

fn normalize_user_login(value: Option<&str>) -> String {
    let Some(value) = value else {
        return "unknown".to_string();
    };
    let mut value = value.trim();
    if value.is_empty() {
        return "unknown".to_string();
    }
    if let Some((_domain, user)) = value.rsplit_once('\\') {
        value = user;
    } else if let Some((_domain, user)) = value.rsplit_once('/') {
        value = user;
    }
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() || normalized.ends_with('$') {
        "unknown".to_string()
    } else {
        normalized
    }
}

fn normalize_process_name(value: &str) -> String {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        "unknown".to_string()
    } else {
        normalized
    }
}

fn duration_to_u32(value: f64) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    value.round().min(u32::MAX as f64) as u32
}

fn event_id(
    bucket_id: &str,
    raw_id: &Option<Value>,
    timestamp: &str,
    duration: f64,
    data: &Value,
) -> String {
    match raw_id {
        Some(Value::String(value)) if !value.trim().is_empty() => value.trim().to_string(),
        Some(Value::Number(value)) => value.to_string(),
        Some(Value::Bool(value)) => value.to_string(),
        _ => {
            let payload = serde_json::to_string(data).unwrap_or_else(|_| "{}".to_string());
            format!("{bucket_id}|{timestamp}|{duration}|{payload}")
        }
    }
}

fn filter_existing_rows<T: SourceKey + Clone>(
    clickhouse: &ClickHouseClient,
    table: &str,
    rows: &[T],
) -> Result<(Vec<T>, usize)> {
    if rows.is_empty() {
        return Ok((Vec::new(), 0));
    }
    let existing = existing_source_keys(clickhouse, table, rows)?;
    let mut new_rows = Vec::new();
    let mut skipped = 0;
    for row in rows {
        let key = (
            row.source_bucket().to_string(),
            row.source_event_id().to_string(),
        );
        if existing.contains(&key) {
            skipped += 1;
        } else {
            new_rows.push(row.clone());
        }
    }
    Ok((new_rows, skipped))
}

fn existing_source_keys<T: SourceKey>(
    clickhouse: &ClickHouseClient,
    table: &str,
    rows: &[T],
) -> Result<HashSet<(String, String)>> {
    let table_name = clickhouse.table_name(table)?;
    let mut by_bucket: BTreeMap<String, HashSet<String>> = BTreeMap::new();
    for row in rows {
        by_bucket
            .entry(row.source_bucket().to_string())
            .or_default()
            .insert(row.source_event_id().to_string());
    }

    let mut existing = HashSet::new();
    for (bucket, ids) in by_bucket {
        let ids: Vec<String> = ids.into_iter().collect();
        for chunk in ids.chunks(500) {
            let id_list = chunk
                .iter()
                .map(|id| clickhouse_string_literal(id))
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!(
                "SELECT source_event_id FROM {table_name} WHERE source_bucket = {} AND source_event_id IN ({id_list}) FORMAT JSONEachRow",
                clickhouse_string_literal(&bucket)
            );
            let found: Vec<ExistingId> = clickhouse.query_json_each_row(&sql)?;
            for item in found {
                existing.insert((bucket.clone(), item.source_event_id));
            }
        }
    }
    Ok(existing)
}

fn parse_time(value: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)
        .with_context(|| format!("parse timestamp {value}"))?
        .with_timezone(&Utc))
}

fn format_aw_timestamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Micros, true)
}

fn format_clickhouse_datetime(value: DateTime<Utc>) -> String {
    value.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn normalize_identifier(value: &str, label: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty()
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        bail!("{label} must contain only ASCII letters, digits and underscores");
    }
    Ok(value.to_string())
}

fn clickhouse_string_literal(value: &str) -> String {
    let mut out = String::from("'");
    for ch in value.chars() {
        match ch {
            '\'' => out.push_str("\\'"),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push('\'');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_HOST: &str = "HOST-EXAMPLE";
    const TEST_USER: &str = "Alice";
    const TEST_BUCKET: &str = "aw-watcher-window_HOST-EXAMPLE";

    #[test]
    fn normalizes_domain_user_login() {
        assert_eq!(normalize_user_login(Some("HOST-EXAMPLE\\Alice")), "alice");
        assert_eq!(normalize_user_login(Some("Bob")), "bob");
        assert_eq!(normalize_user_login(Some("DOMAIN/Carol")), "carol");
        assert_eq!(normalize_user_login(Some("DOMAIN\\Оператор")), "Оператор");
        assert_eq!(normalize_user_login(Some("MACHINE$")), "unknown");
        assert_eq!(normalize_user_login(None), "unknown");
    }

    #[test]
    fn builds_bounded_aw_events_path() {
        let start = parse_time("2026-06-23T10:00:00Z").unwrap();
        let end = parse_time("2026-06-23T11:00:00Z").unwrap();
        let path = build_events_path(TEST_BUCKET, start, end, 500);
        assert!(path.starts_with("/buckets/aw-watcher-window_HOST-EXAMPLE/events?"));
        assert!(path.contains("limit=500"));
        assert!(path.contains("start=2026-06-23T10%3A00%3A00.000000Z"));
        assert!(path.contains("end=2026-06-23T11%3A00%3A00.000000Z"));
    }

    #[test]
    fn maps_window_event_to_clickhouse_row() {
        let raw = RawAwEvent {
            id: Some(Value::from(42)),
            timestamp: "2026-06-23T12:17:13.853Z".to_string(),
            duration: 1.6,
            data: json!({
                "app": "1CV8C.EXE",
                "hostname": TEST_HOST,
                "title": "document",
                "username": TEST_USER
            }),
        };
        let data = normalized_data(raw.data);
        let event = AwEvent {
            bucket_id: TEST_BUCKET.to_string(),
            bucket_hostname: TEST_HOST.to_string(),
            event_id: event_id(TEST_BUCKET, &raw.id, &raw.timestamp, raw.duration, &data),
            timestamp: raw.timestamp,
            duration: raw.duration,
            data,
        };
        let row = map_window_row(&event, "fallback").unwrap();
        assert_eq!(row.event_time, "2026-06-23 12:17:13");
        assert_eq!(row.host_name, TEST_HOST);
        assert_eq!(row.user_login, "alice");
        assert_eq!(row.process_name, "1cv8c.exe");
        assert_eq!(row.duration_sec, 2);
        assert_eq!(row.source_event_id, "42");
    }

    #[test]
    fn detects_invalid_synthetic_window_event() {
        let raw = RawAwEvent {
            id: Some(Value::from(43)),
            timestamp: "2026-06-23T12:18:13.853Z".to_string(),
            duration: 10.0,
            data: json!({
                "app": "",
                "hostname": TEST_HOST,
                "processId": 0,
                "source": "aw-windows-telemetry-rust",
                "title": "",
                "username": TEST_USER
            }),
        };
        let data = normalized_data(raw.data);
        let event = AwEvent {
            bucket_id: TEST_BUCKET.to_string(),
            bucket_hostname: TEST_HOST.to_string(),
            event_id: event_id(TEST_BUCKET, &raw.id, &raw.timestamp, raw.duration, &data),
            timestamp: raw.timestamp,
            duration: raw.duration,
            data,
        };
        let row = map_window_row(&event, "fallback").unwrap();
        assert!(is_invalid_synthetic_window_event(&event, &row));
    }

    #[test]
    fn detects_blank_window_event_without_source_marker() {
        let raw = RawAwEvent {
            id: Some(Value::from(45)),
            timestamp: "2026-06-23T12:20:13.853Z".to_string(),
            duration: 10.0,
            data: json!({}),
        };
        let data = normalized_data(raw.data);
        let event = AwEvent {
            bucket_id: TEST_BUCKET.to_string(),
            bucket_hostname: TEST_HOST.to_string(),
            event_id: event_id(TEST_BUCKET, &raw.id, &raw.timestamp, raw.duration, &data),
            timestamp: raw.timestamp,
            duration: raw.duration,
            data,
        };
        let row = map_window_row(&event, "fallback").unwrap();
        assert!(is_invalid_synthetic_window_event(&event, &row));
    }

    #[test]
    fn detects_window_event_without_user_identity() {
        let raw = RawAwEvent {
            id: Some(Value::from(46)),
            timestamp: "2026-06-23T12:21:13.853Z".to_string(),
            duration: 10.0,
            data: json!({
                "app": "1cv8c.exe",
                "title": "ТРАНСГАЗ"
            }),
        };
        let data = normalized_data(raw.data);
        let event = AwEvent {
            bucket_id: TEST_BUCKET.to_string(),
            bucket_hostname: TEST_HOST.to_string(),
            event_id: event_id(TEST_BUCKET, &raw.id, &raw.timestamp, raw.duration, &data),
            timestamp: raw.timestamp,
            duration: raw.duration,
            data,
        };
        let row = map_window_row(&event, "fallback").unwrap();
        assert!(is_invalid_synthetic_window_event(&event, &row));
    }

    #[test]
    fn keeps_real_window_event_from_rust_collector() {
        let raw = RawAwEvent {
            id: Some(Value::from(44)),
            timestamp: "2026-06-23T12:19:13.853Z".to_string(),
            duration: 0.0,
            data: json!({
                "app": "EXPLORER.EXE",
                "hostname": TEST_HOST,
                "processId": 1000,
                "source": "aw-windows-telemetry-rust",
                "title": "Рабочий стол",
                "username": TEST_USER
            }),
        };
        let data = normalized_data(raw.data);
        let event = AwEvent {
            bucket_id: TEST_BUCKET.to_string(),
            bucket_hostname: TEST_HOST.to_string(),
            event_id: event_id(TEST_BUCKET, &raw.id, &raw.timestamp, raw.duration, &data),
            timestamp: raw.timestamp,
            duration: raw.duration,
            data,
        };
        let row = map_window_row(&event, "fallback").unwrap();
        assert!(!is_invalid_synthetic_window_event(&event, &row));
    }

    #[test]
    fn escapes_clickhouse_string_literals() {
        assert_eq!(
            clickhouse_string_literal("domain\\user'1\n"),
            "'domain\\\\user\\'1\\n'"
        );
    }

    #[test]
    fn loads_and_saves_state_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let state = State {
            last_end: Some("2026-06-23T12:00:00.000000Z".to_string()),
        };
        save_state(&path, &state).unwrap();
        let loaded = load_state(&path).unwrap();
        assert_eq!(loaded.last_end, state.last_end);
    }

    #[test]
    fn missing_state_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = load_state(&dir.path().join("missing.json")).unwrap();
        assert_eq!(loaded.last_end, None);
    }
}
