use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    env,
    ffi::OsStr,
    fs::{self, File},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result, anyhow, bail};
use base64::{Engine as _, engine::general_purpose};
use chrono::{DateTime, Timelike, Utc};
use clap::{Parser, Subcommand};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher, event::RenameMode};
use regex::Regex;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use url::Url;

const DEFAULT_FILE1C_LOG: &str = r"C:\ProgramData\AWatch-rus\logs\file1c-telemetry.log";
const DEFAULT_FILE1C_STATE: &str = r"C:\ProgramData\AWatch-rus\file1c-telemetry-state.json";
const DEFAULT_DLP_LOG: &str = r"C:\ProgramData\AWatch-rus\logs\dlp-evidence-sync.log";
const DEFAULT_DLP_STATE: &str = r"C:\ProgramData\AWatch-rus\dlp-evidence-sync-state.json";
const DEFAULT_DLP_TOKEN: &str = r"C:\ProgramData\AWatch-rus\dlp-evidence-upload-token.txt";
const DEFAULT_REMOTE_ROOT: &str = "/opt/activitywatch/clickhouse-1c/landing";
const DEFAULT_SSH_KEY: &str = r"C:\ProgramData\AWatch-rus\ssh\awops_ed25519";

#[derive(Parser)]
#[command(about = "AWatch-rus Windows telemetry uploader without PowerShell runtime wrappers")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    File1cUpload(File1cUpload),
    DlpEvidenceSync(DlpEvidenceSync),
    ValidateDeployment(ValidateDeployment),
    CollectorGuard(CollectorGuard),
    FileOperationsCollector(FileOperationsCollector),
    BrowserDomainsCollector(BrowserDomainsCollector),
    DlpEndpointCollector(DlpEndpointCollector),
}

#[derive(Parser, Clone)]
struct File1cUpload {
    #[arg(
        long,
        default_value = r"C:\ProgramData\AWatch-rus\deployment-config.json"
    )]
    config_path: PathBuf,
    #[arg(long, default_value = "")]
    analytics_host: String,
    #[arg(long, default_value = "igor")]
    analytics_user: String,
    #[arg(long, default_value = DEFAULT_REMOTE_ROOT)]
    remote_root: String,
    #[arg(long, default_value = DEFAULT_SSH_KEY)]
    remote_key_path: PathBuf,
    #[arg(long)]
    registry_workbook_path: Option<PathBuf>,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Parser, Clone)]
struct DlpEvidenceSync {
    #[arg(
        long,
        default_value = r"C:\ProgramData\AWatch-rus\deployment-config.json"
    )]
    config_path: PathBuf,
    #[arg(
        long,
        default_value = "http://aw-server.example.local:8721/api/dlp/evidence/upload"
    )]
    evidence_api_url: String,
    #[arg(long, default_value = DEFAULT_DLP_TOKEN)]
    token_path: PathBuf,
    #[arg(long, default_value = DEFAULT_DLP_STATE)]
    state_path: PathBuf,
    #[arg(long, default_value = DEFAULT_DLP_LOG)]
    log_path: PathBuf,
    #[arg(long, default_value_t = 200)]
    max_files: usize,
    #[arg(long, default_value_t = 8_388_608)]
    max_bytes: u64,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Parser, Clone)]
struct ValidateDeployment {
    #[arg(
        long,
        default_value = r"C:\ProgramData\AWatch-rus\deployment-config.json"
    )]
    config_path: PathBuf,
    #[arg(long, default_value_t = 15)]
    timeout_seconds: u64,
    #[arg(long, default_value_t = 300)]
    worktime_max_age_seconds: i64,
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    fail_on_error: bool,
}

#[derive(Parser, Clone)]
struct CollectorGuard {
    #[arg(
        long,
        default_value = r"C:\ProgramData\AWatch-rus\deployment-config.json"
    )]
    config_path: PathBuf,
    #[arg(long, default_value = "shadow", value_parser = ["shadow", "enforce"])]
    mode: String,
    #[arg(long, default_value_t = 60)]
    loop_seconds: u64,
    #[arg(long)]
    once: bool,
    #[arg(long, default_value_t = 900)]
    interactive_max_age_seconds: i64,
    #[arg(long, default_value_t = 600)]
    restart_window_seconds: i64,
    #[arg(long, default_value_t = 3)]
    max_restarts: usize,
    #[arg(long, default_value_t = 300)]
    action_cooldown_seconds: i64,
    #[arg(long, default_value_t = 60)]
    interactive_action_cooldown_seconds: i64,
    #[arg(long)]
    self_test: bool,
}

#[derive(Parser, Clone)]
struct FileOperationsCollector {
    #[arg(
        long,
        default_value = r"C:\ProgramData\AWatch-rus\deployment-config.json"
    )]
    config_path: PathBuf,
    #[arg(long, default_value = "shadow", value_parser = ["shadow", "enforce"])]
    mode: String,
    #[arg(long)]
    server_host: Option<String>,
    #[arg(long)]
    server_port: Option<i64>,
    #[arg(long)]
    server_scheme: Option<String>,
    #[arg(long)]
    log_path: Option<PathBuf>,
    #[arg(long, default_value_t = 10)]
    poll_seconds: u64,
    #[arg(long, value_delimiter = ',')]
    watch_paths: Vec<String>,
    #[arg(long)]
    once: bool,
    #[arg(long)]
    duration_seconds: Option<u64>,
    #[arg(long)]
    self_test: bool,
}

#[derive(Parser, Clone)]
struct BrowserDomainsCollector {
    #[arg(
        long,
        default_value = r"C:\ProgramData\AWatch-rus\deployment-config.json"
    )]
    config_path: PathBuf,
    #[arg(long, default_value = "shadow", value_parser = ["shadow", "enforce"])]
    mode: String,
    #[arg(long, default_value_t = 5)]
    poll_seconds: u64,
    #[arg(long, default_value_t = 30)]
    pulse_seconds: u64,
    #[arg(long)]
    once: bool,
    #[arg(long)]
    duration_seconds: Option<u64>,
    #[arg(long)]
    self_test: bool,
}

#[derive(Parser, Clone)]
struct DlpEndpointCollector {
    #[arg(
        long,
        default_value = r"C:\ProgramData\AWatch-rus\deployment-config.json"
    )]
    config_path: PathBuf,
    #[arg(long, default_value = "shadow", value_parser = ["shadow", "enforce"])]
    mode: String,
    #[arg(long, default_value_t = 10)]
    poll_seconds: u64,
    #[arg(long, default_value_t = 30)]
    pulse_seconds: u64,
    #[arg(long)]
    once: bool,
    #[arg(long)]
    duration_seconds: Option<u64>,
    #[arg(long)]
    self_test: bool,
}

#[derive(Debug, Clone)]
struct Infobase {
    user_name: String,
    infobase: String,
    base_id: Option<String>,
    path: PathBuf,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
struct BaseState {
    #[serde(default, rename = "dbSizeBytes")]
    db_size_bytes: u64,
    #[serde(default, rename = "mainLogBytes")]
    main_log_bytes: u64,
    #[serde(default, rename = "schedulerWriteUtc")]
    scheduler_write_utc: String,
}

struct ScpUploadContext<'a> {
    scp: &'a Path,
    key: &'a tempfile::NamedTempFile,
    user: &'a str,
    host: &'a str,
    remote_root: &'a str,
    log_path: &'a Path,
}

#[derive(Debug, Clone)]
struct GuardTaskDefinition {
    task_name: String,
    user_id: String,
}

#[derive(Debug)]
struct ActionAllowed {
    allowed: bool,
    reason: String,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct GuardRuntime {
    #[serde(default, rename = "restartHistory")]
    restart_history: BTreeMap<String, Vec<i64>>,
    #[serde(default, rename = "lastAction")]
    last_action: BTreeMap<String, i64>,
    #[serde(default)]
    quarantine: BTreeMap<String, Value>,
}

impl GuardRuntime {
    fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let value = read_json_file(path)?;
        serde_json::from_value(value).with_context(|| format!("parse {}", path.display()))
    }

    fn action_allowed(
        &mut self,
        key: &str,
        cooldown_seconds: i64,
        window_seconds: i64,
        max_count: usize,
    ) -> ActionAllowed {
        let now = Utc::now().timestamp();
        if let Some(last) = self.last_action.get(key) {
            if now.saturating_sub(*last) < cooldown_seconds {
                return ActionAllowed {
                    allowed: false,
                    reason: "cooldown".to_string(),
                };
            }
        }

        let history = self.restart_history.entry(key.to_string()).or_default();
        history.retain(|item| now.saturating_sub(*item) <= window_seconds);
        if history.len() >= max_count {
            self.quarantine.insert(
                key.to_string(),
                json!({
                    "since": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                    "reason": "restart-budget-exhausted",
                    "count": history.len()
                }),
            );
            return ActionAllowed {
                allowed: false,
                reason: "quarantine".to_string(),
            };
        }

        self.quarantine.remove(key);
        ActionAllowed {
            allowed: true,
            reason: "ok".to_string(),
        }
    }

    fn register_action(&mut self, key: &str) {
        let now = Utc::now().timestamp();
        self.restart_history
            .entry(key.to_string())
            .or_default()
            .push(now);
        self.last_action.insert(key.to_string(), now);
    }

    fn reset_action_budget(&mut self, key: &str) {
        self.restart_history.remove(key);
        self.last_action.remove(key);
        self.quarantine.remove(key);
    }
}

struct GuardLock {
    path: PathBuf,
}

impl GuardLock {
    fn acquire(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        if path.exists() {
            let pid = read_json_file(path)
                .ok()
                .and_then(|value| json_i64(&value, &["pid"]))
                .and_then(|value| u32::try_from(value).ok());
            if pid.is_some_and(process_id_is_running) {
                bail!("another collector guard instance is already running");
            }
            let _ = fs::remove_file(path);
        }
        save_json_file(
            path,
            &json!({
                "pid": std::process::id(),
                "createdAt": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
            }),
        )?;
        Ok(Self {
            path: path.to_path_buf(),
        })
    }
}

impl Drop for GuardLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[derive(Debug, Clone)]
struct FileOpsRuntime {
    api_base: String,
    hostname: String,
    username: String,
    session_id: u32,
    state_root: PathBuf,
    state_path: PathBuf,
    log_path: PathBuf,
    queue_path: PathBuf,
    bucket_id: String,
    mode: String,
    local_logs_enabled: bool,
    metrics: FileOpsMetrics,
}

#[derive(Debug, Default, Clone, Serialize)]
struct FileOpsMetrics {
    #[serde(rename = "eventsEnqueued")]
    events_enqueued: u64,
    #[serde(rename = "eventsFlushed")]
    events_flushed: u64,
    #[serde(rename = "sendFailures")]
    send_failures: u64,
    #[serde(rename = "queueDepth")]
    queue_depth: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct FileOpsQueueItem {
    ts: String,
    uri: String,
    payload: String,
    kind: String,
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Commands::File1cUpload(args) => run_file1c_upload(args),
        Commands::DlpEvidenceSync(args) => run_dlp_evidence_sync(args),
        Commands::ValidateDeployment(args) => run_validate_deployment(args),
        Commands::CollectorGuard(args) => run_collector_guard(args),
        Commands::FileOperationsCollector(args) => run_file_operations_collector(args),
        Commands::BrowserDomainsCollector(args) => run_browser_domains_collector(args),
        Commands::DlpEndpointCollector(args) => run_dlp_endpoint_collector(args),
    }
}

fn run_file1c_upload(mut args: File1cUpload) -> Result<()> {
    let log_path = PathBuf::from(DEFAULT_FILE1C_LOG);
    append_log(&log_path, "file1c exporter start")?;
    let result = run_file1c_upload_inner(&mut args, &log_path);
    match &result {
        Ok(_) => {
            append_log(&log_path, "file1c exporter done")?;
        }
        Err(err) => {
            let _ = append_log(&log_path, &format!("ERROR: {err:#}"));
        }
    }
    result
}

fn run_file1c_upload_inner(args: &mut File1cUpload, log_path: &Path) -> Result<()> {
    let config = read_json_file(&args.config_path).unwrap_or(Value::Null);
    let automation = json_at(&config, &["analytics", "file1cAutomation"]).unwrap_or(&Value::Null);

    if let Some(path) = json_string(automation, &["remoteKeyPath"]).filter(|v| !v.trim().is_empty())
    {
        args.remote_key_path = PathBuf::from(path);
    }
    if args.analytics_host.trim().is_empty() {
        args.analytics_host = json_string(automation, &["targetHost"]).unwrap_or_default();
    }
    if args.analytics_host.trim().is_empty() {
        if let Some(host) = last_successful_analytics_host(log_path)? {
            append_log(
                log_path,
                &format!("recovered analyticsHost={host} from previous successful uploader log"),
            )?;
            args.analytics_host = host;
        }
    }
    if args.analytics_host.trim().is_empty() {
        bail!(
            "AnalyticsHost is empty, deployment-config has no analytics.file1cAutomation.targetHost, and no previous successful uploader log was found"
        );
    }
    if let Some(user) = json_string(automation, &["targetUser"]).filter(|v| !v.trim().is_empty()) {
        args.analytics_user = user;
    }
    if let Some(root) = json_string(automation, &["remoteRoot"]).filter(|v| !v.trim().is_empty()) {
        args.remote_root = root;
    }
    if args.registry_workbook_path.is_none() {
        args.registry_workbook_path = json_string(automation, &["registryWorkbookPath"])
            .filter(|v| !v.trim().is_empty())
            .map(PathBuf::from);
    }

    let scp = system32_path("OpenSSH\\scp.exe");
    if !args.dry_run && !scp.exists() {
        bail!("scp client not found: {}", scp.display());
    }

    let infobases = discover_1c_infobases(log_path)?;
    let state_path = PathBuf::from(DEFAULT_FILE1C_STATE);
    let exporter_state = read_exporter_state(&state_path).unwrap_or_default();
    let now = utc_compact();
    let now_rfc3339 = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let stamp = Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let host = env::var("COMPUTERNAME").unwrap_or_else(|_| "unknown".to_string());

    let mut next_state: BTreeMap<String, BaseState> = BTreeMap::new();
    let mut documents = Vec::new();
    let mut companies = Vec::new();
    let mut reglog = Vec::new();
    let mut audit = Vec::new();

    for base in &infobases {
        let base_path = &base.path;
        let db_file = base_path.join("1Cv8.1CD");
        let db_size = file_len(&db_file).unwrap_or(0);
        let log_dir = base_path.join("1Cv8Log");
        let main_log = latest_lgp(&log_dir);
        let main_log_size = main_log
            .as_ref()
            .and_then(|p| file_len(p).ok())
            .unwrap_or(0);
        let locks = count_matching_files(base_path, |name| {
            name.starts_with("1Cv8") && name.contains(".1CL")
        });
        let temp_db = base_path.join("1Cv8tmp.1CD").exists();
        let scheduler_dir = base_path.join("1Cv8JobScheduler");
        let scheduler_write_utc = modified_utc(&scheduler_dir).ok();
        let owner = if base.user_name.trim().is_empty() {
            "unknown"
        } else {
            &base.user_name
        };
        let organization = base_path
            .parent()
            .and_then(Path::file_name)
            .and_then(OsStr::to_str)
            .unwrap_or("")
            .to_string();
        let status = if locks > 0 || temp_db {
            "busy"
        } else {
            "online"
        };
        let doc_id = base
            .base_id
            .clone()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| stable_doc_id(&base_path.to_string_lossy()));
        let previous = exporter_state.get(&doc_id);
        let is_bootstrap = previous.is_none();
        let db_delta_mb = if let Some(prev) = previous {
            round2((db_size as f64 - prev.db_size_bytes as f64) / 1_048_576.0)
        } else {
            0.0
        };
        let reglog_delta_mb = if let Some(prev) = previous {
            round2((main_log_size as f64 - prev.main_log_bytes as f64) / 1_048_576.0)
        } else {
            0.0
        };
        let scheduler_touched = scheduler_touched(scheduler_write_utc, previous);
        let activity_score = company_activity_score(
            db_delta_mb,
            reglog_delta_mb,
            locks,
            temp_db,
            scheduler_touched,
            status,
            is_bootstrap,
        );

        documents.push(json!({
            "ts": now,
            "infobase": base.infobase,
            "organization": organization,
            "department": "FileBase",
            "doc_type": "InfobaseSnapshot",
            "doc_id": doc_id,
            "doc_number": "",
            "author": owner,
            "counterparty": "",
            "operation_type": "inventory",
            "amount": 0,
            "status": status,
            "posted": 1
        }));

        companies.push(json!({
            "ts": now,
            "infobase": base.infobase,
            "company_name": base.infobase,
            "organization": organization,
            "owner_user": owner,
            "base_id": doc_id,
            "base_path": base_path.to_string_lossy(),
            "status": status,
            "db_size_bytes": db_size,
            "reglog_size_bytes": main_log_size,
            "active_locks": locks,
            "temp_db_present": if temp_db { 1 } else { 0 },
            "scheduler_touched": if scheduler_touched { 1 } else { 0 },
            "activity_score": activity_score
        }));

        if activity_score > 0.0 {
            documents.push(json!({
                "ts": now,
                "infobase": base.infobase,
                "organization": organization,
                "department": "FileBaseActivity",
                "doc_type": "CompanyActivitySnapshot",
                "doc_id": format!("{doc_id}-{stamp}"),
                "doc_number": stamp,
                "author": owner,
                "counterparty": base.infobase,
                "operation_type": "activity_snapshot",
                "amount": activity_score,
                "status": status,
                "posted": 1
            }));
        }

        audit.push(json!({
            "ts": now,
            "infobase": base.infobase,
            "user": owner,
            "object_type": "infobase",
            "object_id": doc_id,
            "action": "inventory_snapshot",
            "before_hash": "",
            "after_hash": "",
            "risk_tag": if status == "busy" { "busy" } else { "" }
        }));

        if let Some(main_log_path) = main_log {
            let main_log_ts = modified_utc(&main_log_path)
                .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
                .unwrap_or_else(|_| now.clone());
            let main_log_name = main_log_path
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or("");
            reglog.push(json!({
                "ts": main_log_ts,
                "infobase": base.infobase,
                "user": owner,
                "host": host,
                "app": "1cv8-file",
                "event_name": "RegLogInventory",
                "level": if main_log_size > 536_870_912 { "warn" } else { "info" },
                "duration_ms": 0,
                "message": format!("Registration log file {main_log_name} size={}MB path={}", round2(main_log_size as f64 / 1_048_576.0), main_log_path.display())
            }));
        }
        if locks > 0 || temp_db {
            reglog.push(json!({
                "ts": now,
                "infobase": base.infobase,
                "user": owner,
                "host": host,
                "app": "1cv8-file",
                "event_name": "FileBaseBusy",
                "level": "warn",
                "duration_ms": 0,
                "message": format!("Detected active file-base markers: locks={locks} tempDb={temp_db}")
            }));
        }
        if let Some(scheduler_ts) = scheduler_write_utc {
            reglog.push(json!({
                "ts": scheduler_ts.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                "infobase": base.infobase,
                "user": owner,
                "host": host,
                "app": "1cv8-file",
                "event_name": "JobSchedulerActivity",
                "level": "info",
                "duration_ms": 0,
                "message": format!("1Cv8JobScheduler touched at {}", scheduler_ts.to_rfc3339())
            }));
        }
        reglog.push(json!({
            "ts": now,
            "infobase": base.infobase,
            "user": owner,
            "host": host,
            "app": "1cv8-file",
            "event_name": "CompanyActivitySnapshot",
            "level": if activity_score > 20.0 { "warn" } else { "info" },
            "duration_ms": 0,
            "message": format!("activityScore={activity_score} dbDeltaMb={db_delta_mb} reglogDeltaMb={reglog_delta_mb} locks={locks} tempDb={temp_db} schedulerTouched={scheduler_touched}")
        }));

        next_state.insert(
            doc_id,
            BaseState {
                db_size_bytes: db_size,
                main_log_bytes: main_log_size,
                scheduler_write_utc: scheduler_write_utc
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_default(),
            },
        );
    }

    let host_rows = vec![host_sample(&now, &host)];
    let outbox = tempfile::Builder::new()
        .prefix(&format!("aw-rus-1c-outbox-{stamp}-"))
        .tempdir()
        .context("create file1c outbox")?;

    let dataset_files = [
        ("documents", documents.as_slice()),
        ("companies", companies.as_slice()),
        ("reglog", reglog.as_slice()),
        ("audit", audit.as_slice()),
        ("host", host_rows.as_slice()),
    ];
    let mut files = BTreeMap::new();
    for (dataset, rows) in dataset_files {
        let path = outbox.path().join(format!("{dataset}-{stamp}.jsonl"));
        write_json_lines(&path, rows)?;
        files.insert(dataset.to_string(), path);
    }
    let registry_path = outbox.path().join(format!("company-registry-{stamp}.xlsx"));
    let registry_workbook_path = args
        .registry_workbook_path
        .as_ref()
        .ok_or_else(|| anyhow!("registry workbook path was not resolved"))?;
    let registry_uploaded = if registry_workbook_path.exists() {
        fs::copy(registry_workbook_path, &registry_path).with_context(|| {
            format!(
                "copy registry workbook {}",
                registry_workbook_path.display()
            )
        })?;
        true
    } else {
        false
    };

    append_log(
        log_path,
        &format!(
            "prepared datasets documents={} companies={} reglog={} audit={} host={}",
            documents.len(),
            companies.len(),
            reglog.len(),
            audit.len(),
            host_rows.len()
        ),
    )?;

    if !args.dry_run {
        let key = temporary_ssh_key(&args.remote_key_path, log_path)?;
        let scp_context = ScpUploadContext {
            scp: &scp,
            key: &key,
            user: &args.analytics_user,
            host: &args.analytics_host,
            remote_root: &args.remote_root,
            log_path,
        };
        for (dataset, path) in &files {
            scp_upload(&scp_context, path, dataset)?;
        }
        if registry_uploaded {
            scp_upload(&scp_context, &registry_path, "registry")?;
        }
    }

    save_json_file(&state_path, &next_state)?;
    append_log(
        log_path,
        &format!(
            "upload complete analyticsHost={} remoteRoot={}",
            args.analytics_host, args.remote_root
        ),
    )?;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "analyticsHost": args.analytics_host,
            "analyticsUser": args.analytics_user,
            "remoteRoot": args.remote_root,
            "infobases": infobases.iter().map(|b| b.infobase.clone()).collect::<Vec<_>>(),
            "datasets": {
                "documents": documents.len(),
                "companies": companies.len(),
                "reglog": reglog.len(),
                "audit": audit.len(),
                "host": host_rows.len(),
                "registry": if registry_uploaded { 1 } else { 0 }
            },
            "generatedAtUtc": now_rfc3339
        }))?
    );
    Ok(())
}

fn run_dlp_evidence_sync(args: DlpEvidenceSync) -> Result<()> {
    let mut result = json!({
        "ok": true,
        "dryRun": args.dry_run,
        "roots": [],
        "scanned": 0,
        "uploaded": 0,
        "skipped": 0,
        "failed": 0,
        "errors": []
    });

    let run = run_dlp_evidence_sync_inner(&args, &mut result);
    if let Err(err) = run {
        result["ok"] = Value::Bool(false);
        increment_json_i64(&mut result, "failed", 1);
        push_json_string(&mut result, "errors", format!("{err:#}"));
        let _ = append_log(&args.log_path, &format!("sync failed: {err:#}"));
    }
    println!("{}", serde_json::to_string_pretty(&result)?);
    if result["ok"] == Value::Bool(true) {
        Ok(())
    } else {
        bail!("dlp evidence sync failed")
    }
}

fn run_dlp_evidence_sync_inner(args: &DlpEvidenceSync, result: &mut Value) -> Result<()> {
    let token = fs::read_to_string(&args.token_path)
        .with_context(|| format!("upload token is missing: {}", args.token_path.display()))?
        .trim()
        .to_string();
    if token.is_empty() {
        bail!("upload token is empty: {}", args.token_path.display());
    }
    let config = read_json_file(&args.config_path).unwrap_or(Value::Null);
    let roots = evidence_roots(&config);
    result["roots"] = Value::Array(
        roots
            .iter()
            .map(|p| Value::String(p.to_string_lossy().to_string()))
            .collect(),
    );

    let mut state = read_json_file(&args.state_path).unwrap_or_else(|_| json!({"uploaded": {}}));
    ensure_uploaded_object(&mut state);
    let mut files = Vec::new();
    for root in &roots {
        collect_dlp_evidence_png_files(root, &mut files);
    }
    files.sort_by_key(|p| std::cmp::Reverse(file_modified(p).unwrap_or(SystemTime::UNIX_EPOCH)));
    files.truncate(args.max_files);

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("build evidence upload HTTP client")?;

    for file in files {
        increment_json_i64(result, "scanned", 1);
        match upload_one_evidence_file(args, &client, &token, &mut state, &file) {
            Ok(UploadOutcome::Uploaded { sha }) => {
                increment_json_i64(result, "uploaded", 1);
                append_log(
                    &args.log_path,
                    &format!("uploaded evidence sha={sha} file={}", file.display()),
                )?;
            }
            Ok(UploadOutcome::Skipped) => {
                increment_json_i64(result, "skipped", 1);
            }
            Err(err) => {
                increment_json_i64(result, "failed", 1);
                push_json_string(result, "errors", format!("{}: {err:#}", file.display()));
                append_log(
                    &args.log_path,
                    &format!("upload failed file={}: {err:#}", file.display()),
                )?;
            }
        }
    }
    save_json_file(&args.state_path, &state)?;
    if result["failed"].as_i64().unwrap_or(0) > 0 {
        result["ok"] = Value::Bool(false);
    }
    Ok(())
}

