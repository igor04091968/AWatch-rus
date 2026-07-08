use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use clap::Parser;
use fs2::FileExt;
use hayabusa_tools::{guess_host_from_filename, read_json_file};
use serde_json::{Value, json};
use zip::ZipArchive;

const LOCK_PATH: &str = "/opt/hayabusa/state/aw-hayabusa-autoprocess.lock";
const WRAPPER: &str = "/usr/local/bin/aw-hayabusa";
const LINKER: &str = "/usr/local/bin/aw-hayabusa-link-case";
const CASE_ALERT: &str = "/usr/local/bin/aw-hayabusa-case-alert";
const LATEST_INTAKE: &str = "/opt/hayabusa/state/latest-intake.json";

#[derive(Debug, Parser)]
#[command(about = "Auto-process Hayabusa zip packages dropped onto aw-rus server")]
struct Cli {
    #[arg(long, default_value = "/opt/activitywatch/aw-rus-ops/drop")]
    drop_dir: PathBuf,

    #[arg(long, default_value = "/opt/hayabusa/quarantine/drop")]
    quarantine_dir: PathBuf,

    #[arg(long, default_value_t = true)]
    once: bool,
}

#[derive(Debug)]
struct Sidecars {
    case_id: Option<i64>,
    host: Option<String>,
    mode: String,
    link_source: String,
    caseid_path: PathBuf,
    meta_path: PathBuf,
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
    let _once = cli.once;
    fs::create_dir_all(&cli.drop_dir)
        .with_context(|| format!("create {}", cli.drop_dir.display()))?;
    let lock_path = Path::new(LOCK_PATH);
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let lock = File::create(lock_path).with_context(|| format!("open {}", lock_path.display()))?;
    if lock.try_lock_exclusive().is_err() {
        eprintln!("autoprocess already running");
        return Ok(0);
    }

    let zips = list_zips(&cli.drop_dir)?;
    if zips.is_empty() {
        println!("no zip packages in drop dir");
        return Ok(0);
    }
    for zip_path in zips {
        match process_one(&zip_path) {
            Ok(result) => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "processed": zip_path.display().to_string(),
                        "latest_intake": result.latest_intake,
                        "case_alert": result.case_alert,
                    }))?
                );
            }
            Err(err) => {
                let quarantined = quarantine_drop_package(&cli.quarantine_dir, &zip_path, &err)?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "quarantined": zip_path.display().to_string(),
                        "quarantine_dir": quarantined.display().to_string(),
                        "reason": err.to_string(),
                    }))?
                );
            }
        }
    }
    Ok(0)
}

struct ProcessResult {
    latest_intake: Value,
    case_alert: Option<Value>,
}

fn list_zips(drop_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut zips = Vec::new();
    for entry in fs::read_dir(drop_dir).with_context(|| format!("read {}", drop_dir.display()))? {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("zip") {
            zips.push(path);
        }
    }
    zips.sort();
    Ok(zips)
}

fn process_one(zip_path: &Path) -> Result<ProcessResult> {
    wait_for_stable_zip(zip_path)?;
    let sidecars = load_sidecars(zip_path)?;
    let host = guess_host(zip_path, &sidecars);
    let mode = if sidecars.mode.is_empty() {
        "incident".to_string()
    } else {
        sidecars.mode.clone()
    };

    let mut accept_cmd = vec![
        "accept".to_string(),
        "--package".to_string(),
        zip_path.display().to_string(),
    ];
    if let Some(host) = host {
        accept_cmd.extend(["--host".to_string(), host]);
    }
    run_checked(Path::new(WRAPPER), &accept_cmd)?;
    run_checked(
        Path::new(WRAPPER),
        &[
            "process-inbox".to_string(),
            "--mode".to_string(),
            mode.clone(),
        ],
    )?;
    let latest = read_json_file(Path::new(LATEST_INTAKE))?;
    let report_dir = PathBuf::from(
        latest
            .get("report_dir")
            .and_then(Value::as_str)
            .context("latest intake report_dir missing")?,
    );
    let mut case_alert = None;
    if Path::new(CASE_ALERT).is_file() {
        let mut alert_cmd = vec![
            "--mode".to_string(),
            mode.clone(),
            "--link-source".to_string(),
            sidecars.link_source.clone(),
        ];
        if let Some(case_id) = sidecars.case_id {
            alert_cmd.extend(["--case-id".to_string(), case_id.to_string()]);
        }
        let output = run_capture(Path::new(CASE_ALERT), &alert_cmd)?;
        case_alert = Some(json!({
            "returncode": output.returncode,
            "stdout": output.stdout.trim(),
            "stderr": output.stderr.trim(),
        }));
    }
    archive_sidecars(&report_dir, &sidecars)?;
    archive_drop_package(&report_dir, zip_path)?;
    if let Some(case_id) = sidecars.case_id {
        if Path::new(CASE_ALERT).is_file() {
            return Ok(ProcessResult {
                latest_intake: latest,
                case_alert,
            });
        }
        run_checked(
            Path::new(LINKER),
            &[
                "--case-id".to_string(),
                case_id.to_string(),
                "--mode".to_string(),
                mode,
                "--link-source".to_string(),
                sidecars.link_source,
            ],
        )?;
    }
    Ok(ProcessResult {
        latest_intake: latest,
        case_alert,
    })
}

