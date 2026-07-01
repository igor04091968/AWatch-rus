use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use calamine::{Data, Reader, open_workbook_auto};
use chrono::{DateTime, NaiveDateTime, Utc};
use clap::Parser;
use filetime::{FileTime, set_file_times};
use fs2::FileExt;
use regex::Regex;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sha1::{Digest, Sha1};

const DATASETS: &[&str] = &[
    "documents",
    "postings",
    "business_events",
    "document_changes",
    "companies",
    "reglog",
    "audit",
    "host",
];

#[derive(Debug, Parser)]
#[command(about = "Rust AW-rus file-1C ingest cycle for ClickHouse")]
struct Cli {
    #[arg(long, env = "AW_1C_ROOT")]
    root: Option<PathBuf>,

    #[arg(long)]
    config: Option<PathBuf>,

    #[arg(long)]
    dataset: Option<String>,

    #[arg(long, default_value_t = 30, env = "AW_1C_INGEST_TIMEOUT_SECONDS")]
    timeout_seconds: u64,

    #[arg(long)]
    dry_run: bool,

    #[arg(long)]
    skip_post_refresh: bool,

    #[arg(long)]
    skip_briefs: bool,

    #[arg(long)]
    json: bool,
}

#[derive(Debug, Deserialize)]
struct RawConfig {
    clickhouse: ClickHouseConfig,
    landing: BTreeMap<String, PathBuf>,
    #[serde(default)]
    formats: BTreeMap<String, String>,
    archive_dir: Option<PathBuf>,
    #[serde(default)]
    delete_after_load: bool,
    #[serde(default = "default_min_file_age_seconds")]
    min_file_age_seconds: u64,
}

#[derive(Debug, Deserialize)]
struct ClickHouseConfig {
    host: String,
    #[serde(default = "default_clickhouse_port")]
    port: u16,
    #[serde(default = "default_clickhouse_user")]
    username: String,
    #[serde(default)]
    password: String,
    #[serde(default = "default_clickhouse_database")]
    database: String,
}

#[derive(Debug, Default)]
struct RunSummary {
    files_seen: usize,
    files_loaded: usize,
    rows_loaded: usize,
    files_skipped_young: usize,
    files_archived: usize,
    sql_statements: usize,
}

#[derive(Debug, Clone)]
struct DocumentMeta {
    company_entity_key: String,
    organization: String,
    department: String,
    document_id: String,
    document_number: String,
    document_type: String,
    operation_type: String,
    user: String,
    counterparty: String,
}

type DocumentIndex = HashMap<(String, String), DocumentMeta>;

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
        .clone()
        .or_else(|| std::env::var_os("AW_1C_ROOT").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("/opt/activitywatch/clickhouse-1c"));
    let config_path = cli
        .config
        .clone()
        .unwrap_or_else(|| root.join("etl/config.yml"));
    let lock_path = root.join(".ingest.lock");
    let lock_file =
        File::create(&lock_path).with_context(|| format!("open lock {}", lock_path.display()))?;
    if lock_file.try_lock_exclusive().is_err() {
        println!("ingest cycle already running");
        return Ok(0);
    }

    let config = load_config(&config_path)?;
    let client = ClickHouseClient::new(&config.clickhouse, cli.timeout_seconds)?;
    let mut summary = RunSummary::default();

    build_business_exports(&config, cli.dry_run, &mut summary)?;
    load_datasets(
        &config,
        &client,
        cli.dataset.as_deref(),
        cli.dry_run,
        &mut summary,
    )?;
    load_registry(&config, &client, &root, cli.dry_run, &mut summary)?;

    if !cli.dry_run {
        run_sql_file(
            &client,
            &root.join("detections/build_entity_timeline.sql"),
            &mut summary,
        )?;
        run_sql_file(
            &client,
            &root.join("clickhouse/init/04_company_intelligence.sql"),
            &mut summary,
        )?;
        if !cli.skip_post_refresh {
            run_optional_script(&root.join("ops/run_company_registry_bindings_refresh.sh"))?;
            run_optional_script(&root.join("ops/run_company_intelligence_refresh.sh"))?;
        }
        run_sql_file(
            &client,
            &root.join("detections/insert_detections.sql"),
            &mut summary,
        )?;
        run_sql_file(
            &client,
            &root.join("detections/open_cases_from_detections.sql"),
            &mut summary,
        )?;
        let security_inbox_schema = root.join("security/security_finding_inbox.sql");
        if security_inbox_schema.exists() {
            run_sql_file(&client, &security_inbox_schema, &mut summary)?;
        }
        if !cli.skip_briefs {
            let _ = run_optional_script(&root.join("ops/run_manager_brief.sh"));
            let _ = run_optional_script(&root.join("ops/run_recovery_brief.sh"));
        }
        let _ = run_optional_script(&root.join("ops/check_ingest_freshness.sh"));
    }

    if cli.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "ok": true,
                "dry_run": cli.dry_run,
                "files_seen": summary.files_seen,
                "files_loaded": summary.files_loaded,
                "rows_loaded": summary.rows_loaded,
                "files_skipped_young": summary.files_skipped_young,
                "files_archived": summary.files_archived,
                "sql_statements": summary.sql_statements,
            }))?
        );
    } else {
        println!(
            "aw-1c-ingest-rust ok dry_run={} files_loaded={} rows_loaded={} skipped_young={} sql_statements={}",
            cli.dry_run,
            summary.files_loaded,
            summary.rows_loaded,
            summary.files_skipped_young,
            summary.sql_statements
        );
    }
    Ok(0)
}

fn default_clickhouse_port() -> u16 {
    8123
}

fn default_clickhouse_user() -> String {
    "default".to_string()
}

fn default_clickhouse_database() -> String {
    "analytics_1c".to_string()
}

fn default_min_file_age_seconds() -> u64 {
    180
}

fn load_config(path: &Path) -> Result<RawConfig> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_yaml::from_str(&text).with_context(|| format!("parse {}", path.display()))
}

