use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use chrono::{SecondsFormat, Utc};
use clap::Parser;
use rusqlite::{Connection, DatabaseName, OpenFlags, params};
use serde::Serialize;
use serde_json::Value;

const DEFAULT_DB_PATH: &str = "/var/lib/activitywatch/aw-server-rust/sqlite.db";
const DEFAULT_BACKUP_DIR: &str = "/var/lib/activitywatch/backups/db";
const DEFAULT_HOST: &str = "HOST-EXAMPLE";
const DEFAULT_SERVICE_UNIT: &str = "activitywatch-server.service";
const DEFAULT_LOCK_PATH: &str = "/run/aw-db-maintenance.lock";
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
    vacuum: bool,

    #[arg(
        long,
        default_value = DEFAULT_SERVICE_UNIT,
        env = "AW_DB_MAINTENANCE_SERVICE_UNIT"
    )]
    service_unit: String,

    #[arg(long, default_value = DEFAULT_LOCK_PATH, env = "AW_DB_MAINTENANCE_LOCK_PATH")]
    lock_path: PathBuf,

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
    lock_path: String,
    skipped_reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct VacuumReport {
    apply: bool,
    generated_at_utc: String,
    db_path: String,
    service_unit: String,
    service_was_active: bool,
    service_restarted: bool,
    backup_path: Option<String>,
    backup_created: bool,
    lock_path: String,
    db_size_before_bytes: Option<u64>,
    vacuumed_path: Option<String>,
    vacuumed_size_bytes: Option<u64>,
    integrity_check: Option<String>,
    replaced_db: bool,
    skipped_reason: Option<String>,
}

struct VacuumResult {
    backup_path: PathBuf,
    vacuumed_path: PathBuf,
    db_size_before_bytes: u64,
    vacuumed_size_bytes: u64,
    integrity_check: String,
}

struct ServiceGuard {
    unit: String,
    was_active: bool,
    restored: bool,
}

struct TempFileGuard {
    path: PathBuf,
    keep: bool,
}

