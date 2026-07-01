use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, SecondsFormat, Utc};
use clap::{Parser, Subcommand, ValueEnum};
use hayabusa_tools::{analyze_report, read_json_file, required_str, severity_meets};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const SCHEMA_SQL: &str =
    include_str!("../../../../clickhouse-1c/security/security_finding_inbox.sql");

#[derive(Debug, Parser)]
#[command(about = "AWatch-rus Security Finding Inbox ingest and workflow CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print ClickHouse schema SQL.
    Schema,
    /// Print a normalized sample finding.
    Sample {
        #[arg(long)]
        pretty: bool,
    },
    /// Validate normalized finding JSON or JSONL without writing.
    Validate {
        #[arg(long)]
        input: PathBuf,
    },
    /// Ingest normalized finding JSON or JSONL into ClickHouse.
    Ingest {
        #[arg(long)]
        input: PathBuf,

        #[arg(long, default_value = "http://127.0.0.1:8123", env = "CLICKHOUSE_URL")]
        clickhouse_url: String,

        #[arg(long, default_value = "analytics_1c", env = "CLICKHOUSE_DATABASE")]
        database: String,

        #[arg(long, default_value = "default", env = "CLICKHOUSE_USER")]
        user: String,

        #[arg(long, default_value = "", env = "CLICKHOUSE_PASSWORD")]
        password: String,

        #[arg(long, default_value_t = 10)]
        timeout_seconds: u64,

        #[arg(long)]
        apply_schema: bool,

        #[arg(long)]
        dry_run: bool,
    },
    /// Convert a real Hayabusa intake/report into normalized findings and ingest them.
    IngestHayabusa {
        #[arg(long, default_value = "/opt/hayabusa/state/latest-intake.json")]
        intake: PathBuf,

        #[arg(long, default_value = "medium")]
        min_severity: String,

        #[arg(long, default_value = "http://127.0.0.1:8123", env = "CLICKHOUSE_URL")]
        clickhouse_url: String,

        #[arg(long, default_value = "analytics_1c", env = "CLICKHOUSE_DATABASE")]
        database: String,

        #[arg(long, default_value = "default", env = "CLICKHOUSE_USER")]
        user: String,

        #[arg(long, default_value = "", env = "CLICKHOUSE_PASSWORD")]
        password: String,

        #[arg(long, default_value_t = 10)]
        timeout_seconds: u64,

        #[arg(long)]
        apply_schema: bool,

        #[arg(long)]
        dry_run: bool,
    },
    /// Convert Velociraptor JSON/JSONL artifact output into normalized findings.
    IngestVelociraptorJson {
        #[arg(long)]
        input: PathBuf,

        #[arg(long, default_value = "high")]
        default_severity: String,

        #[arg(long, default_value = "http://127.0.0.1:8123", env = "CLICKHOUSE_URL")]
        clickhouse_url: String,

        #[arg(long, default_value = "analytics_1c", env = "CLICKHOUSE_DATABASE")]
        database: String,

        #[arg(long, default_value = "default", env = "CLICKHOUSE_USER")]
        user: String,

        #[arg(long, default_value = "", env = "CLICKHOUSE_PASSWORD")]
        password: String,

        #[arg(long, default_value_t = 10)]
        timeout_seconds: u64,

        #[arg(long)]
        apply_schema: bool,

        #[arg(long)]
        dry_run: bool,
    },
    /// Record an operator workflow event for a finding.
    Workflow {
        #[arg(long)]
        finding_id: String,

        #[arg(long, value_enum)]
        event_type: WorkflowEventType,

        #[arg(long, default_value = "operator")]
        actor: String,

        #[arg(long, default_value = "")]
        comment: String,

        #[arg(long, default_value = "")]
        decision_status: String,

        #[arg(long, default_value = "")]
        rollback_plan_id: String,

        #[arg(long, default_value = "")]
        plan_id: String,

        #[arg(long, default_value = "{}")]
        evidence_json: String,

        #[arg(long, default_value = "http://127.0.0.1:8123", env = "CLICKHOUSE_URL")]
        clickhouse_url: String,

        #[arg(long, default_value = "analytics_1c", env = "CLICKHOUSE_DATABASE")]
        database: String,

        #[arg(long, default_value = "default", env = "CLICKHOUSE_USER")]
        user: String,

        #[arg(long, default_value = "", env = "CLICKHOUSE_PASSWORD")]
        password: String,

        #[arg(long, default_value_t = 10)]
        timeout_seconds: u64,

        #[arg(long)]
        dry_run: bool,
    },
    /// Process approved apply_requested findings through containment plan/apply/verify/rollback.
    Executor {
        #[arg(long, default_value = "http://127.0.0.1:8123", env = "CLICKHOUSE_URL")]
        clickhouse_url: String,

        #[arg(long, default_value = "analytics_1c", env = "CLICKHOUSE_DATABASE")]
        database: String,

        #[arg(long, default_value = "default", env = "CLICKHOUSE_USER")]
        user: String,

        #[arg(long, default_value = "", env = "CLICKHOUSE_PASSWORD")]
        password: String,

        #[arg(long, default_value_t = 10)]
        timeout_seconds: u64,

        #[arg(
            long,
            default_value = "/etc/activitywatch/containment-policy.json",
            env = "AW_CONTAINMENT_POLICY"
        )]
        policy: PathBuf,

        #[arg(
            long,
            default_value = "containment-engine",
            env = "AW_CONTAINMENT_ENGINE_BIN"
        )]
        containment_engine_bin: PathBuf,

        #[arg(
            long,
            default_value = "/var/lib/activitywatch/security-finding-executor",
            env = "AW_SECURITY_FINDING_EXECUTOR_WORK_DIR"
        )]
        work_dir: PathBuf,

        #[arg(
            long,
            default_value = "/var/lock/aw-security-finding-executor.lock",
            env = "AW_SECURITY_FINDING_EXECUTOR_LOCK"
        )]
        lock_path: PathBuf,

        #[arg(
            long,
            value_delimiter = ',',
            env = "AW_CONTAINMENT_MANAGEMENT_ALLOWLIST"
        )]
        management_allowlist: Vec<String>,

        #[arg(
            long,
            value_delimiter = ',',
            env = "AW_CONTAINMENT_BLOCKED_REMOTE_ADDRESSES"
        )]
        blocked_remote_addresses: Vec<String>,

        #[arg(long, value_delimiter = ',', default_value = "Domain")]
        profiles: Vec<String>,

        #[arg(long, default_value_t = 60)]
        poll_seconds: u64,

        #[arg(long, default_value_t = 10)]
        limit: usize,

        #[arg(long)]
        once: bool,

        #[arg(long)]
        execute_local: bool,

        #[arg(long, default_value = "NO")]
        confirm_execute: String,

        #[arg(long)]
        executor_host: Option<String>,

        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[clap(rename_all = "snake_case")]