struct ClickHouseClient {
    client: Client,
    base_url: String,
    username: String,
    password: String,
    database: String,
}

impl ClickHouseClient {
    fn new(config: &ClickHouseConfig, timeout_seconds: u64) -> Result<Self> {
        let host = config.host.trim_end_matches('/');
        let base_url = if host.starts_with("http://") || host.starts_with("https://") {
            host.to_string()
        } else {
            format!("http://{}:{}", host, config.port)
        };
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_seconds))
            .build()
            .context("build ClickHouse HTTP client")?;
        Ok(Self {
            client,
            base_url,
            username: config.username.clone(),
            password: config.password.clone(),
            database: config.database.clone(),
        })
    }

    fn execute(&self, sql: &str) -> Result<()> {
        if sql.trim().is_empty() {
            return Ok(());
        }
        let mut req = self
            .client
            .post(&self.base_url)
            .query(&[("database", self.database.as_str())])
            .body(sql.to_string());
        if !self.username.trim().is_empty() {
            req = req.basic_auth(self.username.clone(), Some(self.password.clone()));
        }
        let response = req.send().context("ClickHouse request")?;
        let status = response.status();
        let body = response.text().unwrap_or_default();
        if !status.is_success() {
            bail!("ClickHouse HTTP {status}: {}", body.trim());
        }
        Ok(())
    }

    fn insert_json_each_row(&self, table: &str, columns: &[&str], rows: &[Value]) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let mut payload = String::new();
        payload.push_str(&format!(
            "INSERT INTO {} ({}) FORMAT JSONEachRow\n",
            table,
            columns.join(", ")
        ));
        for row in rows {
            payload.push_str(&serde_json::to_string(row)?);
            payload.push('\n');
        }
        self.execute(&payload)
    }
}

fn dataset_format(config: &RawConfig, dataset: &str) -> String {
    config
        .formats
        .get(dataset)
        .or_else(|| config.formats.get("default"))
        .cloned()
        .unwrap_or_else(|| "jsonl".to_string())
}

fn ready_files(config: &RawConfig, dataset: &str) -> Result<Vec<PathBuf>> {
    let Some(root) = config.landing.get(dataset) else {
        return Ok(Vec::new());
    };
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(root).with_context(|| format!("read {}", root.display()))? {
        let path = entry?.path();
        if path.is_file() {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

fn file_age_seconds(path: &Path) -> Result<u64> {
    let meta = fs::metadata(path).with_context(|| format!("stat {}", path.display()))?;
    let modified = meta
        .modified()
        .with_context(|| format!("mtime {}", path.display()))?;
    Ok(modified.elapsed().unwrap_or_default().as_secs())
}

fn file_is_ready(path: &Path, config: &RawConfig) -> Result<bool> {
    Ok(file_age_seconds(path)? >= config.min_file_age_seconds)
}

fn read_rows(path: &Path, fmt: &str) -> Result<Vec<Map<String, Value>>> {
    match fmt {
        "jsonl" => {
            let text = read_text_lossy(path)?;
            text.lines()
                .filter(|line| !line.trim().is_empty())
                .map(|line| parse_object_line(line, path))
                .collect()
        }
        "json" => {
            let value: Value = serde_json::from_str(&read_text_lossy(path)?)
                .with_context(|| format!("parse json {}", path.display()))?;
            match value {
                Value::Array(items) => items
                    .into_iter()
                    .map(|item| match item {
                        Value::Object(map) => Ok(map),
                        _ => bail!("json item is not object in {}", path.display()),
                    })
                    .collect(),
                Value::Object(map) => Ok(vec![map]),
                _ => bail!("json root is not object/list in {}", path.display()),
            }
        }
        "csv" => {
            let mut reader = csv::Reader::from_path(path)
                .with_context(|| format!("open csv {}", path.display()))?;
            let headers = reader.headers()?.clone();
            let mut rows = Vec::new();
            for record in reader.records() {
                let record = record?;
                let mut map = Map::new();
                for (idx, value) in record.iter().enumerate() {
                    if let Some(header) = headers.get(idx) {
                        map.insert(header.to_string(), Value::String(value.to_string()));
                    }
                }
                rows.push(map);
            }
            Ok(rows)
        }
        other => bail!("unsupported format {other} for {}", path.display()),
    }
}

fn read_text_lossy(path: &Path) -> Result<String> {
    let mut bytes = Vec::new();
    File::open(path)
        .with_context(|| format!("open {}", path.display()))?
        .read_to_end(&mut bytes)?;
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        bytes.drain(0..3);
    }
    String::from_utf8(bytes).with_context(|| format!("decode utf-8 {}", path.display()))
}

fn parse_object_line(line: &str, path: &Path) -> Result<Map<String, Value>> {
    match serde_json::from_str::<Value>(line)
        .with_context(|| format!("parse jsonl {}", path.display()))?
    {
        Value::Object(map) => Ok(map),
        _ => bail!("jsonl row is not object in {}", path.display()),
    }
}

fn load_datasets(
    config: &RawConfig,
    client: &ClickHouseClient,
    only_dataset: Option<&str>,
    dry_run: bool,
    summary: &mut RunSummary,
) -> Result<()> {
    let datasets: Vec<&str> = match only_dataset {
        Some(dataset) => vec![dataset],
        None => DATASETS.to_vec(),
    };
    for dataset in datasets {
        let fmt = dataset_format(config, dataset);
        for path in ready_files(config, dataset)? {
            summary.files_seen += 1;
            if !file_is_ready(&path, config)? {
                summary.files_skipped_young += 1;
                println!(
                    "skip {dataset}: {} age={}s < min_file_age_seconds={}",
                    path.file_name().and_then(|v| v.to_str()).unwrap_or("-"),
                    file_age_seconds(&path)?,
                    config.min_file_age_seconds
                );
                continue;
            }
            let rows = read_rows(&path, &fmt)?;
            if rows.is_empty() {
                archive_or_delete(config, dataset, &path, dry_run, summary)?;
                continue;
            }
            if !dry_run {
                let source_file = file_name(&path);
                let raw_rows: Vec<Value> = rows
                    .iter()
                    .map(|row| {
                        json!({
                            "source_file": source_file,
                            "payload": serde_json::to_string(row).unwrap_or_else(|_| "{}".to_string()),
                        })
                    })
                    .collect();
                client.insert_json_each_row(
                    raw_table(dataset)?,
                    &["source_file", "payload"],
                    &raw_rows,
                )?;
                let core_rows: Vec<Value> = rows
                    .iter()
                    .map(|row| map_core_row(dataset, &source_file, row))
                    .collect::<Result<Vec<_>>>()?;
                client.insert_json_each_row(
                    core_table(dataset)?,
                    core_columns(dataset)?,
                    &core_rows,
                )?;
            }
            summary.files_loaded += 1;
            summary.rows_loaded += rows.len();
            archive_or_delete(config, dataset, &path, dry_run, summary)?;
            println!("loaded {dataset}: {} rows={}", file_name(&path), rows.len());
        }
    }
    Ok(())
}

fn raw_table(dataset: &str) -> Result<&'static str> {
    Ok(match dataset {
        "documents" => "raw_1c_documents",
        "postings" => "raw_1c_postings",
        "business_events" => "raw_1c_business_events",
        "document_changes" => "raw_1c_document_changes",
        "companies" => "raw_1c_companies",
        "reglog" => "raw_reglog",
        "audit" => "raw_audit",
        "host" => "raw_host_metrics",
        _ => bail!("unknown dataset {dataset}"),
    })
}

fn core_table(dataset: &str) -> Result<&'static str> {
    Ok(match dataset {
        "documents" => "documents",
        "postings" => "postings",
        "business_events" => "business_events",
        "document_changes" => "document_change_events",
        "companies" => "companies",
        "reglog" => "reglog_events",
        "audit" => "audit_events",
        "host" => "host_events",
        _ => bail!("unknown dataset {dataset}"),
    })
}

