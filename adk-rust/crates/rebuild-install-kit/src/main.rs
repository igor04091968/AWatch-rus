use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Parser;
use flate2::Compression;
use flate2::write::GzEncoder;
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;

const KIT_DIR: &str = "install-kit-awindows-20260427-211240";
const WINDOWS_TELEMETRY_EXE_SOURCE: &str =
    "adk-rust/target/x86_64-pc-windows-gnu/release/aw-windows-telemetry.exe";
const WINDOWS_TELEMETRY_EXE_DEST: &str = "windows/aw-windows-telemetry.exe";

const README: &str = r#"AWatch-rus Windows Install Kit

Includes:
- windows/* (deploy scripts, collectors, common module, configs/examples)
- windows/aw-windows-telemetry.exe (Rust Windows telemetry runtime)
- ansible/* (Windows and AW server playbooks, examples, inventory, tasks)
- aw-server/* (server installer, health orchestrator, RU patch loader, host groups, default settings)
- scripts/* (install-kit rebuild/validation, quality gates, browser/web smoke checks)

Source:
- Local project snapshot at build time.

Customer-specific deployment configs, inventories, passwords, tokens, domains,
IP addresses and runtime snapshots are intentionally excluded from this public
install-kit.
"#;

const ANSIBLE_FILES: &[&str] = &[
    "ansible/README.md",
    "ansible/deploy_aw_pfsense_poller.yml",
    "ansible/deploy_aw_server.yml",
    "ansible/deploy_aw_windows.yml",
    "ansible/group_vars/all.example.yml",
    "ansible/group_vars/pfsense-poller.example.yml",
    "ansible/group_vars/proxmox-matrix.example.yml",
    "ansible/group_vars/proxmox.example.yml",
    "ansible/group_vars/windows.example.yml",
    "ansible/install_full_stack.yml",
    "ansible/inventory.example.ini",
    "ansible/provision_proxmox_ct_and_deploy_aw.yml",
    "ansible/provision_proxmox_ct_matrix_and_deploy_aw.yml",
    "ansible/tasks/provision_ct_and_deploy_aw.yml",
];

const AW_SERVER_FILES: &[&str] = &[
    "aw-server/activitywatch-server.service",
    "aw-server/apply_webui_ru_patch.sh",
    "aw-server/aw-host-groups.json",
    "aw-server/aw-ru-patch.js",
    "aw-server/aw-rus-healthd.service",
    "aw-server/aw-rus-healthd.timer",
    "aw-server/aw-browser-smoke.service",
    "aw-server/aw-browser-smoke.timer",
    "aw-server/aw-slo-monitor.service",
    "aw-server/aw-slo-monitor.timer",
    "aw-server/aw-server.env.example",
    "aw-server/aw-sw-cleanup.js",
    "aw-server/aw-worktime-api.service",
    "aw-server/aw-worktime-autoheal.service",
    "aw-server/aw-worktime-autoheal.timer",
    "aw-server/aw-worktime-prewarm.service",
    "aw-server/aw-worktime-prewarm.timer",
    "aw-server/aw-worktime-panel.js",
    "aw-server/aw-worktime-ui-bridge.service",
    "aw-server/aw-worktime-ui-bridge.timer",
    "aw-server/install_aw_server.sh",
    "aw-server/settings/classes-worktime.json",
    "aw-server/settings/views-default.json",
];

const WINDOWS_FILES: &[&str] = &[
    "windows/ActivityWatch.Windows.Common.psd1",
    "windows/ActivityWatch.Windows.Common.psm1",
    "windows/browser-domains-native-collector.ps1",
    "windows/deploy-domain-users.ps1",
    "windows/deploy-ensemble.ps1",
    "windows/deploy-single-user.ps1",
    "windows/AWatchRusCollectorGuardService.cs",
    "windows/aw-collector-guard.ps1",
    "windows/install-collector-guard-service.ps1",
    "windows/dlp-endpoint-signals-collector.ps1",
    "windows/dlp-policy.example.json",
    "windows/dlp-policy.native-cross-os.example.json",
    "windows/email-outbound-collector.ps1",
    "windows/hardening-recovery.ps1",
    "windows/migrate-awatch-rus-paths.ps1",
    "windows/validate-deployment.ps1",
    "windows/web-category-rules.example.json",
    "windows/worktime-session-collector.ps1",
];

const SCRIPTS_FILES: &[&str] = &[
    "scripts/aw-webui-browser-smoke.mjs",
    "scripts/aw-webui-browser-smoke.sh",
    "scripts/check_install_kit_vs_repo.sh",
    "scripts/quality-gate.sh",
    "scripts/rebuild_install_kit.sh",
    "scripts/validate_install_kit.sh",
    "scripts/verify_innosetup_installer.sh",
];

#[derive(Debug, Parser)]
#[command(about = "Rebuild ActivityWatch-Russian Windows install-kit directory and archives")]
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
    rebuild(&root)
}

fn rebuild(root: &Path) -> Result<()> {
    let kit = root.join(KIT_DIR);
    for name in ["ansible", "aw-server", "windows", "scripts"] {
        remove_if_exists(&kit.join(name))?;
    }
    remove_server_config_dirs(&kit)?;

    for rel in ANSIBLE_FILES
        .iter()
        .chain(AW_SERVER_FILES)
        .chain(WINDOWS_FILES)
        .chain(SCRIPTS_FILES)
    {
        copy_file(root, &kit, rel)?;
    }
    copy_file_as(
        root,
        &kit,
        WINDOWS_TELEMETRY_EXE_SOURCE,
        WINDOWS_TELEMETRY_EXE_DEST,
    )?;

    write_file_replace(&kit.join("README-INSTALL-KIT.txt"), README.as_bytes())
        .with_context(|| format!("write {}", kit.join("README-INSTALL-KIT.txt").display()))?;
    write_manifest(root, &kit)?;
    write_zip(root, &kit)?;
    write_tar(root, &kit)?;
    Ok(())
}

fn remove_server_config_dirs(kit: &Path) -> Result<()> {
    if !kit.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(kit).with_context(|| format!("read dir {}", kit.display()))? {
        let entry = entry.with_context(|| format!("read dir entry {}", kit.display()))?;
        let path = entry.path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("server-configs-"))
        {
            remove_if_exists(&path)?;
        }
    }
    Ok(())
}

fn copy_file(root: &Path, kit: &Path, rel: &str) -> Result<()> {
    copy_file_as(root, kit, rel, rel)
}

fn copy_file_as(root: &Path, kit: &Path, src_rel: &str, dest_rel: &str) -> Result<()> {
    let src = root.join(src_rel);
    let dest = kit.join(dest_rel);
    fs::create_dir_all(dest.parent().context("destination parent")?)
        .with_context(|| format!("create parent for {}", dest.display()))?;
    let bytes = fs::read(&src).with_context(|| format!("read {}", src.display()))?;
    write_file_replace(&dest, &bytes)
        .with_context(|| format!("copy {} to {}", src.display(), dest.display()))?;
    Ok(())
}

fn write_file_replace(path: &Path, bytes: &[u8]) -> Result<()> {
    remove_if_exists(path)?;
    fs::write(path, bytes).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn remove_if_exists(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let meta = fs::symlink_metadata(path).with_context(|| format!("stat {}", path.display()))?;
    if meta.is_dir() {
        fs::remove_dir_all(path).with_context(|| format!("remove dir {}", path.display()))?;
    } else {
        fs::remove_file(path).with_context(|| format!("remove file {}", path.display()))?;
    }
    Ok(())
}

fn write_manifest(root: &Path, kit: &Path) -> Result<()> {
    let mut files = Vec::new();
    collect_files(kit, &mut files)?;
    files.retain(|path| path.file_name().and_then(|name| name.to_str()) != Some("MANIFEST.txt"));
    files.sort();
    let manifest = kit.join("MANIFEST.txt");
    let mut out = String::new();
    for path in files {
        let digest = sha256_file(&path)?;
        let rel = path
            .strip_prefix(root)
            .with_context(|| format!("strip root prefix from {}", path.display()))?;
        out.push_str(&format!("{digest}  {}\n", slash_path(rel)));
    }
    write_file_replace(&manifest, out.as_bytes())?;
    Ok(())
}

fn write_zip(root: &Path, kit: &Path) -> Result<()> {
    let archive_path = root.join(format!("{KIT_DIR}.zip"));
    remove_if_exists(&archive_path)?;
    let file = File::create(&archive_path)
        .with_context(|| format!("create {}", archive_path.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut files = Vec::new();
    collect_files(kit, &mut files)?;
    files.sort();
    for path in files {
        let rel = path
            .strip_prefix(root)
            .with_context(|| format!("strip root prefix from {}", path.display()))?;
        let name = slash_path(rel);
        zip.start_file(name, options).context("start zip file")?;
        let mut input = File::open(&path).with_context(|| format!("open {}", path.display()))?;
        std::io::copy(&mut input, &mut zip)
            .with_context(|| format!("write zip {}", path.display()))?;
    }
    zip.finish().context("finish zip")?;
    Ok(())
}

fn write_tar(root: &Path, kit: &Path) -> Result<()> {
    let archive_path = root.join(format!("{KIT_DIR}.tar.gz"));
    remove_if_exists(&archive_path)?;
    let file = File::create(&archive_path)
        .with_context(|| format!("create {}", archive_path.display()))?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut builder = tar::Builder::new(encoder);
    let mut files = Vec::new();
    collect_files(kit, &mut files)?;
    files.sort();
    for path in files {
        builder
            .append_path_with_name(
                &path,
                slash_path(
                    path.strip_prefix(root)
                        .with_context(|| format!("strip root prefix from {}", path.display()))?,
                ),
            )
            .with_context(|| format!("append tar {}", path.display()))?;
    }
    builder.finish().context("finish tar")?;
    Ok(())
}

fn collect_files(path: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(path).with_context(|| format!("read dir {}", path.display()))? {
        let entry = entry.with_context(|| format!("read dir entry {}", path.display()))?;
        let entry_path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("read file type {}", entry_path.display()))?;
        if file_type.is_dir() {
            collect_files(&entry_path, out)?;
        } else if (file_type.is_file() || file_type.is_symlink()) && entry_path.is_file() {
            out.push(entry_path);
        }
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buf)
            .with_context(|| format!("read {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn slash_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::remove_server_config_dirs;

    #[test]
    fn removes_server_config_dirs_only() {
        let tmp = tempdir().unwrap();
        let kit = tmp.path();
        fs::create_dir_all(kit.join("server-configs-x")).unwrap();
        fs::create_dir_all(kit.join("windows")).unwrap();
        remove_server_config_dirs(kit).unwrap();
        assert!(!kit.join("server-configs-x").exists());
        assert!(kit.join("windows").exists());
    }
}
