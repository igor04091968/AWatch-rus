use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use clap::Parser;
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::Serialize;
use serde_json::{Value, json};

const DEFAULT_DB_PATH: &str = "/var/lib/activitywatch/aw-server-rust/sqlite.db";
const DEFAULT_HOST: &str = "HOST-EXAMPLE";

#[derive(Debug, Parser)]
#[command(author, version, about = "Read-only ActivityWatch SQLite growth guard")]
struct Cli {
    #[arg(long, default_value = DEFAULT_DB_PATH, env = "AW_DB_HEALTH_DB_PATH")]
    db_path: PathBuf,

    #[arg(long, env = "AW_DB_HEALTH_SESSION_BUCKET")]
    session_bucket: Option<String>,

    #[arg(long, env = "AW_WORKTIME_HOST")]
    host: Option<String>,

    #[arg(long, env = "AW_DB_HEALTH_WINDOWS_CONFIG")]
    windows_config: Option<PathBuf>,

    #[arg(long, default_value_t = gib(2), env = "AW_DB_HEALTH_DB_WARN_BYTES")]
    db_warn_bytes: u64,

    #[arg(long, default_value_t = gib(5), env = "AW_DB_HEALTH_DB_FAIL_BYTES")]
    db_fail_bytes: u64,

    #[arg(long, default_value_t = mib(256), env = "AW_DB_HEALTH_WAL_WARN_BYTES")]
    wal_warn_bytes: u64,

    #[arg(long, default_value_t = gib(1), env = "AW_DB_HEALTH_WAL_FAIL_BYTES")]
    wal_fail_bytes: u64,

    #[arg(long, default_value_t = 10_000, env = "AW_DB_HEALTH_SESSION_ROWS_WARN")]
    session_rows_warn: i64,

    #[arg(
        long,
        default_value_t = 100_000,
        env = "AW_DB_HEALTH_SESSION_ROWS_FAIL"
    )]
    session_rows_fail: i64,

    #[arg(
        long,
        default_value_t = 600,
        env = "AW_DB_HEALTH_RECENT_PROCESS_WINDOW_SECONDS"
    )]
    recent_process_window_seconds: i64,

    #[arg(long, default_value_t = 1, env = "AW_DB_HEALTH_RECENT_PROCESS_WARN")]
    recent_process_warn: i64,

    #[arg(long, default_value_t = 100, env = "AW_DB_HEALTH_RECENT_PROCESS_FAIL")]
    recent_process_fail: i64,

    #[arg(long)]
    json: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Status {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Serialize)]
struct CheckResult {
    name: String,
    status: Status,
    summary: String,
    details: Value,
}

#[derive(Debug, Serialize)]
struct Report {
    ok: bool,
    generated_at_utc: String,
    counts: Counts,
    results: Vec<CheckResult>,
}

#[derive(Debug, Default, Serialize)]
struct Counts {
    ok: usize,
    warn: usize,
    fail: usize,
}

const fn mib(value: u64) -> u64 {
    value * 1024 * 1024
}

const fn gib(value: u64) -> u64 {
    value * 1024 * 1024 * 1024
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
    let report = build_report(&cli)?;
    if cli.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_text(&report);
    }
    Ok(if report.ok { 0 } else { 2 })
}