fn core_columns(dataset: &str) -> Result<&'static [&'static str]> {
    Ok(match dataset {
        "documents" => &[
            "ts",
            "infobase",
            "organization",
            "department",
            "doc_type",
            "doc_id",
            "doc_number",
            "author",
            "counterparty",
            "operation_type",
            "amount",
            "status",
            "posted",
            "source_file",
        ],
        "postings" => &[
            "ts",
            "infobase",
            "registrar",
            "operation_type",
            "account_dt",
            "account_ct",
            "amount",
            "source_file",
        ],
        "business_events" => &[
            "ts",
            "event_id",
            "infobase",
            "company_entity_key",
            "organization",
            "department",
            "document_id",
            "document_number",
            "document_type",
            "registrar",
            "operation_type",
            "event_kind",
            "user",
            "counterparty",
            "counterparty_inn",
            "debit_account",
            "credit_account",
            "amount",
            "currency",
            "line_no",
            "evidence_ref",
            "source_file",
        ],
        "document_changes" => &[
            "ts",
            "change_id",
            "infobase",
            "company_entity_key",
            "organization",
            "document_id",
            "document_number",
            "document_type",
            "change_kind",
            "field_name",
            "user",
            "before_value",
            "after_value",
            "risk_tag",
            "evidence_ref",
            "source_file",
        ],
        "companies" => &[
            "ts",
            "infobase",
            "company_name",
            "organization",
            "owner_user",
            "base_id",
            "base_path",
            "status",
            "db_size_bytes",
            "reglog_size_bytes",
            "active_locks",
            "temp_db_present",
            "scheduler_touched",
            "activity_score",
            "source_file",
        ],
        "reglog" => &[
            "ts",
            "infobase",
            "user",
            "host",
            "app",
            "event_name",
            "level",
            "duration_ms",
            "message",
            "source_file",
        ],
        "audit" => &[
            "ts",
            "infobase",
            "user",
            "object_type",
            "object_id",
            "action",
            "before_hash",
            "after_hash",
            "risk_tag",
            "source_file",
        ],
        "host" => &[
            "ts",
            "host",
            "cpu_pct",
            "ram_pct",
            "disk_free_gb",
            "disk_latency_ms",
            "smb_errors",
            "rdp_sessions",
            "backup_ok",
            "source_file",
        ],
        _ => bail!("unknown dataset {dataset}"),
    })
}

