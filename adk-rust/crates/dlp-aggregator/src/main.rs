use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, SecondsFormat, TimeDelta, Utc};
use clap::Parser;
use detmir_aw_client::ActivityWatchClient;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const DEFAULT_AW_URL: &str = "http://127.0.0.1:5600/api/0";
const DEFAULT_SQLITE_PATH: &str = "data/dlp-events.sqlite3";
const DEFAULT_STATE_PATH: &str = "data/dlp-aggregator-state.json";
const DEFAULT_BUCKET_PREFIXES: &str = "aw-file-operations_,aw-dlp-incidents_";

#[derive(Debug, Parser)]
#[command(about = "Aggregate AWatch-rus DLP buckets into a local warehouse database.")]
struct Cli {
    #[arg(long, default_value = DEFAULT_AW_URL)]
    aw_url: String,

    #[arg(long)]
    postgres_dsn: Option<String>,

    #[arg(long, default_value = DEFAULT_SQLITE_PATH)]
    sqlite_path: PathBuf,

    #[arg(long, default_value = DEFAULT_STATE_PATH)]
    state_path: PathBuf,

    #[arg(long, default_value = DEFAULT_BUCKET_PREFIXES)]
    bucket_prefixes: String,

    #[arg(long)]
    since: Option<String>,

    #[arg(long, default_value_t = 24)]
    lookback_hours: i64,

    #[arg(long, default_value_t = 60)]
    overlap_seconds: i64,

    #[arg(long, default_value_t = 10_000)]
    limit: usize,

    #[arg(long, default_value_t = 15)]
    timeout: u64,

    #[arg(long)]
    dry_run: bool,
}

impl Cli {
    fn apply_env(mut self) -> Self {
        self.aw_url = env_value("AW_URL", &self.aw_url);
        self.postgres_dsn = self
            .postgres_dsn
            .or_else(|| std::env::var("DLP_AGGREGATOR_POSTGRES_DSN").ok())
            .filter(|value| !value.is_empty());
        self.sqlite_path = env_path("DLP_AGGREGATOR_SQLITE_PATH", self.sqlite_path);
        self.state_path = env_path("DLP_AGGREGATOR_STATE_PATH", self.state_path);
        self
    }
}

#[derive(Debug, Clone, Deserialize)]
struct Bucket {
    #[serde(default)]
    r#type: String,
    #[serde(default)]
    hostname: String,
}

#[derive(Debug, Clone, Deserialize)]
struct RawAwEvent {
    #[serde(default)]
    id: Option<Value>,
    timestamp: String,
    #[serde(default)]
    duration: f64,
    #[serde(default)]
    data: Value,
}

#[derive(Debug, Clone)]
struct AwEvent {
    bucket_id: String,
    hostname: String,
    stream_type: String,
    event_id: String,
    timestamp: String,
    duration: f64,
    data: Value,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct State {
    #[serde(default)]
    last_end: Option<String>,
}

#[derive(Debug, Serialize)]
struct DryRunSummary {
    aw_url: String,
    start: String,
    end: String,
    selected_buckets: Vec<String>,
    fetched_events: usize,
}

#[derive(Debug, Serialize)]
struct RunSummary {
    aw_url: String,
    target: String,
    target_path: String,
    state_path: String,
    start: String,
    end: String,
    selected_buckets: usize,
    fetched_events: usize,
    inserted_events: usize,
}

fn env_value(name: &str, fallback: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

fn env_path(name: &str, fallback: PathBuf) -> PathBuf {
    std::env::var(name)
        .ok()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or(fallback)
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>> {
    let normalized = value.replace('Z', "+00:00");
    Ok(DateTime::parse_from_rfc3339(&normalized)
        .with_context(|| format!("parse timestamp {value}"))?
        .with_timezone(&Utc))
}

fn format_aw_timestamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Micros, true)
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
            .with_context(|| format!("create temp file in {}", parent.display()))?;
        serde_json::to_writer_pretty(&mut temp, state)?;
        std::io::Write::write_all(&mut temp, b"\n")?;
        temp.persist(path)
            .map_err(|err| anyhow!("persist {}: {}", path.display(), err))?;
        return Ok(());
    }
    fs::write(path, serde_json::to_string_pretty(state)? + "\n")
        .with_context(|| format!("write {}", path.display()))
}