fn run_file_operations_collector(args: FileOperationsCollector) -> Result<()> {
    if args.self_test {
        file_operations_collector_self_test()?;
        println!("file operations collector self-test OK");
        return Ok(());
    }

    let mut runtime = build_file_ops_runtime(&args)?;
    let watch_paths = resolve_file_ops_watch_paths(&args, &runtime)?;
    if watch_paths.is_empty() {
        let state = file_ops_state(&runtime, &watch_paths, "ok", &[], &[]);
        save_file_ops_runtime_state(&runtime, &state)?;
        println!("{}", serde_json::to_string_pretty(&state)?);
        return Ok(());
    }

    append_file_ops_log(
        &runtime,
        &format!(
            "file operations rust started mode={} once={} paths={}",
            runtime.mode,
            args.once,
            watch_paths
                .iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join(";")
        ),
    )?;

    let mut actions = Vec::new();
    let mut problems = Vec::new();
    if let Err(err) = send_file_ops_health(&mut runtime, &watch_paths, &mut actions, &mut problems)
    {
        record_file_ops_problem(
            &mut runtime,
            &mut problems,
            format!("initial health failed: {err:#}"),
        )?;
    } else if runtime.metrics.queue_depth == 0 {
        problems.clear();
    }
    save_file_ops_runtime_state(
        &runtime,
        &file_ops_state(
            &runtime,
            &watch_paths,
            if problems.is_empty() { "ok" } else { "warn" },
            &actions,
            &problems,
        ),
    )?;

    if args.once {
        let state = file_ops_state(&runtime, &watch_paths, "ok", &actions, &problems);
        save_file_ops_runtime_state(&runtime, &state)?;
        println!("{}", serde_json::to_string_pretty(&state)?);
        append_file_ops_log(&runtime, "file operations rust stopped after once")?;
        return Ok(());
    }

    let (tx, rx) = mpsc::channel();
    let mut watcher: RecommendedWatcher =
        Watcher::new(tx, notify::Config::default()).context("create filesystem watcher")?;
    for path in &watch_paths {
        watcher
            .watch(path, RecursiveMode::Recursive)
            .with_context(|| format!("watch {}", path.display()))?;
    }

    let mut last_health = Utc::now();
    let mut pending_rename: Option<PathBuf> = None;
    let deadline = args
        .duration_seconds
        .map(|seconds| Utc::now() + chrono::Duration::seconds(seconds as i64));
    loop {
        match rx.recv_timeout(Duration::from_secs(args.poll_seconds.max(1))) {
            Ok(Ok(event)) => {
                if let Some(file_event) = file_ops_event_from_notify(&event, &mut pending_rename) {
                    if let Err(err) = send_file_operation_event(
                        &mut runtime,
                        file_event,
                        &mut actions,
                        &mut problems,
                    ) {
                        record_file_ops_problem(
                            &mut runtime,
                            &mut problems,
                            format!("file operation send failed: {err:#}"),
                        )?;
                    }
                }
            }
            Ok(Err(err)) => {
                record_file_ops_problem(
                    &mut runtime,
                    &mut problems,
                    format!("watch error: {err:#}"),
                )?;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => bail!("filesystem watcher disconnected"),
        }

        if let Err(err) = flush_file_ops_queue(&mut runtime, 100) {
            record_file_ops_problem(
                &mut runtime,
                &mut problems,
                format!("queue flush failed: {err:#}"),
            )?;
        } else if runtime.metrics.queue_depth == 0 {
            problems.clear();
        }
        if (Utc::now() - last_health).num_seconds() >= (args.poll_seconds.max(10) * 3) as i64 {
            if let Err(err) =
                send_file_ops_health(&mut runtime, &watch_paths, &mut actions, &mut problems)
            {
                record_file_ops_problem(
                    &mut runtime,
                    &mut problems,
                    format!("health send failed: {err:#}"),
                )?;
            } else if runtime.metrics.queue_depth == 0 {
                problems.clear();
            }
            let state = file_ops_state(
                &runtime,
                &watch_paths,
                if problems.is_empty() { "ok" } else { "warn" },
                &actions,
                &problems,
            );
            if let Err(err) = save_file_ops_runtime_state(&runtime, &state) {
                append_file_ops_log(&runtime, &format!("state write failed: {err:#}"))?;
            }
            last_health = Utc::now();
        }
        if deadline.is_some_and(|until| Utc::now() >= until) {
            break;
        }
    }
    let state = file_ops_state(
        &runtime,
        &watch_paths,
        if problems.is_empty() { "ok" } else { "warn" },
        &actions,
        &problems,
    );
    save_file_ops_runtime_state(&runtime, &state)?;
    println!("{}", serde_json::to_string_pretty(&state)?);
    append_file_ops_log(&runtime, "file operations rust stopped after bounded run")?;
    Ok(())
}

fn file_operations_collector_self_test() -> Result<()> {
    let created = file_ops_payload(
        "Created",
        Path::new(r"C:\Users\user\Downloads\a.zip"),
        None,
        42,
        "HOST-EXAMPLE",
        "user",
    );
    if created.get("archiveHint").and_then(Value::as_bool) != Some(true) {
        bail!("archive hint was not set for zip creation");
    }
    let renamed = file_ops_payload(
        "Renamed",
        Path::new(r"C:\Users\user\Documents\b.txt"),
        Some(Path::new(r"C:\Users\user\Documents\a.txt")),
        0,
        "HOST-EXAMPLE",
        "user",
    );
    if renamed.get("oldPath").and_then(Value::as_str).is_none() {
        bail!("renamed event missed oldPath");
    }
    Ok(())
}

fn build_file_ops_runtime(args: &FileOperationsCollector) -> Result<FileOpsRuntime> {
    let config = read_json_file(&args.config_path)?;
    let state_root = json_string(&config, &["paths", "stateRoot"])
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData\AWatch-rus"));
    let logs_root = json_string(&config, &["paths", "logsRoot"])
        .map(PathBuf::from)
        .unwrap_or_else(|| state_root.join("logs"));
    let local_logs_enabled = json_bool(&config, &["logging", "localAgentLogsEnabled"])
        .unwrap_or(true)
        || args.log_path.is_some();
    let username = env::var("USERNAME")
        .or_else(|_| env::var("USER"))
        .unwrap_or_else(|_| "unknown".to_string());
    let session_id = current_session_id();
    let hostname = json_string(&config, &["awHostname"])
        .filter(|value| !value.trim().is_empty())
        .or_else(|| env::var("COMPUTERNAME").ok())
        .unwrap_or_else(|| "unknown".to_string());
    let scheme = args
        .server_scheme
        .clone()
        .or_else(|| json_string(&config, &["server", "scheme"]))
        .unwrap_or_else(|| "http".to_string());
    let host = args
        .server_host
        .clone()
        .or_else(|| json_string(&config, &["server", "host"]))
        .unwrap_or_else(|| "localhost".to_string());
    let port = args
        .server_port
        .or_else(|| json_i64(&config, &["server", "port"]))
        .unwrap_or(5600);
    let api_base = format!("{scheme}://{host}:{port}/api/0");
    let queue_token = queue_name_token(&username, session_id);
    let queue_name = if args.mode == "enforce" {
        format!("file-operations-queue-{queue_token}.jsonl")
    } else {
        format!("file-operations-rust-shadow-queue-{queue_token}.jsonl")
    };
    let state_name = if args.mode == "enforce" {
        format!("file-operations-rust-{queue_token}-state.json")
    } else {
        format!("file-operations-rust-shadow-{queue_token}-state.json")
    };
    let log_path = args
        .log_path
        .clone()
        .unwrap_or_else(|| logs_root.join(format!("file-operations-rust-{queue_token}.log")));
    Ok(FileOpsRuntime {
        api_base,
        hostname: hostname.clone(),
        username,
        session_id,
        state_root: state_root.clone(),
        state_path: state_root.join(state_name),
        log_path,
        queue_path: state_root.join(queue_name),
        bucket_id: format!("aw-file-operations_{hostname}"),
        mode: args.mode.clone(),
        local_logs_enabled,
        metrics: FileOpsMetrics::default(),
    })
}

fn resolve_file_ops_watch_paths(
    args: &FileOperationsCollector,
    runtime: &FileOpsRuntime,
) -> Result<Vec<PathBuf>> {
    let raw_paths = if args.watch_paths.is_empty() {
        vec![
            "Desktop".to_string(),
            "Documents".to_string(),
            "Downloads".to_string(),
        ]
    } else {
        args.watch_paths.clone()
    };
    let mut out = Vec::new();
    for raw in raw_paths {
        let path = resolve_file_ops_watch_path(&raw);
        if path.exists() && path.is_dir() {
            out.push(path);
        } else {
            append_file_ops_log(
                runtime,
                &format!("skip missing watch path raw={raw} path={}", path.display()),
            )?;
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

fn resolve_file_ops_watch_path(raw: &str) -> PathBuf {
    let trimmed = raw.trim();
    if Path::new(trimmed).is_absolute() {
        return PathBuf::from(trimmed);
    }
    let user_profile = env::var("USERPROFILE")
        .map(PathBuf::from)
        .or_else(|_| env::var("HOME").map(PathBuf::from))
        .unwrap_or_else(|_| PathBuf::from("."));
    match trimmed.to_ascii_lowercase().as_str() {
        "desktop" => user_profile.join("Desktop"),
        "documents" => user_profile.join("Documents"),
        "downloads" => user_profile.join("Downloads"),
        _ => PathBuf::from(trimmed),
    }
}

#[derive(Debug, Clone)]
struct FileOperationEvent {
    operation: String,
    path: PathBuf,
    old_path: Option<PathBuf>,
    size: u64,
}

fn file_ops_event_from_notify(
    event: &Event,
    pending_rename: &mut Option<PathBuf>,
) -> Option<FileOperationEvent> {
    match &event.kind {
        EventKind::Create(_) => event.paths.first().map(|path| FileOperationEvent {
            operation: "Created".to_string(),
            path: path.clone(),
            old_path: None,
            size: file_ops_file_size(path),
        }),
        EventKind::Remove(_) => event.paths.first().map(|path| FileOperationEvent {
            operation: "Deleted".to_string(),
            path: path.clone(),
            old_path: None,
            size: 0,
        }),
        EventKind::Modify(notify::event::ModifyKind::Name(mode)) => {
            if matches!(mode, RenameMode::Both) && event.paths.len() >= 2 {
                Some(FileOperationEvent {
                    operation: "Renamed".to_string(),
                    old_path: event.paths.first().cloned(),
                    path: event.paths[1].clone(),
                    size: file_ops_file_size(&event.paths[1]),
                })
            } else if matches!(mode, RenameMode::From) {
                *pending_rename = event.paths.first().cloned();
                None
            } else if matches!(mode, RenameMode::To) {
                event.paths.first().map(|path| FileOperationEvent {
                    operation: "Renamed".to_string(),
                    old_path: pending_rename.take(),
                    path: path.clone(),
                    size: file_ops_file_size(path),
                })
            } else if event.paths.len() >= 2 {
                Some(FileOperationEvent {
                    operation: "Renamed".to_string(),
                    old_path: event.paths.first().cloned(),
                    path: event.paths[1].clone(),
                    size: file_ops_file_size(&event.paths[1]),
                })
            } else if event.paths.len() == 1 {
                Some(FileOperationEvent {
                    operation: "Renamed".to_string(),
                    old_path: None,
                    path: event.paths[0].clone(),
                    size: file_ops_file_size(&event.paths[0]),
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

fn send_file_operation_event(
    runtime: &mut FileOpsRuntime,
    event: FileOperationEvent,
    actions: &mut Vec<Value>,
    problems: &mut Vec<String>,
) -> Result<()> {
    let data = file_ops_payload(
        &event.operation,
        &event.path,
        event.old_path.as_deref(),
        event.size,
        &runtime.hostname,
        &runtime.username,
    );
    let payload = aw_event_payload(data);
    let uri = format!(
        "{}/buckets/{}/heartbeat?pulsetime=15",
        runtime.api_base.trim_end_matches('/'),
        runtime.bucket_id
    );
    enqueue_file_ops_item(runtime, &uri, &payload, "file_op")?;
    flush_file_ops_queue(runtime, 20)?;
    actions.push(json!({
        "action": "file-op",
        "operation": event.operation,
        "path": event.path.to_string_lossy(),
        "applied": runtime.mode == "enforce"
    }));
    if runtime.metrics.send_failures > 0 {
        problems.push("file operation queue has send failures".to_string());
    }
    Ok(())
}

fn send_file_ops_health(
    runtime: &mut FileOpsRuntime,
    watch_paths: &[PathBuf],
    actions: &mut Vec<Value>,
    problems: &mut Vec<String>,
) -> Result<()> {
    let data = json!({
        "signalType": "collector_health",
        "username": runtime.username,
        "hostname": runtime.hostname,
        "sessionId": runtime.session_id,
        "queueDepth": runtime.metrics.queue_depth,
        "eventsEnqueued": runtime.metrics.events_enqueued,
        "eventsFlushed": runtime.metrics.events_flushed,
        "sendFailures": runtime.metrics.send_failures,
        "source": "aw-windows-telemetry-rust",
        "watchPathCount": watch_paths.len()
    });
    let payload = aw_event_payload(data);
    let uri = format!(
        "{}/buckets/{}/heartbeat?pulsetime=30",
        runtime.api_base.trim_end_matches('/'),
        runtime.bucket_id
    );
    enqueue_file_ops_item(runtime, &uri, &payload, "health")?;
    flush_file_ops_queue(runtime, 50)?;
    actions.push(json!({
        "action": "collector-health",
        "applied": runtime.mode == "enforce"
    }));
    if runtime.metrics.send_failures > 0 {
        problems.push("file operations health queue has send failures".to_string());
    }
    Ok(())
}

fn file_ops_payload(
    operation: &str,
    path: &Path,
    old_path: Option<&Path>,
    size: u64,
    hostname: &str,
    username: &str,
) -> Value {
    let extension = path
        .extension()
        .and_then(OsStr::to_str)
        .map(|value| format!(".{value}"))
        .unwrap_or_default();
    let mut data = Map::new();
    data.insert("operation".to_string(), json!(operation));
    data.insert("path".to_string(), json!(path.to_string_lossy()));
    data.insert("extension".to_string(), json!(extension));
    data.insert("username".to_string(), json!(username));
    data.insert("hostname".to_string(), json!(hostname));
    if let Some(old_path) = old_path {
        data.insert("oldPath".to_string(), json!(old_path.to_string_lossy()));
    }
    if size > 0 {
        data.insert("size".to_string(), json!(size));
    }
    if operation == "Created"
        && matches!(
            path.extension()
                .and_then(OsStr::to_str)
                .map(|ext| ext.to_ascii_lowercase())
                .as_deref(),
            Some("zip" | "7z" | "rar" | "tar" | "gz")
        )
    {
        data.insert("archiveHint".to_string(), Value::Bool(true));
    }
    Value::Object(data)
}

fn aw_event_payload(data: Value) -> Value {
    json!({
        "timestamp": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "duration": 0,
        "data": data
    })
}

fn enqueue_file_ops_item(
    runtime: &mut FileOpsRuntime,
    uri: &str,
    payload: &Value,
    kind: &str,
) -> Result<()> {
    if let Some(parent) = runtime.queue_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let item = FileOpsQueueItem {
        ts: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        uri: uri.to_string(),
        payload: serde_json::to_string(payload)?,
        kind: kind.to_string(),
    };
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&runtime.queue_path)?;
    serde_json::to_writer(&mut file, &item)?;
    file.write_all(b"\n")?;
    runtime.metrics.events_enqueued += 1;
    runtime.metrics.queue_depth = runtime.metrics.queue_depth.saturating_add(1);
    Ok(())
}

fn flush_file_ops_queue(runtime: &mut FileOpsRuntime, max_items: usize) -> Result<()> {
    let items = read_file_ops_queue(&runtime.queue_path)?;
    runtime.metrics.queue_depth = items.len();
    if items.is_empty() {
        return Ok(());
    }
    if runtime.mode != "enforce" {
        save_json_file(
            &runtime
                .state_root
                .join("file-operations-rust-shadow-last-queue.json"),
            &items,
        )?;
        return Ok(());
    }

    let client = Client::builder().timeout(Duration::from_secs(15)).build()?;
    ensure_aw_bucket(
        &client,
        &runtime.api_base,
        &runtime.bucket_id,
        "aw-file-operations",
        "aw.file.operation",
        &runtime.hostname,
    )?;

    let mut remaining = Vec::new();
    let mut sent = 0usize;
    for item in items {
        if sent >= max_items {
            remaining.push(item);
            continue;
        }
        let payload: Value = serde_json::from_str(&item.payload).unwrap_or(Value::Null);
        match client.post(&item.uri).json(&payload).send() {
            Ok(response) if response.status().is_success() => {
                runtime.metrics.events_flushed += 1;
                sent += 1;
            }
            Ok(response) => {
                runtime.metrics.send_failures += 1;
                let uri = item.uri.clone();
                append_file_ops_log(
                    runtime,
                    &format!("POST failed uri={} status={}", uri, response.status()),
                )?;
                remaining.push(item);
            }
            Err(err) => {
                runtime.metrics.send_failures += 1;
                let uri = item.uri.clone();
                append_file_ops_log(runtime, &format!("POST error uri={} err={err:#}", uri))?;
                remaining.push(item);
            }
        }
    }
    write_file_ops_queue(&runtime.queue_path, &remaining)?;
    runtime.metrics.queue_depth = remaining.len();
    Ok(())
}

fn read_file_ops_queue(path: &Path) -> Result<Vec<FileOpsQueueItem>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = File::open(path)?;
    let mut out = Vec::new();
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(item) = serde_json::from_str::<FileOpsQueueItem>(&line) {
            out.push(item);
        }
    }
    Ok(out)
}

fn write_file_ops_queue(path: &Path, items: &[FileOpsQueueItem]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    let mut file = File::create(&tmp)?;
    for item in items {
        serde_json::to_writer(&mut file, item)?;
        file.write_all(b"\n")?;
    }
    fs::rename(&tmp, path).or_else(|_| {
        fs::copy(&tmp, path)?;
        fs::remove_file(&tmp)?;
        Ok::<(), std::io::Error>(())
    })?;
    Ok(())
}

fn ensure_aw_bucket(
    client: &Client,
    api_base: &str,
    bucket_id: &str,
    client_name: &str,
    bucket_type: &str,
    hostname: &str,
) -> Result<()> {
    let url = format!("{}/buckets/{bucket_id}", api_base.trim_end_matches('/'));
    if client
        .get(&url)
        .send()
        .map(|response| response.status().is_success())
        .unwrap_or(false)
    {
        return Ok(());
    }
    let response = client
        .post(&url)
        .json(&json!({
            "client": client_name,
            "type": bucket_type,
            "hostname": hostname
        }))
        .send()
        .with_context(|| format!("POST {url}"))?;
    if !response.status().is_success() {
        bail!(
            "bucket create failed {} status={}",
            bucket_id,
            response.status()
        );
    }
    Ok(())
}

fn append_file_ops_log(runtime: &FileOpsRuntime, message: &str) -> Result<()> {
    if !runtime.local_logs_enabled {
        return Ok(());
    }
    append_log(&runtime.log_path, &format!("[FileCollectorRust] {message}"))
}

fn file_ops_state(
    runtime: &FileOpsRuntime,
    watch_paths: &[PathBuf],
    status: &str,
    actions: &[Value],
    problems: &[String],
) -> Value {
    json!({
        "schema": "aw-windows-telemetry.file-operations-collector.v1",
        "status": status,
        "mode": runtime.mode,
        "host": runtime.hostname,
        "username": runtime.username,
        "sessionId": runtime.session_id,
        "generatedAtUtc": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        "bucketId": runtime.bucket_id,
        "queuePath": runtime.queue_path.to_string_lossy(),
        "watchPaths": watch_paths.iter().map(|path| path.to_string_lossy().to_string()).collect::<Vec<_>>(),
        "metrics": runtime.metrics,
        "actions": actions,
        "problems": problems
    })
}

fn save_file_ops_runtime_state(runtime: &FileOpsRuntime, state: &Value) -> Result<()> {
    save_json_file(&runtime.state_path, state)?;
    if runtime.mode != "enforce" {
        save_json_file(
            &runtime
                .state_root
                .join("file-operations-rust-shadow-state.json"),
            state,
        )?;
    }
    Ok(())
}

fn record_file_ops_problem(
    runtime: &mut FileOpsRuntime,
    problems: &mut Vec<String>,
    message: String,
) -> Result<()> {
    runtime.metrics.send_failures = runtime.metrics.send_failures.saturating_add(1);
    problems.push(message.clone());
    if problems.len() > 8 {
        let overflow = problems.len().saturating_sub(8);
        problems.drain(0..overflow);
    }
    append_file_ops_log(runtime, &message)?;
    Ok(())
}

fn queue_name_token(username: &str, session_id: u32) -> String {
    let raw = format!("{username}-s{session_id}");
    let mut out = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.trim().is_empty() {
        format!("session-{session_id}")
    } else {
        out
    }
}

#[cfg(windows)]
fn current_session_id() -> u32 {
    env::var("AW_RUS_SESSION_ID")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .or_else(|| process_session_id(std::process::id()))
        .unwrap_or(0)
}

#[cfg(not(windows))]
fn current_session_id() -> u32 {
    0
}

fn file_ops_file_size(path: &Path) -> u64 {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

#[derive(Debug, Clone)]
struct RustCollectorRuntime {
    api_base: String,
    hostname: String,
    username: String,
    session_id: u32,
    mode: String,
    pulse_seconds: u64,
    state_root: PathBuf,
    log_path: PathBuf,
    state_path: PathBuf,
    rules_path: PathBuf,
    policy_path: PathBuf,
    incident_screenshot_enabled: bool,
}

#[derive(Debug, Default, Clone, Serialize)]
struct ForegroundWindowContext {
    title: String,
    #[serde(rename = "processId")]
    process_id: u32,
    app: String,
    #[cfg(windows)]
    #[serde(skip)]
    window_handle: isize,
}

fn has_foreground_context(context: &ForegroundWindowContext) -> bool {
    context.process_id != 0 || !context.app.trim().is_empty() || !context.title.trim().is_empty()
}

#[derive(Debug, Clone)]
struct WebCategoryRule {
    name: String,
    group: String,
    domains: Vec<String>,
}

#[derive(Debug, Clone)]
struct WebCategoryMatch {
    name: String,
    group: String,
    rule: String,
}

#[derive(Debug, Clone)]
struct BrowserUrlObservation {
    url: String,
    browser: String,
    domain: String,
    root_domain: String,
    category: WebCategoryMatch,
}

#[derive(Debug, Clone)]
struct DlpPolicy {
    raw: Value,
    source: String,
    defaults_enabled: bool,
    defaults_cooldown_seconds: i64,
    defaults_action: String,
    defaults_severity: String,
    content_dictionary_pack: Option<String>,
    content_regex_pack: Option<String>,
    content_ocr_enabled: bool,
    native_mode: String,
    native_allow_global_block: bool,
    native_channel_actions: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
struct DlpActionDecision {
    requested_action: String,
    action: String,
    enforcement_mode: String,
    native_channel_action: String,
    enforcement_suppressed: bool,
}

#[derive(Debug, Default)]
struct EndpointCollectorState {
    last_clipboard_hash: Option<String>,
    seen_usb: HashSet<String>,
    seen_print_jobs: HashSet<String>,
    cooldown: BTreeMap<String, DateTime<Utc>>,
}

#[derive(Debug, Clone)]
struct UsbDrive {
    drive_letter: String,
    volume_name: String,
}

#[derive(Debug, Clone)]
struct PrintJob {
    id: String,
    printer_name: String,
    document_name: String,
    owner: String,
}

#[derive(Debug, Default, Clone)]
struct AdvancedContentMatches {
    dictionary_matches: Vec<Value>,
    regex_matches: Vec<Value>,
}

fn run_browser_domains_collector(args: BrowserDomainsCollector) -> Result<()> {
    if args.self_test {
        let rules = load_category_rules(Path::new(""));
        let normalized = normalize_browser_url("docs.google.com/a/b").unwrap_or_default();
        let domain = host_from_url(&normalized).unwrap_or_default();
        let category = web_category_for_domain(&domain, &rules);
        let context = foreground_window_context();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "ok": true,
                "collector": "browser-domains-collector",
                "sessionId": current_session_id(),
                "urlNormalization": {
                    "input": "docs.google.com/a/b",
                    "url": normalized,
                    "domain": domain,
                    "category": category.name,
                    "categoryGroup": category.group
                },
                "foreground": context
            }))?
        );
        return Ok(());
    }

    let runtime = build_rust_collector_runtime(
        &args.config_path,
        &args.mode,
        args.pulse_seconds,
        "browser-domains-rust",
    )?;
    append_log(
        &runtime.log_path,
        &format!(
            "browser domains rust started mode={} session={}",
            runtime.mode, runtime.session_id
        ),
    )?;
    let category_rules = load_category_rules(&runtime.rules_path);
    let dlp_policy = load_dlp_policy(&runtime.policy_path);
    let mut incident_cooldown = BTreeMap::<String, DateTime<Utc>>::new();
    let deadline = args
        .duration_seconds
        .map(|seconds| Utc::now() + chrono::Duration::seconds(seconds as i64));
    let mut events_sent = 0u64;
    let mut send_failures = 0u64;
    let mut problems = Vec::<String>::new();
    loop {
        let context = foreground_window_context();
        let mut loop_failed = false;
        if has_foreground_context(&context) {
            match send_browser_window_event(&runtime, &context) {
                Ok(()) => events_sent = events_sent.saturating_add(1),
                Err(err) => {
                    loop_failed = true;
                    record_collector_send_failure(
                        &runtime,
                        &mut problems,
                        &mut send_failures,
                        "browser window heartbeat",
                        &err,
                    );
                }
            }
        }
        match send_browser_category_health(&runtime, &context, events_sent) {
            Ok(()) => events_sent = events_sent.saturating_add(1),
            Err(err) => {
                loop_failed = true;
                record_collector_send_failure(
                    &runtime,
                    &mut problems,
                    &mut send_failures,
                    "browser category heartbeat",
                    &err,
                );
            }
        }
        if let Some(observation) = build_browser_url_observation(&context, &category_rules) {
            match send_browser_web_event(&runtime, &context, &observation) {
                Ok(()) => events_sent = events_sent.saturating_add(1),
                Err(err) => {
                    loop_failed = true;
                    record_collector_send_failure(
                        &runtime,
                        &mut problems,
                        &mut send_failures,
                        "browser web heartbeat",
                        &err,
                    );
                }
            }
            match send_browser_category_event(&runtime, &context, &observation) {
                Ok(()) => events_sent = events_sent.saturating_add(1),
                Err(err) => {
                    loop_failed = true;
                    record_collector_send_failure(
                        &runtime,
                        &mut problems,
                        &mut send_failures,
                        "browser category event",
                        &err,
                    );
                }
            }
            match send_matching_web_dlp_incident(
                &runtime,
                &context,
                &observation,
                &dlp_policy,
                &mut incident_cooldown,
            ) {
                Ok(sent) => events_sent = events_sent.saturating_add(sent),
                Err(err) => {
                    loop_failed = true;
                    record_collector_send_failure(
                        &runtime,
                        &mut problems,
                        &mut send_failures,
                        "browser dlp incident",
                        &err,
                    );
                }
            }
        }
        if !loop_failed {
            problems.clear();
        }
        let state = collector_state(
            "aw-windows-telemetry.browser-domains-collector.v1",
            &runtime,
            if loop_failed { "warn" } else { "ok" },
            events_sent,
            send_failures,
            &problems,
        );
        if let Err(err) = save_json_file(&runtime.state_path, &state) {
            let _ = append_log(
                &runtime.log_path,
                &format!(
                    "state write failed path={} err={err:#}",
                    runtime.state_path.display()
                ),
            );
        }
        if args.once || deadline.is_some_and(|until| Utc::now() >= until) {
            println!("{}", serde_json::to_string_pretty(&state)?);
            break;
        }
        std::thread::sleep(Duration::from_secs(args.poll_seconds.max(1)));
    }
    Ok(())
}

fn run_dlp_endpoint_collector(args: DlpEndpointCollector) -> Result<()> {
    if args.self_test {
        let policy = load_dlp_policy_for_config(&args.config_path);
        let suppressed = resolve_dlp_effective_action(&policy, "block", "clipboard");
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "ok": true,
                "collector": "dlp-endpoint-collector",
                "sessionId": current_session_id(),
                "semantics": {
                    "policySource": policy.source,
                    "clipboardSignalType": "clipboard_change",
                    "usbSignalType": "usb_insert",
                    "printSignalType": "print_job",
                    "suppressedBlockDecision": {
                        "requestedAction": suppressed.requested_action,
                        "action": suppressed.action,
                        "enforcementMode": suppressed.enforcement_mode,
                        "nativeChannelAction": suppressed.native_channel_action,
                        "enforcementSuppressed": suppressed.enforcement_suppressed
                    }
                }
            }))?
        );
        return Ok(());
    }

    let runtime = build_rust_collector_runtime(
        &args.config_path,
        &args.mode,
        args.pulse_seconds,
        "dlp-endpoint-rust",
    )?;
    append_log(
        &runtime.log_path,
        &format!(
            "dlp endpoint rust started mode={} session={}",
            runtime.mode, runtime.session_id
        ),
    )?;
    let policy = load_dlp_policy(&runtime.policy_path);
    let mut endpoint_state = EndpointCollectorState::default();
    let deadline = args
        .duration_seconds
        .map(|seconds| Utc::now() + chrono::Duration::seconds(seconds as i64));
    let mut events_sent = 0u64;
    let mut send_failures = 0u64;
    let mut problems = Vec::<String>::new();
    loop {
        let mut loop_failed = false;
        match send_endpoint_health_event(&runtime, events_sent) {
            Ok(()) => {
                events_sent = events_sent.saturating_add(1);
                problems.clear();
            }
            Err(err) => {
                loop_failed = true;
                record_collector_send_failure(
                    &runtime,
                    &mut problems,
                    &mut send_failures,
                    "dlp endpoint heartbeat",
                    &err,
                );
            }
        }
        match send_endpoint_self_test_event(&runtime, events_sent) {
            Ok(()) => {
                events_sent = events_sent.saturating_add(1);
                if !loop_failed {
                    problems.clear();
                }
            }
            Err(err) => {
                loop_failed = true;
                record_collector_send_failure(
                    &runtime,
                    &mut problems,
                    &mut send_failures,
                    "dlp endpoint self-test heartbeat",
                    &err,
                );
            }
        }
        if policy.defaults_enabled {
            match process_clipboard_endpoint_signal(&runtime, &policy, &mut endpoint_state) {
                Ok(sent) => events_sent = events_sent.saturating_add(sent),
                Err(err) => {
                    loop_failed = true;
                    record_collector_send_failure(
                        &runtime,
                        &mut problems,
                        &mut send_failures,
                        "clipboard endpoint signal",
                        &err,
                    );
                }
            }
            match process_usb_endpoint_signals(&runtime, &policy, &mut endpoint_state) {
                Ok(sent) => events_sent = events_sent.saturating_add(sent),
                Err(err) => {
                    loop_failed = true;
                    record_collector_send_failure(
                        &runtime,
                        &mut problems,
                        &mut send_failures,
                        "usb endpoint signal",
                        &err,
                    );
                }
            }
            match process_print_endpoint_signals(&runtime, &policy, &mut endpoint_state) {
                Ok(sent) => events_sent = events_sent.saturating_add(sent),
                Err(err) => {
                    loop_failed = true;
                    record_collector_send_failure(
                        &runtime,
                        &mut problems,
                        &mut send_failures,
                        "print endpoint signal",
                        &err,
                    );
                }
            }
        }
        let state = collector_state(
            "aw-windows-telemetry.dlp-endpoint-collector.v1",
            &runtime,
            if loop_failed { "warn" } else { "ok" },
            events_sent,
            send_failures,
            &problems,
        );
        if let Err(err) = save_json_file(&runtime.state_path, &state) {
            let _ = append_log(
                &runtime.log_path,
                &format!(
                    "state write failed path={} err={err:#}",
                    runtime.state_path.display()
                ),
            );
        }
        if args.once || deadline.is_some_and(|until| Utc::now() >= until) {
            println!("{}", serde_json::to_string_pretty(&state)?);
            break;
        }
        std::thread::sleep(Duration::from_secs(args.poll_seconds.max(1)));
    }
    Ok(())
}

fn build_rust_collector_runtime(
    config_path: &Path,
    mode: &str,
    pulse_seconds: u64,
    name: &str,
) -> Result<RustCollectorRuntime> {
    let config = read_json_file(config_path)?;
    let state_root = json_string(&config, &["paths", "stateRoot"])
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData\AWatch-rus"));
    let logs_root = json_string(&config, &["paths", "logsRoot"])
        .map(PathBuf::from)
        .unwrap_or_else(|| state_root.join("logs"));
    let rules_path = json_string(&config, &["paths", "rulesPath"])
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData\AWatch-rus\web-category-rules.json"));
    let policy_path = json_string(&config, &["paths", "policyPath"])
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData\AWatch-rus\dlp-policy.json"));
    let incident_screenshot_enabled =
        json_bool(&config, &["incidentCapture", "screenshotEnabled"]).unwrap_or(true);
    let scheme = json_string(&config, &["server", "scheme"]).unwrap_or_else(|| "http".to_string());
    let host = json_string(&config, &["server", "host"]).unwrap_or_else(|| "localhost".to_string());
    let port = json_i64(&config, &["server", "port"]).unwrap_or(5600);
    let hostname = json_string(&config, &["awHostname"])
        .filter(|value| !value.trim().is_empty())
        .or_else(|| env::var("COMPUTERNAME").ok())
        .unwrap_or_else(|| "unknown".to_string());
    let username = env::var("USERNAME")
        .or_else(|_| env::var("USER"))
        .unwrap_or_else(|_| "unknown".to_string());
    let session_id = current_session_id();
    let runtime_token = queue_name_token(&username, session_id);
    Ok(RustCollectorRuntime {
        api_base: format!("{scheme}://{host}:{port}/api/0"),
        hostname,
        username,
        session_id,
        mode: mode.to_string(),
        pulse_seconds: pulse_seconds.max(1),
        state_root: state_root.clone(),
        log_path: logs_root.join(format!("{name}-{runtime_token}.log")),
        state_path: state_root.join(format!("{name}-{runtime_token}-state.json")),
        rules_path,
        policy_path,
        incident_screenshot_enabled,
    })
}

fn send_browser_window_event(
    runtime: &RustCollectorRuntime,
    context: &ForegroundWindowContext,
) -> Result<()> {
    let bucket_id = format!("aw-watcher-window_{}", runtime.hostname);
    let data = json!({
        "app": context.app,
        "title": context.title,
        "processId": context.process_id,
        "username": runtime.username,
        "sessionId": runtime.session_id,
        "hostname": runtime.hostname,
        "source": "aw-windows-telemetry-rust"
    });
    send_collector_aw_event(
        runtime,
        &bucket_id,
        "aw-watcher-window",
        "currentwindow",
        data,
    )
}

fn send_browser_category_health(
    runtime: &RustCollectorRuntime,
    context: &ForegroundWindowContext,
    events_sent: u64,
) -> Result<()> {
    let bucket_id = format!("aw-detmir-web-category_{}", runtime.hostname);
    let browser_detected = browser_key_from_app(&context.app).is_some();
    let url_detected = if browser_detected {
        browser_url_from_foreground_window(context)
            .and_then(|url| normalize_browser_url(&url))
            .is_some()
    } else {
        false
    };
    let data = json!({
        "signalType": "collector_health",
        "title": context.title,
        "app": context.app,
        "processId": context.process_id,
        "foregroundProcess": context.app,
        "foregroundTitle": context.title,
        "browserDetected": browser_detected,
        "urlDetected": url_detected,
        "username": runtime.username,
        "sessionId": runtime.session_id,
        "hostname": runtime.hostname,
        "eventsSent": events_sent,
        "source": "aw-windows-telemetry-rust"
    });
    send_collector_aw_event(
        runtime,
        &bucket_id,
        "aw-detmir-web-category",
        "aw.web.category",
        data,
    )
}

fn build_browser_url_observation(
    context: &ForegroundWindowContext,
    category_rules: &[WebCategoryRule],
) -> Option<BrowserUrlObservation> {
    let browser = browser_key_from_app(&context.app)?;
    let raw_url = browser_url_from_foreground_window(context)?;
    let url = normalize_browser_url(&raw_url)?;
    let domain = host_from_url(&url)?;
    let root_domain = root_domain(&domain);
    let category = web_category_for_domain(&domain, category_rules);
    Some(BrowserUrlObservation {
        url,
        browser,
        domain,
        root_domain,
        category,
    })
}

fn send_browser_web_event(
    runtime: &RustCollectorRuntime,
    context: &ForegroundWindowContext,
    observation: &BrowserUrlObservation,
) -> Result<()> {
    let bucket_id = format!(
        "aw-watcher-web-{}_{}",
        observation.browser, runtime.hostname
    );
    let data = json!({
        "url": observation.url,
        "title": context.title,
        "browser": observation.browser,
        "app": context.app,
        "username": runtime.username,
        "sessionId": runtime.session_id,
        "hostname": runtime.hostname,
        "source": "uia-native-rust"
    });
    send_collector_aw_event(
        runtime,
        &bucket_id,
        &format!("aw-watcher-web-{}", observation.browser),
        "web.tab.current",
        data,
    )
}

fn send_browser_category_event(
    runtime: &RustCollectorRuntime,
    context: &ForegroundWindowContext,
    observation: &BrowserUrlObservation,
) -> Result<()> {
    let bucket_id = format!("aw-detmir-web-category_{}", runtime.hostname);
    let data = json!({
        "url": observation.url,
        "title": context.title,
        "browser": observation.browser,
        "app": context.app,
        "domain": observation.domain,
        "rootDomain": observation.root_domain,
        "category": observation.category.name,
        "categoryGroup": observation.category.group,
        "categoryRule": observation.category.rule,
        "username": runtime.username,
        "sessionId": runtime.session_id,
        "hostname": runtime.hostname,
        "source": "uia-native-rust"
    });
    send_collector_aw_event(
        runtime,
        &bucket_id,
        "aw-detmir-web-category",
        "aw.web.category",
        data,
    )
}

fn send_matching_web_dlp_incident(
    runtime: &RustCollectorRuntime,
    context: &ForegroundWindowContext,
    observation: &BrowserUrlObservation,
    policy: &DlpPolicy,
    cooldown: &mut BTreeMap<String, DateTime<Utc>>,
) -> Result<u64> {
    if !policy.defaults_enabled {
        return Ok(0);
    }
    let Some(rules) = policy.raw.get("rules").and_then(Value::as_array) else {
        return Ok(0);
    };
    for rule in rules {
        if !json_bool_any(rule, &["enabled"]).unwrap_or(true) {
            continue;
        }
        let rule_id = json_string_any(rule, &["id"]).unwrap_or_default();
        if rule_id.trim().is_empty() {
            continue;
        }
        if !web_dlp_rule_matches(rule, context, observation) {
            continue;
        }
        let cooldown_seconds = json_i64_any(rule, &["cooldownSeconds"])
            .unwrap_or(policy.defaults_cooldown_seconds)
            .max(30);
        let fingerprint = format!(
            "web|{}|{}|{}",
            rule_id, observation.root_domain, runtime.username
        );
        if !should_emit_by_cooldown(cooldown, &fingerprint, cooldown_seconds) {
            return Ok(0);
        }
        let action =
            json_string_any(rule, &["action"]).unwrap_or_else(|| policy.defaults_action.clone());
        if !matches!(
            action.to_ascii_lowercase().as_str(),
            "alert" | "block" | "quarantine" | "log"
        ) {
            return Ok(0);
        }
        let severity = json_string_any(rule, &["severity"])
            .unwrap_or_else(|| policy.defaults_severity.clone());
        let message = json_string_any(rule, &["message"])
            .unwrap_or_else(|| format!("DLP web rule matched: {rule_id}"));
        let data = json!({
            "ruleId": rule_id,
            "action": action,
            "severity": severity,
            "message": message,
            "url": observation.url,
            "title": context.title,
            "browser": observation.browser,
            "app": context.app,
            "domain": observation.domain,
            "rootDomain": observation.root_domain,
            "category": observation.category.name,
            "categoryGroup": observation.category.group,
            "username": runtime.username,
            "hostname": runtime.hostname,
            "sessionId": runtime.session_id,
            "source": "uia-native-dlp-rust",
            "screenshotEnabled": runtime.incident_screenshot_enabled,
            "screenshotCaptured": false
        });
        send_dlp_incident_event(runtime, data)?;
        return Ok(1);
    }
    Ok(0)
}

fn web_dlp_rule_matches(
    rule: &Value,
    context: &ForegroundWindowContext,
    observation: &BrowserUrlObservation,
) -> bool {
    let when = rule.get("when").unwrap_or(&Value::Null);
    let hour_from = json_i64_any(when, &["hourFrom"]);
    let hour_to = json_i64_any(when, &["hourTo"]);
    if !dlp_time_window_matches(
        Utc::now().with_timezone(&chrono::Local).hour() as i64,
        hour_from,
        hour_to,
    ) {
        return false;
    }
    let domains = json_string_array(when, "domains");
    if !domains.is_empty()
        && !domain_list_matches(&observation.domain, &domains)
        && !domain_list_matches(&observation.root_domain, &domains)
    {
        return false;
    }
    let category_groups = json_string_array_lower(when, "categoryGroups");
    if !category_groups.is_empty()
        && !category_groups.contains(&observation.category.group.to_ascii_lowercase())
    {
        return false;
    }
    let categories = json_string_array_lower(when, "categories");
    if !categories.is_empty()
        && !categories.contains(&observation.category.name.to_ascii_lowercase())
    {
        return false;
    }
    let browsers = json_string_array_lower(when, "browsers");
    if !browsers.is_empty() && !browsers.contains(&observation.browser.to_ascii_lowercase()) {
        return false;
    }
    if let Some(pattern) = json_string_any(when, &["urlRegex"]) {
        if !regex_matches(&pattern, &observation.url) {
            return false;
        }
    }
    if let Some(pattern) = json_string_any(when, &["titleRegex"]) {
        if !regex_matches(&pattern, &context.title) {
            return false;
        }
    }
    true
}

fn browser_key_from_app(app: &str) -> Option<String> {
    let normalized = app.trim().trim_end_matches(".exe").to_ascii_lowercase();
    let key = match normalized.as_str() {
        "msedge" => "edge",
        "chrome" => "chrome",
        "brave" => "brave",
        "vivaldi" => "vivaldi",
        "opera" => "opera",
        "firefox" => "firefox",
        _ => return None,
    };
    Some(key.to_string())
}

fn normalize_browser_url(value: &str) -> Option<String> {
    let candidate = value.trim();
    if candidate.len() < 4 {
        return None;
    }
    let lower = candidate.to_ascii_lowercase();
    if lower.starts_with("search")
        || lower.starts_with("find")
        || lower.starts_with("address and search")
        || lower.starts_with("search with")
        || lower.starts_with("new tab")
        || lower.starts_with("новая вкладка")
    {
        return None;
    }
    if Regex::new(r"(?i)^(https?|file|ftp|chrome|edge|about|view-source)://")
        .ok()
        .is_some_and(|re| re.is_match(candidate))
    {
        return Some(candidate.to_string());
    }
    if Regex::new(r"(?i)^localhost([/:]|$)")
        .ok()
        .is_some_and(|re| re.is_match(candidate))
    {
        return Some(format!("http://{candidate}"));
    }
    if Regex::new(r"^[a-z0-9.-]+\.[a-z]{2,}([/:?#].*)?$")
        .ok()
        .is_some_and(|re| re.is_match(&lower))
    {
        return Some(format!("https://{candidate}"));
    }
    None
}

fn host_from_url(value: &str) -> Option<String> {
    let parsed = Url::parse(value).ok()?;
    let mut host = parsed.host_str()?.to_ascii_lowercase();
    if let Some(stripped) = host.strip_prefix("www.") {
        host = stripped.to_string();
    }
    Some(host)
}

fn root_domain(domain: &str) -> String {
    let parts = domain.split('.').collect::<Vec<_>>();
    if parts.len() <= 2 {
        return domain.to_ascii_lowercase();
    }
    let suffix =
        format!("{}.{}", parts[parts.len() - 2], parts[parts.len() - 1]).to_ascii_lowercase();
    let compound_tlds = [
        "co.uk", "com.au", "co.jp", "com.br", "co.in", "com.tr", "com.cn",
    ];
    if compound_tlds.contains(&suffix.as_str()) && parts.len() >= 3 {
        return format!("{}.{}", parts[parts.len() - 3], suffix).to_ascii_lowercase();
    }
    suffix
}

fn load_category_rules(path: &Path) -> Vec<WebCategoryRule> {
    let mut rules = default_category_rules();
    if !path.exists() {
        return rules;
    }
    let Ok(parsed) = read_json_file(path) else {
        return rules;
    };
    let source_rules = parsed
        .get("rules")
        .and_then(Value::as_array)
        .cloned()
        .or_else(|| parsed.as_array().cloned())
        .unwrap_or_default();
    let mut custom = Vec::new();
    for rule in source_rules {
        let name = json_string_any(&rule, &["name"]).unwrap_or_default();
        let group = json_string_any(&rule, &["group"]).unwrap_or_default();
        let domains = json_string_array(&rule, "domains");
        if !name.trim().is_empty() && !group.trim().is_empty() && !domains.is_empty() {
            custom.push(WebCategoryRule {
                name,
                group,
                domains,
            });
        }
    }
    custom.extend(rules);
    rules = custom;
    rules
}

fn default_category_rules() -> Vec<WebCategoryRule> {
    vec![
        category_rule(
            "work_business_systems",
            "work",
            &[
                "bitrix24.ru",
                "1c.ru",
                "sbis.ru",
                "kontur.ru",
                "diadoc.ru",
                "nalog.gov.ru",
                "gosuslugi.ru",
            ],
        ),
        category_rule(
            "work_docs_collab",
            "work",
            &[
                "office.com",
                "sharepoint.com",
                "docs.google.com",
                "drive.google.com",
                "notion.so",
                "miro.com",
            ],
        ),
        category_rule(
            "work_dev",
            "work",
            &[
                "github.com",
                "gitlab.com",
                "bitbucket.org",
                "youtrack.cloud",
                "atlassian.net",
            ],
        ),
        category_rule(
            "work_communication",
            "work",
            &[
                "teams.microsoft.com",
                "outlook.office.com",
                "web.telegram.org",
                "slack.com",
                "zoom.us",
            ],
        ),
        category_rule(
            "neutral_search_reference",
            "neutral",
            &[
                "google.com",
                "google.ru",
                "yandex.ru",
                "bing.com",
                "duckduckgo.com",
                "wikipedia.org",
            ],
        ),
        category_rule(
            "neutral_news",
            "neutral",
            &[
                "rbc.ru",
                "tass.ru",
                "ria.ru",
                "kommersant.ru",
                "vedomosti.ru",
            ],
        ),
        category_rule(
            "personal_social",
            "personal",
            &[
                "vk.com",
                "ok.ru",
                "facebook.com",
                "instagram.com",
                "tiktok.com",
                "x.com",
                "twitter.com",
            ],
        ),
        category_rule(
            "personal_video",
            "personal",
            &[
                "youtube.com",
                "youtu.be",
                "rutube.ru",
                "twitch.tv",
                "kinopoisk.ru",
            ],
        ),
        category_rule(
            "personal_marketplace",
            "personal",
            &[
                "ozon.ru",
                "wildberries.ru",
                "avito.ru",
                "aliexpress.com",
                "market.yandex.ru",
            ],
        ),
        category_rule(
            "personal_entertainment",
            "personal",
            &["dzen.ru", "pikabu.ru", "dtf.ru", "playground.ru"],
        ),
    ]
}

fn category_rule(name: &str, group: &str, domains: &[&str]) -> WebCategoryRule {
    WebCategoryRule {
        name: name.to_string(),
        group: group.to_string(),
        domains: domains.iter().map(|domain| domain.to_string()).collect(),
    }
}

fn web_category_for_domain(domain: &str, rules: &[WebCategoryRule]) -> WebCategoryMatch {
    for rule in rules {
        for rule_domain in &rule.domains {
            if domain_matches(domain, rule_domain) {
                return WebCategoryMatch {
                    name: rule.name.clone(),
                    group: rule.group.clone(),
                    rule: rule_domain.clone(),
                };
            }
        }
    }
    WebCategoryMatch {
        name: "uncategorized".to_string(),
        group: "neutral".to_string(),
        rule: "none".to_string(),
    }
}

fn domain_matches(domain: &str, rule_domain: &str) -> bool {
    if domain.trim().is_empty() || rule_domain.trim().is_empty() {
        return false;
    }
    let left = domain.to_ascii_lowercase();
    let right = rule_domain.to_ascii_lowercase();
    left == right || left.ends_with(&format!(".{right}"))
}

fn domain_list_matches(domain: &str, rules: &[String]) -> bool {
    rules.iter().any(|rule| domain_matches(domain, rule))
}

fn load_dlp_policy(path: &Path) -> DlpPolicy {
    let raw = if path.exists() {
        read_json_file(path).unwrap_or(Value::Null)
    } else {
        Value::Null
    };
    let source = if path.exists() { "local" } else { "defaults" };
    dlp_policy_from_value(raw, source)
}

fn load_dlp_policy_for_config(config_path: &Path) -> DlpPolicy {
    if let Ok(config) = read_json_file(config_path) {
        if let Some(policy_path) = json_string(&config, &["paths", "policyPath"]) {
            return load_dlp_policy(Path::new(&policy_path));
        }
    }
    dlp_policy_from_value(Value::Null, "defaults")
}

fn dlp_policy_from_value(raw: Value, source: &str) -> DlpPolicy {
    let defaults = raw.get("defaults").unwrap_or(&Value::Null);
    let mut native_channel_actions = BTreeMap::new();
    for channel in ["clipboard", "usb", "print"] {
        let action = json_string(&raw, &["nativeControls", "channels", channel, "action"])
            .unwrap_or_else(|| "audit".to_string())
            .to_ascii_lowercase();
        native_channel_actions.insert(channel.to_string(), action);
    }
    DlpPolicy {
        content_dictionary_pack: json_string(&raw, &["contentAnalysis", "dictionaryPack"]),
        content_regex_pack: json_string(&raw, &["contentAnalysis", "regexPack"]),
        content_ocr_enabled: json_bool(&raw, &["contentAnalysis", "ocrEnabled"]).unwrap_or(false),
        defaults_enabled: json_bool_any(defaults, &["enabled"]).unwrap_or(true),
        defaults_cooldown_seconds: json_i64_any(defaults, &["cooldownSeconds"]).unwrap_or(300),
        defaults_action: json_string_any(defaults, &["action"])
            .unwrap_or_else(|| "alert".to_string()),
        defaults_severity: json_string_any(defaults, &["severity"])
            .unwrap_or_else(|| "medium".to_string()),
        native_mode: json_string(&raw, &["nativeControls", "mode"])
            .unwrap_or_else(|| "monitor".to_string())
            .to_ascii_lowercase(),
        native_allow_global_block: json_bool(
            &raw,
            &["nativeControls", "rollout", "allowGlobalBlock"],
        )
        .unwrap_or(false),
        native_channel_actions,
        raw,
        source: source.to_string(),
    }
}

fn resolve_dlp_effective_action(
    policy: &DlpPolicy,
    requested_action: &str,
    channel: &str,
) -> DlpActionDecision {
    let requested = requested_action.to_ascii_lowercase();
    let native_channel_action = policy
        .native_channel_actions
        .get(channel)
        .cloned()
        .unwrap_or_else(|| "audit".to_string());
    let mut action = requested.clone();
    let mut enforcement_suppressed = false;
    if requested == "block" {
        let channel_allows_block = matches!(
            native_channel_action.as_str(),
            "block" | "blockwithoverride"
        );
        if policy.native_mode != "enforce"
            || !policy.native_allow_global_block
            || !channel_allows_block
        {
            action = "alert".to_string();
            enforcement_suppressed = true;
        }
    }
    DlpActionDecision {
        requested_action: requested,
        action,
        enforcement_mode: policy.native_mode.clone(),
        native_channel_action,
        enforcement_suppressed,
    }
}

fn should_emit_by_cooldown(
    cooldown: &mut BTreeMap<String, DateTime<Utc>>,
    fingerprint: &str,
    cooldown_seconds: i64,
) -> bool {
    let now = Utc::now();
    if let Some(last) = cooldown.get(fingerprint) {
        if (now - *last).num_seconds() < cooldown_seconds {
            return false;
        }
    }
    cooldown.insert(fingerprint.to_string(), now);
    true
}

fn dlp_time_window_matches(
    current_hour: i64,
    hour_from: Option<i64>,
    hour_to: Option<i64>,
) -> bool {
    let (Some(from), Some(to)) = (hour_from, hour_to) else {
        return true;
    };
    if from == to {
        return true;
    }
    if from < to {
        current_hour >= from && current_hour < to
    } else {
        current_hour >= from || current_hour < to
    }
}

fn send_dlp_incident_event(runtime: &RustCollectorRuntime, data: Value) -> Result<()> {
    let bucket_id = format!("aw-dlp-incidents_{}", runtime.hostname);
    send_collector_aw_event(
        runtime,
        &bucket_id,
        "aw-dlp-incidents",
        "aw.dlp.incident",
        data,
    )
}

fn regex_matches(pattern: &str, text: &str) -> bool {
    Regex::new(pattern)
        .map(|regex| regex.is_match(text))
        .unwrap_or(false)
}

fn send_endpoint_health_event(runtime: &RustCollectorRuntime, events_sent: u64) -> Result<()> {
    let bucket_id = format!("aw-dlp-endpoint-signals_{}", runtime.hostname);
    let data = json!({
        "signalType": "collector_health",
        "username": runtime.username,
        "sessionId": runtime.session_id,
        "hostname": runtime.hostname,
        "eventsSent": events_sent,
        "source": "aw-windows-telemetry-rust",
        "mode": runtime.mode
    });
    send_collector_aw_event(
        runtime,
        &bucket_id,
        "aw-dlp-endpoint-signals",
        "aw.dlp.endpoint.signal",
        data,
    )
}

fn send_endpoint_self_test_event(runtime: &RustCollectorRuntime, events_sent: u64) -> Result<()> {
    let bucket_id = format!("aw-dlp-endpoint-signals_{}", runtime.hostname);
    let data = json!({
        "signalType": "self_test",
        "username": runtime.username,
        "sessionId": runtime.session_id,
        "hostname": runtime.hostname,
        "queueDepth": 0,
        "eventsEnqueued": events_sent,
        "eventsFlushed": events_sent,
        "sendFailures": 0,
        "source": "aw-windows-telemetry-rust",
        "mode": runtime.mode
    });
    send_collector_aw_event(
        runtime,
        &bucket_id,
        "aw-dlp-endpoint-signals",
        "aw.dlp.endpoint.signal",
        data,
    )
}

fn process_clipboard_endpoint_signal(
    runtime: &RustCollectorRuntime,
    policy: &DlpPolicy,
    state: &mut EndpointCollectorState,
) -> Result<u64> {
    let Some(text) = read_clipboard_text_safe() else {
        return Ok(0);
    };
    if text.is_empty() {
        return Ok(0);
    }
    let clipboard_hash = hex_sha256(text.as_bytes());
    if state.last_clipboard_hash.as_deref() == Some(clipboard_hash.as_str()) {
        return Ok(0);
    }
    state.last_clipboard_hash = Some(clipboard_hash.clone());
    let mut sent = 0u64;
    send_endpoint_signal_event(
        runtime,
        "clipboard_change",
        json!({
            "clipboardHash": clipboard_hash,
            "clipboardLength": text.chars().count()
        }),
    )?;
    sent += 1;
    sent += evaluate_clipboard_rules(runtime, policy, state, &text, &clipboard_hash)?;
    Ok(sent)
}

fn process_usb_endpoint_signals(
    runtime: &RustCollectorRuntime,
    policy: &DlpPolicy,
    state: &mut EndpointCollectorState,
) -> Result<u64> {
    let drives = enumerate_usb_drives();
    let mut current = HashSet::new();
    let mut sent = 0u64;
    for drive in drives {
        current.insert(drive.drive_letter.clone());
        if state.seen_usb.contains(&drive.drive_letter) {
            continue;
        }
        state.seen_usb.insert(drive.drive_letter.clone());
        send_endpoint_signal_event(
            runtime,
            "usb_insert",
            json!({
                "driveLetter": drive.drive_letter,
                "volumeName": drive.volume_name
            }),
        )?;
        sent += 1;
        sent += evaluate_usb_rules(runtime, policy, state, &drive)?;
    }
    state.seen_usb.retain(|drive| current.contains(drive));
    Ok(sent)
}

fn process_print_endpoint_signals(
    runtime: &RustCollectorRuntime,
    policy: &DlpPolicy,
    state: &mut EndpointCollectorState,
) -> Result<u64> {
    let jobs = enumerate_print_jobs();
    let mut sent = 0u64;
    for job in jobs {
        if state.seen_print_jobs.contains(&job.id) {
            continue;
        }
        state.seen_print_jobs.insert(job.id.clone());
        send_endpoint_signal_event(
            runtime,
            "print_job",
            json!({
                "printerName": job.printer_name,
                "documentName": job.document_name,
                "owner": job.owner
            }),
        )?;
        sent += 1;
        sent += evaluate_print_rules(runtime, policy, state, &job)?;
    }
    if state.seen_print_jobs.len() > 500 {
        state.seen_print_jobs.clear();
    }
    Ok(sent)
}

fn evaluate_clipboard_rules(
    runtime: &RustCollectorRuntime,
    policy: &DlpPolicy,
    state: &mut EndpointCollectorState,
    text: &str,
    clipboard_hash: &str,
) -> Result<u64> {
    let rules = policy
        .raw
        .get("endpoint")
        .and_then(|endpoint| endpoint.get("clipboard"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut sent = 0u64;
    for rule in rules {
        if !json_bool_any(&rule, &["enabled"]).unwrap_or(true) {
            continue;
        }
        let rule_id = json_string_any(&rule, &["id"]).unwrap_or_default();
        if rule_id.trim().is_empty() {
            continue;
        }
        let min_length = json_i64_any(&rule, &["minLength"]).unwrap_or(0).max(0) as usize;
        if text.chars().count() < min_length {
            continue;
        }
        let regex_patterns = json_string_array(&rule, "regexPatterns");
        let mut matched = regex_patterns
            .iter()
            .any(|pattern| regex_matches(pattern, text));
        let dictionary_pack = json_string_any(&rule, &["dictionaryPack"])
            .or_else(|| policy.content_dictionary_pack.clone());
        let regex_pack =
            json_string_any(&rule, &["regexPack"]).or_else(|| policy.content_regex_pack.clone());
        let ocr_enabled =
            json_bool_any(&rule, &["ocrEnabled"]).unwrap_or(policy.content_ocr_enabled);
        let advanced =
            advanced_content_matches(text, dictionary_pack.as_deref(), regex_pack.as_deref());
        if !advanced.dictionary_matches.is_empty() || !advanced.regex_matches.is_empty() {
            matched = true;
        }
        if !matched {
            continue;
        }
        let cooldown = json_i64_any(&rule, &["cooldownSeconds"])
            .unwrap_or(policy.defaults_cooldown_seconds)
            .max(30);
        let fingerprint = format!("clipboard|{rule_id}|{clipboard_hash}|{}", runtime.username);
        if !should_emit_by_cooldown(&mut state.cooldown, &fingerprint, cooldown) {
            continue;
        }
        let requested_action =
            json_string_any(&rule, &["action"]).unwrap_or_else(|| policy.defaults_action.clone());
        let decision = resolve_dlp_effective_action(policy, &requested_action, "clipboard");
        let severity = json_string_any(&rule, &["severity"])
            .unwrap_or_else(|| policy.defaults_severity.clone());
        let message = json_string_any(&rule, &["message"])
            .unwrap_or_else(|| format!("Clipboard rule matched: {rule_id}"));
        let data = json!({
            "ruleId": rule_id,
            "action": decision.action,
            "severity": severity,
            "message": message,
            "signalType": "clipboard",
            "username": runtime.username,
            "sessionId": runtime.session_id,
            "hostname": runtime.hostname,
            "source": "endpoint-signals-rust",
            "clipboardHash": clipboard_hash,
            "clipboardLength": text.chars().count(),
            "enforced": false,
            "requestedAction": decision.requested_action,
            "enforcementMode": decision.enforcement_mode,
            "nativeChannelAction": decision.native_channel_action,
            "enforcementSuppressed": decision.enforcement_suppressed,
            "dictionaryPack": dictionary_pack,
            "regexPack": regex_pack,
            "dictionaryMatches": advanced.dictionary_matches,
            "regexMatches": advanced.regex_matches,
            "ocrRequested": ocr_enabled,
            "screenshotEnabled": runtime.incident_screenshot_enabled,
            "screenshotCaptured": false
        });
        send_dlp_incident_event(runtime, data)?;
        sent += 1;
    }
    Ok(sent)
}

fn evaluate_usb_rules(
    runtime: &RustCollectorRuntime,
    policy: &DlpPolicy,
    state: &mut EndpointCollectorState,
    drive: &UsbDrive,
) -> Result<u64> {
    let rules = policy
        .raw
        .get("endpoint")
        .and_then(|endpoint| endpoint.get("usb"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut sent = 0u64;
    for rule in rules {
        if !json_bool_any(&rule, &["enabled"]).unwrap_or(true) {
            continue;
        }
        let rule_id = json_string_any(&rule, &["id"]).unwrap_or_default();
        if rule_id.trim().is_empty() {
            continue;
        }
        let cooldown = json_i64_any(&rule, &["cooldownSeconds"])
            .unwrap_or(policy.defaults_cooldown_seconds)
            .max(30);
        let fingerprint = format!(
            "usb|{}|{}|{}",
            rule_id, drive.drive_letter, runtime.username
        );
        if !should_emit_by_cooldown(&mut state.cooldown, &fingerprint, cooldown) {
            continue;
        }
        let requested_action =
            json_string_any(&rule, &["action"]).unwrap_or_else(|| policy.defaults_action.clone());
        let decision = resolve_dlp_effective_action(policy, &requested_action, "usb");
        let severity = json_string_any(&rule, &["severity"])
            .unwrap_or_else(|| policy.defaults_severity.clone());
        let message = json_string_any(&rule, &["message"])
            .unwrap_or_else(|| format!("USB rule matched: {rule_id}"));
        let data = json!({
            "ruleId": rule_id,
            "action": decision.action,
            "severity": severity,
            "message": message,
            "signalType": "usb_insert",
            "username": runtime.username,
            "sessionId": runtime.session_id,
            "hostname": runtime.hostname,
            "source": "endpoint-signals-rust",
            "driveLetter": drive.drive_letter,
            "volumeName": drive.volume_name,
            "enforced": false,
            "requestedAction": decision.requested_action,
            "enforcementMode": decision.enforcement_mode,
            "nativeChannelAction": decision.native_channel_action,
            "enforcementSuppressed": decision.enforcement_suppressed,
            "screenshotEnabled": runtime.incident_screenshot_enabled,
            "screenshotCaptured": false
        });
        send_dlp_incident_event(runtime, data)?;
        sent += 1;
    }
    Ok(sent)
}

fn evaluate_print_rules(
    runtime: &RustCollectorRuntime,
    policy: &DlpPolicy,
    state: &mut EndpointCollectorState,
    job: &PrintJob,
) -> Result<u64> {
    let rules = policy
        .raw
        .get("endpoint")
        .and_then(|endpoint| endpoint.get("print"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut sent = 0u64;
    for rule in rules {
        if !json_bool_any(&rule, &["enabled"]).unwrap_or(true) {
            continue;
        }
        let rule_id = json_string_any(&rule, &["id"]).unwrap_or_default();
        if rule_id.trim().is_empty() {
            continue;
        }
        let mut matched = true;
        if let Some(pattern) = json_string_any(&rule, &["printerRegex"]) {
            matched &= regex_matches(&pattern, &job.printer_name);
        }
        if let Some(pattern) = json_string_any(&rule, &["documentRegex"]) {
            matched &= regex_matches(&pattern, &job.document_name);
        }
        let dictionary_pack = json_string_any(&rule, &["dictionaryPack"])
            .or_else(|| policy.content_dictionary_pack.clone());
        let regex_pack =
            json_string_any(&rule, &["regexPack"]).or_else(|| policy.content_regex_pack.clone());
        let ocr_enabled =
            json_bool_any(&rule, &["ocrEnabled"]).unwrap_or(policy.content_ocr_enabled);
        let advanced = advanced_content_matches(
            &job.document_name,
            dictionary_pack.as_deref(),
            regex_pack.as_deref(),
        );
        if !advanced.dictionary_matches.is_empty() || !advanced.regex_matches.is_empty() {
            matched = true;
        }
        if !matched {
            continue;
        }
        let cooldown = json_i64_any(&rule, &["cooldownSeconds"])
            .unwrap_or(policy.defaults_cooldown_seconds)
            .max(30);
        let fingerprint = format!(
            "print|{}|{}|{}|{}",
            rule_id, job.printer_name, job.owner, runtime.username
        );
        if !should_emit_by_cooldown(&mut state.cooldown, &fingerprint, cooldown) {
            continue;
        }
        let requested_action =
            json_string_any(&rule, &["action"]).unwrap_or_else(|| policy.defaults_action.clone());
        let decision = resolve_dlp_effective_action(policy, &requested_action, "print");
        let severity = json_string_any(&rule, &["severity"])
            .unwrap_or_else(|| policy.defaults_severity.clone());
        let message = json_string_any(&rule, &["message"])
            .unwrap_or_else(|| format!("Print rule matched: {rule_id}"));
        let data = json!({
            "ruleId": rule_id,
            "action": decision.action,
            "severity": severity,
            "message": message,
            "signalType": "print_job",
            "username": runtime.username,
            "sessionId": runtime.session_id,
            "hostname": runtime.hostname,
            "source": "endpoint-signals-rust",
            "printerName": job.printer_name,
            "documentName": job.document_name,
            "owner": job.owner,
            "enforced": false,
            "requestedAction": decision.requested_action,
            "enforcementMode": decision.enforcement_mode,
            "nativeChannelAction": decision.native_channel_action,
            "enforcementSuppressed": decision.enforcement_suppressed,
            "dictionaryPack": dictionary_pack,
            "regexPack": regex_pack,
            "dictionaryMatches": advanced.dictionary_matches,
            "regexMatches": advanced.regex_matches,
            "ocrRequested": ocr_enabled,
            "screenshotEnabled": runtime.incident_screenshot_enabled,
            "screenshotCaptured": false
        });
        send_dlp_incident_event(runtime, data)?;
        sent += 1;
    }
    Ok(sent)
}

fn send_endpoint_signal_event(
    runtime: &RustCollectorRuntime,
    signal_type: &str,
    extra: Value,
) -> Result<()> {
    let bucket_id = format!("aw-dlp-endpoint-signals_{}", runtime.hostname);
    let mut data = Map::new();
    data.insert("signalType".to_string(), json!(signal_type));
    data.insert("username".to_string(), json!(runtime.username));
    data.insert("sessionId".to_string(), json!(runtime.session_id));
    data.insert("hostname".to_string(), json!(runtime.hostname));
    data.insert("source".to_string(), json!("endpoint-signals-rust"));
    if let Some(extra) = extra.as_object() {
        for (key, value) in extra {
            data.insert(key.clone(), value.clone());
        }
    }
    send_collector_aw_event(
        runtime,
        &bucket_id,
        "aw-dlp-endpoint-signals",
        "aw.dlp.endpoint.signal",
        Value::Object(data),
    )
}

fn send_collector_aw_event(
    runtime: &RustCollectorRuntime,
    bucket_id: &str,
    client_name: &str,
    bucket_type: &str,
    data: Value,
) -> Result<()> {
    if runtime.mode != "enforce" {
        return Ok(());
    }
    let client = Client::builder().timeout(Duration::from_secs(15)).build()?;
    ensure_aw_bucket(
        &client,
        &runtime.api_base,
        bucket_id,
        client_name,
        bucket_type,
        &runtime.hostname,
    )?;
    let url = format!(
        "{}/buckets/{bucket_id}/heartbeat?pulsetime={}",
        runtime.api_base.trim_end_matches('/'),
        runtime.pulse_seconds
    );
    let response = client
        .post(&url)
        .json(&aw_event_payload(data))
        .send()
        .with_context(|| format!("POST {url}"))?;
    if !response.status().is_success() {
        bail!(
            "collector heartbeat failed {} status={}",
            bucket_id,
            response.status()
        );
    }
    Ok(())
}

fn collector_state(
    schema: &str,
    runtime: &RustCollectorRuntime,
    status: &str,
    events_sent: u64,
    send_failures: u64,
    problems: &[String],
) -> Value {
    json!({
        "schema": schema,
        "status": status,
        "mode": runtime.mode,
        "host": runtime.hostname,
        "username": runtime.username,
        "sessionId": runtime.session_id,
        "generatedAtUtc": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        "eventsSent": events_sent,
        "sendFailures": send_failures,
        "stateRoot": runtime.state_root.to_string_lossy(),
        "problems": problems
    })
}

fn record_collector_send_failure(
    runtime: &RustCollectorRuntime,
    problems: &mut Vec<String>,
    send_failures: &mut u64,
    operation: &str,
    err: &anyhow::Error,
) {
    *send_failures = send_failures.saturating_add(1);
    let message = format!("{operation} failed: {err:#}");
    problems.push(message.clone());
    if problems.len() > 8 {
        let overflow = problems.len().saturating_sub(8);
        problems.drain(0..overflow);
    }
    let _ = append_log(&runtime.log_path, &message);
}

#[cfg(windows)]
fn browser_url_from_foreground_window(context: &ForegroundWindowContext) -> Option<String> {
    if context.window_handle == 0 {
        return None;
    }
    unsafe {
        use windows::Win32::Foundation::{HWND, RPC_E_CHANGED_MODE};
        use windows::Win32::System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
            CoUninitialize,
        };
        use windows::Win32::UI::Accessibility::{
            CUIAutomation, IUIAutomation, IUIAutomationValuePattern, TreeScope_Descendants,
            UIA_EditControlTypeId, UIA_ValuePatternId,
        };

        let init_hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let should_uninitialize = init_hr.is_ok();
        if init_hr.is_err() && init_hr != RPC_E_CHANGED_MODE {
            return None;
        }
        let result = (|| {
            let automation: IUIAutomation = CoCreateInstance(
                &CUIAutomation,
                None::<&windows::core::IUnknown>,
                CLSCTX_INPROC_SERVER,
            )
            .ok()?;
            let root = automation
                .ElementFromHandle(HWND(context.window_handle as *mut core::ffi::c_void))
                .ok()?;
            let condition = automation.CreateTrueCondition().ok()?;
            let elements = root.FindAll(TreeScope_Descendants, &condition).ok()?;
            let len = elements.Length().ok()?.clamp(0, 160);
            for index in 0..len {
                let Ok(element) = elements.GetElement(index) else {
                    continue;
                };
                if element.CurrentControlType().ok() != Some(UIA_EditControlTypeId) {
                    continue;
                }
                if let Ok(pattern) =
                    element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                {
                    if let Ok(current_value) = pattern.CurrentValue() {
                        let value = current_value.to_string();
                        if normalize_browser_url(&value).is_some() {
                            return Some(value);
                        }
                    }
                }
                if let Ok(name) = element.CurrentName() {
                    let value = name.to_string();
                    if normalize_browser_url(&value).is_some() {
                        return Some(value);
                    }
                }
            }
            None
        })();
        if should_uninitialize {
            CoUninitialize();
        }
        result
    }
}

#[cfg(not(windows))]
fn browser_url_from_foreground_window(_context: &ForegroundWindowContext) -> Option<String> {
    None
}

#[cfg(windows)]
fn read_clipboard_text_safe() -> Option<String> {
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalUnlock};

    const CF_UNICODETEXT: u32 = 13;
    unsafe {
        if IsClipboardFormatAvailable(CF_UNICODETEXT) == 0 {
            return None;
        }
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return None;
        }
        let handle = GetClipboardData(CF_UNICODETEXT);
        if handle.is_null() {
            CloseClipboard();
            return None;
        }
        let ptr = GlobalLock(handle) as *const u16;
        if ptr.is_null() {
            CloseClipboard();
            return None;
        }
        let mut len = 0usize;
        while len < 1_000_000 && *ptr.add(len) != 0 {
            len += 1;
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
        GlobalUnlock(handle);
        CloseClipboard();
        Some(text)
    }
}

#[cfg(not(windows))]
fn read_clipboard_text_safe() -> Option<String> {
    None
}

#[cfg(windows)]
fn enumerate_usb_drives() -> Vec<UsbDrive> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
    };

    const DRIVE_REMOVABLE: u32 = 2;
    let mut out = Vec::new();
    let mask = unsafe { GetLogicalDrives() };
    for index in 0..26u32 {
        if mask & (1 << index) == 0 {
            continue;
        }
        let letter = (b'A' + index as u8) as char;
        let root = format!("{letter}:\\");
        let root_wide = OsStr::new(&root)
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let drive_type = unsafe { GetDriveTypeW(root_wide.as_ptr()) };
        if drive_type != DRIVE_REMOVABLE {
            continue;
        }
        let mut volume = vec![0u16; 260];
        let ok = unsafe {
            GetVolumeInformationW(
                root_wide.as_ptr(),
                volume.as_mut_ptr(),
                volume.len() as u32,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
            )
        };
        out.push(UsbDrive {
            drive_letter: format!("{letter}:"),
            volume_name: if ok != 0 {
                utf16_z_to_string(&volume)
            } else {
                String::new()
            },
        });
    }
    out
}

#[cfg(not(windows))]
fn enumerate_usb_drives() -> Vec<UsbDrive> {
    Vec::new()
}

#[cfg(windows)]
fn enumerate_print_jobs() -> Vec<PrintJob> {
    let output = Command::new(system32_path("wmic.exe"))
        .args([
            "printjob",
            "get",
            "JobId,Name,Owner,Document",
            "/format:csv",
        ])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let text = decode_windows_command_output(&output.stdout);
    let cleaned = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if cleaned.trim().is_empty() {
        return Vec::new();
    }
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(cleaned.as_bytes());
    let mut jobs = Vec::new();
    for row in reader
        .deserialize::<std::collections::HashMap<String, String>>()
        .flatten()
    {
        let id = row.get("JobId").cloned().unwrap_or_default();
        if id.trim().is_empty() {
            continue;
        }
        jobs.push(PrintJob {
            id,
            printer_name: row.get("Name").cloned().unwrap_or_default(),
            document_name: row.get("Document").cloned().unwrap_or_default(),
            owner: row.get("Owner").cloned().unwrap_or_default(),
        });
    }
    jobs
}

#[cfg(not(windows))]
fn enumerate_print_jobs() -> Vec<PrintJob> {
    Vec::new()
}

#[cfg(windows)]
fn decode_windows_command_output(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xff && bytes[1] == 0xfe {
        let words = bytes[2..]
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>();
        String::from_utf16_lossy(&words)
    } else {
        String::from_utf8_lossy(bytes).to_string()
    }
}

fn advanced_content_matches(
    text: &str,
    dictionary_pack: Option<&str>,
    regex_pack: Option<&str>,
) -> AdvancedContentMatches {
    let mut result = AdvancedContentMatches::default();
    if text.trim().is_empty() {
        return result;
    }
    if dictionary_pack == Some("152-fz-pdn") {
        for item in regex_find_all(r"\b\d{10}\b|\b\d{12}\b", text) {
            if valid_inn(&item) {
                result.dictionary_matches.push(json!({
                    "name": "inn",
                    "value": item,
                    "severity": "high"
                }));
            }
        }
        for item in regex_find_all(r"\b\d{3}-\d{3}-\d{3}\s?\d{2}\b", text) {
            if valid_snils(&item) {
                result.dictionary_matches.push(json!({
                    "name": "snils",
                    "value": item,
                    "severity": "high"
                }));
            }
        }
        for item in regex_find_all(r"\b\d{4}\s?\d{6}\b", text) {
            if valid_passport(&item) {
                result.dictionary_matches.push(json!({
                    "name": "passport",
                    "value": item,
                    "severity": "high"
                }));
            }
        }
    }
    let regex_rules = match regex_pack.unwrap_or_default() {
        "financial" => vec![
            ("card-pan", r"\b(?:\d[ -]*?){13,19}\b", "high"),
            ("iban", r"\b[A-Z]{2}\d{2}[A-Z0-9]{11,30}\b", "medium"),
        ],
        "contacts" => vec![
            (
                "email",
                r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}",
                "low",
            ),
            (
                "phone-ru",
                r"(?:\+7|8)\s*\(?\d{3}\)?\s*\d{3}[- ]?\d{2}[- ]?\d{2}",
                "low",
            ),
        ],
        "secrets" => vec![
            ("aws-access-key", r"AKIA[0-9A-Z]{16}", "high"),
            (
                "generic-password",
                r"(?i)(password|пароль)\s*[:=]\s*\S{6,}",
                "medium",
            ),
        ],
        _ => Vec::new(),
    };
    for (name, pattern, severity) in regex_rules {
        for item in regex_find_all(pattern, text) {
            result.regex_matches.push(json!({
                "name": name,
                "value": item,
                "severity": severity
            }));
        }
    }
    result
}

fn regex_find_all(pattern: &str, text: &str) -> Vec<String> {
    Regex::new(pattern)
        .map(|regex| {
            regex
                .find_iter(text)
                .map(|item| item.as_str().to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn valid_inn(value: &str) -> bool {
    let digits = digits_only(value);
    match digits.len() {
        10 => {
            let coef = [2, 4, 10, 3, 5, 9, 4, 6, 8];
            let sum = coef
                .iter()
                .enumerate()
                .map(|(index, coef)| digit_at(&digits, index) * coef)
                .sum::<u32>();
            ((sum % 11) % 10) == digit_at(&digits, 9)
        }
        12 => {
            let c11 = [7, 2, 4, 10, 3, 5, 9, 4, 6, 8];
            let c12 = [3, 7, 2, 4, 10, 3, 5, 9, 4, 6, 8];
            let sum11 = c11
                .iter()
                .enumerate()
                .map(|(index, coef)| digit_at(&digits, index) * coef)
                .sum::<u32>();
            let sum12 = c12
                .iter()
                .enumerate()
                .map(|(index, coef)| digit_at(&digits, index) * coef)
                .sum::<u32>();
            ((sum11 % 11) % 10) == digit_at(&digits, 10)
                && ((sum12 % 11) % 10) == digit_at(&digits, 11)
        }
        _ => false,
    }
}

fn valid_snils(value: &str) -> bool {
    let digits = digits_only(value);
    if digits.len() != 11 {
        return false;
    }
    let checksum = digits[9..11].parse::<u32>().unwrap_or(999);
    let sum = (0..9)
        .map(|index| digit_at(&digits, index) * (9 - index as u32))
        .sum::<u32>();
    let expected = if sum < 100 {
        sum
    } else if sum == 100 || sum == 101 {
        0
    } else {
        let value = sum % 101;
        if value == 100 { 0 } else { value }
    };
    checksum == expected
}

fn valid_passport(value: &str) -> bool {
    let digits = digits_only(value);
    if digits.len() != 10 || digits == "0000000000" {
        return false;
    }
    digits.chars().collect::<HashSet<_>>().len() > 1
}

fn digits_only(value: &str) -> String {
    value.chars().filter(|ch| ch.is_ascii_digit()).collect()
}

fn digit_at(value: &str, index: usize) -> u32 {
    value
        .as_bytes()
        .get(index)
        .map(|byte| (byte.saturating_sub(b'0')) as u32)
        .unwrap_or(0)
}

#[cfg(windows)]
fn foreground_window_context() -> ForegroundWindowContext {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
    };

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return ForegroundWindowContext::default();
        }
        let len = GetWindowTextLengthW(hwnd);
        let mut buffer = vec![0u16; (len.max(0) as usize).saturating_add(1)];
        let title = if !buffer.is_empty() {
            let copied = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
            String::from_utf16_lossy(&buffer[..copied.max(0) as usize])
        } else {
            String::new()
        };
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        ForegroundWindowContext {
            title,
            process_id: pid,
            app: process_name_by_pid(pid).unwrap_or_else(|| "unknown".to_string()),
            window_handle: hwnd as isize,
        }
    }
}

#[cfg(not(windows))]
fn foreground_window_context() -> ForegroundWindowContext {
    ForegroundWindowContext::default()
}

#[cfg(windows)]
fn process_name_by_pid(target_pid: u32) -> Option<String> {
    use std::mem::{MaybeUninit, size_of};
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return None;
    }
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..unsafe { MaybeUninit::zeroed().assume_init() }
    };
    let mut ok = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
    while ok {
        if entry.th32ProcessID == target_pid {
            let name = utf16_z_to_string(&entry.szExeFile);
            unsafe {
                CloseHandle(snapshot);
            }
            return Some(name);
        }
        ok = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }
    unsafe {
        CloseHandle(snapshot);
    }
    None
}

fn run_validate_deployment(args: ValidateDeployment) -> Result<()> {
    let config = read_json_file(&args.config_path)?;
    let state_root = json_string(&config, &["paths", "stateRoot"])
        .unwrap_or_else(|| r"C:\ProgramData\AWatch-rus".to_string());
    let deploy_root = json_string(&config, &["paths", "deployRoot"])
        .or_else(|| json_string(&config, &["paths", "toolkitRoot"]))
        .unwrap_or_else(|| r"C:\Program Files\AWatch-rus".to_string());
    let aw_hostname = json_string(&config, &["awHostname"])
        .filter(|v| !v.trim().is_empty())
        .or_else(|| env::var("COMPUTERNAME").ok())
        .unwrap_or_else(|| "unknown".to_string());
    let server_scheme =
        json_string(&config, &["server", "scheme"]).unwrap_or_else(|| "http".to_string());
    let server_host = json_string(&config, &["server", "host"]).unwrap_or_default();
    let server_port = json_i64(&config, &["server", "port"]).unwrap_or(5600);
    let server_url = format!("{server_scheme}://{server_host}:{server_port}");
    let api_base = format!("{server_url}/api/0");

    let telemetry_exe = json_string(&config, &["paths", "file1cTelemetryExecutable"])
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(&deploy_root)
                .join("windows")
                .join("aw-windows-telemetry.exe")
        });
    let agent_exe = PathBuf::from(&state_root)
        .join("agent")
        .join("awatch-agent-rs.exe");
    let agent_config = PathBuf::from(&state_root)
        .join("agent")
        .join("awatch-agent.toml");
    let file1c_task_name = json_string(&config, &["analytics", "file1cAutomation", "taskName"])
        .unwrap_or_else(|| "ActivityWatch File1C Upload".to_string());
    let dlp_evidence_task_name = json_string(&config, &["evidenceSync", "taskName"])
        .unwrap_or_else(|| "ActivityWatch DLP Evidence Sync".to_string());
    let rust_agent_task_name = json_string(&config, &["agent", "taskName"])
        .unwrap_or_else(|| "AWatch Rust Telemetry Agent".to_string());

    let files = validate_files(&[
        args.config_path.clone(),
        telemetry_exe.clone(),
        agent_exe.clone(),
        agent_config.clone(),
    ]);

    let file1c_task = query_scheduled_task(&file1c_task_name);
    let dlp_evidence_task = query_scheduled_task(&dlp_evidence_task_name);
    let rust_agent_task = query_scheduled_task(&rust_agent_task_name);
    let file1c_task_ok = task_uses_exe_and_arg(&file1c_task, &telemetry_exe, "file1c-upload");
    let dlp_evidence_task_ok =
        task_uses_exe_and_arg(&dlp_evidence_task, &telemetry_exe, "dlp-evidence-sync");
    let rust_agent_task_ok = task_uses_exe_and_arg(&rust_agent_task, &agent_exe, "");

    let process_snapshot = collect_process_snapshot();
    let power_shell_by_kind = power_shell_runtime_by_kind(&process_snapshot.processes);
    let worktime_ps_count = power_shell_by_kind.get("worktime").copied().unwrap_or(0);
    let guard_ps_count = power_shell_by_kind.get("guard").copied().unwrap_or(0);
    let worktime_ps_ok = if process_snapshot.command_line_query_ok {
        worktime_ps_count == 0
    } else {
        true
    };
    let guard_ps_ok = if process_snapshot.command_line_query_ok {
        guard_ps_count == 0
    } else {
        true
    };
    let rust_agent_running = process_snapshot.processes.iter().any(|process| {
        process
            .name
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case("awatch-agent-rs.exe"))
    });
    let rust_collector_guard_running = process_snapshot.processes.iter().any(|process| {
        process
            .name
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case("aw-windows-telemetry.exe"))
            && process
                .command_line
                .as_deref()
                .unwrap_or_default()
                .to_ascii_lowercase()
                .contains("collector-guard")
    });
    let collector_guard_service = query_windows_service("AWatchRusCollectorGuard");
    let collector_guard_binary = collector_guard_service
        .get("binaryPath")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let collector_guard_service_ok = collector_guard_service
        .get("running")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && collector_guard_binary.contains("aw-windows-telemetry.exe")
        && collector_guard_binary.contains("collector-guard");

    let worktime_bucket = get_bucket_health(
        &api_base,
        &format!("aw-worktime-sessions_{aw_hostname}"),
        args.worktime_max_age_seconds,
        args.timeout_seconds,
    );
    let queue_checks = vec![
        get_queue_group_health(
            "endpoint",
            Path::new(&state_root),
            "dlp-endpoint-signals-queue",
            1000,
        ),
        get_queue_group_health(
            "fileops",
            Path::new(&state_root),
            "file-operations-queue",
            1000,
        ),
    ];

    let migrated_paths_ok = file1c_task_ok
        && dlp_evidence_task_ok
        && rust_agent_task_ok
        && rust_agent_running
        && worktime_ps_ok
        && guard_ps_ok
        && rust_collector_guard_running
        && collector_guard_service_ok;
    let buckets_ok = worktime_bucket
        .get("ok")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let queues_ok = queue_checks
        .iter()
        .all(|item| item.get("ok").and_then(Value::as_bool).unwrap_or(false));
    let processes_ok = process_snapshot.query_ok
        && rust_agent_running
        && worktime_ps_ok
        && guard_ps_ok
        && rust_collector_guard_running;
    let files_ok = files.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let tasks_ok =
        file1c_task_ok && dlp_evidence_task_ok && rust_agent_task_ok && collector_guard_service_ok;

    let mut failed_sections = Vec::new();
    if !files_ok {
        failed_sections.push("files");
    }
    if !tasks_ok {
        failed_sections.push("tasks");
    }
    if !processes_ok {
        failed_sections.push("processes");
    }
    if !buckets_ok {
        failed_sections.push("buckets");
    }
    if !queues_ok {
        failed_sections.push("queues");
    }
    let overall_ok = failed_sections.is_empty();

    let report = json!({
        "generatedAtUtc": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        "schema": "aw-windows-telemetry.validate-deployment.v1",
        "configPath": args.config_path.to_string_lossy(),
        "serverUrl": server_url,
        "apiBase": api_base,
        "awHostname": aw_hostname,
        "files": files,
        "tasks": {
            "ok": tasks_ok,
            "file1cRustTask": {
                "ok": file1c_task_ok,
                "expectedExe": telemetry_exe.to_string_lossy(),
                "expectedArg": "file1c-upload",
                "task": file1c_task
            },
            "dlpEvidenceRustTask": {
                "ok": dlp_evidence_task_ok,
                "expectedExe": telemetry_exe.to_string_lossy(),
                "expectedArg": "dlp-evidence-sync",
                "task": dlp_evidence_task
            },
            "rustWorktimeAgentTask": {
                "ok": rust_agent_task_ok,
                "expectedExe": agent_exe.to_string_lossy(),
                "task": rust_agent_task
            },
            "collectorGuardRustService": {
                "ok": collector_guard_service_ok,
                "expectedExe": telemetry_exe.to_string_lossy(),
                "expectedArg": "collector-guard",
                "service": collector_guard_service
            }
        },
        "processes": {
            "ok": processes_ok,
            "queryOk": process_snapshot.query_ok,
            "commandLineQueryOk": process_snapshot.command_line_query_ok,
            "queryError": process_snapshot.error,
            "rustWorktimeAgentRunning": rust_agent_running,
            "rustCollectorGuardRunning": rust_collector_guard_running,
            "noPowerShellWorktimeRuntime": if process_snapshot.command_line_query_ok { Value::Bool(worktime_ps_count == 0) } else { Value::Null },
            "noPowerShellWorktimeRuntimeVerified": process_snapshot.command_line_query_ok,
            "noPowerShellCollectorGuardRuntime": if process_snapshot.command_line_query_ok { Value::Bool(guard_ps_count == 0) } else { Value::Null },
            "noPowerShellCollectorGuardRuntimeVerified": process_snapshot.command_line_query_ok,
            "powerShellRuntimeByKind": power_shell_by_kind,
            "legacyPowerShellRuntimeStillPresent": power_shell_by_kind
                .iter()
                .filter(|(kind, _)| kind.as_str() != "worktime" && kind.as_str() != "guard")
                .map(|(_, count)| *count)
                .sum::<usize>() > 0
        },
        "buckets": {
            "ok": buckets_ok,
            "worktime": worktime_bucket
        },
        "queues": {
            "ok": queues_ok,
            "list": queue_checks
        },
        "migration": {
            "ok": migrated_paths_ok,
            "phase": "phase2-rust-paths",
            "remainingPowerShellIsExpected": true,
            "nextTargets": [
                "validate-deployment.ps1 parity",
                "browser-domains-native-collector.ps1",
                "dlp-endpoint-signals-collector.ps1",
                "file-operations-collector.ps1"
            ]
        },
        "summary": {
            "failedSections": failed_sections,
            "remainingPowerShellRuntimeByKind": power_shell_by_kind
        },
        "overallOk": overall_ok
    });

    println!("{}", serde_json::to_string_pretty(&report)?);
    if !overall_ok && args.fail_on_error {
        std::process::exit(2);
    }
    Ok(())
}