enum WorkflowEventType {
    DecideRequested,
    PlanRequested,
    Approved,
    ApplyRequested,
    VerifyRequested,
    RollbackRequested,
    Rejected,
    FalsePositive,
}

impl WorkflowEventType {
    fn as_str(self) -> &'static str {
        match self {
            Self::DecideRequested => "decide_requested",
            Self::PlanRequested => "plan_requested",
            Self::Approved => "approved",
            Self::ApplyRequested => "apply_requested",
            Self::VerifyRequested => "verify_requested",
            Self::RollbackRequested => "rollback_requested",
            Self::Rejected => "rejected",
            Self::FalsePositive => "false_positive",
        }
    }

    fn status(self) -> &'static str {
        match self {
            Self::DecideRequested => "decision_pending",
            Self::PlanRequested => "plan_pending",
            Self::Approved => "approved",
            Self::ApplyRequested => "apply_pending",
            Self::VerifyRequested => "verify_pending",
            Self::RollbackRequested => "rollback_pending",
            Self::Rejected => "rejected",
            Self::FalsePositive => "false_positive",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SecurityFindingInput {
    ts: Option<String>,
    finding_id: Option<String>,
    host: String,
    user: Option<String>,
    ip: Option<String>,
    department: Option<String>,
    state: Option<String>,
    severity: String,
    confidence: Option<String>,
    score: Option<u16>,
    source: String,
    rule_id: String,
    rule_title: Option<String>,
    summary: String,
    recommended_action: Option<String>,
    management_channel_checked: Option<bool>,
    evidence_ref: Option<String>,
    metadata: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
struct SecurityFindingRow {
    ts: String,
    finding_id: String,
    host: String,
    user: String,
    ip: String,
    department: String,
    state: String,
    severity: String,
    confidence: String,
    score: u16,
    source: String,
    rule_id: String,
    rule_title: String,
    summary: String,
    recommended_action: String,
    management_channel_checked: u8,
    evidence_ref: String,
    raw_json: String,
}

#[derive(Debug, Clone, Serialize)]
struct WorkflowRow {
    ts: String,
    finding_id: String,
    event_type: String,
    status: String,
    actor: String,
    comment: String,
    decision_status: String,
    rollback_plan_id: String,
    plan_id: String,
    evidence_json: String,
}

#[derive(Debug, Clone, Serialize)]
struct IngestSummary {
    ok: bool,
    rows: usize,
    dry_run: bool,
    applied_schema: bool,
    finding_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ExecutorCandidate {
    finding_id: String,
    host: String,
    state: String,
    severity: String,
    confidence: String,
    source: String,
    rule_id: String,
    rule_title: String,
    summary: String,
    recommended_action: String,
    management_channel_checked: u8,
    evidence_ref: String,
    raw_json: String,
}

#[derive(Debug, Clone, Serialize)]
struct ExecutorSummary {
    ok: bool,
    dry_run: bool,
    execute_local: bool,
    candidates: usize,
    processed: usize,
    refused: usize,
    applied: usize,
    failed: usize,
}

#[derive(Debug)]
struct ExecutorConfig {
    client: ClickHouseClient,
    policy: PathBuf,
    containment_engine_bin: PathBuf,
    work_dir: PathBuf,
    management_allowlist: Vec<String>,
    blocked_remote_addresses: Vec<String>,
    profiles: Vec<String>,
    limit: usize,
    execute_local: bool,
    confirm_execute: String,
    executor_host: String,
    dry_run: bool,
}

#[derive(Debug, Serialize)]
struct ContainmentFinding {
    host: String,
    host_role: String,
    state: String,
    confidence: String,
    signals: Vec<ContainmentSignal>,
    recommended_action: String,
    management_channel_checked: bool,
    manual_operator_flag: bool,
}

#[derive(Debug, Serialize)]
struct ContainmentSignal {
    source: String,
    rule_id: String,
    confidence: String,
}

#[derive(Debug, Serialize)]
struct WindowsFirewallRequest {
    target_host: String,
    plan_id: String,
    ttl_minutes: u32,
    reason: String,
    management_allowlist: Vec<String>,
    blocked_remote_addresses: Vec<String>,
    profiles: Vec<String>,
}

struct IngestTarget {
    clickhouse_url: String,
    database: String,
    user: String,
    password: String,
    timeout_seconds: u64,
    apply_schema: bool,
    dry_run: bool,
}

struct ExecutorEvent<'a> {
    finding_id: &'a str,
    event_type: &'a str,
    status: &'a str,
    comment: &'a str,
    decision_status: &'a str,
    plan_id: &'a str,
    evidence: Value,
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
    match cli.command {
        Command::Schema => {
            print!("{SCHEMA_SQL}");
            Ok(())
        }
        Command::Sample { pretty } => print_json(&sample_finding(), pretty),
        Command::Validate { input } => {
            let rows = read_finding_rows(&input)?;
            print_json(
                &json!({
                    "ok": true,
                    "rows": rows.len(),
                    "finding_ids": rows.iter().map(|row| row.finding_id.clone()).collect::<Vec<_>>()
                }),
                true,
            )
        }
        Command::Ingest {
            input,
            clickhouse_url,
            database,
            user,
            password,
            timeout_seconds,
            apply_schema,
            dry_run,
        } => {
            let rows = read_finding_rows(&input)?;
            let client = ClickHouseClient::new(
                clickhouse_url,
                database,
                user,
                password,
                Duration::from_secs(timeout_seconds),
            )?;
            if apply_schema && !dry_run {
                client.apply_schema()?;
            }
            if !dry_run {
                client.insert_json_each_row("security_findings", &rows)?;
            }
            print_json(
                &IngestSummary {
                    ok: true,
                    rows: rows.len(),
                    dry_run,
                    applied_schema: apply_schema && !dry_run,
                    finding_ids: rows.iter().map(|row| row.finding_id.clone()).collect(),
                },
                true,
            )
        }
        Command::IngestHayabusa {
            intake,
            min_severity,
            clickhouse_url,
            database,
            user,
            password,
            timeout_seconds,
            apply_schema,
            dry_run,
        } => {
            let rows = hayabusa_intake_rows(&intake, &min_severity)?;
            ingest_rows(
                rows,
                IngestTarget {
                    clickhouse_url,
                    database,
                    user,
                    password,
                    timeout_seconds,
                    apply_schema,
                    dry_run,
                },
            )
        }
        Command::IngestVelociraptorJson {
            input,
            default_severity,
            clickhouse_url,
            database,
            user,
            password,
            timeout_seconds,
            apply_schema,
            dry_run,
        } => {
            let rows = velociraptor_rows(&input, &default_severity)?;
            ingest_rows(
                rows,
                IngestTarget {
                    clickhouse_url,
                    database,
                    user,
                    password,
                    timeout_seconds,
                    apply_schema,
                    dry_run,
                },
            )
        }
        Command::Workflow {
            finding_id,
            event_type,
            actor,
            comment,
            decision_status,
            rollback_plan_id,
            plan_id,
            evidence_json,
            clickhouse_url,
            database,
            user,
            password,
            timeout_seconds,
            dry_run,
        } => {
            let row = workflow_row(WorkflowBuildInput {
                finding_id: &finding_id,
                event_type,
                actor: &actor,
                comment: &comment,
                decision_status: &decision_status,
                rollback_plan_id: &rollback_plan_id,
                plan_id: &plan_id,
                evidence_json: &evidence_json,
            })?;
            if !dry_run {
                let client = ClickHouseClient::new(
                    clickhouse_url,
                    database,
                    user,
                    password,
                    Duration::from_secs(timeout_seconds),
                )?;
                client.insert_json_each_row(
                    "security_finding_workflow_events",
                    std::slice::from_ref(&row),
                )?;
            }
            print_json(
                &json!({
                    "ok": true,
                    "dry_run": dry_run,
                    "finding_id": row.finding_id,
                    "event_type": row.event_type,
                    "status": row.status
                }),
                true,
            )
        }
        Command::Executor {
            clickhouse_url,
            database,
            user,
            password,
            timeout_seconds,
            policy,
            containment_engine_bin,
            work_dir,
            lock_path,
            management_allowlist,
            blocked_remote_addresses,
            profiles,
            poll_seconds,
            limit,
            once,
            execute_local,
            confirm_execute,
            executor_host,
            dry_run,
        } => {
            let _lock = acquire_lock(&lock_path)?;
            fs::create_dir_all(&work_dir)
                .with_context(|| format!("create executor work_dir {}", work_dir.display()))?;
            let client = ClickHouseClient::new(
                clickhouse_url,
                database,
                user,
                password,
                Duration::from_secs(timeout_seconds),
            )?;
            let config = ExecutorConfig {
                client,
                policy,
                containment_engine_bin,
                work_dir,
                management_allowlist: clean_string_list(management_allowlist),
                blocked_remote_addresses: clean_string_list(blocked_remote_addresses),
                profiles: clean_string_list(profiles),
                limit,
                execute_local,
                confirm_execute,
                executor_host: executor_host.unwrap_or_else(local_executor_host),
                dry_run,
            };
            loop {
                let summary = run_executor_once(&config)?;
                print_json(&summary, true)?;
                if once {
                    break Ok(());
                }
                thread::sleep(Duration::from_secs(poll_seconds.max(5)));
            }
        }
    }
}

fn ingest_rows(rows: Vec<SecurityFindingRow>, target: IngestTarget) -> Result<()> {
    let client = ClickHouseClient::new(
        target.clickhouse_url,
        target.database,
        target.user,
        target.password,
        Duration::from_secs(target.timeout_seconds),
    )?;
    if target.apply_schema && !target.dry_run {
        client.apply_schema()?;
    }
    if !target.dry_run {
        client.insert_json_each_row("security_findings", &rows)?;
    }
    print_json(
        &IngestSummary {
            ok: true,
            rows: rows.len(),
            dry_run: target.dry_run,
            applied_schema: target.apply_schema && !target.dry_run,
            finding_ids: rows.iter().map(|row| row.finding_id.clone()).collect(),
        },
        true,
    )
}

fn hayabusa_intake_rows(intake_path: &Path, min_severity: &str) -> Result<Vec<SecurityFindingRow>> {
    let intake = read_json_file(intake_path)?;
    let host = required_str(&intake, "host")?;
    let report_dir = PathBuf::from(required_str(&intake, "report_dir")?);
    let intake_id = required_str(&intake, "intake_id").unwrap_or("unknown-intake");
    let summary = analyze_report(&report_dir)
        .with_context(|| format!("analyze Hayabusa report {}", report_dir.display()))?;
    let severity = normalize_enum(
        &summary.severity,
        "severity",
        &["low", "medium", "high", "critical"],
    )?;
    let min_severity = normalize_enum(
        min_severity,
        "min_severity",
        &["low", "medium", "high", "critical"],
    )?;
    if !severity_meets(&severity, &min_severity) {
        return Ok(Vec::new());
    }
    let top_rule = summary
        .top_rules
        .first()
        .map(|rule| rule.title.as_str())
        .unwrap_or("hayabusa-no-dominant-rule");
    let first_ts = summary
        .first_timestamp
        .as_deref()
        .or(summary.last_timestamp.as_deref());
    let finding = SecurityFindingInput {
        ts: first_ts.map(str::to_string),
        finding_id: Some(generated_finding_id(
            first_ts.unwrap_or(intake_id),
            host,
            "hayabusa",
            top_rule,
            &format!("events={}", summary.events_total),
        )),
        host: host.to_string(),
        user: None,
        ip: None,
        department: None,
        state: Some("suspected_infected".to_string()),
        severity: severity.clone(),
        confidence: Some(severity.clone()),
        score: Some(summary.score.clamp(0, 100) as u16),
        source: "hayabusa".to_string(),
        rule_id: safe_rule_id(top_rule),
        rule_title: Some(top_rule.to_string()),
        summary: format!(
            "Hayabusa {} finding on {}: events={}, failed_logons={}, suspicious_pwsh={}, credential_events={}",
            severity,
            host,
            summary.events_total,
            summary.failed_logon_rows,
            summary.suspicious_pwsh,
            summary.credential_events
        ),
        recommended_action: Some("windows_firewall_quarantine".to_string()),
        management_channel_checked: Some(false),
        evidence_ref: Some(format!("file://{}", report_dir.display())),
        metadata: Some(json!({
            "source": "hayabusa",
            "intake_path": intake_path.display().to_string(),
            "intake_id": intake_id,
            "report_dir": report_dir.display().to_string(),
            "summary": summary,
        })),
    };
    Ok(vec![normalize_finding(&finding)?])
}

fn velociraptor_rows(input: &PathBuf, default_severity: &str) -> Result<Vec<SecurityFindingRow>> {
    let text = if input.as_os_str() == "-" {
        let mut text = String::new();
        io::stdin().read_to_string(&mut text)?;
        text
    } else {
        fs::read_to_string(input).with_context(|| format!("read {}", input.display()))?
    };
    let severity = normalize_enum(
        default_severity,
        "default_severity",
        &["low", "medium", "high", "critical"],
    )?;
    let values = parse_json_values(&text)?;
    values
        .iter()
        .enumerate()
        .map(|(index, value)| velociraptor_value_to_row(value, &severity, index))
        .collect()
}

fn parse_json_values(text: &str) -> Result<Vec<Value>> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        bail!("empty source JSON");
    }
    if trimmed.starts_with('[') {
        return serde_json::from_str(trimmed).context("parse JSON array");
    }
    if trimmed.starts_with('{') {
        if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
            return Ok(vec![value]);
        }
    }
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(index, line)| {
            serde_json::from_str::<Value>(line.trim())
                .with_context(|| format!("parse JSONL line {}", index + 1))
        })
        .collect()
}

