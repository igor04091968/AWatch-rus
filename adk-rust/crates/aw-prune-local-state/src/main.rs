use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, bail};
use clap::Parser;
use serde::Serialize;

const DEFAULT_DATA_DIR: &str = "/var/lib/activitywatch";
const DEFAULT_WORKTIME_REPORT_CACHE_RETENTION_SECONDS: u64 = 86_400;
const TMP_ARCHIVE_PATTERNS: &[NamePattern] = &[
    NamePattern::PrefixSuffix("activitywatch-", ".zip"),
    NamePattern::PrefixSuffix("hayabusa-", ".zip"),
    NamePattern::Exact("aw-hayabusa-profiles.txt"),
];
const TMP_WEBUI_PATTERNS: &[NamePattern] = &[
    NamePattern::Exact("aw-worktime-ui-bridge.py"),
    NamePattern::Exact("views-default.json"),
    NamePattern::Exact("apply_webui_ru_patch.out"),
];

#[derive(Debug, Parser)]
#[command(about = "Safely prune ActivityWatch local app state and temporary artifacts")]
struct Cli {
    #[arg(long)]
    data_dir: Option<PathBuf>,

    #[arg(long)]
    backup_dir: Option<PathBuf>,

    #[arg(long)]
    browser_smoke_dir: Option<PathBuf>,

    #[arg(long)]
    worktime_report_cache_dir: Option<PathBuf>,

    #[arg(long)]
    tmp_dir: Option<PathBuf>,

    #[arg(long, default_value_t = 7)]
    backup_retention_days: u64,

    #[arg(long, default_value_t = 2)]
    backup_keep_last_db: usize,

    #[arg(long, default_value_t = 2)]
    backup_keep_last_json: usize,

    #[arg(long, default_value_t = 24)]
    browser_smoke_keep_runs: usize,

    #[arg(long, default_value_t = 1)]
    browser_smoke_retention_days: u64,

    #[arg(long, default_value_t = DEFAULT_WORKTIME_REPORT_CACHE_RETENTION_SECONDS)]
    worktime_report_cache_retention_seconds: u64,

    #[arg(long, default_value_t = 1)]
    tmp_archive_retention_days: u64,

    #[arg(long, default_value_t = 2)]
    tmp_webui_retention_days: u64,

    #[arg(long, default_value_t = false)]
    apply: bool,

    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Debug, Clone)]
struct Config {
    data_dir: PathBuf,
    backup_dir: PathBuf,
    browser_smoke_dir: PathBuf,
    worktime_report_cache_dir: PathBuf,
    tmp_dir: PathBuf,
    backup_retention_days: u64,
    backup_keep_last_db: usize,
    backup_keep_last_json: usize,
    browser_smoke_keep_runs: usize,
    browser_smoke_retention_days: u64,
    worktime_report_cache_retention_seconds: u64,
    tmp_archive_retention_days: u64,
    tmp_webui_retention_days: u64,
    apply: bool,
    json: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ItemKind {
    File,
    Directory,
}

#[derive(Debug, Clone, Serialize)]
struct PruneItem {
    path: PathBuf,
    kind: ItemKind,
    reason: String,
    age_days: Option<u64>,
    size_bytes: u64,
}

#[derive(Debug, Serialize)]
struct Summary {
    apply: bool,
    planned: usize,
    deleted: usize,
    failed: usize,
    bytes: u64,
    items: Vec<PruneItem>,
    errors: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
enum NamePattern {
    Exact(&'static str),
    PrefixSuffix(&'static str, &'static str),
}

impl NamePattern {
    fn matches(self, name: &str) -> bool {
        match self {
            Self::Exact(expected) => name == expected,
            Self::PrefixSuffix(prefix, suffix) => {
                name.starts_with(prefix) && name.ends_with(suffix)
            }
        }
    }
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
    let cfg = Config::from_cli(Cli::parse());
    let mut items = Vec::new();
    fs::create_dir_all(&cfg.backup_dir)
        .with_context(|| format!("create backup dir {}", cfg.backup_dir.display()))?;

