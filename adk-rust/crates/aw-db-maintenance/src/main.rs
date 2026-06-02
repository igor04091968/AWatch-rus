use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use chrono::{SecondsFormat, Utc};
use clap::Parser;
use rusqlite::{Connection, DatabaseName, OpenFlags, params};
use serde::Serialize;
use serde_json::Value;

const DEFAULT_DB_PATH: &str = "/var/lib/activitywatch/aw-server-rust/sqlite.db";
const DEFAULT_BACKUP_DIR: &str = "/var/lib/activitywatch/backups/db";
const DEFAULT_HOST: &str = "SHARKON2025";
const ALLOWED_EVENT_TYPES: &[&str] = &["process_start", "process_stop"];

#[derive(Debug, Parser)]
#[command(about = "Safe ActivityWatch SQLite maintenance for old process-level session events")]
struct Cli {
    #[arg(long, default_value = DEFAULT_DB_PATH, env = "AW_DB_MAINTENANCE_DB_PATH")]
    db_path: PathBuf,

    #[arg(long, default_value = DEFAULT_BACKUP_DIR, env = "AW_DB_MAINTENANCE_BACKUP_DIR")]
    backup_dir: PathBuf,

    #[arg(long, env = "AW_DB_MAINTENANCE_SESSION_BUCKET")]
    session_bucket: Option<String>,

    #[arg(long, env = "AW_WORKTIME_HOST")]
    host: Option<String>,

    #[arg(long, default_value_t = 7, env = "AW_DB_MAINTENANCE_RETENTION_DAYS")]
    retention_days: i64,

    #[arg(long, default_value_t = 1000, env = "AW_DB_MAINTENANCE_CHUNK_SIZE")]
    chunk_size: usize,

    #[arg(long)]
    apply: bool,

    #[arg(long)]
    json: bool,
}

#[derive(Debug, Serialize)]
struct Report {
    apply: bool,
    generated_at_utc: String,
    db_path: String,
    backup_path: Option<String>,
    bucket: String,
    bucketrow: Option<i64>,
    retention_days: i64,
    cutoff_ns: i64,
    allowed_event_types: Vec<&'static str>,
    planned_delete_rows: usize,
    deleted_rows: usize,
    backup_created: bool,
    skipped_reason: Option<String>,
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
    Ok(0)
}

fn build_report(cli: &Cli) -> Result<Report> {
    if cli.retention_days < 1 {
        bail!("retention_days must be >= 1");
    }
    if cli.chunk_size == 0 {
        bail!("chunk_size must be > 0");
    }
    let bucket = cli.session_bucket.clone().unwrap_or_else(|| {
        format!(
            "aw-session-events_{}",
            cli.host.as_deref().unwrap_or(DEFAULT_HOST)
        )
    });
    let cutoff_ns = (Utc::now().timestamp() - cli.retention_days * 86_400) * 1_000_000_000;
    let conn = open_connection(&cli.db_path, cli.apply)?;
    let bucketrow = bucket_row(&conn, &bucket)?;
    let Some(bucketrow) = bucketrow else {
        return Ok(base_report(
            cli,
            bucket,
            None,
            cutoff_ns,
            0,
            0,
            None,
            false,
            Some("session bucket not found".to_string()),
        ));
    };

    let delete_ids = find_deletable_event_ids(&conn, bucketrow, cutoff_ns)?;
    let planned = delete_ids.len();
    let mut backup_file = None;
    let mut backup_created = false;
    let mut deleted = 0;
    if cli.apply && planned > 0 {
        fs::create_dir_all(&cli.backup_dir)
            .with_context(|| format!("create backup dir {}", cli.backup_dir.display()))?;
        let backup = backup_path(&cli.backup_dir);
        copy_sqlite_via_backup(&cli.db_path, &backup)?;
        backup_file = Some(backup);
        backup_created = true;
        deleted = delete_events(&conn, &delete_ids, cli.chunk_size)?;
    }

    Ok(base_report(
        cli,
        bucket,
        Some(bucketrow),
        cutoff_ns,
        planned,
        deleted,
        backup_file,
        backup_created,
        None,
    ))
}

#[allow(clippy::too_many_arguments)]
fn base_report(
    cli: &Cli,
    bucket: String,
    bucketrow: Option<i64>,
    cutoff_ns: i64,
    planned_delete_rows: usize,
    deleted_rows: usize,
    backup_path: Option<PathBuf>,
    backup_created: bool,
    skipped_reason: Option<String>,
) -> Report {
    Report {
        apply: cli.apply,
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        db_path: cli.db_path.display().to_string(),
        backup_path: backup_path.map(|path| path.display().to_string()),
        bucket,
        bucketrow,
        retention_days: cli.retention_days,
        cutoff_ns,
        allowed_event_types: ALLOWED_EVENT_TYPES.to_vec(),
        planned_delete_rows,
        deleted_rows,
        backup_created,
        skipped_reason,
    }
}

fn open_connection(path: &Path, writable: bool) -> Result<Connection> {
    let flags = if writable {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    } else {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    };
    let conn = Connection::open_with_flags(path, flags)
        .with_context(|| format!("open SQLite DB {}", path.display()))?;
    conn.busy_timeout(Duration::from_secs(10))?;
    Ok(conn)
}