fn wait_for_stable_zip(zip_path: &Path) -> Result<()> {
    wait_for_stable_zip_with(
        zip_path,
        Duration::from_secs(60),
        Duration::from_secs(1),
        Duration::from_secs(2),
        2,
    )
}

fn wait_for_stable_zip_with(
    zip_path: &Path,
    max_wait: Duration,
    interval: Duration,
    min_modified_age: Duration,
    required_stable_checks: u32,
) -> Result<()> {
    let started = Instant::now();
    let mut last_len = None;
    let mut stable_checks = 0;
    let mut last_zip_error = None;

    loop {
        let metadata =
            fs::metadata(zip_path).with_context(|| format!("stat {}", zip_path.display()))?;
        let len = metadata.len();
        let modified_age = metadata
            .modified()
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .unwrap_or_default();

        if len > 0 && last_len == Some(len) && modified_age >= min_modified_age {
            stable_checks += 1;
        } else {
            stable_checks = 0;
        }

        if stable_checks >= required_stable_checks {
            match verify_zip_readable(zip_path) {
                Ok(()) => return Ok(()),
                Err(err) => {
                    last_zip_error = Some(err.to_string());
                    stable_checks = 0;
                }
            }
        }

        if started.elapsed() >= max_wait {
            if let Some(err) = last_zip_error {
                bail!(
                    "drop zip {} did not become readable: {err}",
                    zip_path.display()
                );
            }
            bail!(
                "drop zip {} did not stabilize within {}s",
                zip_path.display(),
                max_wait.as_secs()
            );
        }

        last_len = Some(len);
        thread::sleep(interval);
    }
}

fn verify_zip_readable(zip_path: &Path) -> Result<()> {
    let file = File::open(zip_path).with_context(|| format!("open {}", zip_path.display()))?;
    let mut archive =
        ZipArchive::new(file).with_context(|| format!("read zip {}", zip_path.display()))?;
    for idx in 0..archive.len() {
        let _entry = archive
            .by_index(idx)
            .with_context(|| format!("read zip entry {idx}"))?;
    }
    Ok(())
}

fn load_sidecars(zip_path: &Path) -> Result<Sidecars> {
    let base = zip_path.with_extension("");
    let caseid_path = base.with_extension("caseid");
    let meta_path = base.with_extension("meta.json");
    let meta = if meta_path.is_file() {
        read_json_file(&meta_path)?
    } else {
        json!({})
    };
    let mut case_id = meta.get("case_id").and_then(Value::as_i64);
    if case_id.is_none() && caseid_path.is_file() {
        let raw = fs::read_to_string(&caseid_path)
            .with_context(|| format!("read {}", caseid_path.display()))?;
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            case_id = Some(
                trimmed
                    .parse::<i64>()
                    .with_context(|| format!("parse {}", caseid_path.display()))?,
            );
        }
    }
    Ok(Sidecars {
        case_id,
        host: meta.get("host").and_then(Value::as_str).map(str::to_string),
        mode: meta
            .get("mode")
            .and_then(Value::as_str)
            .unwrap_or("incident")
            .to_string(),
        link_source: meta
            .get("link_source")
            .and_then(Value::as_str)
            .unwrap_or("aw-rus-drop-autoprocess")
            .to_string(),
        caseid_path,
        meta_path,
    })
}