    plan_backup_group(
        &cfg.backup_dir.join("db"),
        cfg.backup_keep_last_db,
        cfg.backup_retention_days,
        "backup_db",
        &mut items,
    )?;
    plan_backup_group(
        &cfg.backup_dir,
        cfg.backup_keep_last_json,
        cfg.backup_retention_days,
        "backup_root",
        &mut items,
    )?;
    plan_browser_smoke(&cfg, &mut items)?;
    plan_worktime_report_cache(&cfg, &mut items)?;
    plan_tmp(
        &cfg.tmp_dir,
        TMP_ARCHIVE_PATTERNS,
        cfg.tmp_archive_retention_days,
        "tmp_archive",
        &mut items,
    )?;
    plan_tmp(
        &cfg.tmp_dir,
        TMP_WEBUI_PATTERNS,
        cfg.tmp_webui_retention_days,
        "tmp_webui",
        &mut items,
    )?;

    validate_plan(&cfg, &items)?;
    let mut summary = Summary {
        apply: cfg.apply,
        planned: items.len(),
        deleted: 0,
        failed: 0,
        bytes: items.iter().map(|item| item.size_bytes).sum(),
        items,
        errors: Vec::new(),
    };
    if cfg.apply {
        apply_plan(&mut summary);
    }
    print_summary(&summary, cfg.json)?;
    if summary.failed == 0 { Ok(0) } else { Ok(1) }
}

impl Config {
    fn from_cli(cli: Cli) -> Self {
        let data_dir = cli
            .data_dir
            .or_else(|| env_path("AW_DATA_DIR"))
            .unwrap_or_else(|| PathBuf::from(DEFAULT_DATA_DIR));
        let backup_dir = cli
            .backup_dir
            .or_else(|| env_path("AW_BACKUP_DIR"))
            .unwrap_or_else(|| data_dir.join("backups"));
        let browser_smoke_dir = cli
            .browser_smoke_dir
            .or_else(|| env_path("AW_BROWSER_SMOKE_OUTPUT_DIR"))
            .unwrap_or_else(|| data_dir.join("browser-smoke"));
        let worktime_report_cache_dir = cli
            .worktime_report_cache_dir
            .or_else(|| env_path("AW_WORKTIME_REPORT_DISK_CACHE_DIR"))
            .unwrap_or_else(|| data_dir.join("worktime-report-cache"));
        let tmp_dir = cli
            .tmp_dir
            .or_else(|| env_path("AW_TMP_DIR"))
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        Self {
            data_dir,
            backup_dir,
            browser_smoke_dir,
            worktime_report_cache_dir,
            tmp_dir,
            backup_retention_days: env_u64("AW_BACKUP_RETENTION_DAYS", cli.backup_retention_days),
            backup_keep_last_db: env_usize("AW_BACKUP_KEEP_LAST_DB", cli.backup_keep_last_db),
            backup_keep_last_json: env_usize("AW_BACKUP_KEEP_LAST_JSON", cli.backup_keep_last_json),
            browser_smoke_keep_runs: env_usize(
                "AW_BROWSER_SMOKE_KEEP_RUNS",
                cli.browser_smoke_keep_runs,
            ),
            browser_smoke_retention_days: env_u64(
                "AW_BROWSER_SMOKE_RETENTION_DAYS",
                cli.browser_smoke_retention_days,
            ),
            worktime_report_cache_retention_seconds: env_u64(
                "AW_WORKTIME_REPORT_DISK_STALE_TTL_SECONDS",
                cli.worktime_report_cache_retention_seconds,
            ),
            tmp_archive_retention_days: env_u64(
                "AW_TMP_ARCHIVE_RETENTION_DAYS",
                cli.tmp_archive_retention_days,
            ),
            tmp_webui_retention_days: env_u64(
                "AW_TMP_WEBUI_RETENTION_DAYS",
                cli.tmp_webui_retention_days,
            ),
            apply: cli.apply,
            json: cli.json,
        }
    }
}

fn plan_backup_group(
    dir: &Path,
    keep_last: usize,
    keep_days: u64,
    reason: &str,
    items: &mut Vec<PruneItem>,
) -> Result<()> {
    let mut files = list_files(dir)?;
    files.sort_by(|a, b| b.modified.cmp(&a.modified));
    let cutoff = cutoff(keep_days);
    for (idx, candidate) in files.into_iter().enumerate() {
        if idx < keep_last || candidate.modified >= cutoff || is_rollback_critical(&candidate.path)
        {
            continue;
        }
        items.push(candidate.into_item(reason));
    }
    Ok(())
}

fn plan_browser_smoke(cfg: &Config, items: &mut Vec<PruneItem>) -> Result<()> {
    let mut dirs = list_run_dirs(&cfg.browser_smoke_dir)?;
    dirs.sort_by(|a, b| b.modified.cmp(&a.modified));
    let cutoff = cutoff(cfg.browser_smoke_retention_days);
    for (idx, candidate) in dirs.into_iter().enumerate() {
        if idx < cfg.browser_smoke_keep_runs || candidate.modified >= cutoff {
            continue;
        }
        items.push(candidate.into_item("browser_smoke_run"));
    }
    Ok(())
}

fn plan_worktime_report_cache(cfg: &Config, items: &mut Vec<PruneItem>) -> Result<()> {
    if cfg.worktime_report_cache_retention_seconds == 0 || !cfg.worktime_report_cache_dir.exists() {
        return Ok(());
    }
    let cutoff = cutoff_seconds(cfg.worktime_report_cache_retention_seconds);
    for entry in fs::read_dir(&cfg.worktime_report_cache_dir)
        .with_context(|| format!("read {}", cfg.worktime_report_cache_dir.display()))?
    {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.ends_with(".json") {
            continue;
        }
        let meta = entry.metadata()?;
        let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        if modified >= cutoff {
            continue;
        }
        items.push(
            Candidate {
                path: entry.path(),
                kind: ItemKind::File,
                modified,
                size_bytes: meta.len(),
            }
            .into_item("worktime_report_disk_cache"),
        );
    }
    Ok(())
}

fn plan_tmp(
    dir: &Path,
    patterns: &[NamePattern],
    keep_days: u64,
    reason: &str,
    items: &mut Vec<PruneItem>,
) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    let cutoff = cutoff(keep_days);
    for entry in fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if !file_type.is_file() && !file_type.is_symlink() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !patterns.iter().any(|pattern| pattern.matches(&name)) {
            continue;
        }
        let meta = entry.metadata()?;
        let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        if modified >= cutoff {
            continue;
        }
        items.push(
            Candidate {
                path: entry.path(),
                kind: ItemKind::File,
                modified,
                size_bytes: meta.len(),
            }
            .into_item(reason),
        );
    }
    Ok(())
}

#[derive(Debug)]
struct Candidate {
    path: PathBuf,
    kind: ItemKind,
    modified: SystemTime,
    size_bytes: u64,
}

impl Candidate {
    fn into_item(self, reason: &str) -> PruneItem {
        PruneItem {
            path: self.path,
            kind: self.kind,
            reason: reason.to_string(),
            age_days: age_days(self.modified),
            size_bytes: self.size_bytes,
        }
    }
}

fn list_files(dir: &Path) -> Result<Vec<Candidate>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let meta = entry.metadata()?;
        files.push(Candidate {
            path: entry.path(),
            kind: ItemKind::File,
            modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            size_bytes: meta.len(),
        });
    }
    Ok(files)
}