fn map_core_row(dataset: &str, source_file: &str, row: &Map<String, Value>) -> Result<Value> {
    let value = match dataset {
        "documents" => json!({
            "ts": ts_value(row, &["ts", "posted_at", "created_at"]),
            "infobase": s(row, "infobase"),
            "organization": s(row, "organization"),
            "department": s(row, "department"),
            "doc_type": s(row, "doc_type"),
            "doc_id": s(row, "doc_id"),
            "doc_number": s(row, "doc_number"),
            "author": s(row, "author"),
            "counterparty": s(row, "counterparty"),
            "operation_type": s(row, "operation_type"),
            "amount": f(row, "amount"),
            "status": s(row, "status"),
            "posted": u(row, "posted"),
            "source_file": source_file,
        }),
        "postings" => json!({
            "ts": ts_value(row, &["ts"]),
            "infobase": s(row, "infobase"),
            "registrar": s(row, "registrar"),
            "operation_type": s(row, "operation_type"),
            "account_dt": s(row, "account_dt"),
            "account_ct": s(row, "account_ct"),
            "amount": f(row, "amount"),
            "source_file": source_file,
        }),
        "business_events" => json!({
            "ts": ts_value(row, &["ts", "event_time"]),
            "event_id": s(row, "event_id"),
            "infobase": s(row, "infobase"),
            "company_entity_key": s(row, "company_entity_key"),
            "organization": s(row, "organization"),
            "department": s(row, "department"),
            "document_id": s_alt(row, &["document_id", "doc_id"]),
            "document_number": s_alt(row, &["document_number", "doc_number"]),
            "document_type": s_alt(row, &["document_type", "doc_type"]),
            "registrar": s(row, "registrar"),
            "operation_type": s(row, "operation_type"),
            "event_kind": s(row, "event_kind"),
            "user": s_alt(row, &["user", "author"]),
            "counterparty": s(row, "counterparty"),
            "counterparty_inn": s(row, "counterparty_inn"),
            "debit_account": s_alt(row, &["debit_account", "account_dt"]),
            "credit_account": s_alt(row, &["credit_account", "account_ct"]),
            "amount": f(row, "amount"),
            "currency": s_default(row, "currency", "RUB"),
            "line_no": u(row, "line_no"),
            "evidence_ref": s(row, "evidence_ref"),
            "source_file": source_file,
        }),
        "document_changes" => json!({
            "ts": ts_value(row, &["ts", "change_time"]),
            "change_id": s(row, "change_id"),
            "infobase": s(row, "infobase"),
            "company_entity_key": s(row, "company_entity_key"),
            "organization": s(row, "organization"),
            "document_id": s_alt(row, &["document_id", "doc_id"]),
            "document_number": s_alt(row, &["document_number", "doc_number"]),
            "document_type": s_alt(row, &["document_type", "doc_type"]),
            "change_kind": s(row, "change_kind"),
            "field_name": s(row, "field_name"),
            "user": s_alt(row, &["user", "author"]),
            "before_value": s(row, "before_value"),
            "after_value": s(row, "after_value"),
            "risk_tag": s(row, "risk_tag"),
            "evidence_ref": s(row, "evidence_ref"),
            "source_file": source_file,
        }),
        "companies" => json!({
            "ts": ts_value(row, &["ts"]),
            "infobase": s(row, "infobase"),
            "company_name": s_alt(row, &["company_name", "counterparty", "infobase"]),
            "organization": s(row, "organization"),
            "owner_user": s_alt(row, &["owner_user", "author"]),
            "base_id": s_alt(row, &["base_id", "doc_id"]),
            "base_path": s(row, "base_path"),
            "status": s(row, "status"),
            "db_size_bytes": u(row, "db_size_bytes"),
            "reglog_size_bytes": u(row, "reglog_size_bytes"),
            "active_locks": u(row, "active_locks"),
            "temp_db_present": u(row, "temp_db_present"),
            "scheduler_touched": u(row, "scheduler_touched"),
            "activity_score": f_alt(row, &["activity_score", "amount"]),
            "source_file": source_file,
        }),
        "reglog" => json!({
            "ts": ts_value(row, &["ts"]),
            "infobase": s(row, "infobase"),
            "user": s(row, "user"),
            "host": s(row, "host"),
            "app": s(row, "app"),
            "event_name": s(row, "event_name"),
            "level": s_default(row, "level", "info"),
            "duration_ms": u(row, "duration_ms"),
            "message": s(row, "message"),
            "source_file": source_file,
        }),
        "audit" => json!({
            "ts": ts_value(row, &["ts"]),
            "infobase": s(row, "infobase"),
            "user": s(row, "user"),
            "object_type": s(row, "object_type"),
            "object_id": s(row, "object_id"),
            "action": s(row, "action"),
            "before_hash": s(row, "before_hash"),
            "after_hash": s(row, "after_hash"),
            "risk_tag": s(row, "risk_tag"),
            "source_file": source_file,
        }),
        "host" => json!({
            "ts": ts_value(row, &["ts"]),
            "host": s(row, "host"),
            "cpu_pct": f(row, "cpu_pct"),
            "ram_pct": f(row, "ram_pct"),
            "disk_free_gb": f(row, "disk_free_gb"),
            "disk_latency_ms": f(row, "disk_latency_ms"),
            "smb_errors": u(row, "smb_errors"),
            "rdp_sessions": u(row, "rdp_sessions"),
            "backup_ok": u(row, "backup_ok"),
            "source_file": source_file,
        }),
        _ => bail!("unknown dataset {dataset}"),
    };
    Ok(value)
}

fn build_business_exports(
    config: &RawConfig,
    dry_run: bool,
    summary: &mut RunSummary,
) -> Result<()> {
    let company_index = load_company_index(config)?;
    let (by_id, by_number) = build_document_index(config, &company_index)?;

    for path in ready_files(config, "documents")? {
        if !file_is_ready(&path, config)? {
            continue;
        }
        let rows = read_rows(&path, &dataset_format(config, "documents"))?;
        let events = build_document_events(&rows, &file_name(&path), &company_index);
        let out = output_path(
            config,
            "business_events",
            &path,
            "business-events-documents",
        )?;
        if !dry_run {
            write_generated_jsonl(&out, &events, config.min_file_age_seconds)?;
        }
        summary.files_seen += 1;
    }
    for path in ready_files(config, "postings")? {
        if !file_is_ready(&path, config)? {
            continue;
        }
        let rows = read_rows(&path, &dataset_format(config, "postings"))?;
        let events =
            build_posting_events(&rows, &file_name(&path), &company_index, &by_id, &by_number);
        let out = output_path(config, "business_events", &path, "business-events-postings")?;
        if !dry_run {
            write_generated_jsonl(&out, &events, config.min_file_age_seconds)?;
        }
        summary.files_seen += 1;
    }
    for path in ready_files(config, "audit")? {
        if !file_is_ready(&path, config)? {
            continue;
        }
        let rows = read_rows(&path, &dataset_format(config, "audit"))?;
        let events =
            build_document_changes(&rows, &file_name(&path), &company_index, &by_id, &by_number);
        let out = output_path(config, "document_changes", &path, "document-changes-audit")?;
        if !dry_run {
            write_generated_jsonl(&out, &events, config.min_file_age_seconds)?;
        }
        summary.files_seen += 1;
    }
    Ok(())
}