fn run_collector_guard(args: CollectorGuard) -> Result<()> {
    if args.self_test {
        collector_guard_self_test()?;
        println!("collector guard self-test OK");
        return Ok(());
    }

    let config = read_json_file(&args.config_path)?;
    let state_root = json_string(&config, &["paths", "stateRoot"])
        .unwrap_or_else(|| r"C:\ProgramData\AWatch-rus".to_string());
    let logs_root = json_string(&config, &["paths", "logsRoot"]).unwrap_or_else(|| {
        PathBuf::from(&state_root)
            .join("logs")
            .to_string_lossy()
            .to_string()
    });
    let lock_path = PathBuf::from(&state_root).join("collector-guard-rust.lock");
    let runtime_path = PathBuf::from(&state_root).join("collector-guard-rust-runtime.json");
    let log_path = PathBuf::from(&logs_root).join("collector-guard-rust.log");
    let _lock = GuardLock::acquire(&lock_path)?;
    let mut runtime = GuardRuntime::load(&runtime_path)?;

    append_log(
        &log_path,
        &format!(
            "collector guard rust started mode={} loop={} once={}",
            args.mode, args.loop_seconds, args.once
        ),
    )?;
    loop {
        match run_collector_guard_cycle(&args, &mut runtime) {
            Ok(state) => {
                save_json_file(&runtime_path, &runtime)?;
                let state_path = PathBuf::from(&state_root).join("collector-guard-rust-state.json");
                save_json_file(&state_path, &state)?;
                append_log(
                    &log_path,
                    &format!(
                        "cycle status={} actions={} problems={}",
                        state
                            .get("status")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown"),
                        state
                            .get("actions")
                            .and_then(Value::as_array)
                            .map(Vec::len)
                            .unwrap_or(0),
                        state
                            .get("problems")
                            .and_then(Value::as_array)
                            .map(Vec::len)
                            .unwrap_or(0)
                    ),
                )?;
            }
            Err(err) => {
                append_log(&log_path, &format!("cycle error: {err:#}"))?;
            }
        }
        if args.once {
            break;
        }
        std::thread::sleep(Duration::from_secs(args.loop_seconds.max(15)));
    }
    append_log(&log_path, "collector guard rust stopped")?;
    Ok(())
}