fn list_run_dirs(dir: &Path) -> Result<Vec<Candidate>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut dirs = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !looks_like_browser_smoke_run(&name) {
            continue;
        }
        let meta = entry.metadata()?;
        dirs.push(Candidate {
            path: entry.path(),
            kind: ItemKind::Directory,
            modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            size_bytes: dir_size(&entry.path()).unwrap_or(0),
        });
    }
    Ok(dirs)
}

fn looks_like_browser_smoke_run(name: &str) -> bool {
    name.len() >= 20
        && name.starts_with("20")
        && name.contains('T')
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

fn validate_plan(cfg: &Config, items: &[PruneItem]) -> Result<()> {
    for item in items {
        if is_rollback_critical(&item.path) {
            bail!(
                "refusing to delete rollback-critical path {}",
                item.path.display()
            );
        }
        let allowed = match item.reason.as_str() {
            "backup_db" => is_under_or_equal(&item.path, &cfg.backup_dir.join("db")),
            "backup_root" => is_under_or_equal(&item.path, &cfg.backup_dir),
            "browser_smoke_run" => is_under_or_equal(&item.path, &cfg.browser_smoke_dir),
            "worktime_report_disk_cache" => {
                is_under_or_equal(&item.path, &cfg.worktime_report_cache_dir)
            }
            "tmp_archive" | "tmp_webui" => is_under_or_equal(&item.path, &cfg.tmp_dir),
            _ => false,
        };
        if !allowed {
            bail!(
                "refusing to delete path outside allowlist {}",
                item.path.display()
            );
        }
        if item.reason == "backup_root" && item.path.parent() != Some(cfg.backup_dir.as_path()) {
            bail!("refusing nested backup_root delete {}", item.path.display());
        }
        if item.path == cfg.data_dir
            || item.path == cfg.backup_dir
            || item.path == cfg.browser_smoke_dir
            || item.path == cfg.worktime_report_cache_dir
        {
            bail!("refusing to delete root directory {}", item.path.display());
        }
        let name = item
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if is_sqlite_db_name(name) && item.reason != "backup_db" {
            bail!(
                "refusing to delete SQLite DB outside backup_db {}",
                item.path.display()
            );
        }
    }
    Ok(())
}

fn apply_plan(summary: &mut Summary) {
    for item in summary.items.clone() {
        let result = match item.kind {
            ItemKind::File => fs::remove_file(&item.path),
            ItemKind::Directory => fs::remove_dir_all(&item.path),
        };
        match result {
            Ok(()) => summary.deleted += 1,
            Err(err) if !item.path.exists() => {
                summary.deleted += 1;
                summary
                    .errors
                    .push(format!("already gone: {} ({err})", item.path.display()));
            }
            Err(err) => {
                summary.failed += 1;
                summary
                    .errors
                    .push(format!("{}: {err}", item.path.display()));
            }
        }
    }
}

fn print_summary(summary: &Summary, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(summary)?);
        return Ok(());
    }
    let mode = if summary.apply { "apply" } else { "dry-run" };
    println!(
        "aw-prune-local-state: mode={mode} planned={} deleted={} failed={} bytes={}",
        summary.planned, summary.deleted, summary.failed, summary.bytes
    );
    for item in &summary.items {
        let verb = if summary.apply {
            "DELETE"
        } else {
            "WOULD_DELETE"
        };
        println!(
            "{verb} {:?} {} reason={} age_days={} bytes={}",
            item.kind,
            item.path.display(),
            item.reason,
            item.age_days
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            item.size_bytes
        );
    }
    for err in &summary.errors {
        eprintln!("WARN {err}");
    }
    Ok(())
}