fn build_report(cli: &Cli) -> Result<Report> {
    let mut results = Vec::new();
    let db_path = &cli.db_path;
    let wal_path = PathBuf::from(format!("{}-wal", db_path.display()));
    let shm_path = PathBuf::from(format!("{}-shm", db_path.display()));

    let db_size = file_size(db_path)?;
    results.push(threshold_result(
        "sqlite:file-size",
        db_size,
        cli.db_warn_bytes,
        cli.db_fail_bytes,
        "ActivityWatch SQLite DB size",
        json!({
            "path": db_path,
            "size_bytes": db_size,
            "warn_bytes": cli.db_warn_bytes,
            "fail_bytes": cli.db_fail_bytes,
        }),
    ));

    let wal_size = file_size_optional(&wal_path)?;
    results.push(threshold_result(
        "sqlite:wal-size",
        wal_size,
        cli.wal_warn_bytes,
        cli.wal_fail_bytes,
        "ActivityWatch SQLite WAL size",
        json!({
            "path": wal_path,
            "size_bytes": wal_size,
            "warn_bytes": cli.wal_warn_bytes,
            "fail_bytes": cli.wal_fail_bytes,
        }),
    ));

    let shm_size = file_size_optional(&shm_path)?;
    results.push(CheckResult {
        name: "sqlite:shm-size".to_string(),
        status: Status::Ok,
        summary: format!("SHM size is {}", human_bytes(shm_size)),
        details: json!({
            "path": shm_path,
            "size_bytes": shm_size,
        }),
    });

    let conn = open_readonly(db_path)?;
    let bucket = cli.session_bucket.clone().unwrap_or_else(|| {
        format!(
            "aw-session-events_{}",
            cli.host.as_deref().unwrap_or(DEFAULT_HOST)
        )
    });
    match bucket_row(&conn, &bucket)? {
        Some(bucketrow) => {
            let total_rows = count_session_rows(&conn, bucketrow)?;
            results.push(threshold_result_i64(
                "aw-session-events:rows",
                total_rows,
                cli.session_rows_warn,
                cli.session_rows_fail,
                "aw-session-events row count",
                json!({
                    "bucket": bucket,
                    "bucketrow": bucketrow,
                    "rows": total_rows,
                    "warn_rows": cli.session_rows_warn,
                    "fail_rows": cli.session_rows_fail,
                }),
            ));

            let cutoff_ns = now_ns()? - cli.recent_process_window_seconds.max(1) * 1_000_000_000;
            let recent_process = count_recent_process_events(&conn, bucketrow, cutoff_ns)?;
            results.push(threshold_result_i64(
                "aw-session-events:recent-process-events",
                recent_process,
                cli.recent_process_warn,
                cli.recent_process_fail,
                "recent process-level aw-session-events",
                json!({
                    "bucket": bucket,
                    "bucketrow": bucketrow,
                    "recent_process_events": recent_process,
                    "window_seconds": cli.recent_process_window_seconds,
                    "cutoff_ns": cutoff_ns,
                    "warn_events": cli.recent_process_warn,
                    "fail_events": cli.recent_process_fail,
                }),
            ));

            let latest = latest_session_event(&conn, bucketrow)?;
            results.push(CheckResult {
                name: "aw-session-events:latest".to_string(),
                status: Status::Ok,
                summary: latest
                    .as_ref()
                    .map(|event| format!("latest eventType={}", event.event_type))
                    .unwrap_or_else(|| "no session events".to_string()),
                details: json!({
                    "bucket": bucket,
                    "latest": latest,
                }),
            });
        }
        None => {
            results.push(CheckResult {
                name: "aw-session-events:bucket".to_string(),
                status: Status::Warn,
                summary: format!("bucket {bucket} not found"),
                details: json!({ "bucket": bucket }),
            });
        }
    }

    if let Some(path) = &cli.windows_config {
        results.push(check_windows_config(path)?);
    }

    let counts = count_statuses(&results);
    Ok(Report {
        ok: counts.fail == 0,
        generated_at_utc: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        counts,
        results,
    })
}

fn open_readonly(path: &Path) -> Result<Connection> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("open SQLite DB read-only: {}", path.display()))?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(conn)
}

fn bucket_row(conn: &Connection, bucket: &str) -> Result<Option<i64>> {
    conn.query_row("SELECT id FROM buckets WHERE name=?", [bucket], |row| {
        row.get(0)
    })
    .optional()
    .context("lookup bucket row")
}

fn count_session_rows(conn: &Connection, bucketrow: i64) -> Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM events WHERE bucketrow=?",
        [bucketrow],
        |row| row.get(0),
    )
    .context("count session event rows")
}

fn count_recent_process_events(conn: &Connection, bucketrow: i64, cutoff_ns: i64) -> Result<i64> {
    conn.query_row(
        r#"
        SELECT COUNT(*)
        FROM events
        WHERE bucketrow=?
          AND endtime >= ?
          AND (data LIKE ? OR data LIKE ?)
        "#,
        params![
            bucketrow,
            cutoff_ns,
            "%\"eventType\":\"process_start\"%",
            "%\"eventType\":\"process_stop\"%",
        ],
        |row| row.get(0),
    )
    .context("count recent process-level session events")
}

#[derive(Debug, Serialize)]
struct LatestEvent {
    id: i64,
    endtime_ns: i64,
    event_type: String,
    source: String,
}

fn latest_session_event(conn: &Connection, bucketrow: i64) -> Result<Option<LatestEvent>> {
    let row = conn
        .query_row(
            "SELECT id, endtime, data FROM events WHERE bucketrow=? ORDER BY id DESC LIMIT 1",
            [bucketrow],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()
        .context("read latest session event")?;
    Ok(row.map(|(id, endtime_ns, data)| LatestEvent {
        id,
        endtime_ns,
        event_type: json_field(&data, "eventType").unwrap_or_else(|| "unknown".to_string()),
        source: json_field(&data, "source").unwrap_or_else(|| "unknown".to_string()),
    }))
}

fn check_windows_config(path: &Path) -> Result<CheckResult> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("read Windows deployment config {}", path.display()))?;
    let payload: Value = serde_json::from_str(&text).context("parse Windows deployment config")?;
    let enabled = payload
        .pointer("/sessionEvents/processEventsEnabled")
        .and_then(Value::as_bool);
    let status = match enabled {
        Some(false) => Status::Ok,
        Some(true) => Status::Fail,
        None => Status::Warn,
    };
    let summary = match enabled {
        Some(false) => "processEventsEnabled=false".to_string(),
        Some(true) => {
            "processEventsEnabled=true; high-volume process stream is enabled".to_string()
        }
        None => "processEventsEnabled missing".to_string(),
    };
    Ok(CheckResult {
        name: "windows-config:process-events".to_string(),
        status,
        summary,
        details: json!({
            "path": path,
            "processEventsEnabled": enabled,
        }),
    })
}