fn run_collector_guard_cycle(args: &CollectorGuard, runtime: &mut GuardRuntime) -> Result<Value> {
    let config = read_json_file(&args.config_path)?;
    let state_root = json_string(&config, &["paths", "stateRoot"])
        .unwrap_or_else(|| r"C:\ProgramData\AWatch-rus".to_string());
    let aw_hostname = json_string(&config, &["awHostname"])
        .filter(|v| !v.trim().is_empty())
        .or_else(|| env::var("COMPUTERNAME").ok())
        .unwrap_or_else(|| "unknown".to_string());
    let server_scheme =
        json_string(&config, &["server", "scheme"]).unwrap_or_else(|| "http".to_string());
    let server_host = json_string(&config, &["server", "host"]).unwrap_or_default();
    let server_port = json_i64(&config, &["server", "port"]).unwrap_or(5600);
    let api_base = format!("{server_scheme}://{server_host}:{server_port}/api/0");
    let mut process_snapshot = collect_process_snapshot();
    let session_snapshot = collect_session_snapshot();
    let live_session_ids = live_session_ids(&session_snapshot.sessions);
    let mut problems = Vec::new();
    let mut actions = Vec::new();

    let non_live_stop_plan =
        non_live_session_collectors(&process_snapshot.processes, &live_session_ids);
    if !non_live_stop_plan.is_empty() {
        if args.mode == "enforce" {
            for process in &non_live_stop_plan {
                let ok = process.pid.is_some_and(terminate_process);
                actions.push(json!({
                    "action": "stop-non-live-session-collector",
                    "kind": session_scoped_collector_kind(process).unwrap_or("unknown"),
                    "sessionId": process.session_id,
                    "pid": process.pid,
                    "applied": true,
                    "ok": ok
                }));
                if !ok {
                    problems.push(format!(
                        "failed to stop collector pid {:?} in non-live session {:?}",
                        process.pid, process.session_id
                    ));
                }
            }
            process_snapshot = collect_process_snapshot();
        } else {
            for process in &non_live_stop_plan {
                actions.push(json!({
                    "action": "stop-non-live-session-collector",
                    "kind": session_scoped_collector_kind(process).unwrap_or("unknown"),
                    "sessionId": process.session_id,
                    "pid": process.pid,
                    "applied": false,
                    "mode": "shadow"
                }));
            }
        }
    }

    let duplicate_plan = duplicate_legacy_collectors(&process_snapshot.processes);
    if !duplicate_plan.is_empty() {
        if args.mode == "enforce" {
            for duplicate in &duplicate_plan {
                let ok = terminate_process(duplicate.pid);
                actions.push(json!({
                    "action": "dedupe-legacy-collector",
                    "kind": duplicate.kind,
                    "sessionId": duplicate.session_id,
                    "pid": duplicate.pid,
                    "keptPid": duplicate.keep_pid,
                    "applied": true,
                    "ok": ok
                }));
                if !ok {
                    problems.push(format!(
                        "failed to stop duplicate {} collector pid {} in session {}",
                        duplicate.kind, duplicate.pid, duplicate.session_id
                    ));
                }
            }
            process_snapshot = collect_process_snapshot();
        } else {
            for duplicate in &duplicate_plan {
                actions.push(json!({
                    "action": "dedupe-legacy-collector",
                    "kind": duplicate.kind,
                    "sessionId": duplicate.session_id,
                    "pid": duplicate.pid,
                    "keptPid": duplicate.keep_pid,
                    "applied": false,
                    "mode": "shadow"
                }));
            }
        }
    }

    let power_shell_by_kind = power_shell_runtime_by_kind(&process_snapshot.processes);
    let rust_agent_running = process_snapshot.processes.iter().any(|process| {
        process
            .name
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case("awatch-agent-rs.exe"))
    });
    let worktime_ps_count = power_shell_by_kind.get("worktime").copied().unwrap_or(0);
    let worktime_session_mode = json_string(&config, &["collectors", "worktimeSessionMode"])
        .unwrap_or_else(|| "powershell_primary".to_string());
    let worktime_legacy_fallback_enabled =
        json_bool(&config, &["collectors", "worktimeLegacyFallbackEnabled"]).unwrap_or(true);
    let window_enabled = json_bool(&config, &["collectors", "windowEnabled"]).unwrap_or(true);
    let file_ops_enabled = json_bool(&config, &["collectors", "fileOpsEnabled"]).unwrap_or(true);
    let file_ops_mode = json_string(&config, &["collectors", "fileOpsMode"])
        .unwrap_or_else(|| "powershell_primary".to_string());

    let worktime_bucket = get_bucket_health(
        &api_base,
        &format!("aw-worktime-sessions_{aw_hostname}"),
        args.interactive_max_age_seconds,
        15,
    );
    let mut bucket_checks = vec![
        worktime_bucket.clone(),
        get_bucket_health(
            &api_base,
            &format!("aw-watcher-afk_{aw_hostname}"),
            args.interactive_max_age_seconds,
            15,
        ),
    ];
    if window_enabled {
        bucket_checks.push(get_bucket_health(
            &api_base,
            &format!("aw-watcher-window_{aw_hostname}"),
            args.interactive_max_age_seconds,
            15,
        ));
    }
    bucket_checks.push(get_bucket_health(
        &api_base,
        &format!("aw-dlp-endpoint-signals_{aw_hostname}"),
        args.interactive_max_age_seconds,
        15,
    ));
    let interactive_stale = bucket_checks
        .iter()
        .skip(1)
        .any(|item| !item.get("ok").and_then(Value::as_bool).unwrap_or(false));
    if worktime_session_mode.eq_ignore_ascii_case("rust_primary") && !rust_agent_running {
        problems.push("rust worktime agent is not running".to_string());
    }
    if worktime_session_mode.eq_ignore_ascii_case("rust_primary")
        && !worktime_legacy_fallback_enabled
        && worktime_ps_count > 0
    {
        problems.push(
            "PowerShell worktime collector is running while fallback is disabled".to_string(),
        );
    }

    let task_defs = guard_task_definitions(&config);
    let mut missing_fileops_sessions =
        if file_ops_enabled && file_ops_mode.eq_ignore_ascii_case("rust_primary") {
            missing_rust_collector_sessions(
                &process_snapshot.processes,
                "file-operations-collector",
                &["browser-domains-collector", "dlp-endpoint-collector"],
            )
        } else {
            Vec::new()
        };
    missing_fileops_sessions.retain(|session_id| live_session_ids.contains(session_id));
    let has_live_sessions = !live_session_ids.is_empty();
    let effective_interactive_stale = interactive_stale && has_live_sessions;
    let launch_needed = effective_interactive_stale || !missing_fileops_sessions.is_empty();
    if launch_needed {
        let active_legacy_collectors = active_legacy_collector_count(&process_snapshot.processes);
        if effective_interactive_stale
            && missing_fileops_sessions.is_empty()
            && active_legacy_collectors > 0
            && process_snapshot.command_line_query_ok
        {
            problems.push(
                "interactive bucket stale but legacy collectors are already running; skip launch tasks to avoid duplicates"
                    .to_string(),
            );
            actions.push(json!({
                "action": "run-task",
                "applied": false,
                "reason": "legacy-collectors-already-running",
                "activeLegacyCollectors": active_legacy_collectors
            }));
        } else {
            if !missing_fileops_sessions.is_empty() {
                actions.push(json!({
                    "action": "detect-missing-fileops",
                    "applied": false,
                    "mode": "diagnostic",
                    "missingSessions": missing_fileops_sessions.clone()
                }));
            }
            for task in &task_defs {
                if !task.task_name.starts_with("ActivityWatch Launch ") {
                    problems.push(format!("refuse non-allowlisted task {}", task.task_name));
                    continue;
                }
                if !task_has_live_session(&task.user_id, &session_snapshot.sessions) {
                    actions.push(json!({
                        "action": "run-task",
                        "target": task.task_name,
                        "applied": false,
                        "reason": "no-live-session-for-user",
                        "userId": task.user_id
                    }));
                    continue;
                }
                let key = format!("task:{}", task.task_name);
                let allowed = runtime.action_allowed(
                    &key,
                    args.interactive_action_cooldown_seconds,
                    args.restart_window_seconds,
                    args.max_restarts,
                );
                if !allowed.allowed {
                    problems.push(format!("{key} action blocked: {}", allowed.reason));
                    continue;
                }
                if args.mode == "enforce" {
                    let ok = run_scheduled_task(&task.task_name);
                    if ok {
                        runtime.register_action(&key);
                    }
                    actions.push(json!({
                        "action": "run-task",
                        "target": task.task_name,
                        "applied": true,
                        "ok": ok
                    }));
                } else {
                    actions.push(json!({
                        "action": "run-task",
                        "target": task.task_name,
                        "applied": false,
                        "mode": "shadow"
                    }));
                }
            }
        }
    } else {
        for task in &task_defs {
            runtime.reset_action_budget(&format!("task:{}", task.task_name));
        }
    }

    let status = if problems.is_empty() && (args.mode == "enforce" || !effective_interactive_stale)
    {
        "ok"
    } else {
        "warn"
    };
    let state = json!({
        "schema": "aw-windows-telemetry.collector-guard.v1",
        "status": status,
        "mode": args.mode,
        "host": aw_hostname,
        "generatedAtUtc": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        "pid": std::process::id(),
        "configPath": args.config_path.to_string_lossy(),
        "processes": {
            "queryOk": process_snapshot.query_ok,
            "commandLineQueryOk": process_snapshot.command_line_query_ok,
            "rustWorktimeAgentRunning": rust_agent_running,
            "powerShellRuntimeByKind": power_shell_by_kind
        },
        "sessions": {
            "queryOk": session_snapshot.query_ok,
            "source": session_snapshot.source,
            "error": session_snapshot.error,
            "liveSessionIds": live_session_ids.iter().copied().collect::<Vec<_>>(),
            "records": session_snapshot.sessions.iter().map(|session| json!({
                "sessionId": session.session_id,
                "userName": session.user_name,
                "state": session.state,
                "isLive": session.is_live
            })).collect::<Vec<_>>()
        },
        "buckets": bucket_checks,
        "tasks": task_defs.iter().map(|task| json!({
            "taskName": task.task_name,
            "userId": task.user_id
        })).collect::<Vec<_>>(),
        "interactiveStale": interactive_stale,
        "effectiveInteractiveStale": effective_interactive_stale,
        "fileOperationsPresence": {
            "enabled": file_ops_enabled,
            "mode": file_ops_mode,
            "missingSessions": missing_fileops_sessions
        },
        "actions": actions,
        "problems": problems,
        "quarantine": runtime.quarantine.clone(),
        "remainingPowerShellIsExpected": true
    });
    let _ = send_guard_heartbeat(&api_base, &state);
    let _ = save_json_file(
        &PathBuf::from(&state_root).join("collector-guard-state.json"),
        &state,
    );
    Ok(state)
}