fn velociraptor_value_to_row(
    value: &Value,
    default_severity: &str,
    index: usize,
) -> Result<SecurityFindingRow> {
    let host = first_string(
        value,
        &[
            "host",
            "hostname",
            "Hostname",
            "client_hostname",
            "ClientHostname",
            "Computer",
            "ComputerName",
        ],
    )
    .unwrap_or_else(|| {
        first_string(value, &["ClientId", "client_id"])
            .unwrap_or_else(|| "unknown-host".to_string())
    });
    let artifact = first_string(value, &["artifact", "Artifact", "source", "Source"])
        .unwrap_or_else(|| "velociraptor-artifact".to_string());
    let message = first_string(
        value,
        &[
            "summary",
            "message",
            "Message",
            "description",
            "Description",
        ],
    )
    .unwrap_or_else(|| format!("Velociraptor artifact result from {artifact}"));
    let severity = first_string(value, &["severity", "Severity"])
        .and_then(|raw| {
            normalize_enum(&raw, "severity", &["low", "medium", "high", "critical"]).ok()
        })
        .unwrap_or_else(|| default_severity.to_string());
    let ts = first_string(value, &["ts", "timestamp", "Timestamp", "_ts"]);
    let finding = SecurityFindingInput {
        ts,
        finding_id: first_string(value, &["finding_id", "FindingId"]),
        host,
        user: first_string(value, &["user", "User", "Username"]),
        ip: first_string(value, &["ip", "Ip", "IP", "RemoteIP"]),
        department: None,
        state: Some("suspected_infected".to_string()),
        severity: severity.clone(),
        confidence: Some(severity),
        score: first_number(value, &["score", "Score"]).map(|score| score.min(100) as u16),
        source: "velociraptor".to_string(),
        rule_id: safe_rule_id(&artifact),
        rule_title: Some(artifact),
        summary: message,
        recommended_action: Some("windows_firewall_quarantine".to_string()),
        management_channel_checked: Some(false),
        evidence_ref: Some(format!("velociraptor://record/{index}")),
        metadata: Some(value.clone()),
    };
    normalize_finding(&finding)
}