fn quarantine_drop_package(
    quarantine_dir: &Path,
    zip_path: &Path,
    err: &anyhow::Error,
) -> Result<PathBuf> {
    fs::create_dir_all(quarantine_dir)
        .with_context(|| format!("create {}", quarantine_dir.display()))?;
    let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ");
    let zip_name = zip_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("package.zip");
    let target_dir = quarantine_dir.join(format!("{timestamp}_{zip_name}"));
    fs::create_dir_all(&target_dir).with_context(|| format!("create {}", target_dir.display()))?;

    let base = zip_path.with_extension("");
    for path in [
        zip_path.to_path_buf(),
        base.with_extension("caseid"),
        base.with_extension("meta.json"),
        zip_path.with_extension("zip.sha256"),
    ] {
        if path.is_file() {
            let target = target_dir.join(path.file_name().context("quarantine file name")?);
            fs::rename(&path, &target)
                .with_context(|| format!("move {} to {}", path.display(), target.display()))?;
        }
    }

    let reason = json!({
        "quarantined_at": Utc::now().to_rfc3339(),
        "package": zip_path.display().to_string(),
        "reason": err.to_string(),
        "error_chain": format!("{err:#}"),
    });
    fs::write(
        target_dir.join("reason.json"),
        serde_json::to_vec_pretty(&reason).context("serialize quarantine reason")?,
    )
    .with_context(|| format!("write {}", target_dir.join("reason.json").display()))?;
    Ok(target_dir)
}

fn archive_sidecars(report_dir: &Path, sidecars: &Sidecars) -> Result<()> {
    let target_dir = report_dir.join("input-sidecars");
    fs::create_dir_all(&target_dir).with_context(|| format!("create {}", target_dir.display()))?;
    for path in [&sidecars.caseid_path, &sidecars.meta_path] {
        if path.is_file() {
            fs::rename(
                path,
                target_dir.join(path.file_name().context("sidecar file name")?),
            )
            .with_context(|| format!("move {}", path.display()))?;
        }
    }
    Ok(())
}

fn archive_drop_package(report_dir: &Path, zip_path: &Path) -> Result<()> {
    let target_dir = report_dir.join("input-drop");
    fs::create_dir_all(&target_dir).with_context(|| format!("create {}", target_dir.display()))?;
    let target_path = target_dir.join(zip_path.file_name().context("zip file name")?);
    if target_path.exists() {
        fs::remove_file(&target_path)
            .with_context(|| format!("remove {}", target_path.display()))?;
    }
    fs::rename(zip_path, &target_path).with_context(|| format!("move {}", zip_path.display()))?;
    Ok(())
}

fn guess_host(zip_path: &Path, sidecars: &Sidecars) -> Option<String> {
    if let Some(host) = &sidecars.host {
        if !host.is_empty() {
            return Some(host.clone());
        }
    }
    let name = zip_path
        .file_stem()
        .and_then(|name| name.to_str())
        .map(guess_host_from_filename)?;
    (!name.is_empty()).then_some(name)
}

fn run_checked(program: &Path, args: &[String]) -> Result<()> {
    println!(
        "RUN {} {}",
        program.display(),
        args.iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" ")
    );
    let status = Command::new(program)
        .args(args)
        .status()
        .with_context(|| format!("run {}", program.display()))?;
    if !status.success() {
        bail!(
            "{} failed with status {}",
            program.display(),
            status.code().unwrap_or(1)
        );
    }
    Ok(())
}

struct Captured {
    returncode: i32,
    stdout: String,
    stderr: String,
}

fn run_capture(program: &Path, args: &[String]) -> Result<Captured> {
    println!(
        "RUN {} {}",
        program.display(),
        args.iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" ")
    );
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("run {}", program.display()))?;
    Ok(Captured {
        returncode: output.status.code().unwrap_or(1),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;
    use zip::write::SimpleFileOptions;

    #[test]
    fn wait_for_stable_zip_accepts_complete_archive() {
        let dir = tempdir().expect("tempdir");
        let zip_path = dir.path().join("HOST-20260709-000001.zip");
        write_test_zip(&zip_path);

        wait_for_stable_zip_with(
            &zip_path,
            Duration::from_secs(1),
            Duration::from_millis(1),
            Duration::from_secs(0),
            1,
        )
        .expect("complete zip should be accepted");
    }

    #[test]
    fn wait_for_stable_zip_rejects_unreadable_archive() {
        let dir = tempdir().expect("tempdir");
        let zip_path = dir.path().join("HOST-20260709-000001.zip");
        fs::write(&zip_path, b"not a zip").expect("write partial zip");

        let err = wait_for_stable_zip_with(
            &zip_path,
            Duration::from_millis(20),
            Duration::from_millis(1),
            Duration::from_secs(0),
            1,
        )
        .expect_err("invalid zip should be rejected");
        assert!(err.to_string().contains("did not become readable"));
    }

    fn write_test_zip(path: &Path) {
        let file = File::create(path).expect("create zip");
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file("manifest.json", SimpleFileOptions::default())
            .expect("start manifest");
        zip.write_all(br#"{"host":"SHARKON2025"}"#)
            .expect("write manifest");
        zip.finish().expect("finish zip");
    }
}
