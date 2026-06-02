use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use clap::Parser;
use detmir_core::exit_codes;
use detmir_state::write_json_atomic;
use fs2::FileExt;
use serde::Serialize;
use serde_json::Value;

const DEFAULT_STATE_DIR: &str = "/var/lib/detmir-ai";
const DEFAULT_CHECK_BIN: &str = "detmir-check";
const DEFAULT_DLP_BIN: &str = "detmir-dlp";
const DEFAULT_HEAL_BIN: &str = "detmir-heal-safe-rust";
const DEFAULT_POLLI_BIN: &str = "polli-chat";

#[derive(Debug, Parser)]
#[command(about = "DetMir autonomous check/report orchestration.")]
struct Cli {
    #[arg(long, default_value = DEFAULT_STATE_DIR)]
    state_dir: PathBuf,

    #[arg(long)]
    lock_dir: Option<PathBuf>,

    #[arg(long, default_value = DEFAULT_CHECK_BIN)]
    check_bin: String,

    #[arg(long, default_value = DEFAULT_DLP_BIN)]
    dlp_bin: String,

    #[arg(long, default_value = DEFAULT_HEAL_BIN)]
    heal_bin: String,

    #[arg(long, default_value = DEFAULT_POLLI_BIN)]
    polli_bin: String,

    #[arg(long, default_value_t = 14)]
    retain_days: u64,

    #[arg(long, default_value_t = 120)]
    command_timeout_seconds: u64,

    #[arg(long, default_value_t = 120)]
    report_timeout_seconds: u64,

    #[arg(long)]
    no_report: bool,

    #[arg(long)]
    enable_heal: bool,

    #[arg(long)]
    no_heal: bool,
}

impl Cli {
    fn apply_env(mut self) -> Self {
        self.state_dir = env_path("DETMIR_AI_STATE_DIR").unwrap_or(self.state_dir);
        self.lock_dir = env_path("DETMIR_AI_RUN_DIR").or(self.lock_dir);
        self.check_bin = env_string("DETMIR_CHECK_BIN").unwrap_or(self.check_bin);
        self.dlp_bin = env_string("DETMIR_DLP_BIN").unwrap_or(self.dlp_bin);
        self.heal_bin = env_string("DETMIR_HEAL_BIN").unwrap_or(self.heal_bin);
        self.polli_bin = env_string("DETMIR_POLLI_BIN").unwrap_or(self.polli_bin);
        self.retain_days = env_string("DETMIR_AI_RETAIN_DAYS")
            .and_then(|value| value.parse().ok())
            .unwrap_or(self.retain_days);
        self.report_timeout_seconds = env_string("DETMIR_REPORT_TIMEOUT_SECONDS")
            .and_then(|value| value.parse().ok())
            .unwrap_or(self.report_timeout_seconds);
        if env_string("DETMIR_AUTO_HEAL").is_some_and(|value| value == "1") {
            self.enable_heal = true;
        }
        if self.no_heal {
            self.enable_heal = false;
        }
        self
    }

    fn lock_dir(&self) -> PathBuf {
        self.lock_dir
            .clone()
            .unwrap_or_else(|| self.state_dir.join("locks"))
    }
}

#[derive(Debug)]
struct RunPaths {
    state_dir: PathBuf,
    run_dir: PathBuf,
    reports_dir: PathBuf,
    check_file: PathBuf,
    dlp_file: PathBuf,
    check_rc_file: PathBuf,
    dlp_rc_file: PathBuf,
    heal_rc_file: PathBuf,
    heal_log: PathBuf,
    bundle_file: PathBuf,
    report_file: PathBuf,
    state_file: PathBuf,
}

#[derive(Debug, Serialize)]
struct AutoSummary {
    check_rc: i32,
    dlp_rc: i32,
    check_ok: bool,
    dlp_ok: bool,
    severity: String,
    needs_heal: bool,
    reasons: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    detmir_summary: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dlp_counts: Option<Value>,
}

