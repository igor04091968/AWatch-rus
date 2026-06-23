use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use clap::Parser;

#[derive(Debug, Parser)]
#[command(about = "Run ActivityWatch-Russian repository quality gates")]
struct Cli {
    #[arg(long, default_value = ".")]
    root: PathBuf,
}

fn main() {
    let code = match run() {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("{err:#}");
            1
        }
    };
    std::process::exit(code);
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let root = cli
        .root
        .canonicalize()
        .with_context(|| format!("canonicalize root {}", cli.root.display()))?;
    quality_gate(&root)
}

fn quality_gate(root: &Path) -> Result<()> {
    println!("[1/6] Bash syntax check");
    for file in collect_by_extension(root, &["aw-server", "proxmox", "scripts"], "sh")? {
        run_status(Command::new("bash").arg("-n").arg(&file), false)?;
    }

    println!("[2/6] Shellcheck (if available)");
    if command_exists("shellcheck") {
        let mut files = collect_by_extension(root, &["aw-server", "proxmox"], "sh")?;
        files.push(root.join("scripts/aw-webui-browser-smoke.sh"));
        let mut command = Command::new("shellcheck");
        command.arg("-e").arg("SC1007,SC1090,SC2016");
        for file in files.into_iter().filter(|path| path.is_file()) {
            command.arg(file);
        }
        run_status(&mut command, false)?;
    } else {
        println!("shellcheck not found, skipping.");
    }

    println!("[3/6] Node syntax check (if node available)");
    if command_exists("node") {
        run_status(
            Command::new("node")
                .arg("--check")
                .arg(root.join("scripts/aw-webui-browser-smoke.mjs"))
                .stdout(Stdio::null()),
            false,
        )?;
    } else {
        println!("node not found, skipping.");
    }

    println!("[4/6] PowerShell parse check (if pwsh available)");
    if command_exists("pwsh") {
        let ps = r#"
$ErrorActionPreference = "Stop"
Get-ChildItem windows -Filter *.ps1 | ForEach-Object {
  $tokens = $null
  $errors = $null
  [void][System.Management.Automation.Language.Parser]::ParseFile($_.FullName,[ref]$tokens,[ref]$errors)
  if ($errors.Count) { $errors | ForEach-Object { Write-Error $_.Message }; exit 1 }
}
foreach ($path in @("windows/ActivityWatch.Windows.Common.psm1", "windows/ActivityWatch.Windows.Common.psd1")) {
  $tokens = $null
  $errors = $null
  [void][System.Management.Automation.Language.Parser]::ParseFile((Resolve-Path $path),[ref]$tokens,[ref]$errors)
  if ($errors.Count) { $errors | ForEach-Object { Write-Error $_.Message }; exit 1 }
}
"#;
        run_status(
            Command::new("pwsh")
                .arg("-NoLogo")
                .arg("-NoProfile")
                .arg("-Command")
                .arg(ps)
                .current_dir(root),
            false,
        )?;
        run_status(
            Command::new("pwsh")
                .arg("-NoLogo")
                .arg("-NoProfile")
                .arg("-File")
                .arg("windows/aw-collector-guard.ps1")
                .arg("-SelfTest")
                .current_dir(root)
                .stdout(Stdio::null()),
            false,
        )?;
    } else {
        println!("pwsh not found, skipping.");
    }

    println!("[5/6] Ansible syntax check (if ansible-playbook available)");
    if command_exists("ansible-playbook") {
        for playbook in collect_top_level_yml(&root.join("ansible"))? {
            run_status(
                Command::new("ansible-playbook")
                    .arg("--syntax-check")
                    .arg(&playbook)
                    .arg("-i")
                    .arg(root.join("ansible/inventory.example.ini"))
                    .stdout(Stdio::null()),
                false,
            )?;
        }
    } else {
        println!("ansible-playbook not found, skipping.");
    }

    println!("[6/6] DetMir Python runtime retirement guard");
    python_runtime_guard(root)?;

    println!("quality-gate: OK");
    Ok(())
}

fn run_status(command: &mut Command, quiet: bool) -> Result<()> {
    let program = command.get_program().to_string_lossy().to_string();
    if quiet {
        command.stdout(Stdio::null()).stderr(Stdio::null());
    }
    let status = command.status().with_context(|| format!("run {program}"))?;
    if !status.success() {
        bail!("{program} failed with status {status}");
    }
    Ok(())
}

fn command_exists(name: &str) -> bool {
    let Some(path) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path).any(|dir| dir.join(name).is_file())
}

fn collect_by_extension(root: &Path, dirs: &[&str], extension: &str) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for dir in dirs {
        let path = root.join(dir);
        if path.is_dir() {
            collect_by_extension_inner(&path, extension, &mut out)?;
        }
    }
    out.sort();
    Ok(out)
}

fn collect_by_extension_inner(path: &Path, extension: &str, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(path).with_context(|| format!("read dir {}", path.display()))? {
        let entry = entry.with_context(|| format!("read dir entry {}", path.display()))?;
        let entry_path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("read file type {}", entry_path.display()))?;
        if file_type.is_dir() {
            collect_by_extension_inner(&entry_path, extension, out)?;
        } else if entry_path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext == extension)
        {
            out.push(entry_path);
        }
    }
    Ok(())
}