struct LockFileGuard {
    path: PathBuf,
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
    if cli.vacuum {
        let report = build_vacuum_report(&cli)?;
        if cli.json {
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else {
            print_vacuum_text(&report);
        }
    } else {
        let report = build_report(&cli)?;
        if cli.json {
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else {
            print_text(&report);
        }
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
    let _lock_guard = if cli.apply && planned > 0 {
        Some(LockFileGuard::acquire(&cli.lock_path)?)
    } else {
        None
    };
    if cli.apply && planned > 0 {
        fs::create_dir_all(&cli.backup_dir)
            .with_context(|| format!("create backup dir {}", cli.backup_dir.display()))?;
        let backup = backup_path(&cli.backup_dir, "aw-sqlite-before-db-maintenance");
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

fn build_vacuum_report(cli: &Cli) -> Result<VacuumReport> {
    if !cli.db_path.exists() {
        return Ok(vacuum_report(
            cli,
            false,
            None,
            false,
            false,
            false,
            None,
            None,
            None,
            None,
            Some("database not found".to_string()),
        ));
    }

    if !cli.apply {
        return Ok(vacuum_report(
            cli,
            false,
            Some(file_size(&cli.db_path)?),
            false,
            false,
            false,
            None,
            None,
            None,
            None,
            Some("dry-run".to_string()),
        ));
    }

    let _lock_guard = LockFileGuard::acquire(&cli.lock_path)?;
    let mut service_guard = ServiceGuard::stop_if_active(&cli.service_unit)?;
    let service_was_active = service_guard.was_active;
    let result = vacuum_sqlite_db(&cli.db_path, &cli.backup_dir)?;
    let service_restarted = service_guard.restore()?;

    Ok(vacuum_report(
        cli,
        true,
        Some(result.db_size_before_bytes),
        true,
        service_was_active,
        service_restarted,
        Some(result.backup_path),
        Some(result.vacuumed_path),
        Some(result.vacuumed_size_bytes),
        Some(result.integrity_check),
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
        lock_path: cli.lock_path.display().to_string(),
        skipped_reason,
    }
}

#[allow(clippy::too_many_arguments)]
fn vacuum_report(
    cli: &Cli,
    apply: bool,
    db_size_before_bytes: Option<u64>,
    backup_created: bool,
    service_was_active: bool,
    service_restarted: bool,
    backup_path: Option<PathBuf>,
    vacuumed_path: Option<PathBuf>,
    vacuumed_size_bytes: Option<u64>,
    integrity_check: Option<String>,
    skipped_reason: Option<String>,
) -> VacuumReport {
    VacuumReport {
        apply,
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        db_path: cli.db_path.display().to_string(),
        service_unit: cli.service_unit.clone(),
        service_was_active,
        service_restarted,
        backup_path: backup_path.map(|path| path.display().to_string()),
        backup_created,
        lock_path: cli.lock_path.display().to_string(),
        db_size_before_bytes,
        vacuumed_path: vacuumed_path.map(|path| path.display().to_string()),
        vacuumed_size_bytes,
        integrity_check,
        replaced_db: apply && skipped_reason.is_none(),
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

fn backup_path(backup_dir: &Path, prefix: &str) -> PathBuf {
    backup_dir.join(format!(
        "{}-{}.db",
        prefix,
        Utc::now().format("%Y%m%dT%H%M%SZ")
    ))
}

fn vacuum_sqlite_db(db_path: &Path, backup_dir: &Path) -> Result<VacuumResult> {
    fs::create_dir_all(backup_dir)
        .with_context(|| format!("create backup dir {}", backup_dir.display()))?;
    let db_size_before_bytes = file_size(db_path)?;
    let backup_path = backup_path(backup_dir, "aw-sqlite-before-db-vacuum");
    copy_sqlite_via_backup(db_path, &backup_path)?;
    let vacuumed_path = vacuumed_path(db_path)?;
    let mut vacuum_cleanup = TempFileGuard::new(vacuumed_path.clone());
    vacuum_into(db_path, &vacuumed_path)?;
    preserve_sqlite_metadata(db_path, &vacuumed_path)?;
    let vacuumed_size_bytes = file_size(&vacuumed_path)?;
    let integrity_check = integrity_check(&vacuumed_path)?;
    remove_sqlite_sidecars(db_path)?;
    fs::rename(&vacuumed_path, db_path).with_context(|| {
        format!(
            "replace {} with {}",
            db_path.display(),
            vacuumed_path.display()
        )
    })?;
    vacuum_cleanup.disarm();
    Ok(VacuumResult {
        backup_path,
        vacuumed_path,
        db_size_before_bytes,
        vacuumed_size_bytes,
        integrity_check,
    })
}

fn vacuum_into(src: &Path, dst: &Path) -> Result<()> {
    let conn = open_connection(src, true)?;
    let sql = format!("VACUUM INTO {}", sqlite_string_literal(dst));
    conn.execute_batch(&sql)
        .with_context(|| format!("VACUUM INTO {}", dst.display()))
}

fn integrity_check(path: &Path) -> Result<String> {
    let conn = open_connection(path, false)?;
    let result: String = conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if result != "ok" {
        bail!("integrity_check failed for {}: {result}", path.display());
    }
    Ok(result)
}

fn remove_sqlite_sidecars(db_path: &Path) -> Result<()> {
    for suffix in ["-wal", "-shm", "-journal"] {
        let sidecar = sqlite_sidecar_path(db_path, suffix)?;
        match fs::remove_file(&sidecar) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err).with_context(|| format!("remove {}", sidecar.display())),
        }
    }
    Ok(())
}

fn sqlite_sidecar_path(db_path: &Path, suffix: &str) -> Result<PathBuf> {
    let file_name = db_path
        .file_name()
        .and_then(|value| value.to_str())
        .context("database path must have a file name")?;
    Ok(db_path.with_file_name(format!("{file_name}{suffix}")))
}

fn vacuumed_path(db_path: &Path) -> Result<PathBuf> {
    let file_name = db_path
        .file_name()
        .and_then(|value| value.to_str())
        .context("database path must have a file name")?;
    Ok(db_path.with_file_name(format!(
        "{file_name}.vacuumed-{}",
        Utc::now().format("%Y%m%dT%H%M%SZ")
    )))
}

fn file_size(path: &Path) -> Result<u64> {
    Ok(fs::metadata(path)
        .with_context(|| format!("stat {}", path.display()))?
        .len())
}

fn sqlite_string_literal(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "''"))
}

fn preserve_sqlite_metadata(src: &Path, dst: &Path) -> Result<()> {
    let metadata = fs::metadata(src).with_context(|| format!("stat {}", src.display()))?;
    let permissions = metadata.permissions();
    fs::set_permissions(dst, permissions)
        .with_context(|| format!("preserve permissions for {}", dst.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        let dst_metadata = fs::metadata(dst).with_context(|| format!("stat {}", dst.display()))?;
        if dst_metadata.uid() != metadata.uid() || dst_metadata.gid() != metadata.gid() {
            let status = Command::new("chown")
                .arg(format!("{}:{}", metadata.uid(), metadata.gid()))
                .arg(dst)
                .status()
                .context("run chown for vacuumed SQLite DB")?;
            if !status.success() {
                bail!("chown failed for {}", dst.display());
            }
        }
    }
    Ok(())
}

fn systemctl_is_active(unit: &str) -> Result<bool> {
    let load_state = systemctl_load_state(unit)?;
    if load_state != "loaded" {
        bail!("refusing SQLite VACUUM because systemd unit {unit} load_state={load_state:?}");
    }
    let output = Command::new("systemctl")
        .args(["is-active", unit])
        .output()
        .with_context(|| format!("systemctl is-active {unit}"))?;
    let state = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if output.status.success() && state == "active" {
        return Ok(true);
    }
    if output.status.code() == Some(3) && state == "inactive" {
        return Ok(false);
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    bail!(
        "refusing SQLite VACUUM because systemctl is-active {unit} returned state={state:?}, status={}, stderr={stderr:?}",
        output.status
    );
}

fn systemctl_load_state(unit: &str) -> Result<String> {
    let output = Command::new("systemctl")
        .args(["show", "-p", "LoadState", "--value", unit])
        .output()
        .with_context(|| format!("systemctl show LoadState {unit}"))?;
    let state = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if output.status.success() && !state.is_empty() {
        return Ok(state);
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    bail!(
        "refusing SQLite VACUUM because systemctl show LoadState {unit} failed with status={}, stderr={stderr:?}",
        output.status
    );
}

fn systemctl_action(action: &str, unit: &str) -> Result<()> {
    let status = Command::new("systemctl")
        .args([action, unit])
        .status()
        .with_context(|| format!("systemctl {action} {unit}"))?;
    if status.success() {
        Ok(())
    } else {
        bail!("systemctl {action} {unit} failed with status {status}");
    }
}

impl ServiceGuard {
    fn stop_if_active(unit: &str) -> Result<Self> {
        let was_active = systemctl_is_active(unit)?;
        if was_active {
            systemctl_action("stop", unit)?;
        }
        Ok(Self {
            unit: unit.to_string(),
            was_active,
            restored: !was_active,
        })
    }

    fn restore(&mut self) -> Result<bool> {
        if self.was_active && !self.restored {
            systemctl_action("start", &self.unit)?;
            self.restored = true;
        }
        Ok(self.was_active)
    }
}

impl Drop for ServiceGuard {
    fn drop(&mut self) {
        if self.was_active && !self.restored {
            let _ = systemctl_action("start", &self.unit);
        }
    }
}

impl TempFileGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, keep: false }
    }

    fn disarm(&mut self) {
        self.keep = true;
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if !self.keep {
            let _ = fs::remove_file(&self.path);
        }
    }
}

impl LockFileGuard {
    fn acquire(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create lock parent {}", parent.display()))?;
        }
        let mut file = match OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                bail!("maintenance lock already exists: {}", path.display());
            }
            Err(err) => return Err(err).with_context(|| format!("create lock {}", path.display())),
        };
        writeln!(
            file,
            "pid={} generated_at_utc={}",
            std::process::id(),
            Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
        )
        .with_context(|| format!("write lock {}", path.display()))?;
        Ok(Self {
            path: path.to_path_buf(),
        })
    }
}

impl Drop for LockFileGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
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
    println!("lock_path: {}", report.lock_path);
    if let Some(path) = &report.backup_path {
        println!("backup_path: {path}");
    }
    if let Some(reason) = &report.skipped_reason {
        println!("skipped_reason: {reason}");
    }
}