fn threshold_result(
    name: &str,
    value: u64,
    warn: u64,
    fail: u64,
    label: &str,
    details: Value,
) -> CheckResult {
    let status = status_for_u64(value, warn, fail);
    CheckResult {
        name: name.to_string(),
        status,
        summary: format!("{label}: {}", human_bytes(value)),
        details,
    }
}

fn threshold_result_i64(
    name: &str,
    value: i64,
    warn: i64,
    fail: i64,
    label: &str,
    details: Value,
) -> CheckResult {
    let status = status_for_i64(value, warn, fail);
    CheckResult {
        name: name.to_string(),
        status,
        summary: format!("{label}: {value}"),
        details,
    }
}

fn status_for_u64(value: u64, warn: u64, fail: u64) -> Status {
    if value >= fail {
        Status::Fail
    } else if value >= warn {
        Status::Warn
    } else {
        Status::Ok
    }
}

fn status_for_i64(value: i64, warn: i64, fail: i64) -> Status {
    if value >= fail {
        Status::Fail
    } else if value >= warn {
        Status::Warn
    } else {
        Status::Ok
    }
}

fn count_statuses(results: &[CheckResult]) -> Counts {
    let mut counts = Counts::default();
    for result in results {
        match result.status {
            Status::Ok => counts.ok += 1,
            Status::Warn => counts.warn += 1,
            Status::Fail => counts.fail += 1,
        }
    }
    counts
}

fn file_size(path: &Path) -> Result<u64> {
    Ok(fs::metadata(path)
        .with_context(|| format!("stat {}", path.display()))?
        .len())
}

fn file_size_optional(path: &Path) -> Result<u64> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.len()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(err) => Err(err).with_context(|| format!("stat {}", path.display())),
    }
}

fn now_ns() -> Result<i64> {
    Utc::now()
        .timestamp_nanos_opt()
        .ok_or_else(|| anyhow!("current timestamp out of range"))
}

fn json_field(data: &str, key: &str) -> Option<String> {
    let value = serde_json::from_str::<Value>(data).ok()?;
    value.get(key)?.as_str().map(ToString::to_string)
}

fn human_bytes(value: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = value as f64;
    let mut unit = 0usize;
    while size >= 1024.0 && unit + 1 < UNITS.len() {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{value} {}", UNITS[unit])
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

fn print_text(report: &Report) {
    println!("=== AW DB Health ===");
    println!("Timestamp: {}", report.generated_at_utc);
    for result in &report.results {
        let mark = match result.status {
            Status::Ok => "✓",
            Status::Warn => "⚠",
            Status::Fail => "✗",
        };
        println!("{mark} {}: {}", result.name, result.summary);
    }
    println!(
        "Summary: ok={} warn={} fail={}",
        report.counts.ok, report.counts.warn, report.counts.fail
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    #[test]
    fn classifies_thresholds() {
        assert_eq!(status_for_u64(10, 20, 30), Status::Ok);
        assert_eq!(status_for_u64(20, 20, 30), Status::Warn);
        assert_eq!(status_for_u64(30, 20, 30), Status::Fail);
        assert_eq!(status_for_i64(0, 1, 100), Status::Ok);
        assert_eq!(status_for_i64(1, 1, 100), Status::Warn);
        assert_eq!(status_for_i64(101, 0, 100), Status::Fail);
    }

    #[test]
    fn reads_session_metrics() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let conn = Connection::open(tmp.path()).unwrap();
        conn.execute(
            "CREATE TABLE buckets (id INTEGER PRIMARY KEY, name TEXT)",
            [],
        )
        .unwrap();
        conn.execute(
            "CREATE TABLE events (id INTEGER PRIMARY KEY, bucketrow INTEGER, endtime INTEGER, data TEXT)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO buckets (id, name) VALUES (15, 'aw-session-events_TEST')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO events (bucketrow, endtime, data) VALUES (15, ?, ?)",
            params![
                now_ns().unwrap(),
                r#"{"eventType":"process_start","source":"worktime-session-collector"}"#
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO events (bucketrow, endtime, data) VALUES (15, ?, ?)",
            params![
                now_ns().unwrap(),
                r#"{"eventType":"logon","source":"launch-watchers-awatch-rus"}"#
            ],
        )
        .unwrap();
        drop(conn);

        let conn = open_readonly(tmp.path()).unwrap();
        let bucketrow = bucket_row(&conn, "aw-session-events_TEST")
            .unwrap()
            .unwrap();
        assert_eq!(count_session_rows(&conn, bucketrow).unwrap(), 2);
        assert_eq!(
            count_recent_process_events(&conn, bucketrow, now_ns().unwrap() - 60_000_000_000)
                .unwrap(),
            1
        );
        let latest = latest_session_event(&conn, bucketrow).unwrap().unwrap();
        assert_eq!(latest.event_type, "logon");
    }

    #[test]
    fn checks_windows_config_flag() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        fs::write(
            tmp.path(),
            r#"{"sessionEvents":{"processEventsEnabled":false}}"#,
        )
        .unwrap();
        let result = check_windows_config(tmp.path()).unwrap();
        assert_eq!(result.status, Status::Ok);
    }
}