fn collect_top_level_yml(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    for entry in fs::read_dir(dir).with_context(|| format!("read dir {}", dir.display()))? {
        let entry = entry.with_context(|| format!("read dir entry {}", dir.display()))?;
        let path = entry.path();
        if path.is_file()
            && path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext == "yml")
        {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

fn python_runtime_guard(root: &Path) -> Result<()> {
    let mut violations = Vec::new();
    for rel in tracked_files(root)? {
        if is_allowed_python_runtime_path(&rel) {
            continue;
        }
        if rel.ends_with(".py") && is_detmir_retired_runtime_path(&rel) {
            violations.push(rel);
        }
    }

    if !violations.is_empty() {
        violations.sort();
        bail!(
            "Python runtime regression in Rust-retired DetMir paths:\n{}",
            violations.join("\n")
        );
    }
    Ok(())
}

fn tracked_files(root: &Path) -> Result<Vec<String>> {
    if command_exists("git") {
        let output = Command::new("git")
            .arg("ls-files")
            .current_dir(root)
            .output()
            .context("run git ls-files")?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(ToOwned::to_owned)
                .collect());
        }
    }

    let mut out = Vec::new();
    collect_tracked_fallback(root, root, &mut out)?;
    out.sort();
    Ok(out)
}

fn collect_tracked_fallback(root: &Path, path: &Path, out: &mut Vec<String>) -> Result<()> {
    for entry in fs::read_dir(path).with_context(|| format!("read dir {}", path.display()))? {
        let entry = entry.with_context(|| format!("read dir entry {}", path.display()))?;
        let entry_path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("read file type {}", entry_path.display()))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if file_type.is_dir() {
            if matches!(
                name.as_ref(),
                ".git" | "target" | "node_modules" | ".venv" | ".ops" | ".playwright-cli"
            ) {
                continue;
            }
            collect_tracked_fallback(root, &entry_path, out)?;
        } else if file_type.is_file() {
            let rel = entry_path
                .strip_prefix(root)
                .with_context(|| format!("strip root from {}", entry_path.display()))?
                .to_string_lossy()
                .replace('\\', "/");
            out.push(rel);
        }
    }
    Ok(())
}

fn is_allowed_python_runtime_path(rel: &str) -> bool {
    rel.starts_with("aw-server/dlp-content-analysis/")
        || rel.starts_with("clickhouse-1c/ai/")
        || rel.starts_with("clickhouse-1c/etl/")
        || rel == "detmir-mcp/main.py"
        || rel.starts_with("grafana-1c/")
        || rel.starts_with("pfsense/")
        || rel == "proxmox/tsj_guardian_bot.py"
        || rel == "proxmox/test_tsj_guardian_bot.py"
        || rel == "scripts/package_rust_release_binaries.py"
        || rel == "scripts/public_secret_pattern_check.py"
}

fn is_detmir_retired_runtime_path(rel: &str) -> bool {
    rel.starts_with("aw-server/")
        || rel.starts_with("proxmox/")
        || rel.starts_with("scripts/")
        || rel.starts_with("ansible/")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{
        collect_by_extension, collect_top_level_yml, is_allowed_python_runtime_path,
        is_detmir_retired_runtime_path,
    };

    #[test]
    fn collects_recursive_shell_files_sorted() {
        let tmp = tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("scripts/nested")).unwrap();
        fs::write(tmp.path().join("scripts/b.sh"), "").unwrap();
        fs::write(tmp.path().join("scripts/nested/a.sh"), "").unwrap();
        fs::write(tmp.path().join("scripts/skip.py"), "").unwrap();
        let files = collect_by_extension(tmp.path(), &["scripts"], "sh").unwrap();
        let names: Vec<_> = files
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["b.sh", "a.sh"]);
    }

    #[test]
    fn collects_top_level_yml_only() {
        let tmp = tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("ansible/nested")).unwrap();
        fs::write(tmp.path().join("ansible/a.yml"), "").unwrap();
        fs::write(tmp.path().join("ansible/nested/b.yml"), "").unwrap();
        let files = collect_top_level_yml(&tmp.path().join("ansible")).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_name().unwrap(), "a.yml");
    }

    #[test]
    fn allows_agreed_python_runtime_exceptions() {
        assert!(is_allowed_python_runtime_path(
            "aw-server/dlp-content-analysis/content_analyzer.py"
        ));
        assert!(is_allowed_python_runtime_path(
            "proxmox/tsj_guardian_bot.py"
        ));
        assert!(is_allowed_python_runtime_path("clickhouse-1c/etl/load.py"));
        assert!(is_allowed_python_runtime_path("detmir-mcp/main.py"));
        assert!(is_allowed_python_runtime_path(
            "pfsense/pfsense-aw-poller.py"
        ));
        assert!(is_allowed_python_runtime_path(
            "scripts/package_rust_release_binaries.py"
        ));
        assert!(is_allowed_python_runtime_path(
            "scripts/public_secret_pattern_check.py"
        ));
    }

    #[test]
    fn flags_retired_detmir_python_runtime_paths() {
        assert!(is_detmir_retired_runtime_path(
            "aw-server/dlp-policy-engine/server.py"
        ));
        assert!(is_detmir_retired_runtime_path("scripts/dlp-admin-cli.py"));
        assert!(!is_allowed_python_runtime_path(
            "aw-server/dlp-policy-engine/server.py"
        ));
        assert!(!is_allowed_python_runtime_path("scripts/dlp-admin-cli.py"));
    }
}
