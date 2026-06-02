use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Parser;
use fs2::FileExt;
use hayabusa_tools::{guess_host_from_filename, read_json_file};
use serde_json::{Value, json};

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
        let result = process_one(&zip_path)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "processed": zip_path.display().to_string(),
                "latest_intake": result.latest_intake,
                "case_alert": result.case_alert,
            }))?
        );
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
            "--limit".to_string(),
            "1".to_string(),
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
