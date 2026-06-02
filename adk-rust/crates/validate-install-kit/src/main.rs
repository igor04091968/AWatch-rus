use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Parser;
use flate2::read::GzDecoder;
use serde::Serialize;
use sha2::{Digest, Sha256};

const DEFAULT_KIT_DIR: &str = "install-kit-awindows-20260427-211240";
const DEFAULT_ZIP_ARCHIVE: &str = "install-kit-awindows-20260427-211240.zip";
const DEFAULT_TAR_ARCHIVE: &str = "install-kit-awindows-20260427-211240.tar.gz";
const MANIFEST_NAME: &str = "MANIFEST.txt";
const REQUIRED_RELATIVE_FILES: &[&str] = &[
    "MANIFEST.txt",
    "README-INSTALL-KIT.txt",
    "windows/deploy-ensemble.ps1",
    "windows/validate-deployment.ps1",
    "ansible/deploy_aw_windows.yml",
    "aw-server/install_aw_server.sh",
    "scripts/rebuild_install_kit.sh",
    "scripts/validate_install_kit.sh",
    "scripts/check_install_kit_vs_repo.sh",
    "scripts/quality-gate.sh",
];

#[derive(Debug, Parser)]
#[command(about = "Validate ActivityWatch-Russian Windows install-kit artifacts")]
struct Cli {
    #[arg(long, default_value = ".")]
    root: PathBuf,

    #[arg(long, default_value = DEFAULT_KIT_DIR)]
    kit_dir: PathBuf,

    #[arg(long, default_value = DEFAULT_ZIP_ARCHIVE)]
    zip_archive: PathBuf,

    #[arg(long, default_value = DEFAULT_TAR_ARCHIVE)]
    tar_archive: PathBuf,

    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Debug, Serialize)]
struct ValidationReport {
    ok: bool,
    required_files: StageReport,
    manifest_checksums: StageReport,
    manifest_completeness: CompletenessReport,
    archive_composition: ArchiveReport,
}

#[derive(Debug, Default, Serialize)]
struct StageReport {
    ok: bool,
    checked: usize,
    errors: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
struct CompletenessReport {
    ok: bool,
    tracked_files: usize,
    missing_listed: Vec<String>,
    unlisted_files: Vec<String>,
    errors: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
struct ArchiveReport {
    ok: bool,
    files: usize,
    errors: Vec<String>,
}

#[derive(Debug, Clone)]
struct ManifestEntry {
    digest: String,
    path: String,
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
    let cfg = Config::new(root, cli.kit_dir, cli.zip_archive, cli.tar_archive);
    let report = validate(&cfg);
    print_report(&report, cli.json)?;
    Ok(if report.ok { 0 } else { 1 })
}

#[derive(Debug)]
struct Config {
    root: PathBuf,
    kit_dir: PathBuf,
    zip_archive: PathBuf,
    tar_archive: PathBuf,
}

impl Config {
    fn new(root: PathBuf, kit_dir: PathBuf, zip_archive: PathBuf, tar_archive: PathBuf) -> Self {
        Self {
            kit_dir: resolve_path(&root, kit_dir),
            zip_archive: resolve_path(&root, zip_archive),
            tar_archive: resolve_path(&root, tar_archive),
            root,
        }
    }