fn env_string(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

fn env_path(name: &str) -> Option<PathBuf> {
    env_string(name).map(PathBuf::from)
}

fn utc_stamp() -> String {
    Utc::now().format("%Y%m%d-%H%M%S").to_string()
}

fn utc_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn init_paths(state_dir: PathBuf) -> Result<RunPaths> {
    let stamp = utc_stamp();
    let run_dir = state_dir.join("runs").join(&stamp);
    let reports_dir = state_dir.join("reports");
    fs::create_dir_all(&run_dir)?;
    fs::create_dir_all(&reports_dir)?;
    fs::create_dir_all(state_dir.join("logs"))?;

    Ok(RunPaths {
        state_dir: state_dir.clone(),
        run_dir: run_dir.clone(),
        reports_dir: reports_dir.clone(),
        check_file: run_dir.join("detmir-check.json"),
        dlp_file: run_dir.join("detmir-dlp.json"),
        check_rc_file: run_dir.join("check.rc"),
        dlp_rc_file: run_dir.join("dlp.rc"),
        heal_rc_file: run_dir.join("heal.rc"),
        heal_log: run_dir.join("heal.log"),
        bundle_file: run_dir.join("bundle.txt"),
        report_file: reports_dir.join(format!("detmir-report-{stamp}.md")),
        state_file: state_dir.join(format!("state-{stamp}.json")),
    })
}

fn acquire_lock(lock_dir: &Path) -> Result<Option<File>> {
    fs::create_dir_all(lock_dir)?;
    let lock_path = lock_dir.join("detmir-auto.lock");
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| format!("failed to open lock {}", lock_path.display()))?;
    match file.try_lock_exclusive() {
        Ok(()) => Ok(Some(file)),
        Err(err) => {
            if err.kind() == std::io::ErrorKind::WouldBlock {
                println!("detmir-auto: another run is active");
                Ok(None)
            } else {
                Err(err).with_context(|| format!("failed to lock {}", lock_path.display()))
            }
        }
    }
}

fn run_to_file(
    command: &str,
    args: &[&str],
    output_path: &Path,
    rc_path: &Path,
    timeout: Duration,
) -> Result<i32> {
    let stdout = File::create(output_path)
        .with_context(|| format!("failed to create {}", output_path.display()))?;
    let stderr_path = output_path.with_extension("stderr");
    let stderr = File::create(&stderr_path)
        .with_context(|| format!("failed to create {}", stderr_path.display()))?;

    let mut child = Command::new(command)
        .args(args)
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .with_context(|| format!("failed to execute {command}"))?;

    let started = std::time::Instant::now();
    let rc = loop {
        if let Some(status) = child.try_wait()? {
            break status.code().unwrap_or(1);
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            let mut stderr = OpenOptions::new().append(true).open(&stderr_path)?;
            writeln!(
                stderr,
                "{command} timed out after {} seconds",
                timeout.as_secs()
            )?;
            break 124;
        }
        std::thread::sleep(Duration::from_millis(200));
    };

    if fs::metadata(&stderr_path)
        .map(|meta| meta.len())
        .unwrap_or(0)
        == 0
    {
        let _ = fs::remove_file(&stderr_path);
    }

    fs::write(rc_path, format!("{rc}\n"))?;
    Ok(rc)
}

fn read_rc(path: &Path) -> i32 {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(1)
}