fn collector_guard_self_test() -> Result<()> {
    let mut runtime = GuardRuntime::default();
    let allowed = runtime.action_allowed("task:test", 1, 60, 3);
    if !allowed.allowed {
        bail!(
            "expected initial action to be allowed, got {}",
            allowed.reason
        );
    }
    runtime.register_action("task:test");
    let blocked = runtime.action_allowed("task:test", 300, 60, 3);
    if blocked.allowed || blocked.reason != "cooldown" {
        bail!("expected cooldown after registered action");
    }

    let mut budget_runtime = GuardRuntime::default();
    for _ in 0..3 {
        budget_runtime.register_action("task:budget");
    }
    let budget_blocked = budget_runtime.action_allowed("task:budget", 0, 600, 3);
    if budget_blocked.allowed || budget_blocked.reason != "quarantine" {
        bail!("expected quarantine when restart budget is exhausted");
    }
    budget_runtime.reset_action_budget("task:budget");
    let budget_allowed = budget_runtime.action_allowed("task:budget", 0, 600, 3);
    if !budget_allowed.allowed {
        bail!("expected reset action budget to clear quarantine");
    }

    let config = json!({
        "userTasks": [
            {
                "launchTaskName": "ActivityWatch Launch [HOST-EXAMPLE_user]",
                "userId": "HOST-EXAMPLE\\user"
            },
            {
                "LaunchTaskName": "ActivityWatch Launch [HOST-EXAMPLE_admin]",
                "UserId": "HOST-EXAMPLE\\admin"
            }
        ]
    });
    let tasks = guard_task_definitions(&config);
    if tasks.len() != 2
        || tasks[0].task_name != "ActivityWatch Launch [HOST-EXAMPLE_user]"
        || tasks[1].user_id != "HOST-EXAMPLE\\admin"
    {
        bail!("failed to parse guard task definitions");
    }
    Ok(())
}