fn bucket_row(conn: &Connection, bucket: &str) -> Result<Option<i64>> {
    let mut stmt =
        conn.prepare("select rowid from buckets where name = ?1 order by rowid limit 1")?;
    let mut rows = stmt.query([bucket])?;
    Ok(rows.next()?.map(|row| row.get::<_, i64>(0)).transpose()?)
}

fn find_deletable_event_ids(conn: &Connection, bucketrow: i64, cutoff_ns: i64) -> Result<Vec<i64>> {
    let mut stmt = conn
        .prepare("select id, data from events where bucketrow = ?1 and endtime < ?2 order by id")?;
    let rows = stmt.query_map(params![bucketrow, cutoff_ns], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut ids = Vec::new();
    for row in rows {
        let (id, data) = row?;
        if is_allowed_process_event(&data) {
            ids.push(id);
        }
    }
    Ok(ids)
}

fn is_allowed_process_event(data: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(data) else {
        return false;
    };
    let event_type = value
        .pointer("/eventType")
        .or_else(|| value.pointer("/data/eventType"))
        .and_then(Value::as_str);
    event_type.is_some_and(|event_type| ALLOWED_EVENT_TYPES.contains(&event_type))
}

fn copy_sqlite_via_backup(src: &Path, dst: &Path) -> Result<()> {
    let source =
        Connection::open(src).with_context(|| format!("open backup source {}", src.display()))?;
    source
        .backup(DatabaseName::Main, dst, None)
        .with_context(|| format!("backup {} to {}", src.display(), dst.display()))
}

fn delete_events(conn: &Connection, ids: &[i64], chunk_size: usize) -> Result<usize> {
    let mut deleted = 0;
    for chunk in ids.chunks(chunk_size) {
        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare("delete from events where id = ?1")?;
            for id in chunk {
                deleted += stmt.execute([id])?;
            }
        }
        tx.commit()?;
    }
    Ok(deleted)
}

fn backup_path(backup_dir: &Path) -> PathBuf {
    backup_dir.join(format!(
        "aw-sqlite-before-db-maintenance-{}.db",
        Utc::now().format("%Y%m%dT%H%M%SZ")
    ))
}

fn print_text(report: &Report) {
    println!(
        "aw-db-maintenance: {}",
        if report.apply { "apply" } else { "dry-run" }
    );
    println!("db_path: {}", report.db_path);
    println!("bucket: {}", report.bucket);
    println!("retention_days: {}", report.retention_days);
    println!("planned_delete_rows: {}", report.planned_delete_rows);
    println!("deleted_rows: {}", report.deleted_rows);
    println!("backup_created: {}", report.backup_created);
    if let Some(path) = &report.backup_path {
        println!("backup_path: {path}");
    }
    if let Some(reason) = &report.skipped_reason {
        println!("skipped_reason: {reason}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_allows_process_start_stop_events() {
        assert!(is_allowed_process_event(r#"{"eventType":"process_start"}"#));
        assert!(is_allowed_process_event(
            r#"{"data":{"eventType":"process_stop"}}"#
        ));
        assert!(!is_allowed_process_event(r#"{"eventType":"logon"}"#));
        assert!(!is_allowed_process_event(r#"not-json"#));
    }

    #[test]
    fn dry_run_does_not_delete_or_backup() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("aw.db");
        create_fixture_db(&db);
        let cli = Cli {
            db_path: db.clone(),
            backup_dir: dir.path().join("backups"),
            session_bucket: Some("aw-session-events_TEST".to_string()),
            host: None,
            retention_days: 7,
            chunk_size: 100,
            apply: false,
            json: true,
        };
        let report = build_report(&cli).unwrap();
        assert_eq!(report.planned_delete_rows, 2);
        assert_eq!(report.deleted_rows, 0);
        assert!(!report.backup_created);
        assert_eq!(count_events(&db), 3);
    }

    #[test]
    fn apply_deletes_only_old_process_events_and_keeps_logon() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("aw.db");
        create_fixture_db(&db);
        let cli = Cli {
            db_path: db.clone(),
            backup_dir: dir.path().join("backups"),
            session_bucket: Some("aw-session-events_TEST".to_string()),
            host: None,
            retention_days: 7,
            chunk_size: 1,
            apply: true,
            json: true,
        };
        let report = build_report(&cli).unwrap();
        assert_eq!(report.planned_delete_rows, 2);
        assert_eq!(report.deleted_rows, 2);
        assert!(report.backup_created);
        assert_eq!(count_events(&db), 1);
    }

    fn create_fixture_db(path: &Path) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            "create table buckets (name text);
             create table events (id integer primary key autoincrement, bucketrow integer, starttime integer, endtime integer, data text);",
        )
        .unwrap();
        conn.execute(
            "insert into buckets (name) values ('aw-session-events_TEST')",
            [],
        )
        .unwrap();
        conn.execute(
            "insert into events (bucketrow,starttime,endtime,data) values (1,1,2,'{\"eventType\":\"process_start\"}')",
            [],
        )
        .unwrap();
        conn.execute(
            "insert into events (bucketrow,starttime,endtime,data) values (1,3,4,'{\"eventType\":\"process_stop\"}')",
            [],
        )
        .unwrap();
        conn.execute(
            "insert into events (bucketrow,starttime,endtime,data) values (1,5,6,'{\"eventType\":\"logon\"}')",
            [],
        )
        .unwrap();
    }

    fn count_events(path: &Path) -> i64 {
        Connection::open(path)
            .unwrap()
            .query_row("select count(*) from events", [], |row| row.get(0))
            .unwrap()
    }
}