fn summarize(paths: &RunPaths) -> AutoSummary {
    let check_rc = read_rc(&paths.check_rc_file);
    let dlp_rc = read_rc(&paths.dlp_rc_file);
    let mut summary = AutoSummary {
        check_rc,
        dlp_rc,
        check_ok: false,
        dlp_ok: false,
        severity: if check_rc != 0 || dlp_rc != 0 {
            "FAIL".to_string()
        } else {
            "OK".to_string()
        },
        needs_heal: check_rc != 0 || dlp_rc != 0,
        reasons: Vec::new(),
        detmir_summary: None,
        dlp_counts: None,
    };

    match read_json(&paths.check_file) {
        Ok(check) => {
            summary.check_ok = check.get("ok").and_then(Value::as_bool).unwrap_or(false);
            let check_summary = check.get("summary").cloned().unwrap_or(Value::Null);
            if check_summary.is_object() {
                if !summary.check_ok
                    && (int_field(&check_summary, "bucket_dead") > 0
                        || int_field(&check_summary, "bucket_stale") > 0
                        || int_field(&check_summary, "service_failures") > 0)
                {
                    summary.reasons.push(
                        "detmir-check has stale/dead bucket or required service failure"
                            .to_string(),
                    );
                }
                summary.detmir_summary = Some(check_summary);
            }
        }
        Err(err) => summary
            .reasons
            .push(format!("detmir-check parse failed: {err}")),
    }

    match read_json(&paths.dlp_file) {
        Ok(dlp) => {
            summary.dlp_ok = dlp.get("ok").and_then(Value::as_bool).unwrap_or(false);
            let counts = dlp.get("counts").cloned().unwrap_or(Value::Null);
            if counts.is_object() {
                if !summary.dlp_ok
                    && (int_field(&counts, "fail") > 0 || int_field(&counts, "warn") > 0)
                {
                    summary
                        .reasons
                        .push("dlp-health-check has warn/fail".to_string());
                }
                summary.dlp_counts = Some(counts);
            }
        }
        Err(err) => summary
            .reasons
            .push(format!("detmir-dlp parse failed: {err}")),
    }

    if summary.check_ok && summary.dlp_ok {
        summary.severity = "OK".to_string();
        summary.needs_heal = false;
    } else if summary.reasons.is_empty() {
        summary.severity = "WARN".to_string();
    } else {
        summary.severity = "FAIL".to_string();
    }

    summary
}

fn read_json(path: &Path) -> Result<Value> {
    let raw = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&raw)?)
}

fn int_field(value: &Value, key: &str) -> i64 {
    value.get(key).and_then(Value::as_i64).unwrap_or(0)
}

fn write_bundle(paths: &RunPaths, summary_after: &AutoSummary) -> Result<()> {
    let mut out = String::new();
    out.push_str(
        "Ты операторский AI-помощник DetMir. По фактам ниже дай короткий русский отчет.\n",
    );
    out.push_str("Структура ответа:\n");
    out.push_str("1. Состояние: OK/WARN/FAIL\n");
    out.push_str("2. Что важно\n");
    out.push_str("3. Что уже сделал автомат\n");
    out.push_str("4. Что сделать человеку, если нужно\n\n");
    out.push_str("Правила:\n");
    out.push_str("- Не предлагай рестарты, если факты чистые.\n");
    out.push_str("- Отличай event-driven bucket от dead/stale.\n");
    out.push_str("- DLP sendFailures важны только при новом sendFailuresDelta или warn/fail.\n");
    out.push_str(
        "- Auto-heal умеет только серверные systemd-сервисы AW/DLP; Windows/RDP не трогает.\n\n",
    );
    out.push_str("=== summary-before ===\n");
    out.push_str(
        &fs::read_to_string(paths.run_dir.join("summary-before.json")).unwrap_or_default(),
    );
    out.push_str("\n\n=== summary-after ===\n");
    out.push_str(&serde_json::to_string_pretty(summary_after)?);
    out.push_str("\n\n=== heal-log ===\n");
    out.push_str(&fs::read_to_string(&paths.heal_log).unwrap_or_default());
    out.push_str("\n\n=== detmir-check ===\n");
    out.push_str(&truncate_file(&paths.check_file, 1600));
    out.push_str("\n\n=== detmir-dlp ===\n");
    out.push_str(&truncate_file(&paths.dlp_file, 1600));
    fs::write(&paths.bundle_file, out)?;
    Ok(())
}

fn truncate_file(path: &Path, max_lines: usize) -> String {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .take(max_lines)
        .collect::<Vec<_>>()
        .join("\n")
}