fn guard_task_definitions(config: &Value) -> Vec<GuardTaskDefinition> {
    let Some(tasks) = json_at(config, &["userTasks"]).and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for task in tasks {
        let task_name = json_string_any(task, &["launchTaskName", "LaunchTaskName"])
            .unwrap_or_default()
            .trim()
            .to_string();
        if task_name.is_empty() || !seen.insert(task_name.clone()) {
            continue;
        }
        let user_id = json_string_any(task, &["userId", "UserId"])
            .unwrap_or_default()
            .trim()
            .to_string();
        out.push(GuardTaskDefinition { task_name, user_id });
    }
    out
}

fn live_session_ids(sessions: &[SessionInfo]) -> HashSet<u32> {
    sessions
        .iter()
        .filter(|session| session.is_live)
        .map(|session| session.session_id)
        .collect()
}

fn task_has_live_session(user_id: &str, sessions: &[SessionInfo]) -> bool {
    let candidates = user_candidates(user_id);
    sessions.iter().any(|session| {
        session.is_live
            && session
                .user_name
                .as_deref()
                .is_some_and(|user| user_matches_candidates(user, &candidates))
    })
}

fn user_candidates(user_id: &str) -> HashSet<String> {
    let normalized = user_id.trim().to_ascii_lowercase();
    let mut out = HashSet::new();
    if normalized.is_empty() {
        return out;
    }
    out.insert(normalized.clone());
    if let Some((_, leaf)) = normalized.rsplit_once('\\') {
        out.insert(leaf.to_string());
    }
    out
}

fn user_matches_candidates(user_name: &str, candidates: &HashSet<String>) -> bool {
    let normalized = user_name.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return false;
    }
    if candidates.contains(&normalized) {
        return true;
    }
    normalized
        .rsplit_once('\\')
        .is_some_and(|(_, leaf)| candidates.contains(leaf))
}

fn run_scheduled_task(task_name: &str) -> bool {
    if !task_name.starts_with("ActivityWatch Launch ") {
        return false;
    }
    Command::new(system32_path("schtasks.exe"))
        .args(["/Run", "/TN", task_name])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn non_live_session_collectors<'a>(
    processes: &'a [ProcessInfo],
    live_session_ids: &HashSet<u32>,
) -> Vec<&'a ProcessInfo> {
    processes
        .iter()
        .filter(|process| {
            let Some(session_id) = process.session_id else {
                return false;
            };
            session_id > 0
                && !live_session_ids.contains(&session_id)
                && session_scoped_collector_kind(process).is_some()
                && process.pid.is_some()
        })
        .collect()
}

fn session_scoped_collector_kind(process: &ProcessInfo) -> Option<&'static str> {
    if let Some(kind) = legacy_collector_kind(process) {
        return Some(kind);
    }
    let name = process.name.as_deref().unwrap_or_default();
    if name.eq_ignore_ascii_case("aw-watcher-afk.exe") {
        return Some("afk");
    }
    if name.eq_ignore_ascii_case("aw-watcher-window.exe") {
        return Some("window");
    }
    if name.eq_ignore_ascii_case("aw-windows-telemetry.exe") {
        let command_line = process
            .command_line
            .as_deref()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if command_line.contains("browser-domains-collector") {
            return Some("browser");
        }
        if command_line.contains("dlp-endpoint-collector") {
            return Some("dlp_endpoint");
        }
        if command_line.contains("file-operations-collector") {
            return Some("fileops");
        }
    }
    None
}

#[derive(Debug, Clone, Serialize)]
struct LegacyCollectorDuplicate {
    kind: &'static str,
    session_id: u32,
    pid: u32,
    keep_pid: u32,
}

fn active_legacy_collector_count(processes: &[ProcessInfo]) -> usize {
    processes
        .iter()
        .filter(|process| legacy_collector_kind(process).is_some())
        .count()
}

fn missing_rust_collector_sessions(
    processes: &[ProcessInfo],
    required_subcommand: &str,
    peer_subcommands: &[&str],
) -> Vec<u32> {
    let required_sessions = rust_collector_sessions(processes, required_subcommand);
    let mut expected_sessions = BTreeSet::new();
    for subcommand in peer_subcommands {
        expected_sessions.extend(rust_collector_sessions(processes, subcommand));
    }
    expected_sessions
        .into_iter()
        .filter(|session_id| !required_sessions.contains(session_id))
        .collect()
}

fn rust_collector_sessions(processes: &[ProcessInfo], subcommand: &str) -> BTreeSet<u32> {
    let subcommand = subcommand.to_ascii_lowercase();
    processes
        .iter()
        .filter_map(|process| {
            let name = process.name.as_deref().unwrap_or_default();
            if !name.eq_ignore_ascii_case("aw-windows-telemetry.exe") {
                return None;
            }
            let command_line = process
                .command_line
                .as_deref()
                .unwrap_or_default()
                .to_ascii_lowercase();
            if !command_line.contains(&subcommand) {
                return None;
            }
            process.session_id
        })
        .collect()
}

fn duplicate_legacy_collectors(processes: &[ProcessInfo]) -> Vec<LegacyCollectorDuplicate> {
    let mut groups: BTreeMap<(&'static str, u32), Vec<&ProcessInfo>> = BTreeMap::new();
    for process in processes {
        let Some(kind) = legacy_collector_kind(process) else {
            continue;
        };
        let Some(session_id) = process.session_id else {
            continue;
        };
        if process.pid.is_none() {
            continue;
        }
        groups.entry((kind, session_id)).or_default().push(process);
    }

    let mut out = Vec::new();
    for ((kind, session_id), mut group) in groups {
        if group.len() <= 1 {
            continue;
        }
        group.sort_by_key(|process| {
            (
                process.created_unix_seconds.unwrap_or(i64::MIN),
                process.pid.unwrap_or(0),
            )
        });
        let keep_pid = group.last().and_then(|process| process.pid).unwrap_or(0);
        for duplicate in group
            .into_iter()
            .take_while(|process| process.pid != Some(keep_pid))
        {
            if let Some(pid) = duplicate.pid {
                out.push(LegacyCollectorDuplicate {
                    kind,
                    session_id,
                    pid,
                    keep_pid,
                });
            }
        }
    }
    out
}

fn legacy_collector_kind(process: &ProcessInfo) -> Option<&'static str> {
    let name = process.name.as_deref().unwrap_or_default();
    if !name.eq_ignore_ascii_case("powershell.exe") && !name.eq_ignore_ascii_case("pwsh.exe") {
        return None;
    }
    match classify_powershell_runtime(process.command_line.as_deref().unwrap_or_default()) {
        "browser" => Some("browser"),
        "fileops" => Some("fileops"),
        "dlp_endpoint" => Some("dlp_endpoint"),
        _ => None,
    }
}

fn terminate_process(pid: u32) -> bool {
    Command::new(system32_path("taskkill.exe"))
        .args(["/PID", &pid.to_string(), "/F"])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn send_guard_heartbeat(api_base: &str, state: &Value) -> bool {
    let host = state
        .get("host")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("unknown");
    let bucket_id = format!("aw-rus-collector-guard_{host}");
    let client = match Client::builder().timeout(Duration::from_secs(15)).build() {
        Ok(client) => client,
        Err(_) => return false,
    };
    let bucket_url = format!("{}/buckets/{bucket_id}", api_base.trim_end_matches('/'));
    let bucket_present = client
        .get(&bucket_url)
        .send()
        .map(|response| response.status().is_success())
        .unwrap_or(false);
    if !bucket_present {
        let created = client
            .post(&bucket_url)
            .json(&json!({
                "client": "aw-rus-collector-guard-rust",
                "type": "aw.rus.collector.guard",
                "hostname": host
            }))
            .send()
            .map(|response| response.status().is_success())
            .unwrap_or(false);
        if !created {
            return false;
        }
    }
    let heartbeat_url = format!(
        "{}/buckets/{bucket_id}/heartbeat?pulsetime=120",
        api_base.trim_end_matches('/')
    );
    client
        .post(heartbeat_url)
        .json(&json!({
            "timestamp": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            "duration": 0,
            "data": state
        }))
        .send()
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}

#[cfg(windows)]
fn process_id_is_running(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return false;
    }
    unsafe {
        CloseHandle(handle);
    }
    true
}

#[cfg(not(windows))]
fn process_id_is_running(pid: u32) -> bool {
    PathBuf::from(format!("/proc/{pid}")).exists()
}

enum UploadOutcome {
    Uploaded { sha: String },
    Skipped,
}

fn upload_one_evidence_file(
    args: &DlpEvidenceSync,
    client: &Client,
    token: &str,
    state: &mut Value,
    file: &Path,
) -> Result<UploadOutcome> {
    let metadata = fs::metadata(file)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > args.max_bytes {
        return Ok(UploadOutcome::Skipped);
    }
    let bytes = fs::read(file)?;
    let sha = hex_sha256(&bytes);
    if already_uploaded(state, &sha, file, &metadata) {
        return Ok(UploadOutcome::Skipped);
    }

    let response_json = if args.dry_run {
        json!({"ok": true, "stored": false, "dryRun": true})
    } else {
        let payload = json!({
            "sha256": sha,
            "content_base64": general_purpose::STANDARD.encode(&bytes),
            "content_type": "image/png",
            "source_file": file.file_name().and_then(OsStr::to_str).unwrap_or(""),
            "source_path": file.to_string_lossy(),
            "hostname": env::var("COMPUTERNAME").unwrap_or_default(),
            "username": env::var("USERNAME").unwrap_or_default()
        });
        let response = client
            .post(&args.evidence_api_url)
            .bearer_auth(token)
            .json(&payload)
            .send()
            .with_context(|| format!("POST {}", args.evidence_api_url))?;
        let status = response.status();
        if !status.is_success() {
            bail!("upload HTTP status {status}");
        }
        response
            .json::<Value>()
            .context("parse evidence upload response")?
    };
    if response_json.get("ok").and_then(Value::as_bool) != Some(true) {
        bail!("upload response is not ok");
    }
    mark_uploaded(
        state,
        &sha,
        file,
        &metadata,
        response_json
            .get("stored")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    )?;
    Ok(UploadOutcome::Uploaded { sha })
}

fn discover_1c_infobases(log_path: &Path) -> Result<Vec<Infobase>> {
    let mut out = Vec::new();
    let users_root = PathBuf::from(r"C:\Users");
    let Ok(entries) = fs::read_dir(&users_root) else {
        return Ok(out);
    };
    let mut seen = HashSet::new();
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_dir() {
            continue;
        }
        let user_name = entry.file_name().to_string_lossy().to_string();
        let launcher = entry.path().join(r"AppData\Roaming\1C\1CEStart\ibases.v8i");
        if !launcher.exists() {
            continue;
        }
        match parse_v8i_file(&launcher, &user_name) {
            Ok(items) => {
                for item in items {
                    let key = format!("{}\n{}", item.infobase, item.path.display());
                    if seen.insert(key) {
                        out.push(item);
                    }
                }
            }
            Err(err) => {
                append_log(
                    log_path,
                    &format!(
                        "skip unreadable launcher file={} reason={err:#}",
                        launcher.display()
                    ),
                )?;
            }
        }
    }
    Ok(out)
}

fn parse_v8i_file(path: &Path, user_name: &str) -> Result<Vec<Infobase>> {
    let raw = fs::read(path)?;
    let text = String::from_utf8_lossy(&raw);
    Ok(parse_v8i_text(&text, user_name, path))
}

fn parse_v8i_text(text: &str, user_name: &str, _launcher_file: &Path) -> Vec<Infobase> {
    let mut current_name: Option<String> = None;
    let mut current_id: Option<String> = None;
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') && line.ends_with(']') && line.len() >= 2 {
            current_name = Some(line[1..line.len() - 1].to_string());
            current_id = None;
            continue;
        }
        if let Some(rest) = line.strip_prefix("ID=") {
            current_id = Some(rest.trim().to_string());
            continue;
        }
        if let Some(name) = &current_name {
            if let Some(path) = line
                .strip_prefix("Connect=File=\"")
                .and_then(|v| v.strip_suffix("\";"))
            {
                out.push(Infobase {
                    user_name: user_name.to_string(),
                    infobase: name.clone(),
                    base_id: current_id.clone(),
                    path: PathBuf::from(path),
                });
            }
        }
    }
    out
}

fn latest_lgp(dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(SystemTime, PathBuf)> = None;
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path
            .extension()
            .and_then(OsStr::to_str)
            .is_some_and(|ext| ext.eq_ignore_ascii_case("lgp"))
        {
            continue;
        }
        let modified = file_modified(&path).unwrap_or(SystemTime::UNIX_EPOCH);
        if best.as_ref().is_none_or(|(old, _)| modified > *old) {
            best = Some((modified, path));
        }
    }
    best.map(|(_, path)| path)
}

fn count_matching_files(path: &Path, predicate: impl Fn(&str) -> bool) -> usize {
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|entry| entry.file_type().map(|ft| ft.is_file()).unwrap_or(false))
        .filter(|entry| predicate(&entry.file_name().to_string_lossy()))
        .count()
}

fn scheduler_touched(current: Option<DateTime<Utc>>, previous: Option<&BaseState>) -> bool {
    let Some(current) = current else { return false };
    let Some(previous) = previous else {
        return false;
    };
    if previous.scheduler_write_utc.trim().is_empty() {
        return true;
    }
    DateTime::parse_from_rfc3339(&previous.scheduler_write_utc)
        .map(|prev| current > prev.with_timezone(&Utc))
        .unwrap_or(true)
}

fn company_activity_score(
    db_delta_mb: f64,
    reglog_delta_mb: f64,
    active_locks: usize,
    has_temp_db: bool,
    scheduler_touched: bool,
    status: &str,
    is_bootstrap: bool,
) -> f64 {
    let mut score = db_delta_mb.abs() + reglog_delta_mb.abs();
    if active_locks > 0 {
        score += active_locks as f64 * 5.0;
    }
    if has_temp_db {
        score += 10.0;
    }
    if scheduler_touched {
        score += 3.0;
    }
    if status == "busy" {
        score += 5.0;
    }
    if is_bootstrap && score <= 0.0 {
        score = 1.0;
    }
    round2(score)
}

fn host_sample(now: &str, host: &str) -> Value {
    let (ram_pct, disk_free_gb) = host_resource_sample();
    json!({
        "ts": now,
        "host": host,
        "cpu_pct": 0.0,
        "ram_pct": ram_pct,
        "disk_free_gb": disk_free_gb,
        "disk_latency_ms": 0,
        "smb_errors": 0,
        "rdp_sessions": rdp_session_count(),
        "backup_ok": 1
    })
}

#[cfg(windows)]
fn host_resource_sample() -> (f64, f64) {
    use std::mem::size_of;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    let mut mem = MEMORYSTATUSEX {
        dwLength: size_of::<MEMORYSTATUSEX>() as u32,
        ..unsafe { std::mem::zeroed() }
    };
    let ram_pct = if unsafe { GlobalMemoryStatusEx(&mut mem) } != 0 && mem.ullTotalPhys > 0 {
        round2(((mem.ullTotalPhys - mem.ullAvailPhys) as f64 / mem.ullTotalPhys as f64) * 100.0)
    } else {
        0.0
    };
    let mut free: u64 = 0;
    let mut total: u64 = 0;
    let mut total_free: u64 = 0;
    let mut path: Vec<u16> = OsStr::new(r"E:\").encode_wide().chain(Some(0)).collect();
    let disk_free_gb = if unsafe {
        GetDiskFreeSpaceExW(path.as_mut_ptr(), &mut free, &mut total, &mut total_free)
    } != 0
    {
        round2(free as f64 / 1_073_741_824.0)
    } else {
        0.0
    };
    (ram_pct, disk_free_gb)
}

#[cfg(not(windows))]
fn host_resource_sample() -> (f64, f64) {
    (0.0, 0.0)
}

fn rdp_session_count() -> usize {
    let query = system32_path("query.exe");
    let output = Command::new(query).arg("user").output();
    match output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
            .lines()
            .skip(1)
            .filter(|line| !line.trim().is_empty())
            .count(),
        _ => 0,
    }
}

fn temporary_ssh_key(source: &Path, log_path: &Path) -> Result<tempfile::NamedTempFile> {
    let temp = tempfile::Builder::new()
        .prefix("awops_ed25519-")
        .tempfile()
        .context("create temporary ssh key copy")?;
    let mut candidates = Vec::new();
    candidates.push(source.to_path_buf());
    if let Ok(profile) = env::var("USERPROFILE") {
        candidates.push(PathBuf::from(profile).join(r".ssh\awops_ed25519"));
    }
    let mut copied = false;
    for candidate in candidates {
        if candidate.as_os_str().is_empty() || !candidate.exists() {
            continue;
        }
        match fs::copy(&candidate, temp.path()) {
            Ok(_) => {
                copied = true;
                break;
            }
            Err(err) => {
                append_log(
                    log_path,
                    &format!(
                        "skip unusable ssh key path={} reason={err}",
                        candidate.display()
                    ),
                )?;
            }
        }
    }
    if !copied {
        bail!("No usable SSH private key found");
    }
    let username = env::var("USERNAME").unwrap_or_else(|_| "Users".to_string());
    let _ = Command::new("icacls.exe")
        .arg(temp.path())
        .args(["/inheritance:r"])
        .status();
    let _ = Command::new("icacls.exe")
        .arg(temp.path())
        .args(["/grant:r", &format!("{username}:(F)")])
        .status();
    Ok(temp)
}

fn scp_upload(context: &ScpUploadContext<'_>, source: &Path, dataset: &str) -> Result<()> {
    let destination = format!(
        "{}@{}:{}/{dataset}/",
        context.user,
        context.host,
        context.remote_root.trim_end_matches('/')
    );
    for attempt in 1..=3 {
        append_log(
            context.log_path,
            &format!(
                "scp attempt={attempt} source={} destination={destination}",
                source.display()
            ),
        )?;
        let status = Command::new(context.scp)
            .arg("-q")
            .arg("-i")
            .arg(context.key.path())
            .args([
                "-o",
                "LogLevel=ERROR",
                "-o",
                "StrictHostKeyChecking=no",
                "-o",
                "UserKnownHostsFile=NUL",
            ])
            .arg(source)
            .arg(&destination)
            .status()
            .with_context(|| format!("start scp {}", source.display()))?;
        if status.success() {
            append_log(
                context.log_path,
                &format!("scp success source={}", source.display()),
            )?;
            return Ok(());
        }
        if attempt == 3 {
            bail!(
                "scp upload failed after 3 attempts for {} with rc={:?}",
                source.display(),
                status.code()
            );
        }
        append_log(
            context.log_path,
            &format!(
                "scp retry source={} rc={:?} delay=5s",
                source.display(),
                status.code()
            ),
        )?;
        std::thread::sleep(Duration::from_secs(5));
    }
    unreachable!()
}