fn load_company_index(config: &RawConfig) -> Result<HashMap<String, String>> {
    let mut latest: HashMap<String, (String, String)> = HashMap::new();
    for path in ready_files(config, "companies")? {
        if !file_is_ready(&path, config)? {
            continue;
        }
        for row in read_rows(&path, &dataset_format(config, "companies"))? {
            let infobase = s(&row, "infobase");
            if infobase.is_empty() {
                continue;
            }
            let key =
                canonical_company_entity_key(&s(&row, "base_id"), &s(&row, "base_path"), &infobase);
            let ts = ts_value(&row, &["ts"]);
            match latest.get(&infobase) {
                Some((current_ts, _)) if current_ts > &ts => {}
                _ => {
                    latest.insert(infobase, (ts, key));
                }
            }
        }
    }
    Ok(latest.into_iter().map(|(k, (_, v))| (k, v)).collect())
}

fn build_document_index(
    config: &RawConfig,
    company_index: &HashMap<String, String>,
) -> Result<(DocumentIndex, DocumentIndex)> {
    let mut by_id = HashMap::new();
    let mut by_number = HashMap::new();
    for path in ready_files(config, "documents")? {
        if !file_is_ready(&path, config)? {
            continue;
        }
        for row in read_rows(&path, &dataset_format(config, "documents"))? {
            let infobase = s(&row, "infobase");
            if infobase.is_empty() {
                continue;
            }
            let document_id = s_alt(&row, &["doc_id", "document_id"]);
            let document_number = s_alt(&row, &["doc_number", "document_number"]);
            let meta = DocumentMeta {
                company_entity_key: derive_company_entity_key(&row, company_index),
                organization: s(&row, "organization"),
                department: s(&row, "department"),
                document_id: document_id.clone(),
                document_number: document_number.clone(),
                document_type: s_alt(&row, &["doc_type", "document_type"]),
                operation_type: s(&row, "operation_type"),
                user: s_alt(&row, &["author", "user"]),
                counterparty: s(&row, "counterparty"),
            };
            if !document_id.is_empty() {
                by_id.insert((infobase.clone(), document_id), meta.clone());
            }
            if !document_number.is_empty() {
                by_number.insert((infobase, document_number), meta);
            }
        }
    }
    Ok((by_id, by_number))
}

fn build_document_events(
    rows: &[Map<String, Value>],
    source_file: &str,
    company_index: &HashMap<String, String>,
) -> Vec<Value> {
    rows.iter()
        .map(|row| {
            let ts = ts_value(row, &["ts", "posted_at", "created_at"]);
            let infobase = s(row, "infobase");
            let document_id = s_alt(row, &["doc_id", "document_id"]);
            let document_number = s_alt(row, &["doc_number", "document_number"]);
            json!({
                "ts": ts,
                "event_id": stable_id("document_snapshot", &[source_file, &infobase, &document_id, &document_number, &ts]),
                "infobase": infobase,
                "company_entity_key": derive_company_entity_key(row, company_index),
                "organization": s(row, "organization"),
                "department": s(row, "department"),
                "document_id": document_id,
                "document_number": document_number,
                "document_type": s_alt(row, &["doc_type", "document_type"]),
                "registrar": s_alt(row, &["doc_id", "doc_number", "document_id", "document_number"]),
                "operation_type": s(row, "operation_type"),
                "event_kind": "document_snapshot",
                "user": s_alt(row, &["author", "user"]),
                "counterparty": s(row, "counterparty"),
                "counterparty_inn": s(row, "counterparty_inn"),
                "debit_account": "",
                "credit_account": "",
                "amount": f(row, "amount"),
                "currency": s_default(row, "currency", "RUB"),
                "line_no": 0,
                "evidence_ref": format!("document:{}", s_alt(row, &["doc_id", "doc_number", "document_id", "document_number"])),
            })
        })
        .collect()
}

fn build_posting_events(
    rows: &[Map<String, Value>],
    source_file: &str,
    company_index: &HashMap<String, String>,
    by_id: &HashMap<(String, String), DocumentMeta>,
    by_number: &HashMap<(String, String), DocumentMeta>,
) -> Vec<Value> {
    rows.iter()
        .enumerate()
        .map(|(idx, row)| {
            let ts = ts_value(row, &["ts"]);
            let infobase = s(row, "infobase");
            let registrar = s(row, "registrar");
            let meta = lookup_document_meta(row, by_id, by_number);
            let line_no = u_alt(row, &["line_no"]).unwrap_or((idx + 1) as u64);
            json!({
                "ts": ts,
                "event_id": stable_id("posting", &[source_file, &infobase, &registrar, &line_no.to_string(), &ts]),
                "infobase": infobase,
                "company_entity_key": meta.map(|m| m.company_entity_key.clone()).unwrap_or_else(|| derive_company_entity_key(row, company_index)),
                "organization": meta.map(|m| m.organization.clone()).unwrap_or_else(|| s(row, "organization")),
                "department": meta.map(|m| m.department.clone()).unwrap_or_else(|| s(row, "department")),
                "document_id": meta.map(|m| m.document_id.clone()).unwrap_or_else(|| registrar.clone()),
                "document_number": meta.map(|m| m.document_number.clone()).unwrap_or_else(|| s(row, "document_number")),
                "document_type": meta.map(|m| m.document_type.clone()).unwrap_or_else(|| s(row, "document_type")),
                "registrar": registrar,
                "operation_type": s_default(row, "operation_type", &meta.map(|m| m.operation_type.clone()).unwrap_or_default()),
                "event_kind": "posting",
                "user": meta.map(|m| m.user.clone()).unwrap_or_else(|| s_alt(row, &["user", "author"])),
                "counterparty": meta.map(|m| m.counterparty.clone()).unwrap_or_else(|| s(row, "counterparty")),
                "counterparty_inn": s(row, "counterparty_inn"),
                "debit_account": s_alt(row, &["account_dt", "debit_account"]),
                "credit_account": s_alt(row, &["account_ct", "credit_account"]),
                "amount": f(row, "amount"),
                "currency": s_default(row, "currency", "RUB"),
                "line_no": line_no,
                "evidence_ref": format!("posting:{}:{}", s(row, "registrar"), line_no),
            })
        })
        .collect()
}