fn write_report(
    paths: &RunPaths,
    polli_bin: &str,
    no_report: bool,
    report_timeout: Duration,
    summary: &AutoSummary,
) -> Result<()> {
    let mut report = format!(
        "# DetMir Autonomous Report\n\n- generated_at_utc: {}\n- run_dir: {}\n\n",
        utc_iso(),
        paths.run_dir.display()
    );

    if no_report {
        report.push_str("Pollinations report skipped; raw summary follows.\n");
        report.push_str(&serde_json::to_string_pretty(summary)?);
    } else {
        let bundle = File::open(&paths.bundle_file)?;
        match run_report_command(polli_bin, bundle, report_timeout) {
            Ok(output) if output.status.success() => {
                report.push_str(&String::from_utf8_lossy(&output.stdout));
            }
            Ok(output) => {
                report.push_str("Pollinations report failed; raw summary follows.\n");
                if !output.stderr.is_empty() {
                    report.push_str(&String::from_utf8_lossy(&output.stderr));
                    report.push('\n');
                }
                report.push_str(&serde_json::to_string_pretty(summary)?);
            }
            Err(err) => {
                report.push_str("Pollinations report failed; raw summary follows.\n");
                report.push_str(&format!("{err}\n"));
                report.push_str(&serde_json::to_string_pretty(summary)?);
            }
        }
    }

    fs::write(&paths.report_file, report)?;
    Ok(())
}

fn run_report_command(polli_bin: &str, bundle: File, timeout: Duration) -> Result<Output> {
    let mut child = Command::new(polli_bin)
        .args(["--model", "text.daily", "--max-tokens", "900"])
        .stdin(Stdio::from(bundle))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to execute {polli_bin}"))?;

    let started = std::time::Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            return child
                .wait_with_output()
                .with_context(|| format!("failed to collect {polli_bin} output"));
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!(
                "Pollinations report timed out after {} seconds",
                timeout.as_secs()
            );
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn update_latest(paths: &RunPaths) -> Result<()> {
    update_symlink(&paths.run_dir, &paths.state_dir.join("latest-run"))?;
    update_symlink(
        &paths.report_file,
        &paths.state_dir.join("latest-report.md"),
    )?;
    update_symlink(
        &paths.state_file,
        &paths.state_dir.join("latest-state.json"),
    )?;
    Ok(())
}

fn update_symlink(target: &Path, link: &Path) -> Result<()> {
    let tmp = link.with_extension(format!("tmp.{}", std::process::id()));
    let _ = fs::remove_file(&tmp);
    symlink(target, &tmp)?;
    fs::rename(&tmp, link)
        .with_context(|| format!("failed to update symlink {}", link.display()))?;
    Ok(())
}

fn cleanup_retention(paths: &RunPaths, retain_days: u64) -> Result<()> {
    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(retain_days * 24 * 60 * 60))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    cleanup_old_dirs(&paths.state_dir.join("runs"), cutoff)?;
    cleanup_old_files(&paths.reports_dir, "detmir-report-", Some(".md"), cutoff)?;
    cleanup_old_files(&paths.state_dir, "state-", Some(".json"), cutoff)?;
    Ok(())
}

fn cleanup_old_dirs(dir: &Path, cutoff: SystemTime) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() && is_old(&entry.path(), cutoff) {
            fs::remove_dir_all(entry.path())?;
        }
    }
    Ok(())
}