fn run_executor_once(config: &ExecutorConfig) -> Result<ExecutorSummary> {
    let candidates = query_executor_candidates(config)?;
    let mut summary = ExecutorSummary {
        ok: true,
        dry_run: config.dry_run,
        execute_local: config.execute_local,
        candidates: candidates.len(),
        processed: 0,
        refused: 0,
        applied: 0,
        failed: 0,
    };
    for candidate in candidates {
        match process_executor_candidate(config, &candidate) {
            Ok("applied") => {
                summary.processed += 1;
                summary.applied += 1;
            }
            Ok("refused") => {
                summary.processed += 1;
                summary.refused += 1;
                summary.ok = false;
            }
            Ok(_) => {
                summary.processed += 1;
            }
            Err(err) => {
                summary.processed += 1;
                summary.failed += 1;
                summary.ok = false;
                record_executor_event(
                    &config.client,
                    ExecutorEvent {
                        finding_id: &candidate.finding_id,
                        event_type: "executor_failed",
                        status: "executor_failed",
                        comment: &format!("{err:#}"),
                        decision_status: "",
                        plan_id: "",
                        evidence: json!({"error": err.to_string()}),
                    },
                )?;
            }
        }
    }
    Ok(summary)
}

fn query_executor_candidates(config: &ExecutorConfig) -> Result<Vec<ExecutorCandidate>> {
    let limit = config.limit.clamp(1, 100);
    let sql = format!(
        r#"
SELECT
    finding_id,
    host,
    state,
    severity,
    confidence,
    source,
    rule_id,
    rule_title,
    summary,
    recommended_action,
    toUInt8(management_channel_checked) AS management_channel_checked,
    evidence_ref,
    raw_json
FROM {database}.security_finding_inbox
WHERE last_workflow_event = 'apply_requested'
  AND workflow_status = 'apply_pending'
  AND finding_id IN (
      SELECT finding_id
      FROM {database}.security_finding_workflow_events
      WHERE event_type = 'approved'
  )
  AND finding_id NOT IN (
      SELECT finding_id
      FROM {database}.security_finding_workflow_events
      WHERE event_type IN (
          'executor_apply_succeeded',
          'executor_apply_failed',
          'executor_refused',
          'executor_rollback_succeeded',
          'executor_rollback_failed'
      )
  )
ORDER BY
    multiIf(severity = 'critical', 4, severity = 'high', 3, severity = 'medium', 2, 1) DESC,
    last_seen DESC
LIMIT {limit}
FORMAT JSONEachRow
"#,
        database = config.client.database
    );
    config
        .client
        .query_json_each_row(&sql)?
        .into_iter()
        .map(|value| serde_json::from_value(value).context("decode executor candidate"))
        .collect()
}

