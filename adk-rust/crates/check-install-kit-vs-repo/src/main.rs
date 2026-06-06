use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Parser;
use serde::Serialize;
use sha2::{Digest, Sha256};

const DEFAULT_KIT_DIR: &str = "install-kit-awindows-20260427-211240";
const MANIFEST_NAME: &str = "MANIFEST.txt";
const ALLOWED_KIT_ONLY_FILES: &[&str] =
    &["README-INSTALL-KIT.txt", "windows/aw-windows-telemetry.exe"];
const ALLOWED_KIT_ONLY_PREFIXES: &[&str] = &["server-configs-"];

#[derive(Debug, Parser)]
#[command(about = "Compare ActivityWatch-Russian install-kit contents against the repository")]
struct Cli {
    #[arg(long, default_value = ".")]
    root: PathBuf,

    #[arg(long, default_value = DEFAULT_KIT_DIR)]
    kit_dir: PathBuf,

    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Debug, Default, Serialize)]
struct Report {
    compared_files: usize,
    missing_in_repo: Vec<String>,
    mismatches: Vec<String>,
    powershell_mismatches: Vec<String>,
}

impl Report {
    fn is_ok(&self) -> bool {
        self.missing_in_repo.is_empty() && self.mismatches.is_empty()
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
    let cli = Cli::parse();
    let report = compare_install_kit(&cli.root, &cli.kit_dir)?;
    print_report(&report, cli.json)?;
    Ok(if report.is_ok() { 0 } else { 1 })
}

fn compare_install_kit(root: &Path, kit_dir_arg: &Path) -> Result<Report> {
    let root = root
        .canonicalize()
        .with_context(|| format!("canonicalize root {}", root.display()))?;
    let kit_dir = if kit_dir_arg.is_absolute() {
        kit_dir_arg.to_path_buf()
    } else {
        root.join(kit_dir_arg)
    };
    if !kit_dir.exists() {
        bail!("Install kit directory not found: {}", kit_dir.display());
    }
    if !kit_dir.is_dir() {
        bail!("Install kit path is not a directory: {}", kit_dir.display());
    }

    let mut report = Report::default();
    for kit_file in collect_files(&kit_dir)? {
        let rel = kit_file
            .strip_prefix(&kit_dir)
            .with_context(|| format!("strip kit prefix from {}", kit_file.display()))?;
        let rel_str = slash_path(rel);
        if rel.file_name().and_then(|name| name.to_str()) == Some(MANIFEST_NAME) {
            continue;
        }

        let repo_file = root.join(rel);
        if !repo_file.exists() {
            if is_allowed_kit_only(&rel_str) {
                continue;
            }
            report.missing_in_repo.push(rel_str);
            continue;
        }
        if !repo_file.is_file() {
            report.missing_in_repo.push(rel_str);
            continue;
        }

        report.compared_files += 1;
        if sha256_file(&kit_file)? != sha256_file(&repo_file)? {
            report.mismatches.push(rel_str);
        }
    }

    report.missing_in_repo.sort();
    report.mismatches.sort();
    report.powershell_mismatches = report
        .mismatches
        .iter()
        .filter(|path| is_powershell_path(path))
        .cloned()
        .collect();
    Ok(report)
}

fn print_report(report: &Report, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }

    println!("Compared files: {}", report.compared_files);
    println!("Missing in repo: {}", report.missing_in_repo.len());
    println!("Mismatched content: {}", report.mismatches.len());
    if !report.missing_in_repo.is_empty() {
        println!("--- Missing in repo ---");
        for path in &report.missing_in_repo {
            println!("{path}");
        }
    }
    if !report.mismatches.is_empty() {
        println!("--- Mismatches ---");
        for path in &report.mismatches {
            println!("{path}");
        }
    }
    println!(
        "PowerShell mismatches: {}",
        report.powershell_mismatches.len()
    );
    if !report.powershell_mismatches.is_empty() {
        println!("--- PowerShell mismatches ---");
        for path in &report.powershell_mismatches {
            println!("{path}");
        }
    }
    Ok(())
}