fn cutoff(days: u64) -> SystemTime {
    cutoff_seconds(days.saturating_mul(86_400))
}

fn cutoff_seconds(seconds: u64) -> SystemTime {
    SystemTime::now()
        .checked_sub(Duration::from_secs(seconds))
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

fn age_days(modified: SystemTime) -> Option<u64> {
    SystemTime::now()
        .duration_since(modified)
        .ok()
        .map(|age| age.as_secs() / 86_400)
}

fn dir_size(path: &Path) -> Result<u64> {
    let mut total = 0;
    for entry in fs::read_dir(path).with_context(|| format!("read {}", path.display()))? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let child = entry.path();
        if file_type.is_dir() {
            total += dir_size(&child).unwrap_or(0);
        } else if file_type.is_file() || file_type.is_symlink() {
            total += entry.metadata().map(|meta| meta.len()).unwrap_or(0);
        }
    }
    Ok(total)
}

fn is_under_or_equal(path: &Path, root: &Path) -> bool {
    path == root || path.starts_with(root)
}

fn is_rollback_critical(path: &Path) -> bool {
    let text = path.to_string_lossy().to_ascii_lowercase();
    text.contains("switch-backups")
        || text.contains("before-rust")
        || text.contains("rollback")
        || text.contains("pre-switch")
}