fn process_executor_candidate(
    config: &ExecutorConfig,
    candidate: &ExecutorCandidate,
) -> Result<&'static str> {
    let mut blockers = executor_candidate_blockers(config, candidate);
    if !blockers.is_empty() {
        blockers.sort();
        blockers.dedup();
        record_executor_event(
            &config.client,
            ExecutorEvent {
                finding_id: &candidate.finding_id,
                event_type: "executor_refused",
                status: "executor_refused",
                comment: &blockers.join(";"),
                decision_status: "",
                plan_id: "",
                evidence: json!({"blockers": blockers, "candidate": candidate_context(candidate), "dry_run": config.dry_run}),
            },
        )?;
        return Ok("refused");
    }

    let finding_path = config
        .work_dir
        .join(format!("{}-finding.json", candidate.finding_id));
    let request_path = config.work_dir.join(format!(
        "{}-windows-firewall-request.json",
        candidate.finding_id
    ));
    let plan_path = config.work_dir.join(format!(
        "{}-windows-firewall-plan.json",
        candidate.finding_id
    ));
    let containment_finding = containment_finding_from_candidate(candidate);
    write_json_file(&finding_path, &containment_finding)?;

    let decision_args = vec![
        "decide".to_string(),
        "--policy".to_string(),
        path_arg(&config.policy),
        "--finding".to_string(),
        path_arg(&finding_path),
    ];
    let decision = run_json_command_owned(&config.containment_engine_bin, &decision_args)?;
    let decision_status = decision
        .get("decision_status")
        .and_then(Value::as_str)
        .unwrap_or("");
    let decision_ok = decision.get("ok").and_then(Value::as_bool).unwrap_or(false);
    if !decision_ok || !matches!(decision_status, "manual_approval_required" | "auto_ready") {
        record_executor_event(
            &config.client,
            ExecutorEvent {
                finding_id: &candidate.finding_id,
                event_type: "executor_refused",
                status: "executor_refused",
                comment: &format!("containment decision refused: {decision_status}"),
                decision_status,
                plan_id: "",
                evidence: json!({"decision": decision, "candidate": candidate_context(candidate), "dry_run": config.dry_run}),
            },
        )?;
        return Ok("refused");
    }

    let plan_id = candidate_plan_id(candidate);
    let request = WindowsFirewallRequest {
        target_host: candidate.host.clone(),
        plan_id: plan_id.clone(),
        ttl_minutes: 60,
        reason: format!(
            "AWatch-rus approved containment for {}",
            candidate.finding_id
        ),
        management_allowlist: config.management_allowlist.clone(),
        blocked_remote_addresses: config.blocked_remote_addresses.clone(),
        profiles: config.profiles.clone(),
    };
    write_json_file(&request_path, &request)?;
    let plan_args = vec![
        "windows-firewall".to_string(),
        "plan".to_string(),
        "--request".to_string(),
        path_arg(&request_path),
    ];
    let plan = run_json_command_owned(&config.containment_engine_bin, &plan_args)?;
    write_json_file(&plan_path, &plan)?;
    if plan
        .get("blockers")
        .and_then(Value::as_array)
        .is_some_and(|items| !items.is_empty())
    {
        record_executor_event(
            &config.client,
            ExecutorEvent {
                finding_id: &candidate.finding_id,
                event_type: "executor_refused",
                status: "executor_refused",
                comment: "windows firewall plan has blockers",
                decision_status,
                plan_id: &plan_id,
                evidence: json!({"decision": decision, "plan": plan, "candidate": candidate_context(candidate), "dry_run": config.dry_run}),
            },
        )?;
        return Ok("refused");
    }

    record_executor_event(
        &config.client,
        ExecutorEvent {
            finding_id: &candidate.finding_id,
            event_type: "executor_plan_ready",
            status: "plan_ready",
            comment: "windows firewall plan generated",
            decision_status,
            plan_id: &plan_id,
            evidence: json!({"decision": decision, "plan_path": plan_path.display().to_string(), "candidate": candidate_context(candidate), "dry_run": config.dry_run}),
        },
    )?;

    let mut apply_args = vec![
        "windows-firewall".to_string(),
        "apply".to_string(),
        "--plan".to_string(),
        path_arg(&plan_path),
        "--confirm-apply".to_string(),
        if config.execute_local && !config.dry_run {
            config.confirm_execute.clone()
        } else {
            "YES".to_string()
        },
    ];
    if config.execute_local && !config.dry_run {
        apply_args.push("--execute-local".to_string());
    }
    let apply_result = run_json_command_owned(&config.containment_engine_bin, &apply_args)?;
    let apply_ok = apply_result
        .get("ok")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !apply_ok {
        record_executor_event(
            &config.client,
            ExecutorEvent {
                finding_id: &candidate.finding_id,
                event_type: "executor_apply_failed",
                status: "apply_failed",
                comment: "windows firewall apply failed or was refused",
                decision_status,
                plan_id: &plan_id,
                evidence: json!({"apply": apply_result, "dry_run": config.dry_run}),
            },
        )?;
        if config.execute_local && !config.dry_run {
            run_executor_rollback(config, candidate, &plan_path, decision_status, &plan_id)?;
        }
        return Ok("failed");
    }
    record_executor_event(
        &config.client,
        ExecutorEvent {
            finding_id: &candidate.finding_id,
            event_type: "executor_apply_succeeded",
            status: if config.dry_run {
                "apply_dry_run_ready"
            } else {
                "contained"
            },
            comment: "windows firewall apply completed",
            decision_status,
            plan_id: &plan_id,
            evidence: json!({"apply": apply_result, "dry_run": config.dry_run}),
        },
    )?;

    let mut verify_args = vec![
        "windows-firewall".to_string(),
        "verify".to_string(),
        "--plan".to_string(),
        path_arg(&plan_path),
    ];
    if config.execute_local && !config.dry_run {
        verify_args.push("--execute-local".to_string());
    }
    let verify_result = run_json_command_owned(&config.containment_engine_bin, &verify_args)?;
    let verify_ok = verify_result
        .get("ok")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    record_executor_event(
        &config.client,
        ExecutorEvent {
            finding_id: &candidate.finding_id,
            event_type: if verify_ok {
                "executor_verify_succeeded"
            } else {
                "executor_verify_failed"
            },
            status: if verify_ok {
                "verify_succeeded"
            } else {
                "verify_failed"
            },
            comment: "windows firewall verify completed",
            decision_status,
            plan_id: &plan_id,
            evidence: json!({"verify": verify_result, "dry_run": config.dry_run}),
        },
    )?;
    Ok("applied")
}

fn run_executor_rollback(
    config: &ExecutorConfig,
    candidate: &ExecutorCandidate,
    plan_path: &Path,
    decision_status: &str,
    plan_id: &str,
) -> Result<()> {
    let args = vec![
        "windows-firewall".to_string(),
        "rollback".to_string(),
        "--plan".to_string(),
        path_arg(plan_path),
        "--confirm-rollback".to_string(),
        config.confirm_execute.clone(),
        "--execute-local".to_string(),
    ];
    let rollback_result = run_json_command_owned(&config.containment_engine_bin, &args)?;
    let rollback_ok = rollback_result
        .get("ok")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    record_executor_event(
        &config.client,
        ExecutorEvent {
            finding_id: &candidate.finding_id,
            event_type: if rollback_ok {
                "executor_rollback_succeeded"
            } else {
                "executor_rollback_failed"
            },
            status: if rollback_ok {
                "rollback_succeeded"
            } else {
                "rollback_failed"
            },
            comment: "rollback attempted after apply failure",
            decision_status,
            plan_id,
            evidence: json!({"rollback": rollback_result}),
        },
    )
}

fn executor_candidate_blockers(
    config: &ExecutorConfig,
    candidate: &ExecutorCandidate,
) -> Vec<String> {
    let mut blockers = Vec::new();
    if candidate.recommended_action != "windows_firewall_quarantine" {
        blockers.push(format!(
            "unsupported_recommended_action:{}",
            candidate.recommended_action
        ));
    }
    if !matches!(
        candidate.state.as_str(),
        "suspected_infected" | "confirmed_infected"
    ) {
        blockers.push(format!("finding_state_not_actionable:{}", candidate.state));
    }
    if candidate.management_channel_checked == 0 {
        blockers.push("management_channel_not_checked".to_string());
    }
    if config.management_allowlist.is_empty() {
        blockers.push("management_allowlist_empty".to_string());
    }
    if config.blocked_remote_addresses.is_empty() {
        blockers.push("blocked_remote_addresses_empty".to_string());
    }
    if config.execute_local && !config.dry_run {
        if config.confirm_execute != "YES" {
            blockers.push("confirm_execute_must_be_YES".to_string());
        }
        if !same_host(&candidate.host, &config.executor_host) {
            blockers.push(format!(
                "executor_host_mismatch:finding_host={} executor_host={}",
                candidate.host, config.executor_host
            ));
        }
    }
    blockers
}