fn get_start_time(cli: &Cli, state: &State) -> Result<DateTime<Utc>> {
    if let Some(since) = &cli.since {
        return parse_timestamp(since);
    }
    if let Some(last_end) = &state.last_end {
        return Ok(parse_timestamp(last_end)? - TimeDelta::seconds(cli.overlap_seconds));
    }
    Ok(Utc::now() - TimeDelta::hours(cli.lookback_hours))
}

fn parse_prefixes(value: &str) -> Result<Vec<String>> {
    let prefixes: Vec<String> = value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    if prefixes.is_empty() {
        bail!("at least one bucket prefix is required");
    }
    Ok(prefixes)
}

fn bucket_stream_type(bucket_id: &str, bucket: &Bucket) -> Option<&'static str> {
    if bucket_id.starts_with("aw-file-operations_") || bucket.r#type == "aw.file.operation" {
        return Some("file_operation");
    }
    if bucket_id.starts_with("aw-dlp-incidents_") || bucket.r#type == "aw.dlp.incident" {
        return Some("dlp_incident");
    }
    None
}

fn select_buckets(
    buckets: &BTreeMap<String, Bucket>,
    prefixes: &[String],
) -> Vec<(String, Bucket, &'static str)> {
    buckets
        .iter()
        .filter_map(|(bucket_id, bucket)| {
            let stream_type = bucket_stream_type(bucket_id, bucket)?;
            if prefixes.iter().any(|prefix| bucket_id.starts_with(prefix)) {
                Some((bucket_id.clone(), bucket.clone(), stream_type))
            } else {
                None
            }
        })
        .collect()
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

fn normalized_data(value: Value) -> Value {
    if value.is_object() {
        value
    } else {
        json!({ "raw": value })
    }
}

fn event_key(bucket_id: &str, timestamp: &str, duration: f64, data: &Value) -> String {
    let payload = serde_json::to_string(data).unwrap_or_else(|_| "{}".to_string());
    format!("{bucket_id}|{timestamp}|{duration}|{payload}")
}

fn event_id(bucket_id: &str, event: &RawAwEvent, data: &Value) -> String {
    match &event.id {
        Some(Value::String(value)) if !value.is_empty() => value.clone(),
        Some(Value::Number(value)) => value.to_string(),
        Some(Value::Bool(value)) => value.to_string(),
        _ => event_key(bucket_id, &event.timestamp, event.duration, data),
    }
}

fn fetch_bucket_events(
    client: &ActivityWatchClient,
    bucket_id: &str,
    bucket: &Bucket,
    stream_type: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    limit: usize,
) -> Result<Vec<AwEvent>> {
    let path = build_events_path(bucket_id, start, end, limit);
    let raw_events: Vec<RawAwEvent> = client.get_json(&path)?;
    Ok(raw_events
        .into_iter()
        .map(|raw| {
            let data = normalized_data(raw.data.clone());
            let hostname = if bucket.hostname.is_empty() {
                first_string(&data, &["hostname"]).unwrap_or_default()
            } else {
                bucket.hostname.clone()
            };
            AwEvent {
                bucket_id: bucket_id.to_string(),
                hostname,
                stream_type: stream_type.to_string(),
                event_id: event_id(bucket_id, &raw, &data),
                timestamp: raw.timestamp,
                duration: raw.duration,
                data,
            }
        })
        .collect())
}

fn first_string(data: &Value, keys: &[&str]) -> Option<String> {
    let object = data.as_object()?;
    for key in keys {
        if let Some(value) = object.get(*key) {
            match value {
                Value::Null => {}
                Value::String(s) if !s.is_empty() => return Some(s.clone()),
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

fn bool_as_int(value: Option<&Value>) -> i64 {
    match value {
        Some(Value::Bool(value)) => i64::from(*value),
        Some(Value::String(value)) => i64::from(matches!(
            value.to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "y"
        )),
        Some(Value::Number(value)) => i64::from(value.as_i64().unwrap_or(0) != 0),
        Some(Value::Null) | None => 0,
        Some(_) => 1,
    }
}

fn ensure_schema(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        r#"
        create table if not exists dlp_events (
            id integer primary key autoincrement,
            bucket_id text not null,
            event_id text not null,
            stream_type text not null,
            hostname text not null,
            username text,
            event_ts text not null,
            duration real not null default 0,
            operation text,
            file_path text,
            old_file_path text,
            extension text,
            archive_hint integer not null default 0,
            rule_id text,
            action text,
            severity text,
            signal_type text,
            message text,
            source text,
            screenshot_path text,
            raw_json text not null,
            ingested_at text not null,
            unique (bucket_id, event_id)
        );
        create index if not exists idx_dlp_events_event_ts on dlp_events(event_ts);
        create index if not exists idx_dlp_events_host_ts on dlp_events(hostname, event_ts);
        create index if not exists idx_dlp_events_stream_ts on dlp_events(stream_type, event_ts);
        create index if not exists idx_dlp_events_archive on dlp_events(archive_hint, event_ts);
        create index if not exists idx_dlp_events_rule on dlp_events(rule_id, event_ts);

        create view if not exists dlp_file_operations as
        select *
        from dlp_events
        where stream_type = 'file_operation';

        create view if not exists dlp_incidents as
        select *
        from dlp_events
        where stream_type = 'dlp_incident';
        "#,
    )?;
    Ok(())
}

fn connect_sqlite(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let connection = Connection::open(path).with_context(|| format!("open {}", path.display()))?;
    connection.execute_batch(
        r#"
        pragma journal_mode=WAL;
        pragma synchronous=NORMAL;
        pragma foreign_keys=ON;
        "#,
    )?;
    Ok(connection)
}

fn insert_events(connection: &mut Connection, events: &[AwEvent]) -> Result<usize> {
    let ingested_at = format_aw_timestamp(Utc::now());
    let tx = connection.transaction()?;
    let mut inserted = 0;
    {
        let mut statement = tx.prepare(
            r#"
            insert or ignore into dlp_events (
                bucket_id,
                event_id,
                stream_type,
                hostname,
                username,
                event_ts,
                duration,
                operation,
                file_path,
                old_file_path,
                extension,
                archive_hint,
                rule_id,
                action,
                severity,
                signal_type,
                message,
                source,
                screenshot_path,
                raw_json,
                ingested_at
            )
            values (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )?;
        for event in events {
            let raw_json = serde_json::to_string(&event.data)?;
            let archive_hint = bool_as_int(event.data.get("archiveHint"));
            let row_count = statement.execute(params![
                event.bucket_id,
                event.event_id,
                event.stream_type,
                event.hostname,
                first_string(&event.data, &["username", "user"]),
                event.timestamp,
                event.duration,
                first_string(&event.data, &["operation"]),
                first_string(&event.data, &["path", "filePath"]),
                first_string(&event.data, &["oldPath", "oldFilePath"]),
                first_string(&event.data, &["extension"]),
                archive_hint,
                first_string(&event.data, &["ruleId", "rule"]),
                first_string(&event.data, &["action"]),
                first_string(&event.data, &["severity"]),
                first_string(&event.data, &["signalType"]),
                first_string(&event.data, &["message"]),
                first_string(&event.data, &["source"]),
                first_string(
                    &event.data,
                    &["screenshotPath", "capturePath", "artifactPath"]
                ),
                raw_json,
                ingested_at,
            ])?;
            inserted += usize::from(row_count > 0);
        }
    }
    tx.commit()?;
    Ok(inserted)
}

fn run(cli: &Cli) -> Result<()> {
    if cli.postgres_dsn.is_some() {
        bail!(
            "PostgreSQL mode is not supported by the Rust DLP aggregator yet; use SQLite mode or the legacy Python script"
        );
    }
    let prefixes = parse_prefixes(&cli.bucket_prefixes)?;
    let state = load_state(&cli.state_path)?;
    let start = get_start_time(cli, &state)?;
    let end = Utc::now();
    let client = ActivityWatchClient::new(&cli.aw_url, Duration::from_secs(cli.timeout))?;
    let buckets: BTreeMap<String, Bucket> = client.get_json("/buckets")?;
    let selected = select_buckets(&buckets, &prefixes);
    let mut events = Vec::new();
    for (bucket_id, bucket, stream_type) in &selected {
        events.extend(fetch_bucket_events(
            &client,
            bucket_id,
            bucket,
            stream_type,
            start,
            end,
            cli.limit,
        )?);
    }

    if cli.dry_run {
        let summary = DryRunSummary {
            aw_url: cli.aw_url.clone(),
            start: format_aw_timestamp(start),
            end: format_aw_timestamp(end),
            selected_buckets: selected
                .iter()
                .map(|(bucket_id, _bucket, _stream_type)| bucket_id.clone())
                .collect(),
            fetched_events: events.len(),
        };
        println!("{}", serde_json::to_string_pretty(&summary)?);
        return Ok(());
    }

    let mut connection = connect_sqlite(&cli.sqlite_path)?;
    ensure_schema(&connection)?;
    let inserted = insert_events(&mut connection, &events)?;
    save_state(
        &cli.state_path,
        &State {
            last_end: Some(format_aw_timestamp(end)),
        },
    )?;
    let summary = RunSummary {
        aw_url: cli.aw_url.clone(),
        target: "sqlite".to_string(),
        target_path: cli.sqlite_path.display().to_string(),
        state_path: cli.state_path.display().to_string(),
        start: format_aw_timestamp(start),
        end: format_aw_timestamp(end),
        selected_buckets: selected.len(),
        fetched_events: events.len(),
        inserted_events: inserted,
    };
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse().apply_env();
    run(&cli)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_expected_bucket_prefixes() {
        let buckets = BTreeMap::from([
            (
                "aw-file-operations_HOST".to_string(),
                Bucket {
                    r#type: "aw.file.operation".to_string(),
                    hostname: "HOST".to_string(),
                },
            ),
            (
                "aw-dlp-incidents_HOST".to_string(),
                Bucket {
                    r#type: "aw.dlp.incident".to_string(),
                    hostname: "HOST".to_string(),
                },
            ),
            (
                "aw-watcher-window_HOST".to_string(),
                Bucket {
                    r#type: "currentwindow".to_string(),
                    hostname: "HOST".to_string(),
                },
            ),
        ]);
        let selected = select_buckets(
            &buckets,
            &parse_prefixes("aw-file-operations_,aw-dlp-incidents_").unwrap(),
        );
        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].2, "dlp_incident");
        assert_eq!(selected[1].2, "file_operation");
    }

    #[test]
    fn event_row_helpers_match_python_contract() {
        let data = json!({
            "username": "igor",
            "path": "C:/x.zip",
            "oldFilePath": "C:/x.tmp",
            "archiveHint": "true",
            "rule": "archive",
            "signalType": "file_operation"
        });
        assert_eq!(
            first_string(&data, &["username", "user"]).as_deref(),
            Some("igor")
        );
        assert_eq!(
            first_string(&data, &["oldPath", "oldFilePath"]).as_deref(),
            Some("C:/x.tmp")
        );
        assert_eq!(bool_as_int(data.get("archiveHint")), 1);
        assert_eq!(
            first_string(&data, &["ruleId", "rule"]).as_deref(),
            Some("archive")
        );
    }

    #[test]
    fn inserts_events_once() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("dlp.sqlite");
        let mut connection = connect_sqlite(&path).unwrap();
        ensure_schema(&connection).unwrap();
        let event = AwEvent {
            bucket_id: "aw-file-operations_HOST".to_string(),
            hostname: "HOST".to_string(),
            stream_type: "file_operation".to_string(),
            event_id: "1".to_string(),
            timestamp: "2026-05-31T10:00:00Z".to_string(),
            duration: 0.0,
            data: json!({"username":"igor","archiveHint":true}),
        };
        assert_eq!(
            insert_events(&mut connection, std::slice::from_ref(&event)).unwrap(),
            1
        );
        assert_eq!(insert_events(&mut connection, &[event]).unwrap(), 0);
        let count: i64 = connection
            .query_row("select count(*) from dlp_events", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
}