fn build_document_changes(
    rows: &[Map<String, Value>],
    source_file: &str,
    company_index: &HashMap<String, String>,
    by_id: &HashMap<(String, String), DocumentMeta>,
    by_number: &HashMap<(String, String), DocumentMeta>,
) -> Vec<Value> {
    rows.iter()
        .map(|row| {
            let ts = ts_value(row, &["ts"]);
            let infobase = s(row, "infobase");
            let object_type = s(row, "object_type");
            let object_id = s(row, "object_id");
            let meta = lookup_document_meta(row, by_id, by_number);
            json!({
                "ts": ts,
                "change_id": stable_id("change", &[source_file, &infobase, &object_type, &object_id, &s(row, "action"), &ts]),
                "infobase": infobase,
                "company_entity_key": meta.map(|m| m.company_entity_key.clone()).unwrap_or_else(|| derive_company_entity_key(row, company_index)),
                "organization": meta.map(|m| m.organization.clone()).unwrap_or_else(|| s(row, "organization")),
                "document_id": meta.map(|m| m.document_id.clone()).unwrap_or_else(|| if object_type == "document" { object_id.clone() } else { s(row, "document_id") }),
                "document_number": meta.map(|m| m.document_number.clone()).unwrap_or_else(|| s(row, "document_number")),
                "document_type": meta.map(|m| m.document_type.clone()).unwrap_or_else(|| s(row, "document_type")),
                "change_kind": s_alt(row, &["change_kind", "action", "object_type"]),
                "field_name": s_alt(row, &["field_name", "object_type"]),
                "user": s_alt(row, &["user", "author"]),
                "before_value": s_alt(row, &["before_value", "before_hash"]),
                "after_value": s_alt(row, &["after_value", "after_hash"]),
                "risk_tag": s(row, "risk_tag"),
                "evidence_ref": format!("audit:{}:{}", object_type, object_id),
            })
        })
        .collect()
}

fn lookup_document_meta<'a>(
    row: &Map<String, Value>,
    by_id: &'a HashMap<(String, String), DocumentMeta>,
    by_number: &'a HashMap<(String, String), DocumentMeta>,
) -> Option<&'a DocumentMeta> {
    let infobase = s(row, "infobase");
    let registrar = s_alt(row, &["registrar", "document_id", "doc_id"]);
    let number = s_alt(row, &["document_number", "doc_number"]);
    by_id
        .get(&(infobase.clone(), registrar))
        .or_else(|| by_number.get(&(infobase, number)))
}

fn output_path(config: &RawConfig, dataset: &str, source: &Path, prefix: &str) -> Result<PathBuf> {
    let root = config
        .landing
        .get(dataset)
        .ok_or_else(|| anyhow!("missing landing root for {dataset}"))?;
    let stem = source
        .file_stem()
        .and_then(|v| v.to_str())
        .unwrap_or("input");
    Ok(root.join(format!("{prefix}-{stem}.jsonl")))
}

