use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Parser;
use hayabusa_tools::{guess_host_from_filename, windows_filename};
use regex::Regex;
use serde_json::Value;

const WINDOWS_EXPORT_CMD: &str = r"powershell.exe -ExecutionPolicy Bypass -File C:\ProgramData\AWatch-rus\export-evtx-for-hayabusa.ps1 -DaysBack {days_back} | ConvertTo-Json -Depth 8 -Compress";
const WINDOWS_LATEST_ZIP_CMD: &str = r"Get-ChildItem 'C:\ProgramData\AWatch-rus\forensics\evtx-exports' -File -Filter '*.zip' | Sort-Object LastWriteTime -Descending | Select-Object -First 1 FullName,Length,LastWriteTime | ConvertTo-Json -Compress";

#[derive(Debug, Parser)]
#[command(
    about = "Run Windows EVTX export and Hayabusa intake directly from aw-server, without the laptop"
)]
struct Cli {
    #[arg(
        long,
        default_value = "/opt/activitywatch/aw-rus-ops/ansible/inventory.ini"
    )]
    inventory: PathBuf,

    #[arg(long, default_value = "/opt/activitywatch/aw-rus-ops/venv/bin/ansible")]
    ansible_bin: PathBuf,

    #[arg(long, default_value = "/opt/activitywatch/aw-rus-ops/drop")]
    drop_dir: PathBuf,

    #[arg(long, default_value_t = 1)]
    days_back: i64,

    #[arg(long, default_value = "incident", value_parser = ["quick", "incident", "full"])]
    mode: String,

    #[arg(long)]
    case_id: Option<i64>,

    #[arg(long, default_value = "aw-rus-ops-from-windows")]
    link_source: String,

    #[arg(long, default_value = "aw_windows")]
    windows_group: String,

    #[arg(long, default_value = "/usr/local/bin/aw-hayabusa")]
    wrapper: PathBuf,

    #[arg(long, default_value = "/usr/local/bin/aw-hayabusa-link-case")]
    linker: PathBuf,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    ensure_file(&cli.inventory, "inventory")?;
    ensure_file(&cli.ansible_bin, "ansible binary")?;
    ensure_file(&cli.wrapper, "wrapper")?;
    fs::create_dir_all(&cli.drop_dir)
        .with_context(|| format!("create {}", cli.drop_dir.display()))?;

    let export_arg = WINDOWS_EXPORT_CMD.replace("{days_back}", &cli.days_back.to_string());
    let export_cmd = vec![
        cli.windows_group.as_str(),
        "-i",
        path_str(&cli.inventory)?,
        "-m",
        "win_shell",
        "-a",
        export_arg.as_str(),
    ];
    print_run("RUN_EXPORT", &cli.ansible_bin, &export_cmd);
    let export_out = run_capture(&cli.ansible_bin, &export_cmd)?;
    let export_json = extract_json_blob(&export_out)?;

    let list_cmd = vec![
        cli.windows_group.as_str(),
        "-i",
        path_str(&cli.inventory)?,
        "-m",
        "win_shell",
        "-a",
        WINDOWS_LATEST_ZIP_CMD,
    ];
    print_run("RUN_LIST", &cli.ansible_bin, &list_cmd);
    let latest_out = run_capture(&cli.ansible_bin, &list_cmd)?;
    let latest = normalize_latest_zip(extract_json_blob(&latest_out)?)?;
    let remote_zip = latest
        .get("FullName")
        .and_then(Value::as_str)
        .context("latest zip FullName missing")?;
    let filename = windows_filename(remote_zip);
    let local_zip = cli.drop_dir.join(&filename);
    let remote_zip_posix = remote_zip.replace('\\', "/");
    let fetch_arg = format!(
        "src={} dest={}/ flat=yes",
        remote_zip_posix,
        cli.drop_dir.display()
    );
    let fetch_cmd = vec![
        cli.windows_group.as_str(),
        "-i",
        path_str(&cli.inventory)?,
        "-m",
        "fetch",
        "-a",
        fetch_arg.as_str(),
    ];
    print_run("RUN_FETCH", &cli.ansible_bin, &fetch_cmd);
    run_checked(&cli.ansible_bin, &fetch_cmd)?;
    if !local_zip.is_file() {
        bail!("fetched zip not found: {}", local_zip.display());
    }

    let host = export_json
        .get("hostname")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| guess_host_from_filename(&filename));
    let mut accept_args = vec![
        "accept".to_string(),
        "--package".to_string(),
        local_zip.display().to_string(),
    ];
    if !host.is_empty() {
        accept_args.extend(["--host".to_string(), host]);
    }
    print_run_owned("RUN_ACCEPT", &cli.wrapper, &accept_args);
    run_checked_owned(&cli.wrapper, &accept_args)?;

    let process_args = vec![
        "process-inbox".to_string(),
        "--mode".to_string(),
        cli.mode.clone(),
        "--limit".to_string(),
        "1".to_string(),
    ];
    print_run_owned("RUN_PROCESS", &cli.wrapper, &process_args);
    run_checked_owned(&cli.wrapper, &process_args)?;

    if let Some(case_id) = cli.case_id {
        ensure_file(&cli.linker, "linker")?;
        let link_args = vec![
            "--case-id".to_string(),
            case_id.to_string(),
            "--mode".to_string(),
            cli.mode,
            "--link-source".to_string(),
            cli.link_source,
        ];
        print_run_owned("RUN_LINK", &cli.linker, &link_args);
        run_checked_owned(&cli.linker, &link_args)?;
    }

    let latest_intake = Path::new("/opt/hayabusa/state/latest-intake.json");
    println!("LATEST_INTAKE");
    println!(
        "{}",
        fs::read_to_string(latest_intake).context("read latest intake")?
    );
    Ok(())
}

fn ensure_file(path: &Path, label: &str) -> Result<()> {
    if !path.is_file() {
        bail!("{label} not found: {}", path.display());
    }
    Ok(())
}

fn path_str(path: &Path) -> Result<&str> {
    path.to_str()
        .with_context(|| format!("path is not UTF-8: {}", path.display()))
}

fn run_capture(program: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("run {}", program.display()))?;
    if !output.status.success() {
        print!("{}", String::from_utf8_lossy(&output.stdout));
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        std::process::exit(output.status.code().unwrap_or(1));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn run_checked(program: &Path, args: &[&str]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .status()
        .with_context(|| format!("run {}", program.display()))?;
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

fn run_checked_owned(program: &Path, args: &[String]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .status()
        .with_context(|| format!("run {}", program.display()))?;
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

fn print_run(label: &str, program: &Path, args: &[&str]) {
    println!("{label} {} {}", program.display(), args.join(" "));
}

fn print_run_owned(label: &str, program: &Path, args: &[String]) {
    println!("{label} {} {}", program.display(), args.join(" "));
}

fn extract_json_blob(text: &str) -> Result<Value> {
    let re = Regex::new(r"(?s)(\{.*\}|\[.*\])").context("compile JSON extractor")?;
    let matches = re.find_iter(text).collect::<Vec<_>>();
    for candidate in matches.iter().rev().map(|item| item.as_str()) {
        if let Ok(value) = serde_json::from_str::<Value>(candidate) {
            return Ok(value);
        }
    }
    bail!("cannot parse JSON from ansible output:\n{text}")
}

fn normalize_latest_zip(value: Value) -> Result<Value> {
    if let Some(items) = value.as_array() {
        items.first().cloned().context("latest zip list is empty")
    } else {
        Ok(value)
    }
}