fn evidence_roots(config: &Value) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(path) = json_string(config, &["incidentCapture", "artifactsRoot"]) {
        add_root_if_exists(&mut roots, PathBuf::from(path));
    }
    if let Some(state_root) = json_string(config, &["paths", "stateRoot"]) {
        add_root_if_exists(
            &mut roots,
            PathBuf::from(state_root).join("incident-artifacts"),
        );
    }
    add_root_if_exists(
        &mut roots,
        PathBuf::from(r"C:\ProgramData\AWatch-rus\incident-artifacts"),
    );
    if let Ok(entries) = fs::read_dir(r"C:\Users") {
        for entry in entries.flatten() {
            if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                add_root_if_exists(
                    &mut roots,
                    entry
                        .path()
                        .join(r"AppData\Local\AWatch-rus\incident-artifacts"),
                );
            }
        }
    }
    roots
}

fn add_root_if_exists(roots: &mut Vec<PathBuf>, path: PathBuf) {
    if path.exists() {
        let full = path.canonicalize().unwrap_or(path);
        if !roots.iter().any(|p| p == &full) {
            roots.push(full);
        }
    }
}

fn collect_dlp_evidence_png_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            collect_dlp_evidence_png_files(&path, out);
        } else if ft.is_file() && is_dlp_evidence_screenshot_path(&path) {
            out.push(path);
        }
    }
}

fn is_dlp_evidence_screenshot_path(path: &Path) -> bool {
    if !path
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
    {
        return false;
    }
    let Some(name) = path.file_name().and_then(OsStr::to_str) else {
        return false;
    };
    is_dlp_evidence_screenshot_name(name)
}

fn is_dlp_evidence_screenshot_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let allowed_signal = ["web", "clipboard", "usb_insert", "print_job"]
        .iter()
        .any(|signal| lower.contains(&format!("_{signal}_")));
    lower.ends_with(".png")
        && lower.contains("_sid")
        && allowed_signal
        && !lower.contains("file1c")
        && !lower.contains("1c")
}

fn already_uploaded(state: &Value, sha: &str, file: &Path, metadata: &fs::Metadata) -> bool {
    let Some(entry) = state
        .get("uploaded")
        .and_then(Value::as_object)
        .and_then(|uploaded| uploaded.get(sha))
    else {
        return false;
    };
    let last_write = metadata
        .modified()
        .ok()
        .map(system_time_o)
        .unwrap_or_default();
    entry.get("path").and_then(Value::as_str) == Some(&file.to_string_lossy())
        && entry.get("length").and_then(Value::as_i64) == Some(metadata.len() as i64)
        && entry.get("lastWriteUtc").and_then(Value::as_str) == Some(last_write.as_str())
}

fn mark_uploaded(
    state: &mut Value,
    sha: &str,
    file: &Path,
    metadata: &fs::Metadata,
    response_stored: bool,
) -> Result<()> {
    ensure_uploaded_object(state);
    let uploaded = state
        .get_mut("uploaded")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| anyhow!("state.uploaded is not an object"))?;
    uploaded.insert(
        sha.to_string(),
        json!({
            "path": file.to_string_lossy(),
            "length": metadata.len() as i64,
            "lastWriteUtc": metadata.modified().ok().map(system_time_o).unwrap_or_default(),
            "uploadedAtUtc": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            "responseStored": response_stored
        }),
    );
    Ok(())
}

fn ensure_uploaded_object(state: &mut Value) {
    if !state.is_object() {
        *state = json!({});
    }
    let object = state.as_object_mut().expect("object ensured");
    if !object.get("uploaded").is_some_and(Value::is_object) {
        object.insert("uploaded".to_string(), Value::Object(Map::new()));
    }
}

#[derive(Debug, Default)]
struct ProcessSnapshot {
    query_ok: bool,
    command_line_query_ok: bool,
    error: Option<String>,
    processes: Vec<ProcessInfo>,
}

#[derive(Debug, Clone, Serialize)]
struct ProcessInfo {
    name: Option<String>,
    pid: Option<u32>,
    session_id: Option<u32>,
    created_unix_seconds: Option<i64>,
    command_line: Option<String>,
}

#[derive(Debug, Default)]
struct SessionSnapshot {
    query_ok: bool,
    source: String,
    error: Option<String>,
    sessions: Vec<SessionInfo>,
}

#[derive(Debug, Clone)]
struct SessionInfo {
    session_id: u32,
    user_name: Option<String>,
    state: String,
    is_live: bool,
}

fn validate_files(paths: &[PathBuf]) -> Value {
    let mut list = Vec::new();
    let mut missing = Vec::new();
    for path in paths {
        let exists = path.exists();
        if !exists {
            missing.push(path.to_string_lossy().to_string());
        }
        let metadata = fs::metadata(path).ok();
        list.push(json!({
            "path": path.to_string_lossy(),
            "exists": exists,
            "size": metadata.as_ref().map(|m| m.len()),
            "modifiedUtc": metadata
                .and_then(|m| m.modified().ok())
                .map(system_time_o)
        }));
    }
    json!({
        "ok": missing.is_empty(),
        "required": list,
        "missing": missing
    })
}

fn query_scheduled_task(task_name: &str) -> Value {
    let schtasks = system32_path("schtasks.exe");
    let output = Command::new(&schtasks)
        .args(["/Query", "/TN", task_name, "/XML"])
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let xml = String::from_utf8_lossy(&out.stdout);
            let command = xml_tag_text(&xml, "Command");
            let arguments = xml_tag_text(&xml, "Arguments");
            json!({
                "present": true,
                "taskName": task_name,
                "actionExec": command,
                "actionArgs": arguments
            })
        }
        Ok(out) => json!({
            "present": false,
            "taskName": task_name,
            "error": String::from_utf8_lossy(&out.stderr).trim()
        }),
        Err(err) => json!({
            "present": false,
            "taskName": task_name,
            "error": format!("start {}: {err}", schtasks.display())
        }),
    }
}

fn query_windows_service(service_name: &str) -> Value {
    let sc = system32_path("sc.exe");
    let query_output = Command::new(&sc).args(["query", service_name]).output();
    let qc_output = Command::new(&sc).args(["qc", service_name]).output();
    let query_text = query_output
        .as_ref()
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).to_string())
        .unwrap_or_default();
    let qc_text = qc_output
        .as_ref()
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).to_string())
        .unwrap_or_default();
    let running = query_text.to_ascii_uppercase().contains("RUNNING");
    let binary_path = parse_sc_binary_path(&qc_text).unwrap_or_default();
    json!({
        "present": query_output.as_ref().is_ok_and(|out| out.status.success())
            && qc_output.as_ref().is_ok_and(|out| out.status.success()),
        "serviceName": service_name,
        "running": running,
        "binaryPath": binary_path,
        "queryError": query_output.as_ref().err().map(|err| err.to_string()),
        "configError": qc_output.as_ref().err().map(|err| err.to_string())
    })
}

fn parse_sc_binary_path(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let trimmed = line.trim();
        trimmed
            .strip_prefix("BINARY_PATH_NAME")
            .and_then(|rest| {
                rest.split_once(':')
                    .map(|(_, value)| value.trim().to_string())
            })
            .filter(|value| !value.is_empty())
    })
}

fn task_uses_exe_and_arg(task: &Value, expected_exe: &Path, expected_arg: &str) -> bool {
    if task.get("present").and_then(Value::as_bool) != Some(true) {
        return false;
    }
    let action_exec = task
        .get("actionExec")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !path_text_matches(action_exec, expected_exe) {
        return false;
    }
    if expected_arg.is_empty() {
        return true;
    }
    task.get("actionArgs")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase()
        .contains(&expected_arg.to_ascii_lowercase())
}

fn path_text_matches(actual: &str, expected: &Path) -> bool {
    let actual_norm = normalize_windows_path_text(actual);
    let expected_norm = normalize_windows_path_text(&expected.to_string_lossy());
    actual_norm == expected_norm
        || actual_norm.ends_with(
            &format!(
                "\\{}",
                expected
                    .file_name()
                    .and_then(OsStr::to_str)
                    .unwrap_or_default()
            )
            .to_ascii_lowercase(),
        )
}

fn normalize_windows_path_text(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .replace('/', "\\")
        .to_ascii_lowercase()
}

fn xml_tag_text(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml_unescape(xml[start..end].trim()))
}

fn xml_unescape(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn collect_process_snapshot() -> ProcessSnapshot {
    #[cfg(windows)]
    if let Some(snapshot) = collect_native_process_snapshot() {
        return snapshot;
    }

    let wmic = system32_path(r"wbem\wmic.exe");
    let output = Command::new(&wmic)
        .args([
            "process",
            "get",
            "Name,ProcessId,SessionId,CommandLine",
            "/format:csv",
        ])
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let processes = parse_wmic_process_csv(&out.stdout);
            ProcessSnapshot {
                query_ok: true,
                command_line_query_ok: true,
                error: None,
                processes,
            }
        }
        Ok(out) => collect_tasklist_process_snapshot(Some(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        )),
        Err(err) => {
            collect_tasklist_process_snapshot(Some(format!("start {}: {err}", wmic.display())))
        }
    }
}

fn collect_session_snapshot() -> SessionSnapshot {
    if let Some(raw) = command_output_utf16le("cmd", &["/U", "/C", "query user"])
        .or_else(|| command_output_utf16le("cmd", &["/U", "/C", "quser"]))
    {
        let sessions = parse_query_user_sessions(&raw);
        if !sessions.is_empty() {
            return SessionSnapshot {
                query_ok: true,
                source: "quser_utf16".to_string(),
                error: None,
                sessions,
            };
        }
    }

    if let Some(raw) = command_output_lossy_combined("cmd", &["/C", "query user"])
        .or_else(|| command_output_lossy_combined("cmd", &["/C", "quser"]))
    {
        let sessions = parse_query_user_sessions(&raw);
        if !sessions.is_empty() {
            return SessionSnapshot {
                query_ok: true,
                source: "quser_lossy".to_string(),
                error: None,
                sessions,
            };
        }
    }

    SessionSnapshot {
        query_ok: false,
        source: "unavailable".to_string(),
        error: Some("query user and quser returned no sessions".to_string()),
        sessions: Vec::new(),
    }
}

fn command_output_utf16le(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let mut bytes = output.stdout;
    bytes.extend_from_slice(&output.stderr);
    if bytes.is_empty() {
        return None;
    }
    let words = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();
    String::from_utf16(&words)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn command_output_lossy_combined(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let mut bytes = output.stdout;
    bytes.extend_from_slice(&output.stderr);
    Some(String::from_utf8_lossy(&bytes).trim().to_string()).filter(|value| !value.is_empty())
}

fn parse_query_user_sessions(raw: &str) -> Vec<SessionInfo> {
    raw.lines()
        .filter_map(parse_query_user_line)
        .collect::<Vec<_>>()
}

fn parse_query_user_line(line: &str) -> Option<SessionInfo> {
    let cleaned = line.trim().trim_start_matches('>').trim();
    if cleaned.is_empty()
        || cleaned.to_ascii_lowercase().starts_with("username")
        || cleaned.starts_with("ПОЛЬЗОВАТЕЛЬ")
    {
        return None;
    }
    let parts = cleaned.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 3 {
        return None;
    }
    let username = parts.first()?.trim();
    let (session_id, state, has_session_name) =
        if parts.get(1)?.chars().all(|ch| ch.is_ascii_digit()) {
            (*parts.get(1)?, *parts.get(2)?, false)
        } else {
            (*parts.get(2)?, *parts.get(3).unwrap_or(&"Unknown"), true)
        };
    let session_id = session_id.parse::<u32>().ok()?;
    Some(SessionInfo {
        session_id,
        user_name: (!username.is_empty()).then(|| username.to_string()),
        state: state.to_string(),
        is_live: session_state_is_live(state)
            || (has_session_name && !session_state_is_disconnected(state)),
    })
}

fn session_state_is_live(state: &str) -> bool {
    let lower = state.to_lowercase();
    lower.contains("active") || lower.contains("conn") || lower.contains("актив")
}

fn session_state_is_disconnected(state: &str) -> bool {
    let lower = state.to_lowercase();
    lower.contains("disc") || lower.contains("диск")
}

#[cfg(windows)]
fn collect_native_process_snapshot() -> Option<ProcessSnapshot> {
    use std::mem::{MaybeUninit, size_of};
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return None;
    }
    let mut processes = Vec::new();
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..unsafe { MaybeUninit::zeroed().assume_init() }
    };
    let mut ok = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
    while ok {
        let name = utf16_z_to_string(&entry.szExeFile);
        let pid = entry.th32ProcessID;
        let command_line = read_process_command_line(pid).map(|value| mask_sensitive_text(&value));
        if process_is_relevant(Some(&name), command_line.as_deref()) {
            let session_id = process_session_id(pid);
            let created_unix_seconds = process_creation_unix_seconds(pid);
            processes.push(ProcessInfo {
                name: Some(name),
                pid: Some(pid),
                session_id,
                created_unix_seconds,
                command_line,
            });
        }
        ok = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }
    unsafe {
        CloseHandle(snapshot);
    }
    Some(ProcessSnapshot {
        query_ok: true,
        command_line_query_ok: true,
        error: None,
        processes,
    })
}

#[cfg(windows)]
fn process_session_id(pid: u32) -> Option<u32> {
    use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;

    let mut session_id = 0u32;
    let ok = unsafe { ProcessIdToSessionId(pid, &mut session_id) } != 0;
    ok.then_some(session_id)
}

#[cfg(windows)]
fn process_creation_unix_seconds(pid: u32) -> Option<i64> {
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME};
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let mut created = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut exited = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut kernel = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut user = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let ok =
        unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) } != 0;
    unsafe {
        CloseHandle(handle);
    }
    if !ok {
        return None;
    }
    filetime_to_unix_seconds(created)
}

#[cfg(windows)]
fn filetime_to_unix_seconds(value: windows_sys::Win32::Foundation::FILETIME) -> Option<i64> {
    let ticks = (u64::from(value.dwHighDateTime) << 32) | u64::from(value.dwLowDateTime);
    if ticks == 0 {
        return None;
    }
    const WINDOWS_TO_UNIX_EPOCH_100NS: u64 = 116_444_736_000_000_000;
    if ticks < WINDOWS_TO_UNIX_EPOCH_100NS {
        return None;
    }
    Some(((ticks - WINDOWS_TO_UNIX_EPOCH_100NS) / 10_000_000) as i64)
}

#[cfg(windows)]
#[repr(C)]
struct ProcessBasicInformation {
    reserved1: *mut std::ffi::c_void,
    peb_base_address: *mut std::ffi::c_void,
    reserved2: [*mut std::ffi::c_void; 2],
    unique_process_id: usize,
    reserved3: *mut std::ffi::c_void,
}

#[cfg(windows)]
#[repr(C)]
struct PartialPeb64 {
    reserved: [u8; 0x20],
    process_parameters: *mut std::ffi::c_void,
}

#[cfg(windows)]
#[repr(C)]
#[derive(Clone, Copy)]
struct UnicodeString64 {
    length: u16,
    maximum_length: u16,
    buffer: *const u16,
}

#[cfg(windows)]
#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtQueryInformationProcess(
        process_handle: windows_sys::Win32::Foundation::HANDLE,
        process_information_class: u32,
        process_information: *mut std::ffi::c_void,
        process_information_length: u32,
        return_length: *mut u32,
    ) -> i32;
}

#[cfg(windows)]
fn read_process_command_line(pid: u32) -> Option<String> {
    use std::ffi::c_void;
    use std::mem::{MaybeUninit, size_of};
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
    };

    const PROCESS_BASIC_INFORMATION_CLASS: u32 = 0;
    const RTL_USER_PROCESS_PARAMETERS_COMMAND_LINE_OFFSET_X64: usize = 0x70;

    let handle: HANDLE =
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, pid) };
    if handle.is_null() {
        return None;
    }

    let mut pbi = MaybeUninit::<ProcessBasicInformation>::zeroed();
    let status = unsafe {
        NtQueryInformationProcess(
            handle,
            PROCESS_BASIC_INFORMATION_CLASS,
            pbi.as_mut_ptr().cast::<c_void>(),
            size_of::<ProcessBasicInformation>() as u32,
            null_mut(),
        )
    };
    if status < 0 {
        unsafe {
            CloseHandle(handle);
        }
        return None;
    }
    let pbi = unsafe { pbi.assume_init() };

    let mut peb = MaybeUninit::<PartialPeb64>::zeroed();
    let peb_ok = unsafe {
        ReadProcessMemory(
            handle,
            pbi.peb_base_address.cast::<c_void>(),
            peb.as_mut_ptr().cast::<c_void>(),
            size_of::<PartialPeb64>(),
            null_mut(),
        )
    } != 0;
    if !peb_ok {
        unsafe {
            CloseHandle(handle);
        }
        return None;
    }
    let peb = unsafe { peb.assume_init() };
    if peb.process_parameters.is_null() {
        unsafe {
            CloseHandle(handle);
        }
        return None;
    }

    let command_line_addr = (peb.process_parameters as usize
        + RTL_USER_PROCESS_PARAMETERS_COMMAND_LINE_OFFSET_X64)
        as *const c_void;
    let mut unicode = MaybeUninit::<UnicodeString64>::zeroed();
    let unicode_ok = unsafe {
        ReadProcessMemory(
            handle,
            command_line_addr,
            unicode.as_mut_ptr().cast::<c_void>(),
            size_of::<UnicodeString64>(),
            null_mut(),
        )
    } != 0;
    if !unicode_ok {
        unsafe {
            CloseHandle(handle);
        }
        return None;
    }
    let unicode = unsafe { unicode.assume_init() };
    if unicode.buffer.is_null() || unicode.length == 0 || unicode.length > 32768 {
        unsafe {
            CloseHandle(handle);
        }
        return None;
    }

    let char_count = usize::from(unicode.length) / 2;
    let mut buffer = vec![0u16; char_count];
    let read_ok = unsafe {
        ReadProcessMemory(
            handle,
            unicode.buffer.cast::<c_void>(),
            buffer.as_mut_ptr().cast::<c_void>(),
            usize::from(unicode.length),
            null_mut(),
        )
    } != 0;
    unsafe {
        CloseHandle(handle);
    }
    if !read_ok {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer))
}

#[cfg(windows)]
fn utf16_z_to_string(value: &[u16]) -> String {
    let end = value.iter().position(|ch| *ch == 0).unwrap_or(value.len());
    String::from_utf16_lossy(&value[..end])
}

fn collect_tasklist_process_snapshot(wmic_error: Option<String>) -> ProcessSnapshot {
    let tasklist = system32_path("tasklist.exe");
    let output = Command::new(&tasklist).args(["/FO", "CSV", "/NH"]).output();
    match output {
        Ok(out) if out.status.success() => ProcessSnapshot {
            query_ok: true,
            command_line_query_ok: false,
            error: wmic_error,
            processes: parse_tasklist_csv(&out.stdout),
        },
        Ok(out) => ProcessSnapshot {
            query_ok: false,
            command_line_query_ok: false,
            error: Some(format!(
                "wmic={}; tasklist={}",
                wmic_error.unwrap_or_default(),
                String::from_utf8_lossy(&out.stderr).trim()
            )),
            processes: Vec::new(),
        },
        Err(err) => ProcessSnapshot {
            query_ok: false,
            command_line_query_ok: false,
            error: Some(format!(
                "wmic={}; start {}: {err}",
                wmic_error.unwrap_or_default(),
                tasklist.display()
            )),
            processes: Vec::new(),
        },
    }
}

fn parse_wmic_process_csv(bytes: &[u8]) -> Vec<ProcessInfo> {
    let mut reader = csv::ReaderBuilder::new().flexible(true).from_reader(bytes);
    let Ok(headers) = reader.headers().cloned() else {
        return Vec::new();
    };
    let index = |name: &str| {
        headers
            .iter()
            .position(|item| item.eq_ignore_ascii_case(name))
    };
    let command_idx = index("CommandLine");
    let name_idx = index("Name");
    let pid_idx = index("ProcessId");
    let session_idx = index("SessionId");
    let mut out = Vec::new();
    for record in reader.records().flatten() {
        let name = name_idx
            .and_then(|idx| record.get(idx))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);
        let command_line = command_idx
            .and_then(|idx| record.get(idx))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(mask_sensitive_text);
        if !process_is_relevant(name.as_deref(), command_line.as_deref()) {
            continue;
        }
        out.push(ProcessInfo {
            name,
            pid: pid_idx
                .and_then(|idx| record.get(idx))
                .and_then(|value| value.trim().parse::<u32>().ok()),
            session_id: session_idx
                .and_then(|idx| record.get(idx))
                .and_then(|value| value.trim().parse::<u32>().ok()),
            created_unix_seconds: None,
            command_line,
        });
    }
    out
}

fn parse_tasklist_csv(bytes: &[u8]) -> Vec<ProcessInfo> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(bytes);
    let mut out = Vec::new();
    for record in reader.records().flatten() {
        let name = record
            .get(0)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);
        let pid = record
            .get(1)
            .map(|value| value.trim().replace(',', ""))
            .and_then(|value| value.parse::<u32>().ok());
        let session_id = record
            .get(3)
            .map(|value| value.trim().replace(',', ""))
            .and_then(|value| value.parse::<u32>().ok());
        if !process_is_relevant(name.as_deref(), None) {
            continue;
        }
        out.push(ProcessInfo {
            name,
            pid,
            session_id,
            created_unix_seconds: None,
            command_line: None,
        });
    }
    out
}

fn process_is_relevant(name: Option<&str>, command_line: Option<&str>) -> bool {
    let name = name.unwrap_or_default().to_ascii_lowercase();
    let command_line = command_line.unwrap_or_default().to_ascii_lowercase();
    name.contains("powershell")
        || name == "pwsh.exe"
        || name == "awatch-agent-rs.exe"
        || name == "aw-windows-telemetry.exe"
        || command_line.contains("awatch-rus")
        || command_line.contains("activitywatch")
        || command_line.contains(".ps1")
}

fn power_shell_runtime_by_kind(processes: &[ProcessInfo]) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for process in processes {
        let name = process.name.as_deref().unwrap_or_default();
        if !name.eq_ignore_ascii_case("powershell.exe") && !name.eq_ignore_ascii_case("pwsh.exe") {
            continue;
        }
        let kind = classify_powershell_runtime(process.command_line.as_deref().unwrap_or_default());
        *out.entry(kind.to_string()).or_insert(0) += 1;
    }
    out
}

fn classify_powershell_runtime(command_line: &str) -> &'static str {
    let lower = command_line.to_ascii_lowercase();
    if lower.contains("worktime-session-collector.ps1") {
        "worktime"
    } else if lower.contains("browser-domains-native-collector.ps1") {
        "browser"
    } else if lower.contains("file-operations-collector.ps1") {
        "fileops"
    } else if lower.contains("dlp-endpoint-signals-collector.ps1") {
        "dlp_endpoint"
    } else if lower.contains("recovery-loop.ps1") {
        "recovery"
    } else if lower.contains("aw-collector-guard.ps1") {
        "guard"
    } else if lower.contains("export-upload-hayabusa-to-aw-server.ps1") {
        "hayabusa_upload"
    } else if lower.contains("export-upload-file-1c-telemetry.ps1") {
        "file1c_legacy"
    } else if lower.contains("sync-dlp-evidence-artifacts.ps1") {
        "dlp_evidence_legacy"
    } else {
        "other"
    }
}

fn mask_sensitive_text(value: &str) -> String {
    let mut out = Vec::new();
    let mut redact_next = false;
    for token in value.split_whitespace() {
        let lower = token.to_ascii_lowercase();
        if redact_next {
            out.push("***".to_string());
            redact_next = false;
            continue;
        }
        if lower.contains("token") || lower.contains("password") || lower.contains("secret") {
            if token.contains('=') || token.contains(':') {
                out.push(
                    token
                        .split_once('=')
                        .map(|(key, _)| format!("{key}=***"))
                        .or_else(|| token.split_once(':').map(|(key, _)| format!("{key}:***")))
                        .unwrap_or_else(|| "***".to_string()),
                );
            } else {
                out.push(token.to_string());
                redact_next = true;
            }
        } else {
            out.push(token.to_string());
        }
    }
    out.join(" ")
}