fn write_generated_jsonl(path: &Path, rows: &[Value], min_file_age_seconds: u64) -> Result<()> {
    if rows.is_empty() {
        let _ = fs::remove_file(path);
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut tmp = tempfile::NamedTempFile::new_in(path.parent().unwrap_or_else(|| Path::new(".")))?;
    for row in rows {
        writeln!(tmp, "{}", serde_json::to_string(row)?)?;
    }
    tmp.persist(path)
        .map_err(|err| anyhow!("persist {}: {}", path.display(), err))?;
    let aged = FileTime::from_unix_time(
        Utc::now().timestamp() - (min_file_age_seconds as i64 + 1).max(5),
        0,
    );
    set_file_times(path, aged, aged).with_context(|| format!("set mtime {}", path.display()))?;
    Ok(())
}

fn load_registry(
    config: &RawConfig,
    client: &ClickHouseClient,
    root: &Path,
    dry_run: bool,
    summary: &mut RunSummary,
) -> Result<()> {
    let landing = root.join("landing/registry");
    if !landing.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(&landing)? {
        let path = entry?.path();
        if !path.is_file()
            || path
                .extension()
                .and_then(|v| v.to_str())
                .unwrap_or("")
                .to_lowercase()
                != "xlsx"
        {
            continue;
        }
        summary.files_seen += 1;
        if !file_is_ready(&path, config)? {
            summary.files_skipped_young += 1;
            continue;
        }
        let rows = parse_registry_xlsx(&path)?;
        if !dry_run && !rows.is_empty() {
            let raw_rows: Vec<Value> = rows
                .iter()
                .map(|row| {
                    json!({
                        "source_file": row.get("source_file").and_then(Value::as_str).unwrap_or(""),
                        "source_sheet": row.get("source_sheet").and_then(Value::as_str).unwrap_or(""),
                        "payload": serde_json::to_string(row).unwrap_or_else(|_| "{}".to_string()),
                    })
                })
                .collect();
            client.insert_json_each_row(
                "raw_1c_company_registry",
                &["source_file", "source_sheet", "payload"],
                &raw_rows,
            )?;
            client.insert_json_each_row(
                "company_registry",
                &[
                    "ts",
                    "source_file",
                    "source_sheet",
                    "company_name",
                    "company_key",
                    "assignee_name",
                    "registry_status",
                    "share_text",
                    "key_contour",
                    "inn",
                    "kpp",
                ],
                &rows,
            )?;
        }
        summary.files_loaded += 1;
        summary.rows_loaded += rows.len();
        archive_or_delete(config, "registry", &path, dry_run, summary)?;
        println!("loaded registry: {} rows={}", file_name(&path), rows.len());
    }
    Ok(())
}

fn parse_registry_xlsx(path: &Path) -> Result<Vec<Value>> {
    let mut workbook =
        open_workbook_auto(path).with_context(|| format!("open xlsx {}", path.display()))?;
    let now = Utc::now()
        .naive_utc()
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let mut tax_map: HashMap<String, (String, String)> = HashMap::new();
    if let Ok(range) = workbook.worksheet_range("Лист2") {
        for row in range.rows().skip(2) {
            let company_name = cell_text(row.get(1));
            if company_name.is_empty() {
                continue;
            }
            tax_map.insert(
                normalize_company_key(&company_name),
                (cell_text(row.get(2)), cell_text(row.get(3))),
            );
        }
    }
    let mut out = Vec::new();
    let Ok(range) = workbook.worksheet_range("ОСНОВНОЙ") else {
        return Ok(out);
    };
    let rows: Vec<_> = range.rows().collect();
    if rows.len() < 3 {
        return Ok(out);
    }
    let top_headers: Vec<String> = rows[0].iter().map(|v| cell_text(Some(v))).collect();
    let manager_headers: Vec<String> = rows[1].iter().map(|v| cell_text(Some(v))).collect();
    let mut specs = Vec::<(usize, String, Option<usize>, String, String)>::new();
    let mut current: Option<(usize, String, String)> = None;
    for idx in 1..manager_headers.len() {
        let manager = manager_headers.get(idx).cloned().unwrap_or_default();
        let top = top_headers.get(idx).cloned().unwrap_or_default();
        if !manager.is_empty() {
            current = Some((idx, manager.clone(), "active".to_string()));
            specs.push((idx, manager, None, String::new(), "active".to_string()));
        } else if top.to_lowercase().contains("исключ") {
            current = Some((idx, String::new(), "excluded".to_string()));
            specs.push((
                idx,
                String::new(),
                None,
                String::new(),
                "excluded".to_string(),
            ));
        } else if let Some((col, assignee, status)) = current.take() {
            if let Some(last) = specs.last_mut() {
                if last.0 == col && last.1 == assignee && last.4 == status {
                    last.2 = Some(idx);
                    last.3 = top;
                }
            }
        }
    }
    for row in rows.into_iter().skip(2) {
        for (col, assignee, meta_col, meta_label, status) in &specs {
            let company_name = cell_text(row.get(*col));
            if company_name.is_empty() {
                continue;
            }
            let meta = meta_col
                .map(|idx| cell_text(row.get(idx)))
                .unwrap_or_default();
            let company_key = normalize_company_key(&company_name);
            let (inn, kpp) = tax_map.get(&company_key).cloned().unwrap_or_default();
            let key_contour = if meta.to_uppercase() == "ЕСТЬ"
                && meta_label.to_lowercase().contains("ключ")
            {
                1
            } else {
                0
            };
            out.push(json!({
                "ts": now,
                "source_file": file_name(path),
                "source_sheet": "ОСНОВНОЙ",
                "company_name": company_name,
                "company_key": company_key,
                "assignee_name": assignee,
                "registry_status": status,
                "share_text": if meta_label.to_lowercase().contains("ключ") { "" } else { &meta },
                "key_contour": key_contour,
                "inn": inn,
                "kpp": kpp,
            }));
        }
    }
    Ok(out)
}

fn cell_text(value: Option<&Data>) -> String {
    match value {
        Some(Data::String(v)) => v.trim().to_string(),
        Some(Data::Float(v)) => {
            if v.fract() == 0.0 {
                format!("{}", *v as i64)
            } else {
                v.to_string()
            }
        }
        Some(Data::Int(v)) => v.to_string(),
        Some(Data::Bool(v)) => v.to_string(),
        Some(Data::DateTime(v)) => v.to_string(),
        Some(Data::DateTimeIso(v)) => v.trim().to_string(),
        Some(Data::DurationIso(v)) => v.trim().to_string(),
        Some(Data::Empty) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

fn archive_or_delete(
    config: &RawConfig,
    dataset: &str,
    path: &Path,
    dry_run: bool,
    summary: &mut RunSummary,
) -> Result<()> {
    if dry_run {
        return Ok(());
    }
    if let Some(root) = &config.archive_dir {
        let archive_root = root.join(dataset);
        fs::create_dir_all(&archive_root)
            .with_context(|| format!("create {}", archive_root.display()))?;
        let dest = unique_archive_path(&archive_root.join(file_name(path)));
        fs::rename(path, &dest)
            .or_else(|_| {
                fs::copy(path, &dest)?;
                fs::remove_file(path)
            })
            .with_context(|| format!("archive {} -> {}", path.display(), dest.display()))?;
        summary.files_archived += 1;
    } else if config.delete_after_load {
        fs::remove_file(path).with_context(|| format!("delete {}", path.display()))?;
    }
    Ok(())
}

fn unique_archive_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path.file_stem().and_then(|v| v.to_str()).unwrap_or("file");
    let ext = path.extension().and_then(|v| v.to_str()).unwrap_or("");
    let suffix = Utc::now().format("%Y%m%d%H%M%S");
    if ext.is_empty() {
        parent.join(format!("{stem}.{suffix}"))
    } else {
        parent.join(format!("{stem}.{suffix}.{ext}"))
    }
}

fn run_sql_file(client: &ClickHouseClient, path: &Path, summary: &mut RunSummary) -> Result<()> {
    let sql = fs::read_to_string(path).with_context(|| format!("read SQL {}", path.display()))?;
    for statement in split_sql_statements(&sql) {
        client
            .execute(&statement)
            .with_context(|| format!("execute SQL {}", path.display()))?;
        summary.sql_statements += 1;
    }
    Ok(())
}

fn split_sql_statements(sql: &str) -> Vec<String> {
    let cleaned = sql
        .lines()
        .filter(|line| !line.trim_start().starts_with("--"))
        .collect::<Vec<_>>()
        .join("\n");
    cleaned
        .split(';')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| format!("{item};"))
        .collect()
}

fn run_optional_script(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let status = Command::new(path)
        .status()
        .with_context(|| format!("run {}", path.display()))?;
    if !status.success() {
        bail!("{} exited with {}", path.display(), status);
    }
    Ok(())
}

fn derive_company_entity_key(row: &Map<String, Value>, index: &HashMap<String, String>) -> String {
    let explicit = s(row, "company_entity_key");
    if !explicit.is_empty() {
        return explicit;
    }
    let infobase = s(row, "infobase");
    index
        .get(&infobase)
        .cloned()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| {
            canonical_company_entity_key(&s(row, "base_id"), &s(row, "base_path"), &infobase)
        })
}

