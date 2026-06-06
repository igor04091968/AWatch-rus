use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use clap::Parser;
use serde::Serialize;

const DEFAULT_INSTALLER: &str = "windows/installkit/innosetup/AWatch-rus-InstallKit.exe";
const DEFAULT_WINEPREFIX: &str = "/tmp/aw-inno-verify-wineprefix";
const INSTALL_DIR_WIN: &str = r"C:\AWatchRusExtract";
const REQUIRED_FILES: &[&str] = &[
    "windows/AWatchRusCollectorGuardService.cs",
    "windows/aw-collector-guard.ps1",
    "windows/install-collector-guard-service.ps1",
    "windows/aw-windows-telemetry.exe",
    "windows/dlp-policy.native-cross-os.example.json",
];
const WINDOWS_TELEMETRY_EXE_SOURCE: &str =
    "adk-rust/target/x86_64-pc-windows-gnu/release/aw-windows-telemetry.exe";
const GUARD_MARKER: &str = "collector guard self-test OK";

#[derive(Debug, Parser)]
#[command(about = "Verify AWatch-rus InnoSetup installer payload through Wine")]
struct Cli {
    #[arg(default_value = DEFAULT_INSTALLER)]
    installer: PathBuf,

    #[arg(long, default_value = ".")]
    root: PathBuf,

    #[arg(long, env = "WINEPREFIX_VERIFY", default_value = DEFAULT_WINEPREFIX)]
    wineprefix: PathBuf,

    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Debug, Serialize)]
struct Report {
    ok: bool,
    installer: PathBuf,
    wineprefix: PathBuf,
    checked_files: Vec<String>,
    errors: Vec<String>,
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
    let root = cli
        .root
        .canonicalize()
        .with_context(|| format!("canonicalize root {}", cli.root.display()))?;
    let installer = resolve_path(&root, cli.installer);
    let report = verify(&root, &installer, &cli.wineprefix);
    print_report(&report, cli.json)?;
    Ok(if report.ok { 0 } else { 1 })
}

fn verify(root: &Path, installer: &Path, wineprefix: &Path) -> Report {
    let mut errors = Vec::new();
    if !installer.is_file() {
        errors.push(format!("Installer not found: {}", installer.display()));
        return report(false, installer, wineprefix, Vec::new(), errors);
    }
    if command_path("wine").is_none() {
        errors.push("wine not found".to_string());
        return report(false, installer, wineprefix, Vec::new(), errors);
    }
    if command_path("wineboot").is_none() {
        errors.push("wineboot not found".to_string());
        return report(false, installer, wineprefix, Vec::new(), errors);
    }
    if command_path("wineserver").is_none() {
        errors.push("wineserver not found".to_string());
        return report(false, installer, wineprefix, Vec::new(), errors);
    }

    if wineprefix.exists()
        && let Err(err) = fs::remove_dir_all(wineprefix)
    {
        errors.push(format!(
            "failed to remove WINEPREFIX {}: {err}",
            wineprefix.display()
        ));
        return report(false, installer, wineprefix, Vec::new(), errors);
    }
    if let Err(err) = fs::create_dir_all(wineprefix) {
        errors.push(format!(
            "failed to create WINEPREFIX {}: {err}",
            wineprefix.display()
        ));
        return report(false, installer, wineprefix, Vec::new(), errors);
    }

    if let Err(err) = run_quiet(
        Command::new("wineboot")
            .arg("-u")
            .env("WINEPREFIX", wineprefix)
            .env(
                "WINEDEBUG",
                env::var("WINEDEBUG").unwrap_or_else(|_| "-all".to_string()),
            ),
    ) {
        errors.push(err.to_string());
        return report(false, installer, wineprefix, Vec::new(), errors);
    }
    if let Err(err) = run_quiet(
        Command::new("wine")
            .arg(installer)
            .arg("/VERYSILENT")
            .arg("/SUPPRESSMSGBOXES")
            .arg("/NORESTART")
            .arg("/SP-")
            .arg("/TASKS=")
            .arg(format!(r#"/DIR={INSTALL_DIR_WIN}"#))
            .env("WINEPREFIX", wineprefix)
            .env(
                "WINEDEBUG",
                env::var("WINEDEBUG").unwrap_or_else(|_| "-all".to_string()),
            ),
    ) {
        errors.push(err.to_string());
        return report(false, installer, wineprefix, Vec::new(), errors);
    }
    if let Err(err) = run_quiet(
        Command::new("wineserver")
            .arg("-w")
            .env("WINEPREFIX", wineprefix),
    ) {
        errors.push(err.to_string());
        return report(false, installer, wineprefix, Vec::new(), errors);
    }

    let install_dir = wineprefix.join("drive_c/AWatchRusExtract");
    let mut checked = Vec::new();
    for rel in REQUIRED_FILES {
        let extracted = install_dir.join(rel);
        let repo = if *rel == "windows/aw-windows-telemetry.exe" {
            root.join(WINDOWS_TELEMETRY_EXE_SOURCE)
        } else {
            root.join(rel)
        };
        if !extracted.is_file() {
            errors.push(format!("Missing extracted file: {rel}"));
            continue;
        }
        match files_equal(&repo, &extracted) {
            Ok(true) => checked.push((*rel).to_string()),
            Ok(false) => errors.push(format!("Extracted file differs from repo: {rel}")),
            Err(err) => errors.push(err.to_string()),
        }
    }
    let guard = install_dir.join("windows/aw-collector-guard.ps1");
    match fs::read_to_string(&guard) {
        Ok(text) if text.contains(GUARD_MARKER) => {}
        Ok(_) => {
            errors.push("Guard self-test marker missing in extracted installer payload".to_string())
        }
        Err(err) => errors.push(format!(
            "read extracted guard script {}: {err}",
            guard.display()
        )),
    }

    report(errors.is_empty(), installer, wineprefix, checked, errors)
}

fn run_quiet(command: &mut Command) -> Result<()> {
    let program = command.get_program().to_string_lossy().to_string();
    let status = command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("run {program}"))?;
    if !status.success() {
        bail!("{program} failed with status {status}");
    }
    Ok(())
}

fn files_equal(left: &Path, right: &Path) -> Result<bool> {
    let left = fs::read(left).with_context(|| format!("read {}", left.display()))?;
    let right = fs::read(right).with_context(|| format!("read {}", right.display()))?;
    Ok(left == right)
}

fn print_report(report: &Report, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }
    for err in &report.errors {
        eprintln!("{err}");
    }
    if report.ok {
        println!("verify_innosetup_installer: OK");
    }
    Ok(())
}

fn report(
    ok: bool,
    installer: &Path,
    wineprefix: &Path,
    checked_files: Vec<String>,
    errors: Vec<String>,
) -> Report {
    Report {
        ok,
        installer: installer.to_path_buf(),
        wineprefix: wineprefix.to_path_buf(),
        checked_files,
        errors,
    }
}

fn command_path(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

fn resolve_path(root: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{files_equal, resolve_path};

    #[test]
    fn compares_files_by_bytes() {
        let tmp = tempdir().unwrap();
        let a = tmp.path().join("a");
        let b = tmp.path().join("b");
        fs::write(&a, b"same").unwrap();
        fs::write(&b, b"same").unwrap();
        assert!(files_equal(&a, &b).unwrap());
        fs::write(&b, b"different").unwrap();
        assert!(!files_equal(&a, &b).unwrap());
    }

    #[test]
    fn resolves_relative_path_against_root() {
        let root = std::path::Path::new("/tmp/root");
        assert_eq!(resolve_path(root, "a/b".into()), root.join("a/b"));
    }
}