fn get_bucket_health(
    api_base: &str,
    bucket_id: &str,
    max_age_seconds: i64,
    timeout_seconds: u64,
) -> Value {
    let url = format!(
        "{}/buckets/{}/events?limit=5",
        api_base.trim_end_matches('/'),
        bucket_id
    );
    let client = match Client::builder()
        .timeout(Duration::from_secs(timeout_seconds))
        .build()
    {
        Ok(client) => client,
        Err(err) => {
            return json!({
                "bucketId": bucket_id,
                "ok": false,
                "queryOk": false,
                "error": format!("build HTTP client: {err:#}")
            });
        }
    };
    let response = match client.get(&url).send() {
        Ok(response) => response,
        Err(err) => {
            return json!({
                "bucketId": bucket_id,
                "ok": false,
                "queryOk": false,
                "error": format!("{err:#}")
            });
        }
    };
    let status = response.status();
    if !status.is_success() {
        return json!({
            "bucketId": bucket_id,
            "ok": false,
            "queryOk": false,
            "httpStatus": status.as_u16()
        });
    }
    let events = response.json::<Value>().unwrap_or(Value::Null);
    let latest = latest_event_timestamp_utc(&events);
    let age_seconds = latest.map(|ts| (Utc::now() - ts).num_seconds());
    let ok = age_seconds.is_some_and(|age| age <= max_age_seconds);
    json!({
        "bucketId": bucket_id,
        "ok": ok,
        "queryOk": true,
        "maxAgeSeconds": max_age_seconds,
        "latestTimestampUtc": latest.map(|ts| ts.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
        "ageSeconds": age_seconds,
        "count": events.as_array().map(Vec::len).unwrap_or(0)
    })
}

fn latest_event_timestamp_utc(events: &Value) -> Option<DateTime<Utc>> {
    events
        .as_array()?
        .iter()
        .filter_map(|event| {
            event
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
                .map(|dt| dt.with_timezone(&Utc))
        })
        .max()
}

fn get_queue_group_health(name: &str, state_root: &Path, prefix: &str, max_depth: usize) -> Value {
    let mut queues = Vec::new();
    if let Ok(entries) = fs::read_dir(state_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = path.file_name().and_then(OsStr::to_str).unwrap_or_default();
            if !file_name.starts_with(prefix) || !file_name.ends_with(".jsonl") {
                continue;
            }
            let depth = count_lines(&path);
            queues.push(json!({
                "path": path.to_string_lossy(),
                "depth": depth,
                "sizeBytes": fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
                "ok": depth <= max_depth
            }));
        }
    }
    let depth_total: usize = queues
        .iter()
        .filter_map(|queue| queue.get("depth").and_then(Value::as_u64))
        .map(|value| value as usize)
        .sum();
    json!({
        "name": name,
        "ok": queues.iter().all(|queue| queue.get("ok").and_then(Value::as_bool).unwrap_or(false)),
        "queueCount": queues.len(),
        "depth": depth_total,
        "maxDepth": max_depth,
        "queues": queues
    })
}

fn count_lines(path: &Path) -> usize {
    File::open(path)
        .map(|file| BufReader::new(file).lines().map_while(Result::ok).count())
        .unwrap_or(0)
}

fn read_exporter_state(path: &Path) -> Result<BTreeMap<String, BaseState>> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let value = read_json_file(path)?;
    let mut out = BTreeMap::new();
    if let Some(object) = value.as_object() {
        for (key, value) in object {
            if let Ok(state) = serde_json::from_value::<BaseState>(value.clone()) {
                out.insert(key.clone(), state);
            }
        }
    }
    Ok(out)
}

fn write_json_lines(path: &Path, rows: &[Value]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = File::create(path)?;
    for row in rows {
        serde_json::to_writer(&mut file, row)?;
        file.write_all(b"\n")?;
    }
    Ok(())
}

fn append_log(path: &Path, message: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(
        file,
        "{} {}",
        Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
        message
    )?;
    Ok(())
}

fn read_json_file(path: &Path) -> Result<Value> {
    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(raw.trim_start_matches('\u{feff}'))
        .with_context(|| format!("parse JSON {}", path.display()))
}

fn save_json_file<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    fs::rename(&tmp, path).or_else(|_| {
        fs::copy(&tmp, path)?;
        fs::remove_file(&tmp)?;
        Ok::<(), std::io::Error>(())
    })?;
    Ok(())
}

fn json_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut cursor = value;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    Some(cursor)
}

fn json_string(value: &Value, path: &[&str]) -> Option<String> {
    json_at(value, path)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn json_string_any(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .map(ToString::to_string)
}

fn json_i64_any(value: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| value.get(*key)?.as_i64())
}

fn json_bool_any(value: &Value, keys: &[&str]) -> Option<bool> {
    keys.iter().find_map(|key| value.get(*key)?.as_bool())
}

fn json_string_array(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn json_string_array_lower(value: &Value, key: &str) -> Vec<String> {
    json_string_array(value, key)
        .into_iter()
        .map(|item| item.to_ascii_lowercase())
        .collect()
}

fn json_i64(value: &Value, path: &[&str]) -> Option<i64> {
    json_at(value, path).and_then(Value::as_i64)
}

fn json_bool(value: &Value, path: &[&str]) -> Option<bool> {
    json_at(value, path).and_then(Value::as_bool)
}

fn last_successful_analytics_host(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let file = File::open(path)?;
    let mut matched = None;
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        if let Some(rest) = line.split("upload complete analyticsHost=").nth(1) {
            if let Some(host) = rest.split_whitespace().next() {
                matched = Some(host.to_string());
            }
        }
    }
    Ok(matched)
}

fn file_len(path: &Path) -> Result<u64> {
    Ok(fs::metadata(path)?.len())
}

fn file_modified(path: &Path) -> Result<SystemTime> {
    Ok(fs::metadata(path)?.modified()?)
}

fn modified_utc(path: &Path) -> Result<DateTime<Utc>> {
    Ok(DateTime::<Utc>::from(file_modified(path)?))
}

fn system_time_o(time: SystemTime) -> String {
    DateTime::<Utc>::from(time).to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn utc_compact() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn stable_doc_id(input: &str) -> String {
    general_purpose::STANDARD
        .encode(input.as_bytes())
        .trim_end_matches('=')
        .replace('/', "_")
        .replace('+', "-")
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn system32_path(relative: &str) -> PathBuf {
    env::var("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(r"C:\Windows"))
        .join("System32")
        .join(relative)
}

fn increment_json_i64(value: &mut Value, key: &str, delta: i64) {
    let current = value.get(key).and_then(Value::as_i64).unwrap_or(0);
    value[key] = Value::from(current + delta);
}

fn push_json_string(value: &mut Value, key: &str, item: String) {
    if !value.get(key).is_some_and(Value::is_array) {
        value[key] = Value::Array(Vec::new());
    }
    value
        .get_mut(key)
        .and_then(Value::as_array_mut)
        .expect("array ensured")
        .push(Value::String(item));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_file_infobases_from_v8i() {
        let text = r#"[База 1]
ID=abc-123
Connect=File="E:\Bases\Org\Base1";

[ServerBase]
ID=skip
Connect=Srvr="srv";Ref="x";
"#;
        let items = parse_v8i_text(text, "fixture-user", Path::new("ibases.v8i"));
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].infobase, "База 1");
        assert_eq!(items[0].base_id.as_deref(), Some("abc-123"));
        assert_eq!(items[0].path, PathBuf::from(r"E:\Bases\Org\Base1"));
    }

    #[test]
    fn activity_score_bootstrap_never_zero() {
        assert_eq!(
            company_activity_score(0.0, 0.0, 0, false, false, "online", true),
            1.0
        );
        assert_eq!(
            company_activity_score(1.234, 2.0, 1, true, true, "busy", false),
            26.23
        );
    }

    #[test]
    fn stable_doc_id_matches_powershell_shape() {
        let id = stable_doc_id(r"E:\Bases\Org\Base1");
        assert!(!id.contains('/'));
        assert!(!id.contains('+'));
        assert!(!id.ends_with('='));
    }

    #[test]
    fn uploaded_state_detects_same_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.png");
        fs::write(&file, b"png").unwrap();
        let metadata = fs::metadata(&file).unwrap();
        let sha = hex_sha256(b"png");
        let mut state = json!({"uploaded": {}});
        mark_uploaded(&mut state, &sha, &file, &metadata, true).unwrap();
        assert!(already_uploaded(&state, &sha, &file, &metadata));
    }

    #[test]
    fn reads_json_with_utf8_bom() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.json");
        fs::write(&file, "\u{feff}{\"ok\":true}").unwrap();
        let value = read_json_file(&file).unwrap();
        assert_eq!(value.get("ok").and_then(Value::as_bool), Some(true));
    }

    #[test]
    fn dlp_evidence_sync_accepts_only_dlp_screenshot_names() {
        assert!(is_dlp_evidence_screenshot_name(
            "20260605_120000_001_HOST_user_sid4_clipboard_rule-secret.png"
        ));
        assert!(is_dlp_evidence_screenshot_name(
            "20260605_120000_001_HOST_user_sid4_usb_insert_usb-rule.png"
        ));
        assert!(is_dlp_evidence_screenshot_name(
            "20260605_120000_001_HOST_user_sid4_print_job_print-rule.png"
        ));
        assert!(is_dlp_evidence_screenshot_name(
            "20260605_120000_001_HOST_user_sid4_web_web-rule.png"
        ));
        assert!(!is_dlp_evidence_screenshot_name("1c-work-screenshot.png"));
        assert!(!is_dlp_evidence_screenshot_name(
            "20260605_120000_001_HOST_user_sid4_1c_activity.png"
        ));
        assert!(!is_dlp_evidence_screenshot_name("random.png"));
        assert!(!is_dlp_evidence_screenshot_name(
            "20260605_120000_001_HOST_user_sid4_clipboard_rule-secret.jpg"
        ));
    }

    #[test]
    fn browser_url_domain_category_matches_legacy_rules() {
        let rules = default_category_rules();
        let url = normalize_browser_url("docs.google.com/document/d/1").unwrap();
        let domain = host_from_url(&url).unwrap();
        let root = root_domain("a.b.example.co.uk");
        let category = web_category_for_domain(&domain, &rules);
        assert_eq!(url, "https://docs.google.com/document/d/1");
        assert_eq!(domain, "docs.google.com");
        assert_eq!(root, "example.co.uk");
        assert_eq!(category.name, "work_docs_collab");
        assert_eq!(category.group, "work");
        assert!(normalize_browser_url("new tab").is_none());
    }

    #[test]
    fn foreground_context_requires_real_window_signal() {
        assert!(!has_foreground_context(&ForegroundWindowContext::default()));
        assert!(has_foreground_context(&ForegroundWindowContext {
            process_id: 1000,
            ..ForegroundWindowContext::default()
        }));
        assert!(has_foreground_context(&ForegroundWindowContext {
            title: "1C".to_string(),
            ..ForegroundWindowContext::default()
        }));
    }

    #[test]
    fn dlp_block_is_suppressed_without_native_enforce() {
        let policy = dlp_policy_from_value(
            json!({
                "defaults": {"enabled": true},
                "nativeControls": {
                    "mode": "monitor",
                    "rollout": {"allowGlobalBlock": false},
                    "channels": {"clipboard": {"action": "audit"}}
                }
            }),
            "test",
        );
        let decision = resolve_dlp_effective_action(&policy, "block", "clipboard");
        assert_eq!(decision.requested_action, "block");
        assert_eq!(decision.action, "alert");
        assert!(decision.enforcement_suppressed);
    }

    #[test]
    fn endpoint_advanced_content_matches_legacy_packs() {
        let matches = advanced_content_matches("contact user@example.com", None, Some("contacts"));
        assert_eq!(matches.regex_matches.len(), 1);
        assert_eq!(
            matches.regex_matches[0].get("name").and_then(Value::as_str),
            Some("email")
        );
        let secrets = advanced_content_matches("password: secret123", None, Some("secrets"));
        assert_eq!(
            secrets.regex_matches[0].get("name").and_then(Value::as_str),
            Some("generic-password")
        );
    }

    #[test]
    fn validate_deployment_extracts_scheduled_task_action_xml() {
        let xml = r#"
<Task>
  <Actions Context="Author">
    <Exec>
      <Command>C:\Program Files\AWatch-rus\windows\aw-windows-telemetry.exe</Command>
      <Arguments>file1c-upload --config-path &quot;C:\ProgramData\AWatch-rus\deployment-config.json&quot;</Arguments>
    </Exec>
  </Actions>
</Task>
"#;
        assert_eq!(
            xml_tag_text(xml, "Command").as_deref(),
            Some(r"C:\Program Files\AWatch-rus\windows\aw-windows-telemetry.exe")
        );
        assert_eq!(
            xml_tag_text(xml, "Arguments").as_deref(),
            Some(
                r#"file1c-upload --config-path "C:\ProgramData\AWatch-rus\deployment-config.json""#
            )
        );
    }

    #[test]
    fn validate_deployment_matches_task_exe_and_args() {
        let task = json!({
            "present": true,
            "actionExec": r"C:\Program Files\AWatch-rus\windows\aw-windows-telemetry.exe",
            "actionArgs": r#"dlp-evidence-sync --state-path "C:\ProgramData\AWatch-rus\state.json""#
        });
        assert!(task_uses_exe_and_arg(
            &task,
            Path::new(r"C:\Program Files\AWatch-rus\windows\aw-windows-telemetry.exe"),
            "dlp-evidence-sync"
        ));
        assert!(!task_uses_exe_and_arg(
            &task,
            Path::new(r"C:\Program Files\AWatch-rus\windows\aw-windows-telemetry.exe"),
            "file1c-upload"
        ));
    }

    #[test]
    fn validate_deployment_parses_service_binary_path() {
        let text = r#"
[SC] QueryServiceConfig SUCCESS

SERVICE_NAME: AWatchRusCollectorGuard
        TYPE               : 10  WIN32_OWN_PROCESS
        START_TYPE         : 2   AUTO_START
        BINARY_PATH_NAME   : "C:\Program Files\AWatch-rus\windows\AWatchRusCollectorGuardService.exe" --exec "C:\Program Files\AWatch-rus\windows\aw-windows-telemetry.exe" --args "collector-guard --mode enforce"
"#;
        let path = parse_sc_binary_path(text).unwrap();
        assert!(path.contains("aw-windows-telemetry.exe"));
        assert!(path.contains("collector-guard"));
    }

    #[test]
    fn file_operations_payload_matches_legacy_shape() {
        let payload = file_ops_payload(
            "Created",
            Path::new(r"C:\Users\user\Downloads\archive.zip"),
            None,
            1024,
            "HOST-EXAMPLE",
            "user",
        );
        assert_eq!(
            payload.get("operation").and_then(Value::as_str),
            Some("Created")
        );
        assert_eq!(
            payload.get("extension").and_then(Value::as_str),
            Some(".zip")
        );
        assert_eq!(
            payload.get("archiveHint").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(payload.get("size").and_then(Value::as_u64), Some(1024));
    }

    #[test]
    fn file_operations_queue_token_is_filename_safe() {
        let token = queue_name_token(r"DOMAIN\operator", 3);
        assert!(token.ends_with("-s3"));
        assert!(!token.contains('\\'));
        assert!(
            token
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        );
        assert_eq!(queue_name_token("", 7), "-s7");
    }

    #[test]
    fn file_operations_pairs_split_rename_events() {
        let mut pending = None;
        let from = Event::new(EventKind::Modify(notify::event::ModifyKind::Name(
            RenameMode::From,
        )))
        .add_path(PathBuf::from(r"C:\tmp\old.txt"));
        assert!(file_ops_event_from_notify(&from, &mut pending).is_none());
        let to = Event::new(EventKind::Modify(notify::event::ModifyKind::Name(
            RenameMode::To,
        )))
        .add_path(PathBuf::from(r"C:\tmp\new.txt"));
        let event = file_ops_event_from_notify(&to, &mut pending).unwrap();
        assert_eq!(event.operation, "Renamed");
        assert_eq!(
            event.old_path.as_deref(),
            Some(Path::new(r"C:\tmp\old.txt"))
        );
        assert_eq!(event.path, PathBuf::from(r"C:\tmp\new.txt"));
        assert!(pending.is_none());
    }

    #[test]
    fn validate_deployment_classifies_powershell_runtime() {
        assert_eq!(
            classify_powershell_runtime(
                r#"powershell.exe -File C:\ProgramData\AWatch-rus\worktime-session-collector.ps1"#
            ),
            "worktime"
        );
        assert_eq!(
            classify_powershell_runtime(
                r#"powershell.exe -File C:\ProgramData\AWatch-rus\browser-domains-native-collector.ps1"#
            ),
            "browser"
        );
        assert_eq!(
            classify_powershell_runtime(
                r#"powershell.exe -File C:\ProgramData\AWatch-rus\dlp-endpoint-signals-collector.ps1"#
            ),
            "dlp_endpoint"
        );
        assert_eq!(
            classify_powershell_runtime("powershell.exe -EncodedCommand x"),
            "other"
        );
    }

    #[test]
    fn collector_guard_detects_legacy_collector_duplicates_by_session() {
        let processes = vec![
            ProcessInfo {
                name: Some("powershell.exe".to_string()),
                pid: Some(100),
                session_id: Some(3),
                created_unix_seconds: Some(10),
                command_line: Some(
                    r#"powershell.exe -File C:\ProgramData\AWatch-rus\file-operations-collector.ps1"#
                        .to_string(),
                ),
            },
            ProcessInfo {
                name: Some("powershell.exe".to_string()),
                pid: Some(200),
                session_id: Some(3),
                created_unix_seconds: Some(20),
                command_line: Some(
                    r#"powershell.exe -File C:\ProgramData\AWatch-rus\file-operations-collector.ps1"#
                        .to_string(),
                ),
            },
            ProcessInfo {
                name: Some("powershell.exe".to_string()),
                pid: Some(300),
                session_id: Some(4),
                created_unix_seconds: Some(30),
                command_line: Some(
                    r#"powershell.exe -File C:\ProgramData\AWatch-rus\file-operations-collector.ps1"#
                        .to_string(),
                ),
            },
            ProcessInfo {
                name: Some("powershell.exe".to_string()),
                pid: Some(400),
                session_id: Some(3),
                created_unix_seconds: Some(40),
                command_line: Some(
                    r#"powershell.exe -File C:\ProgramData\AWatch-rus\browser-domains-native-collector.ps1"#
                        .to_string(),
                ),
            },
        ];

        let duplicates = duplicate_legacy_collectors(&processes);
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].kind, "fileops");
        assert_eq!(duplicates[0].session_id, 3);
        assert_eq!(duplicates[0].pid, 100);
        assert_eq!(duplicates[0].keep_pid, 200);
        assert_eq!(active_legacy_collector_count(&processes), 4);
    }

    #[test]
    fn collector_guard_detects_missing_rust_fileops_by_session() {
        let processes = vec![
            ProcessInfo {
                name: Some("aw-windows-telemetry.exe".to_string()),
                pid: Some(100),
                session_id: Some(2),
                created_unix_seconds: Some(10),
                command_line: Some(
                    r#""C:\Program Files\AWatch-rus\windows\aw-windows-telemetry.exe" browser-domains-collector --config-path C:\ProgramData\AWatch-rus\deployment-config.json"#
                        .to_string(),
                ),
            },
            ProcessInfo {
                name: Some("aw-windows-telemetry.exe".to_string()),
                pid: Some(101),
                session_id: Some(2),
                created_unix_seconds: Some(11),
                command_line: Some(
                    r#""C:\Program Files\AWatch-rus\windows\aw-windows-telemetry.exe" dlp-endpoint-collector --config-path C:\ProgramData\AWatch-rus\deployment-config.json"#
                        .to_string(),
                ),
            },
            ProcessInfo {
                name: Some("aw-windows-telemetry.exe".to_string()),
                pid: Some(200),
                session_id: Some(3),
                created_unix_seconds: Some(20),
                command_line: Some(
                    r#""C:\Program Files\AWatch-rus\windows\aw-windows-telemetry.exe" browser-domains-collector --config-path C:\ProgramData\AWatch-rus\deployment-config.json"#
                        .to_string(),
                ),
            },
            ProcessInfo {
                name: Some("aw-windows-telemetry.exe".to_string()),
                pid: Some(201),
                session_id: Some(3),
                created_unix_seconds: Some(21),
                command_line: Some(
                    r#""C:\Program Files\AWatch-rus\windows\aw-windows-telemetry.exe" file-operations-collector --config-path C:\ProgramData\AWatch-rus\deployment-config.json"#
                        .to_string(),
                ),
            },
        ];

        assert_eq!(
            missing_rust_collector_sessions(
                &processes,
                "file-operations-collector",
                &["browser-domains-collector", "dlp-endpoint-collector"],
            ),
            vec![2]
        );
    }

    #[test]
    fn collector_guard_parses_live_and_disconnected_sessions() {
        let sessions = parse_query_user_sessions(
            r#"
 USERNAME              SESSIONNAME        ID  STATE   IDLE TIME  LOGON TIME
 user1                 rdp-tcp#5           5  Active      none   09.07.2026 10:00
 user2                                      2  Disc         3:14  07.07.2026 8:34
"#,
        );

        assert_eq!(sessions.len(), 2);
        assert_eq!(live_session_ids(&sessions), HashSet::from([5]));
        assert!(task_has_live_session(r"SHARKON2025\user1", &sessions));
        assert!(!task_has_live_session(r"SHARKON2025\user2", &sessions));
    }

    #[test]
    fn collector_guard_treats_named_rdp_session_as_live_when_state_is_localized() {
        let sessions = parse_query_user_sessions(
            r#"
 USERNAME              SESSIONNAME        ID  STATE   IDLE TIME  LOGON TIME
 user1                 rdp-tcp#12         12  ?????       none   09.07.2026 10:00
 user2                                      2  ?????       3:14  07.07.2026 8:34
"#,
        );

        assert_eq!(sessions.len(), 2);
        assert_eq!(live_session_ids(&sessions), HashSet::from([12]));
        assert!(task_has_live_session(r"SHARKON2025\user1", &sessions));
        assert!(!task_has_live_session(r"SHARKON2025\user2", &sessions));
    }

    #[test]
    fn collector_guard_stops_session_collectors_outside_live_sessions() {
        let processes = vec![
            ProcessInfo {
                name: Some("aw-watcher-afk.exe".to_string()),
                pid: Some(100),
                session_id: Some(2),
                created_unix_seconds: Some(10),
                command_line: Some("aw-watcher-afk.exe --host 10.10.10.13".to_string()),
            },
            ProcessInfo {
                name: Some("aw-windows-telemetry.exe".to_string()),
                pid: Some(101),
                session_id: Some(2),
                created_unix_seconds: Some(11),
                command_line: Some(
                    "aw-windows-telemetry.exe browser-domains-collector --mode enforce".to_string(),
                ),
            },
            ProcessInfo {
                name: Some("aw-watcher-window.exe".to_string()),
                pid: Some(200),
                session_id: Some(5),
                created_unix_seconds: Some(20),
                command_line: Some("aw-watcher-window.exe --host 10.10.10.13".to_string()),
            },
            ProcessInfo {
                name: Some("awatch-agent-rs.exe".to_string()),
                pid: Some(300),
                session_id: Some(0),
                created_unix_seconds: Some(30),
                command_line: Some("awatch-agent-rs.exe --config x".to_string()),
            },
        ];
        let stop_plan = non_live_session_collectors(&processes, &HashSet::from([5]));
        let pids = stop_plan
            .iter()
            .filter_map(|process| process.pid)
            .collect::<Vec<_>>();

        assert_eq!(pids, vec![100, 101]);
    }

    #[test]
    fn validate_deployment_parses_wmic_process_csv() {
        let csv = br#"Node,CommandLine,Name,ProcessId,SessionId
HOST,"powershell.exe -File C:\ProgramData\AWatch-rus\file-operations-collector.ps1",powershell.exe,123,4
HOST,"C:\ProgramData\AWatch-rus\agent\awatch-agent-rs.exe --config x",awatch-agent-rs.exe,456,0
HOST,,notepad.exe,789,4
"#;
        let processes = parse_wmic_process_csv(csv);
        assert_eq!(processes.len(), 2);
        let grouped = power_shell_runtime_by_kind(&processes);
        assert_eq!(grouped.get("fileops"), Some(&1));
        assert!(processes.iter().any(|process| {
            process
                .name
                .as_deref()
                .is_some_and(|name| name == "awatch-agent-rs.exe")
        }));
    }

    #[test]
    fn validate_deployment_parses_tasklist_fallback_csv() {
        let csv = br#""awatch-agent-rs.exe","7064","Services","0","12,000 K"
"powershell.exe","11688","Services","0","90,000 K"
"notepad.exe","10","Console","1","1,000 K"
"#;
        let processes = parse_tasklist_csv(csv);
        assert_eq!(processes.len(), 2);
        assert!(processes.iter().any(|process| {
            process
                .name
                .as_deref()
                .is_some_and(|name| name == "awatch-agent-rs.exe")
        }));
        assert!(processes.iter().any(|process| {
            process
                .name
                .as_deref()
                .is_some_and(|name| name == "powershell.exe")
        }));
    }
}