fn canonical_company_entity_key(base_id: &str, base_path: &str, infobase: &str) -> String {
    if !base_id.trim().is_empty() {
        return format!("baseid:{}", base_id.trim());
    }
    if !base_path.trim().is_empty() {
        let normalized = normalize_base_path(base_path);
        if !normalized.is_empty() {
            return format!("basepath:{normalized}");
        }
    }
    let normalized = normalize_infobase_key(infobase);
    if normalized.is_empty() {
        String::new()
    } else {
        format!("infobase:{normalized}")
    }
}

fn normalize_base_path(value: &str) -> String {
    let re_slashes = Regex::new(r"/+").unwrap();
    let re_bad = Regex::new(r"[^0-9A-ZА-ЯЁ:/._ -]+").unwrap();
    let text = value.to_uppercase().replace('\\', "/");
    collapse_ws(&re_bad.replace_all(&re_slashes.replace_all(&text, "/"), " "))
}

fn normalize_infobase_key(value: &str) -> String {
    let re_year = Regex::new(r"(^|\s)20[0-9]{2}($|\s)").unwrap();
    let re_bad = Regex::new(r"[^0-9A-ZА-ЯЁ]+").unwrap();
    let text = value.to_uppercase();
    collapse_ws(&re_bad.replace_all(&re_year.replace_all(&text, " "), " "))
}

fn normalize_company_key(value: &str) -> String {
    let re_year = Regex::new(r"(^|\s)20\d{2}($|\s)").unwrap();
    let re_bad = Regex::new(r"[^0-9A-ZА-Я]+").unwrap();
    let text = value.to_uppercase().replace('Ё', "Е");
    collapse_ws(&re_bad.replace_all(&re_year.replace_all(&text, " "), " "))
}

fn collapse_ws(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn stable_id(prefix: &str, parts: &[&str]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(parts.join("|").as_bytes());
    let digest = hasher.finalize();
    format!("{prefix}:{:x}", digest)[..prefix.len() + 1 + 16].to_string()
}

fn ts_value(row: &Map<String, Value>, keys: &[&str]) -> String {
    let raw = keys
        .iter()
        .find_map(|key| row.get(*key))
        .cloned()
        .unwrap_or(Value::Null);
    normalize_ts(&raw)
}

fn normalize_ts(value: &Value) -> String {
    let raw = value_to_string(value);
    if raw.trim().is_empty() {
        return Utc::now()
            .naive_utc()
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
    }
    if let Ok(ts) = DateTime::parse_from_rfc3339(&raw.replace('Z', "+00:00")) {
        return ts.naive_utc().format("%Y-%m-%d %H:%M:%S").to_string();
    }
    for fmt in [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
        "%d.%m.%Y %H:%M:%S",
    ] {
        if let Ok(ts) = NaiveDateTime::parse_from_str(&raw, fmt) {
            return ts.format("%Y-%m-%d %H:%M:%S").to_string();
        }
    }
    raw
}

fn s(row: &Map<String, Value>, key: &str) -> String {
    row.get(key)
        .map(value_to_string)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn s_alt(row: &Map<String, Value>, keys: &[&str]) -> String {
    keys.iter()
        .map(|key| s(row, key))
        .find(|value| !value.is_empty())
        .unwrap_or_default()
}

fn s_default(row: &Map<String, Value>, key: &str, default: &str) -> String {
    let value = s(row, key);
    if value.is_empty() {
        default.to_string()
    } else {
        value
    }
}

fn f(row: &Map<String, Value>, key: &str) -> f64 {
    row.get(key).and_then(value_to_f64).unwrap_or(0.0)
}

fn f_alt(row: &Map<String, Value>, keys: &[&str]) -> f64 {
    keys.iter()
        .find_map(|key| row.get(*key).and_then(value_to_f64))
        .unwrap_or(0.0)
}

fn u(row: &Map<String, Value>, key: &str) -> u64 {
    row.get(key).and_then(value_to_u64).unwrap_or(0)
}

fn u_alt(row: &Map<String, Value>, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| row.get(*key).and_then(value_to_u64))
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(v) => v.clone(),
        Value::Number(v) => v.to_string(),
        Value::Bool(v) => {
            if *v {
                "1".to_string()
            } else {
                "0".to_string()
            }
        }
        other => other.to_string(),
    }
}

fn value_to_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(v) => v.as_f64(),
        Value::String(v) => v.replace(',', ".").parse().ok(),
        Value::Bool(v) => Some(if *v { 1.0 } else { 0.0 }),
        _ => None,
    }
}

fn value_to_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Number(v) => v.as_u64().or_else(|| v.as_f64().map(|v| v.max(0.0) as u64)),
        Value::String(v) => v.parse().ok(),
        Value::Bool(v) => Some(if *v { 1 } else { 0 }),
        _ => None,
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("unknown")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_company_key_matches_python_shape() {
        assert_eq!(
            canonical_company_entity_key("", "c:\\base\\ООО Тест 2026", ""),
            "basepath:C:/BASE/ООО ТЕСТ 2026"
        );
        assert_eq!(
            canonical_company_entity_key("", "", "НОРД 2026"),
            "infobase:НОРД"
        );
    }

    #[test]
    fn core_document_row_maps_defaults() {
        let mut row = Map::new();
        row.insert("infobase".to_string(), json!("БАЗА"));
        row.insert("amount".to_string(), json!("12.50"));
        let mapped = map_core_row("documents", "docs.jsonl", &row).unwrap();
        assert_eq!(mapped["infobase"], "БАЗА");
        assert_eq!(mapped["amount"], 12.5);
        assert_eq!(mapped["source_file"], "docs.jsonl");
    }

    #[test]
    fn split_sql_ignores_empty_chunks() {
        assert_eq!(split_sql_statements("SELECT 1; ; SELECT 2;").len(), 2);
    }
}