fn cleanup_old_files(
    dir: &Path,
    prefix: &str,
    suffix: Option<&str>,
    cutoff: SystemTime,
) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let suffix_ok = suffix.is_none_or(|suffix| name.ends_with(suffix));
        if entry.file_type()?.is_file()
            && name.starts_with(prefix)
            && suffix_ok
            && is_old(&entry.path(), cutoff)
        {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

fn is_old(path: &Path, cutoff: SystemTime) -> bool {
    path.metadata()
        .and_then(|meta| meta.modified())
        .map(|modified| modified < cutoff)
        .unwrap_or(false)
}

fn main() -> Result<()> {
    let args = Cli::parse().apply_env();
    let Some(_lock) = acquire_lock(&args.lock_dir())? else {
        return Ok(());
    };

    let paths = init_paths(args.state_dir.clone())?;
    let command_timeout = Duration::from_secs(args.command_timeout_seconds);
    let report_timeout = Duration::from_secs(args.report_timeout_seconds);
    run_to_file(
        &args.check_bin,
        &["--json"],
        &paths.check_file,
        &paths.check_rc_file,
        command_timeout,
    )?;
    run_to_file(
        &args.dlp_bin,
        &[],
        &paths.dlp_file,
        &paths.dlp_rc_file,
        command_timeout,
    )?;
    let summary_before = summarize(&paths);
    write_json_atomic(paths.run_dir.join("summary-before.json"), &summary_before)?;

    if summary_before.needs_heal && args.enable_heal {
        fs::write(
            &paths.heal_log,
            format!("auto-heal started at {}\n", utc_iso()),
        )?;
        let heal_rc = run_to_file(
            &args.heal_bin,
            &["--apply", "--json"],
            &paths.heal_log,
            &paths.heal_rc_file,
            command_timeout,
        )?;
        let mut heal_log = OpenOptions::new().append(true).open(&paths.heal_log)?;
        writeln!(
            heal_log,
            "\nauto-heal finished at {} rc={heal_rc}",
            utc_iso()
        )?;
        std::thread::sleep(Duration::from_secs(10));
        run_to_file(
            &args.check_bin,
            &["--json"],
            &paths.check_file,
            &paths.check_rc_file,
            command_timeout,
        )?;
        run_to_file(
            &args.dlp_bin,
            &[],
            &paths.dlp_file,
            &paths.dlp_rc_file,
            command_timeout,
        )?;
    } else {
        fs::write(
            &paths.heal_log,
            if summary_before.needs_heal {
                "auto-heal skipped (disabled)\n"
            } else {
                "auto-heal skipped\n"
            },
        )?;
        fs::write(&paths.heal_rc_file, "0\n")?;
    }

    let summary_after = summarize(&paths);
    write_json_atomic(&paths.state_file, &summary_after)?;
    write_bundle(&paths, &summary_after)?;
    write_report(
        &paths,
        &args.polli_bin,
        args.no_report,
        report_timeout,
        &summary_after,
    )?;
    update_latest(&paths)?;
    cleanup_retention(&paths, args.retain_days)?;

    print!("{}", fs::read_to_string(&paths.report_file)?);
    std::io::stdout().flush().ok();

    let final_check_rc = read_rc(&paths.check_rc_file);
    let final_dlp_rc = read_rc(&paths.dlp_rc_file);
    std::process::exit(
        if summary_after.severity == "OK" && final_check_rc == 0 && final_dlp_rc == 0 {
            exit_codes::OK
        } else {
            exit_codes::CHECK_FAILED
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarizes_clean_inputs() {
        let dir = tempfile::tempdir().unwrap();
        let paths = init_paths(dir.path().to_path_buf()).unwrap();
        fs::write(
            &paths.check_file,
            r#"{"ok": true, "summary": {"bucket_ok": 8, "bucket_stale": 0, "bucket_dead": 0, "service_failures": 0, "service_warnings": 0}}"#,
        )
        .unwrap();
        fs::write(
            &paths.dlp_file,
            r#"{"ok": true, "counts": {"ok": 22, "warn": 0, "fail": 0}}"#,
        )
        .unwrap();
        fs::write(&paths.check_rc_file, "0\n").unwrap();
        fs::write(&paths.dlp_rc_file, "0\n").unwrap();
        let summary = summarize(&paths);
        assert_eq!(summary.severity, "OK");
        assert!(!summary.needs_heal);
        assert!(summary.reasons.is_empty());
    }

    #[test]
    fn does_not_keep_reasons_when_child_reports_ok() {
        let dir = tempfile::tempdir().unwrap();
        let paths = init_paths(dir.path().to_path_buf()).unwrap();
        fs::write(
            &paths.check_file,
            r#"{"ok": true, "summary": {"bucket_ok": 8, "bucket_stale": 0, "bucket_dead": 0, "service_failures": 0, "service_warnings": 0}}"#,
        )
        .unwrap();
        fs::write(
            &paths.dlp_file,
            r#"{"ok": true, "counts": {"ok": 21, "warn": 1, "fail": 0}}"#,
        )
        .unwrap();
        fs::write(&paths.check_rc_file, "0\n").unwrap();
        fs::write(&paths.dlp_rc_file, "0\n").unwrap();
        let summary = summarize(&paths);
        assert_eq!(summary.severity, "OK");
        assert!(!summary.needs_heal);
        assert!(summary.reasons.is_empty());
    }
}