fn is_sqlite_db_name(name: &str) -> bool {
    name.ends_with(".sqlite")
        || name.ends_with(".sqlite3")
        || name.ends_with(".db")
        || name.ends_with(".db-shm")
        || name.ends_with(".db-wal")
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default)
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    #[test]
    fn browser_smoke_run_name_is_narrow() {
        assert!(looks_like_browser_smoke_run("2026-06-02T03-02-19-530Z"));
        assert!(!looks_like_browser_smoke_run(".cache"));
        assert!(!looks_like_browser_smoke_run("latest-result.json"));
    }

    #[test]
    fn rollback_critical_paths_are_protected() {
        assert!(is_rollback_critical(Path::new(
            "/var/lib/activitywatch/health/switch-backups/file"
        )));
        assert!(is_rollback_critical(Path::new(
            "/var/lib/activitywatch/backups/db/before-rust.sqlite"
        )));
    }

    #[test]
    fn backup_group_keeps_newest_even_with_zero_retention() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let old = tmp.path().join("old.json");
        let new = tmp.path().join("new.json");
        File::create(&old).expect("old");
        std::thread::sleep(Duration::from_millis(5));
        File::create(&new).expect("new");
        let mut items = Vec::new();
        plan_backup_group(tmp.path(), 1, 0, "backup_root", &mut items).expect("plan");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].path, old);
    }

    #[test]
    fn validation_rejects_sqlite_outside_backup_db() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cfg = Config {
            data_dir: tmp.path().to_path_buf(),
            backup_dir: tmp.path().join("backups"),
            browser_smoke_dir: tmp.path().join("browser-smoke"),
            worktime_report_cache_dir: tmp.path().join("worktime-report-cache"),
            tmp_dir: tmp.path().join("tmp"),
            backup_retention_days: 1,
            backup_keep_last_db: 1,
            backup_keep_last_json: 1,
            browser_smoke_keep_runs: 1,
            browser_smoke_retention_days: 1,
            worktime_report_cache_retention_seconds: 1,
            tmp_archive_retention_days: 1,
            tmp_webui_retention_days: 1,
            apply: false,
            json: false,
        };
        let item = PruneItem {
            path: cfg.browser_smoke_dir.join("state.db"),
            kind: ItemKind::File,
            reason: "browser_smoke_run".to_string(),
            age_days: Some(2),
            size_bytes: 1,
        };
        assert!(validate_plan(&cfg, &[item]).is_err());
    }

    #[test]
    fn worktime_report_cache_prunes_only_json_files_inside_cache_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cache_dir = tmp.path().join("worktime-report-cache");
        fs::create_dir_all(&cache_dir).expect("cache dir");
        fs::write(cache_dir.join("old-cache.json"), b"{}").expect("cache json");
        fs::write(cache_dir.join("keep.txt"), b"keep").expect("non-json");
        fs::create_dir_all(cache_dir.join("nested")).expect("nested dir");
        fs::write(cache_dir.join("nested").join("nested-cache.json"), b"{}")
            .expect("nested cache json");

        let cfg = Config {
            data_dir: tmp.path().to_path_buf(),
            backup_dir: tmp.path().join("backups"),
            browser_smoke_dir: tmp.path().join("browser-smoke"),
            worktime_report_cache_dir: cache_dir.clone(),
            tmp_dir: tmp.path().join("tmp"),
            backup_retention_days: 1,
            backup_keep_last_db: 1,
            backup_keep_last_json: 1,
            browser_smoke_keep_runs: 1,
            browser_smoke_retention_days: 1,
            worktime_report_cache_retention_seconds: 1,
            tmp_archive_retention_days: 1,
            tmp_webui_retention_days: 1,
            apply: false,
            json: false,
        };

        std::thread::sleep(Duration::from_secs(2));
        let mut items = Vec::new();
        plan_worktime_report_cache(&cfg, &mut items).expect("plan cache");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].reason, "worktime_report_disk_cache");
        assert_eq!(items[0].path, cache_dir.join("old-cache.json"));
        validate_plan(&cfg, &items).expect("valid cache plan");
    }
}