fn containment_finding_from_candidate(candidate: &ExecutorCandidate) -> ContainmentFinding {
    ContainmentFinding {
        host: candidate.host.clone(),
        host_role: "workstation".to_string(),
        state: candidate.state.clone(),
        confidence: candidate.confidence.clone(),
        signals: vec![ContainmentSignal {
            source: candidate.source.clone(),
            rule_id: candidate.rule_id.clone(),
            confidence: candidate.confidence.clone(),
        }],
        recommended_action: candidate.recommended_action.clone(),
        management_channel_checked: candidate.management_channel_checked > 0,
        manual_operator_flag: true,
    }
}

fn candidate_context(candidate: &ExecutorCandidate) -> Value {
    json!({
        "finding_id": candidate.finding_id,
        "host": candidate.host,
        "state": candidate.state,
        "severity": candidate.severity,
        "confidence": candidate.confidence,
        "source": candidate.source,
        "rule_id": candidate.rule_id,
        "rule_title": candidate.rule_title,
        "summary": candidate.summary,
        "recommended_action": candidate.recommended_action,
        "management_channel_checked": candidate.management_channel_checked > 0,
        "evidence_ref": candidate.evidence_ref,
        "raw_json": candidate.raw_json,
    })
}

fn record_executor_event(client: &ClickHouseClient, event: ExecutorEvent<'_>) -> Result<()> {
    let row = WorkflowRow {
        ts: clickhouse_ts(Utc::now()),
        finding_id: sanitize_required(event.finding_id, "finding_id", 128)?,
        event_type: sanitize_required(event.event_type, "event_type", 128)?,
        status: sanitize_required(event.status, "status", 128)?,
        actor: "security-finding-executor".to_string(),
        comment: sanitize_optional(Some(event.comment), 512),
        decision_status: sanitize_optional(Some(event.decision_status), 128),
        rollback_plan_id: String::new(),
        plan_id: sanitize_optional(Some(event.plan_id), 128),
        evidence_json: serde_json::to_string(&event.evidence)?,
    };
    client.insert_json_each_row("security_finding_workflow_events", &[row])
}

fn run_json_command_owned(binary: &Path, args: &[String]) -> Result<Value> {
    let output = ProcessCommand::new(binary)
        .args(args)
        .output()
        .with_context(|| format!("execute {} {}", binary.display(), args.join(" ")))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        bail!(
            "{} {} failed: status={} stderr={}",
            binary.display(),
            args.join(" "),
            output.status,
            stderr.trim()
        );
    }
    serde_json::from_str(stdout.trim()).with_context(|| {
        format!(
            "{} {} did not return JSON; stdout={} stderr={}",
            binary.display(),
            args.join(" "),
            stdout.trim(),
            stderr.trim()
        )
    })
}

fn write_json_file<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let temp_path = path.with_extension("tmp");
    {
        let mut file =
            File::create(&temp_path).with_context(|| format!("create {}", temp_path.display()))?;
        serde_json::to_writer_pretty(&mut file, value)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
    }
    fs::rename(&temp_path, path)
        .with_context(|| format!("rename {} -> {}", temp_path.display(), path.display()))
}

fn acquire_lock(path: &Path) -> Result<LockGuard> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("acquire lock {}", path.display()))?;
    writeln!(&file, "pid={}", std::process::id()).ok();
    Ok(LockGuard {
        path: path.to_path_buf(),
        _file: file,
    })
}

struct LockGuard {
    path: PathBuf,
    _file: File,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn clean_string_list(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

fn local_executor_host() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown-host".to_string())
}

fn same_host(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

fn candidate_plan_id(candidate: &ExecutorCandidate) -> String {
    let mut hasher = Sha256::new();
    hasher.update(candidate.finding_id.as_bytes());
    hasher.update(candidate.host.as_bytes());
    format!("awfw-{:x}", hasher.finalize())[..24].to_string()
}

fn path_arg(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn safe_rule_id(value: &str) -> String {
    let mut output = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.') {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    while output.contains("--") {
        output = output.replace("--", "-");
    }
    output.trim_matches('-').chars().take(128).collect()
}

fn first_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn first_number(value: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_u64))
}

fn read_finding_rows(path: &PathBuf) -> Result<Vec<SecurityFindingRow>> {
    let text = if path.as_os_str() == "-" {
        let mut text = String::new();
        io::stdin().read_to_string(&mut text)?;
        text
    } else {
        fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?
    };
    let findings = parse_findings(&text)?;
    findings.iter().map(normalize_finding).collect()
}

fn parse_findings(text: &str) -> Result<Vec<SecurityFindingInput>> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        bail!("empty finding input");
    }
    if trimmed.starts_with('[') {
        return serde_json::from_str(trimmed).context("parse finding JSON array");
    }
    if trimmed.starts_with('{') {
        if let Ok(item) = serde_json::from_str(trimmed) {
            return Ok(vec![item]);
        }
    }
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(index, line)| {
            serde_json::from_str(line.trim())
                .with_context(|| format!("parse finding JSONL line {}", index + 1))
        })
        .collect()
}