    fn manifest(&self) -> PathBuf {
        self.kit_dir.join(MANIFEST_NAME)
    }
}

fn validate(cfg: &Config) -> ValidationReport {
    let required_files = check_required_files(cfg);
    let manifest_entries = read_manifest(&cfg.manifest());
    let manifest_checksums = match &manifest_entries {
        Ok(entries) => check_manifest_checksums(cfg, entries),
        Err(err) => StageReport {
            ok: false,
            checked: 0,
            errors: vec![err.to_string()],
        },
    };
    let manifest_completeness = match &manifest_entries {
        Ok(entries) => check_manifest_completeness(cfg, entries),
        Err(err) => CompletenessReport {
            ok: false,
            errors: vec![err.to_string()],
            ..CompletenessReport::default()
        },
    };
    let archive_composition = check_archive_composition(cfg);
    let ok = required_files.ok
        && manifest_checksums.ok
        && manifest_completeness.ok
        && archive_composition.ok;
    ValidationReport {
        ok,
        required_files,
        manifest_checksums,
        manifest_completeness,
        archive_composition,
    }
}

fn check_required_files(cfg: &Config) -> StageReport {
    let mut errors = Vec::new();
    for rel in REQUIRED_RELATIVE_FILES {
        let path = cfg.kit_dir.join(rel);
        if !path.is_file() {
            errors.push(format!("Missing required file: {}", path.display()));
        }
    }
    StageReport {
        ok: errors.is_empty(),
        checked: REQUIRED_RELATIVE_FILES.len(),
        errors,
    }
}

fn read_manifest(path: &Path) -> Result<Vec<ManifestEntry>> {
    let file = File::open(path).with_context(|| format!("open manifest {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();
    for (idx, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("read manifest line {}", idx + 1))?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((digest, path)) = line.split_once("  ") else {
            bail!("Invalid MANIFEST line {}: {}", idx + 1, line);
        };
        if digest.len() != 64 || !digest.chars().all(|ch| ch.is_ascii_hexdigit()) {
            bail!("Invalid MANIFEST digest on line {}: {}", idx + 1, digest);
        }
        entries.push(ManifestEntry {
            digest: digest.to_ascii_lowercase(),
            path: path.to_string(),
        });
    }
    Ok(entries)
}

fn check_manifest_checksums(cfg: &Config, entries: &[ManifestEntry]) -> StageReport {
    let mut errors = Vec::new();
    for entry in entries {
        let path = cfg.root.join(&entry.path);
        if !path.is_file() {
            errors.push(format!("Manifest file missing: {}", entry.path));
            continue;
        }
        match sha256_file(&path) {
            Ok(actual) if actual == entry.digest => {}
            Ok(actual) => errors.push(format!(
                "Checksum mismatch: {} expected {} got {}",
                entry.path, entry.digest, actual
            )),
            Err(err) => errors.push(err.to_string()),
        }
    }
    StageReport {
        ok: errors.is_empty(),
        checked: entries.len(),
        errors,
    }
}

fn check_manifest_completeness(cfg: &Config, entries: &[ManifestEntry]) -> CompletenessReport {
    let listed: BTreeSet<String> = entries.iter().map(|entry| entry.path.clone()).collect();
    let actual = match collect_kit_manifest_paths(&cfg.kit_dir) {
        Ok(paths) => paths,
        Err(err) => {
            return CompletenessReport {
                ok: false,
                errors: vec![err.to_string()],
                ..CompletenessReport::default()
            };
        }
    };
    let missing_listed: Vec<String> = listed.difference(&actual).cloned().collect();
    let unlisted_files: Vec<String> = actual.difference(&listed).cloned().collect();
    CompletenessReport {
        ok: missing_listed.is_empty() && unlisted_files.is_empty(),
        tracked_files: actual.len(),
        missing_listed,
        unlisted_files,
        errors: Vec::new(),
    }
}

fn collect_kit_manifest_paths(kit_dir: &Path) -> Result<BTreeSet<String>> {
    let mut files = Vec::new();
    collect_files(kit_dir, &mut files)?;
    let mut out = BTreeSet::new();
    for file in files {
        let rel = file
            .strip_prefix(kit_dir)
            .with_context(|| format!("strip kit prefix from {}", file.display()))?;
        if rel.file_name().and_then(|name| name.to_str()) == Some(MANIFEST_NAME) {
            continue;
        }
        out.insert(format!("{}/{}", DEFAULT_KIT_DIR, slash_path(rel)));
    }
    Ok(out)
}

fn check_archive_composition(cfg: &Config) -> ArchiveReport {
    let zip_files = match read_zip_files(&cfg.zip_archive) {
        Ok(files) => files,
        Err(err) => {
            return ArchiveReport {
                ok: false,
                errors: vec![err.to_string()],
                ..ArchiveReport::default()
            };
        }
    };
    let tar_files = match read_tar_files(&cfg.tar_archive) {
        Ok(files) => files,
        Err(err) => {
            return ArchiveReport {
                ok: false,
                errors: vec![err.to_string()],
                ..ArchiveReport::default()
            };
        }
    };

    let mut errors = Vec::new();
    if zip_files != tar_files {
        errors.push("ZIP and TAR contents differ".to_string());
    }
    let expected_prefix = format!("{DEFAULT_KIT_DIR}/");
    if !zip_files
        .iter()
        .all(|path| path.starts_with(&expected_prefix))
    {
        errors.push("Unexpected archive prefix layout".to_string());
    }
    ArchiveReport {
        ok: errors.is_empty(),
        files: zip_files.len(),
        errors,
    }
}

fn read_zip_files(path: &Path) -> Result<Vec<String>> {
    let file = File::open(path).with_context(|| format!("open zip {}", path.display()))?;
    let mut archive =
        zip::ZipArchive::new(file).with_context(|| format!("read zip {}", path.display()))?;
    let mut files = Vec::new();
    for idx in 0..archive.len() {
        let file = archive
            .by_index(idx)
            .with_context(|| format!("read zip entry {idx}"))?;
        let name = normalize_archive_path(file.name());
        if !name.ends_with('/') {
            files.push(name);
        }
    }
    files.sort();
    Ok(files)
}

fn read_tar_files(path: &Path) -> Result<Vec<String>> {
    let file = File::open(path).with_context(|| format!("open tar {}", path.display()))?;
    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    let mut files = Vec::new();
    for entry in archive
        .entries()
        .with_context(|| format!("read tar {}", path.display()))?
    {
        let entry = entry.with_context(|| format!("read tar entry {}", path.display()))?;
        if entry.header().entry_type().is_file() {
            let path = entry.path().context("read tar entry path")?;
            files.push(normalize_archive_path(&path.to_string_lossy()));
        }
    }
    files.sort();
    Ok(files)
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

fn print_report(report: &ValidationReport, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }

    println!("[1/4] Required files presence");
    for err in &report.required_files.errors {
        println!("{err}");
    }
    println!("[2/4] Manifest checksum verification");
    for err in &report.manifest_checksums.errors {
        println!("{err}");
    }
    println!("[3/4] Manifest completeness");
    if report.manifest_completeness.ok {
        println!(
            "MANIFEST complete: {} files tracked",
            report.manifest_completeness.tracked_files
        );
    } else {
        println!(
            "Missing files listed in MANIFEST: {:?}",
            report.manifest_completeness.missing_listed
        );
        println!(
            "Files not listed in MANIFEST: {:?}",
            report.manifest_completeness.unlisted_files
        );
        for err in &report.manifest_completeness.errors {
            println!("{err}");
        }
    }
    println!("[4/4] Archive composition check");
    if report.archive_composition.ok {
        println!("Archives match: {} files", report.archive_composition.files);
    } else {
        for err in &report.archive_composition.errors {
            println!("{err}");
        }
    }
    if report.ok {
        println!("validate_install_kit: OK");
    }
    Ok(())
}

fn resolve_path(root: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

fn normalize_archive_path(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_string()
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
    use std::io::Write;

    use flate2::Compression;
    use flate2::write::GzEncoder;
    use tempfile::tempdir;
    use zip::write::SimpleFileOptions;

    use super::{Config, DEFAULT_KIT_DIR, check_archive_composition, read_manifest, validate};

    #[test]
    fn manifest_parser_rejects_bad_line() {
        let tmp = tempdir().unwrap();
        let manifest = tmp.path().join("MANIFEST.txt");
        fs::write(&manifest, "not-a-manifest-line\n").unwrap();
        let err = read_manifest(&manifest).unwrap_err();
        assert!(err.to_string().contains("Invalid MANIFEST line"));
    }

    #[test]
    fn validates_clean_fixture() {
        let tmp = tempdir().unwrap();
        create_fixture(tmp.path(), true);
        let cfg = Config::new(
            tmp.path().to_path_buf(),
            DEFAULT_KIT_DIR.into(),
            format!("{DEFAULT_KIT_DIR}.zip").into(),
            format!("{DEFAULT_KIT_DIR}.tar.gz").into(),
        );
        let report = validate(&cfg);
        assert!(report.ok, "{report:#?}");
        assert_eq!(report.manifest_completeness.tracked_files, 9);
        assert_eq!(report.archive_composition.files, 10);
    }

    #[test]
    fn detects_manifest_extra_file() {
        let tmp = tempdir().unwrap();
        create_fixture(tmp.path(), true);
        fs::write(
            tmp.path().join(DEFAULT_KIT_DIR).join("unlisted.txt"),
            "extra",
        )
        .unwrap();
        let cfg = Config::new(
            tmp.path().to_path_buf(),
            DEFAULT_KIT_DIR.into(),
            format!("{DEFAULT_KIT_DIR}.zip").into(),
            format!("{DEFAULT_KIT_DIR}.tar.gz").into(),
        );
        let report = validate(&cfg);
        assert!(!report.ok);
        assert!(
            report
                .manifest_completeness
                .unlisted_files
                .contains(&format!("{DEFAULT_KIT_DIR}/unlisted.txt"))
        );
    }

    #[test]
    fn detects_archive_mismatch() {
        let tmp = tempdir().unwrap();
        create_fixture(tmp.path(), false);
        let cfg = Config::new(
            tmp.path().to_path_buf(),
            DEFAULT_KIT_DIR.into(),
            format!("{DEFAULT_KIT_DIR}.zip").into(),
            format!("{DEFAULT_KIT_DIR}.tar.gz").into(),
        );
        let report = check_archive_composition(&cfg);
        assert!(!report.ok);
        assert!(report.errors.iter().any(|err| err.contains("differ")));
    }

    fn create_fixture(root: &std::path::Path, matching_archives: bool) {
        let kit = root.join(DEFAULT_KIT_DIR);
        for rel in [
            "windows/deploy-ensemble.ps1",
            "windows/validate-deployment.ps1",
            "ansible/deploy_aw_windows.yml",
            "aw-server/install_aw_server.sh",
            "scripts/rebuild_install_kit.sh",
            "scripts/validate_install_kit.sh",
            "scripts/check_install_kit_vs_repo.sh",
            "scripts/quality-gate.sh",
        ] {
            let path = kit.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, rel).unwrap();
        }
        fs::write(kit.join("README-INSTALL-KIT.txt"), "readme").unwrap();
        let files = [
            "README-INSTALL-KIT.txt",
            "windows/deploy-ensemble.ps1",
            "windows/validate-deployment.ps1",
            "ansible/deploy_aw_windows.yml",
            "aw-server/install_aw_server.sh",
            "scripts/rebuild_install_kit.sh",
            "scripts/validate_install_kit.sh",
            "scripts/check_install_kit_vs_repo.sh",
            "scripts/quality-gate.sh",
        ];
        let mut manifest = String::new();
        for rel in files {
            let path = kit.join(rel);
            let digest = super::sha256_file(&path).unwrap();
            manifest.push_str(&format!("{digest}  {DEFAULT_KIT_DIR}/{rel}\n"));
        }
        fs::write(kit.join("MANIFEST.txt"), manifest).unwrap();
        write_zip(root, matching_archives);
        write_tar(root, matching_archives);
    }

    fn archive_files(matching: bool) -> Vec<(&'static str, &'static [u8])> {
        let mut files = vec![
            ("README-INSTALL-KIT.txt", b"readme".as_slice()),
            ("MANIFEST.txt", b"manifest".as_slice()),
            ("windows/deploy-ensemble.ps1", b"deploy".as_slice()),
            ("windows/validate-deployment.ps1", b"validate".as_slice()),
            ("ansible/deploy_aw_windows.yml", b"ansible".as_slice()),
            ("aw-server/install_aw_server.sh", b"server".as_slice()),
            ("scripts/rebuild_install_kit.sh", b"rebuild".as_slice()),
            (
                "scripts/validate_install_kit.sh",
                b"validate-kit".as_slice(),
            ),
            ("scripts/check_install_kit_vs_repo.sh", b"check".as_slice()),
            ("scripts/quality-gate.sh", b"quality".as_slice()),
        ];
        if !matching {
            files.pop();
            files.push(("scripts/other.sh", b"other".as_slice()));
        }
        files
    }

    fn write_zip(root: &std::path::Path, _matching: bool) {
        let path = root.join(format!("{DEFAULT_KIT_DIR}.zip"));
        let file = fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        for (rel, data) in archive_files(true) {
            zip.start_file(format!("{DEFAULT_KIT_DIR}/{rel}"), options)
                .unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
    }

    fn write_tar(root: &std::path::Path, matching: bool) {
        let path = root.join(format!("{DEFAULT_KIT_DIR}.tar.gz"));
        let file = fs::File::create(path).unwrap();
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = tar::Builder::new(encoder);
        for (rel, data) in archive_files(matching) {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, format!("{DEFAULT_KIT_DIR}/{rel}"), data)
                .unwrap();
        }
        builder.finish().unwrap();
    }
}