fn print_vacuum_text(report: &VacuumReport) {
    println!(
        "aw-db-vacuum: {}",
        if report.apply { "apply" } else { "dry-run" }
    );
    println!("db_path: {}", report.db_path);
    println!("service_unit: {}", report.service_unit);
    println!("service_was_active: {}", report.service_was_active);
    println!("service_restarted: {}", report.service_restarted);
    println!("backup_created: {}", report.backup_created);
    println!("lock_path: {}", report.lock_path);
    if let Some(path) = &report.backup_path {
        println!("backup_path: {path}");
    }
    if let Some(size) = report.db_size_before_bytes {
        println!("db_size_before_bytes: {size}");
    }
    if let Some(path) = &report.vacuumed_path {
        println!("vacuumed_path: {path}");
    }
    if let Some(size) = report.vacuumed_size_bytes {
        println!("vacuumed_size_bytes: {size}");
    }
    if let Some(check) = &report.integrity_check {
        println!("integrity_check: {check}");
    }
    println!("replaced_db: {}", report.replaced_db);
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
            vacuum: false,
            service_unit: DEFAULT_SERVICE_UNIT.to_string(),
            lock_path: dir.path().join("maintenance.lock"),
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
            vacuum: false,
            service_unit: DEFAULT_SERVICE_UNIT.to_string(),
            lock_path: dir.path().join("maintenance.lock"),
            json: true,
        };
        let report = build_report(&cli).unwrap();
        assert_eq!(report.planned_delete_rows, 2);
        assert_eq!(report.deleted_rows, 2);
        assert!(report.backup_created);
        assert_eq!(count_events(&db), 1);
    }

    #[test]
    fn vacuum_apply_compacts_database_and_preserves_rows() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("sqlite.db");
        create_vacuum_fixture_db(&db);

        let before = file_size(&db).unwrap();
        let result = vacuum_sqlite_db(&db, dir.path()).unwrap();
        let after = file_size(&db).unwrap();

        assert!(result.vacuumed_size_bytes < result.db_size_before_bytes);
        assert!(after < before);
        assert_eq!(result.integrity_check, "ok");
        assert!(result.backup_path.exists());
        assert_eq!(count_rows(&db), 32);
    }

    #[test]
    fn vacuum_dry_run_skips_mutation() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("sqlite.db");
        create_vacuum_fixture_db(&db);
        let cli = Cli {
            db_path: db.clone(),
            backup_dir: dir.path().join("backups"),
            session_bucket: None,
            host: None,
            retention_days: 7,
            chunk_size: 100,
            apply: false,
            vacuum: true,
            service_unit: DEFAULT_SERVICE_UNIT.to_string(),
            lock_path: dir.path().join("maintenance.lock"),
            json: true,
        };
        let report = build_vacuum_report(&cli).unwrap();
        assert!(!report.backup_created);
        assert!(!report.replaced_db);
        assert_eq!(report.skipped_reason.as_deref(), Some("dry-run"));
        assert_eq!(count_rows(&db), 32);
    }

    #[test]
    fn apply_refuses_when_lock_exists() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("aw.db");
        create_fixture_db(&db);
        let lock_path = dir.path().join("maintenance.lock");
        fs::write(&lock_path, "busy").unwrap();
        let cli = Cli {
            db_path: db.clone(),
            backup_dir: dir.path().join("backups"),
            session_bucket: Some("aw-session-events_TEST".to_string()),
            host: None,
            retention_days: 7,
            chunk_size: 1,
            apply: true,
            vacuum: false,
            service_unit: DEFAULT_SERVICE_UNIT.to_string(),
            lock_path,
            json: true,
        };
        let err = build_report(&cli).unwrap_err().to_string();
        assert!(err.contains("maintenance lock already exists"));
        assert_eq!(count_events(&db), 3);
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

    fn create_vacuum_fixture_db(path: &Path) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            "create table items (id integer primary key autoincrement, payload text);",
        )
        .unwrap();
        let payload = "x".repeat(4096);
        for _ in 0..64 {
            conn.execute("insert into items (payload) values (?1)", [&payload])
                .unwrap();
        }
        for id in 1..=32 {
            conn.execute("delete from items where id = ?1", [id])
                .unwrap();
        }
    }

    fn count_events(path: &Path) -> i64 {
        Connection::open(path)
            .unwrap()
            .query_row("select count(*) from events", [], |row| row.get(0))
            .unwrap()
    }

    fn count_rows(path: &Path) -> i64 {
        Connection::open(path)
            .unwrap()
            .query_row("select count(*) from items", [], |row| row.get(0))
            .unwrap()
    }
}