fn normalize_finding(input: &SecurityFindingInput) -> Result<SecurityFindingRow> {
    let ts = normalize_ts(input.ts.as_deref())?;
    let host = sanitize_required(&input.host, "host", 128)?;
    let source = sanitize_required(&input.source, "source", 64)?;
    let rule_id = sanitize_required(&input.rule_id, "rule_id", 128)?;
    let severity = normalize_enum(
        &input.severity,
        "severity",
        &["low", "medium", "high", "critical"],
    )?;
    let confidence = normalize_enum(
        input.confidence.as_deref().unwrap_or(&severity),
        "confidence",
        &["low", "medium", "high", "critical"],
    )?;
    let state = normalize_enum(
        input.state.as_deref().unwrap_or("suspected_infected"),
        "state",
        &[
            "new",
            "suspected_infected",
            "confirmed_infected",
            "contained",
            "released",
            "false_positive",
        ],
    )?;
    let recommended_action = normalize_enum(
        input
            .recommended_action
            .as_deref()
            .unwrap_or("windows_firewall_quarantine"),
        "recommended_action",
        &[
            "windows_firewall_quarantine",
            "pfsense_host_block",
            "switch_vlan_quarantine",
            "disable_workstation_account",
            "manual_review",
        ],
    )?;
    let score = input
        .score
        .unwrap_or_else(|| severity_default_score(&severity));
    if score > 100 {
        bail!("score must be <= 100");
    }
    let raw_json = serde_json::to_string(input)?;
    Ok(SecurityFindingRow {
        finding_id: input
            .finding_id
            .clone()
            .unwrap_or_else(|| generated_finding_id(&ts, &host, &source, &rule_id, &input.summary)),
        ts,
        host,
        user: sanitize_optional(input.user.as_deref(), 128),
        ip: sanitize_optional(input.ip.as_deref(), 64),
        department: sanitize_optional(input.department.as_deref(), 128),
        state,
        severity,
        confidence,
        score,
        source,
        rule_id,
        rule_title: sanitize_optional(input.rule_title.as_deref(), 240),
        summary: sanitize_required(&input.summary, "summary", 512)?,
        recommended_action,
        management_channel_checked: u8::from(input.management_channel_checked.unwrap_or(false)),
        evidence_ref: sanitize_optional(input.evidence_ref.as_deref(), 512),
        raw_json,
    })
}

struct WorkflowBuildInput<'a> {
    finding_id: &'a str,
    event_type: WorkflowEventType,
    actor: &'a str,
    comment: &'a str,
    decision_status: &'a str,
    rollback_plan_id: &'a str,
    plan_id: &'a str,
    evidence_json: &'a str,
}

fn workflow_row(input: WorkflowBuildInput<'_>) -> Result<WorkflowRow> {
    let evidence: Value =
        serde_json::from_str(input.evidence_json).context("evidence_json must be valid JSON")?;
    Ok(WorkflowRow {
        ts: clickhouse_ts(Utc::now()),
        finding_id: sanitize_required(input.finding_id, "finding_id", 128)?,
        event_type: input.event_type.as_str().to_string(),
        status: input.event_type.status().to_string(),
        actor: sanitize_required(input.actor, "actor", 128)?,
        comment: sanitize_optional(Some(input.comment), 512),
        decision_status: sanitize_optional(Some(input.decision_status), 128),
        rollback_plan_id: sanitize_optional(Some(input.rollback_plan_id), 128),
        plan_id: sanitize_optional(Some(input.plan_id), 128),
        evidence_json: serde_json::to_string(&evidence)?,
    })
}

#[derive(Debug, Clone)]
struct ClickHouseClient {
    http: Client,
    url: String,
    database: String,
    user: String,
    password: String,
}

impl ClickHouseClient {
    fn new(
        url: String,
        database: String,
        user: String,
        password: String,
        timeout: Duration,
    ) -> Result<Self> {
        let database = clickhouse_identifier(&database)
            .ok_or_else(|| anyhow!("invalid ClickHouse database identifier"))?;
        Ok(Self {
            http: Client::builder()
                .timeout(timeout)
                .no_proxy()
                .build()
                .context("ClickHouse HTTP client")?,
            url: url.trim_end_matches('/').to_string(),
            database,
            user,
            password,
        })
    }

    fn apply_schema(&self) -> Result<()> {
        for statement in split_sql_statements(SCHEMA_SQL) {
            self.execute(&statement)?;
        }
        Ok(())
    }

    fn insert_json_each_row<T: Serialize>(&self, table: &str, rows: &[T]) -> Result<()> {
        let table = clickhouse_identifier(table)
            .ok_or_else(|| anyhow!("invalid ClickHouse table identifier"))?;
        let mut body = format!(
            "INSERT INTO {}.{} FORMAT JSONEachRow\n",
            self.database, table
        );
        for row in rows {
            body.push_str(&serde_json::to_string(row)?);
            body.push('\n');
        }
        self.execute(&body)
    }

    fn query_json_each_row(&self, sql: &str) -> Result<Vec<Value>> {
        let body = self.request(sql)?.trim().to_string();
        if body.is_empty() {
            return Ok(Vec::new());
        }
        body.lines()
            .enumerate()
            .map(|(index, line)| {
                serde_json::from_str::<Value>(line.trim())
                    .with_context(|| format!("decode ClickHouse JSONEachRow line {}", index + 1))
            })
            .collect()
    }

    fn execute(&self, sql: &str) -> Result<()> {
        self.request(sql).map(|_| ())
    }

    fn request(&self, sql: &str) -> Result<String> {
        let mut request = self
            .http
            .post(&self.url)
            .query(&[("database", self.database.as_str())])
            .body(sql.to_string());
        if !self.user.trim().is_empty() {
            request = request.basic_auth(self.user.trim().to_string(), Some(self.password.clone()));
        }
        let response = request.send().context("ClickHouse request")?;
        let status = response.status();
        let body = response.text().unwrap_or_default();
        if !status.is_success() {
            bail!("ClickHouse HTTP {status}: {}", body.trim());
        }
        Ok(body)
    }
}

fn split_sql_statements(sql: &str) -> Vec<String> {
    sql.lines()
        .filter(|line| !line.trim_start().starts_with("--"))
        .collect::<Vec<_>>()
        .join("\n")
        .split(';')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| format!("{item};"))
        .collect()
}

fn normalize_ts(value: Option<&str>) -> Result<String> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => {
            let parsed = DateTime::parse_from_rfc3339(value)
                .with_context(|| format!("invalid RFC3339 ts: {value}"))?;
            Ok(clickhouse_ts(parsed.with_timezone(&Utc)))
        }
        None => Ok(clickhouse_ts(Utc::now())),
    }
}

fn clickhouse_ts(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn normalize_enum(value: &str, name: &str, allowed: &[&str]) -> Result<String> {
    let normalized = value.trim().to_ascii_lowercase();
    if !allowed.contains(&normalized.as_str()) {
        bail!("{name} is unsupported: {value}");
    }
    Ok(normalized)
}

fn severity_default_score(severity: &str) -> u16 {
    match severity {
        "critical" => 95,
        "high" => 80,
        "medium" => 50,
        _ => 20,
    }
}

fn sanitize_required(value: &str, name: &str, max_len: usize) -> Result<String> {
    let value = sanitize_optional(Some(value), max_len);
    if value.is_empty() {
        bail!("{name} is required");
    }
    Ok(value)
}

fn sanitize_optional(value: Option<&str>, max_len: usize) -> String {
    value
        .unwrap_or("")
        .chars()
        .filter(|ch| !ch.is_control())
        .take(max_len)
        .collect::<String>()
        .trim()
        .to_string()
}

fn generated_finding_id(
    ts: &str,
    host: &str,
    source: &str,
    rule_id: &str,
    summary: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(ts.as_bytes());
    hasher.update(host.as_bytes());
    hasher.update(source.as_bytes());
    hasher.update(rule_id.as_bytes());
    hasher.update(summary.as_bytes());
    format!("sf-{:x}", hasher.finalize())[..19].to_string()
}

fn clickhouse_identifier(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || !trimmed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return None;
    }
    Some(trimmed.to_string())
}