fn collect_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    collect_files_inner(root, &mut out)?;
    out.sort();
    Ok(out)
}

fn collect_files_inner(path: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(path).with_context(|| format!("read dir {}", path.display()))? {
        let entry = entry.with_context(|| format!("read dir entry {}", path.display()))?;
        let entry_path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("read file type {}", entry_path.display()))?;
        if file_type.is_dir() {
            collect_files_inner(&entry_path, out)?;
        } else if (file_type.is_file() || file_type.is_symlink()) && entry_path.is_file() {
            out.push(entry_path);
        }
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buf = [0_u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buf)
            .with_context(|| format!("read {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn is_allowed_kit_only(rel: &str) -> bool {
    ALLOWED_KIT_ONLY_FILES.contains(&rel)
        || ALLOWED_KIT_ONLY_PREFIXES
            .iter()
            .any(|prefix| rel.starts_with(prefix))
}

fn is_powershell_path(path: &str) -> bool {
    path.starts_with("windows/")
        && (path.ends_with(".ps1") || path.ends_with(".psm1") || path.ends_with(".psd1"))
}

fn slash_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[allow(dead_code)]
fn sorted_set(paths: &[String]) -> BTreeSet<String> {
    paths.iter().cloned().collect()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{compare_install_kit, sorted_set};

    #[test]
    fn reports_clean_tree() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        let kit = root.join("install-kit-awindows-20260427-211240");
        fs::create_dir_all(kit.join("windows")).unwrap();
        fs::write(root.join("common.txt"), "same").unwrap();
        fs::write(kit.join("common.txt"), "same").unwrap();
        fs::create_dir_all(root.join("windows")).unwrap();
        fs::write(root.join("windows/script.ps1"), "same").unwrap();
        fs::write(kit.join("windows/script.ps1"), "same").unwrap();
        fs::write(kit.join("README-INSTALL-KIT.txt"), "kit-only").unwrap();
        fs::write(kit.join("MANIFEST.txt"), "ignored").unwrap();

        let report = compare_install_kit(root, &kit).unwrap();
        assert!(report.is_ok());
        assert_eq!(report.compared_files, 2);
    }

    #[test]
    fn reports_mismatches_and_powershell_subset() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        let kit = root.join("install-kit-awindows-20260427-211240");
        fs::create_dir_all(root.join("windows")).unwrap();
        fs::create_dir_all(kit.join("windows")).unwrap();
        fs::write(root.join("windows/script.ps1"), "repo").unwrap();
        fs::write(kit.join("windows/script.ps1"), "kit").unwrap();
        fs::write(root.join("plain.txt"), "repo").unwrap();
        fs::write(kit.join("plain.txt"), "kit").unwrap();

        let report = compare_install_kit(root, &kit).unwrap();
        assert!(!report.is_ok());
        assert_eq!(
            sorted_set(&report.mismatches),
            sorted_set(&["plain.txt".to_string(), "windows/script.ps1".to_string()])
        );
        assert_eq!(
            report.powershell_mismatches,
            vec!["windows/script.ps1".to_string()]
        );
    }

    #[test]
    fn reports_unexpected_kit_only_files() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        let kit = root.join("install-kit-awindows-20260427-211240");
        fs::create_dir_all(&kit).unwrap();
        fs::write(kit.join("unexpected.txt"), "kit").unwrap();
        fs::create_dir_all(kit.join("server-configs-198.51.100.18")).unwrap();
        fs::write(
            kit.join("server-configs-198.51.100.18/config.deployment-config.json"),
            "{}",
        )
        .unwrap();

        let report = compare_install_kit(root, &kit).unwrap();
        assert_eq!(report.missing_in_repo, vec!["unexpected.txt"]);
    }
}