fn sample_finding() -> SecurityFindingInput {
    SecurityFindingInput {
        ts: Some("2026-06-25T10:00:00Z".to_string()),
        finding_id: None,
        host: "HOST-EXAMPLE".to_string(),
        user: Some("user-example".to_string()),
        ip: Some("10.10.20.42".to_string()),
        department: Some("demo".to_string()),
        state: Some("suspected_infected".to_string()),
        severity: "critical".to_string(),
        confidence: Some("high".to_string()),
        score: Some(95),
        source: "hayabusa".to_string(),
        rule_id: "demo-sigma-critical".to_string(),
        rule_title: Some("Demo high-confidence suspicious workstation".to_string()),
        summary: "Demo finding for Security Finding Inbox validation.".to_string(),
        recommended_action: Some("windows_firewall_quarantine".to_string()),
        management_channel_checked: Some(true),
        evidence_ref: Some("demo://hayabusa/HOST-EXAMPLE/demo-sigma-critical".to_string()),
        metadata: Some(json!({"sample": "true"})),
    }
}

fn print_json<T: Serialize>(value: &T, pretty: bool) -> Result<()> {
    if pretty {
        println!("{}", serde_json::to_string_pretty(value)?);
    } else {
        println!("{}", serde_json::to_string(value)?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn sample_finding_normalizes() {
        let row = normalize_finding(&sample_finding()).unwrap();
        assert_eq!(row.host, "HOST-EXAMPLE");
        assert_eq!(row.severity, "critical");
        assert_eq!(row.score, 95);
        assert!(row.finding_id.starts_with("sf-"));
    }

    #[test]
    fn invalid_severity_is_rejected() {
        let mut finding = sample_finding();
        finding.severity = "panic".to_string();
        assert!(normalize_finding(&finding).is_err());
    }

    #[test]
    fn parse_jsonl() {
        let line = serde_json::to_string(&sample_finding()).unwrap();
        let input = format!("{line}\n{line}\n");
        assert_eq!(parse_findings(&input).unwrap().len(), 2);
    }

    #[test]
    fn workflow_event_status_is_mapped() {
        let row = workflow_row(WorkflowBuildInput {
            finding_id: "sf-demo",
            event_type: WorkflowEventType::Approved,
            actor: "operator",
            comment: "ok",
            decision_status: "",
            rollback_plan_id: "",
            plan_id: "",
            evidence_json: "{}",
        })
        .unwrap();
        assert_eq!(row.event_type, "approved");
        assert_eq!(row.status, "approved");
    }

    #[test]
    fn sql_splitter_ignores_comments() {
        let statements = split_sql_statements("-- comment\nSELECT 1;\nSELECT 2;");
        assert_eq!(statements, vec!["SELECT 1;", "SELECT 2;"]);
    }

    #[test]
    fn clickhouse_identifier_rejects_injection() {
        assert_eq!(
            clickhouse_identifier("analytics_1c").as_deref(),
            Some("analytics_1c")
        );
        assert!(clickhouse_identifier("analytics_1c;DROP").is_none());
    }

    #[test]
    fn hayabusa_intake_maps_to_finding_row() {
        let dir = tempdir().unwrap();
        let report_dir = dir.path().join("report");
        fs::create_dir_all(&report_dir).unwrap();
        fs::write(
            report_dir.join("timeline.jsonl"),
            r#"{"Level":"crit","RuleTitle":"Credential Dump","Timestamp":"2026-06-25T10:00:00Z"}"#,
        )
        .unwrap();
        fs::write(report_dir.join("logon-summary-failed.csv"), "h\n1\n").unwrap();
        let intake_path = dir.path().join("latest-intake.json");
        fs::write(
            &intake_path,
            format!(
                r#"{{
  "host": "HOST-EXAMPLE",
  "status": "ok",
  "intake_id": "test-intake",
  "package_path": "{}/HOST-EXAMPLE.zip",
  "sha256": "demo",
  "report_dir": "{}"
}}"#,
                dir.path().display(),
                report_dir.display()
            ),
        )
        .unwrap();
        let rows = hayabusa_intake_rows(&intake_path, "low").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].source, "hayabusa");
        assert_eq!(rows[0].host, "HOST-EXAMPLE");
        assert_eq!(rows[0].severity, "critical");
    }

    #[test]
    fn velociraptor_json_maps_to_finding_row() {
        let value = json!({
            "Hostname": "HOST-EXAMPLE",
            "Artifact": "Windows.Hayabusa.Monitoring",
            "Severity": "high",
            "Message": "suspicious workstation",
            "User": "user-example"
        });
        let row = velociraptor_value_to_row(&value, "medium", 0).unwrap();
        assert_eq!(row.source, "velociraptor");
        assert_eq!(row.host, "HOST-EXAMPLE");
        assert_eq!(row.severity, "high");
        assert_eq!(row.user, "user-example");
    }

    #[test]
    fn executor_blocks_local_apply_for_other_host() {
        let candidate = ExecutorCandidate {
            finding_id: "sf-demo".to_string(),
            host: "HOST-A".to_string(),
            state: "suspected_infected".to_string(),
            severity: "high".to_string(),
            confidence: "high".to_string(),
            source: "hayabusa".to_string(),
            rule_id: "rule".to_string(),
            rule_title: "Rule".to_string(),
            summary: "summary".to_string(),
            recommended_action: "windows_firewall_quarantine".to_string(),
            management_channel_checked: 1,
            evidence_ref: "demo".to_string(),
            raw_json: "{}".to_string(),
        };
        let client = ClickHouseClient::new(
            "http://127.0.0.1:8123".to_string(),
            "analytics_1c".to_string(),
            "default".to_string(),
            String::new(),
            Duration::from_secs(1),
        )
        .unwrap();
        let config = ExecutorConfig {
            client,
            policy: PathBuf::from("/tmp/policy.json"),
            containment_engine_bin: PathBuf::from("containment-engine"),
            work_dir: PathBuf::from("/tmp"),
            management_allowlist: vec!["10.10.10.10".to_string()],
            blocked_remote_addresses: vec!["10.10.20.0/24".to_string()],
            profiles: vec!["Domain".to_string()],
            limit: 1,
            execute_local: true,
            confirm_execute: "YES".to_string(),
            executor_host: "HOST-B".to_string(),
            dry_run: false,
        };
        let blockers = executor_candidate_blockers(&config, &candidate);
        assert!(
            blockers
                .iter()
                .any(|item| item.starts_with("executor_host_mismatch"))
        );
    }
}
