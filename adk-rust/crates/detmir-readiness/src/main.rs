use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Utc;
use clap::Parser;
use detmir_core::runtime_guard::ensure_influx_runtime_config;
use detmir_core::{StatusLevel, exit_codes, now_utc_rfc3339};
use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const DEFAULT_AW_ENV_FILE: &str = "/etc/activitywatch/aw-server.env";
const DEFAULT_GRAFANA_ENV_FILE: &str = "/etc/detmir-grafana-check.env";
const DEFAULT_GRAFANA_URL: &str = "http://127.0.0.1:3000";
const DEFAULT_GRAFANA_DATASOURCE_UID: &str = "influxdb_aw";
const DEFAULT_SYSTEMD_SERVICES: &str = "activitywatch-server,aw-worktime-api,aw-worktime-influx-exporter.timer,aw-dlp-influx-exporter.timer";
const DEFAULT_RETENTION_DAYS: i64 = 30;

#[derive(Debug, Parser)]
#[command(
    about = "DetMir production readiness check: env, systemd, Influx write and Grafana datasource"
)]
struct Cli {
    #[arg(long)]
    json: bool,

    #[arg(long, default_value = DEFAULT_AW_ENV_FILE)]
    aw_env_file: PathBuf,

    #[arg(long, default_value = DEFAULT_GRAFANA_ENV_FILE)]
    grafana_env_file: PathBuf,

    #[arg(long, default_value = DEFAULT_SYSTEMD_SERVICES)]
    systemd_services: String,

    #[arg(long, default_value_t = 15)]
    timeout_seconds: u64,

    #[arg(long, env = "DETMIR_READINESS_SKIP_SYSTEMD")]
    skip_systemd: bool,

    #[arg(long, env = "DETMIR_READINESS_SKIP_INFLUX_WRITE")]
    skip_influx_write: bool,

    #[arg(long, env = "DETMIR_READINESS_ALLOW_DISABLED_INFLUX")]
    allow_disabled_influx: bool,

    #[arg(long, env = "DETMIR_READINESS_SKIP_GRAFANA")]
    skip_grafana: bool,

    #[arg(long)]
    grafana_url: Option<String>,

    #[arg(long)]
    grafana_user: Option<String>,

    #[arg(long)]
    grafana_password: Option<String>,

    #[arg(
        long,
        env = "DETMIR_GRAFANA_DATASOURCE_UID",
        default_value = DEFAULT_GRAFANA_DATASOURCE_UID
    )]
    grafana_datasource_uid: String,

    #[arg(long)]
    output_json: Option<PathBuf>,

    #[arg(long)]
    output_markdown: Option<PathBuf>,

    #[arg(long)]
    output_html: Option<PathBuf>,

    #[arg(long)]
    output_pdf: Option<PathBuf>,

    #[arg(long)]
    output_dir: Option<PathBuf>,

    #[arg(long, env = "DETMIR_READINESS_SIGNING_KEY")]
    signing_key: Option<PathBuf>,

    #[arg(long, env = "DETMIR_READINESS_REQUIRE_SIGNATURE")]
    require_signature: bool,

    #[arg(
        long,
        env = "DETMIR_READINESS_RETENTION_DAYS",
        default_value_t = DEFAULT_RETENTION_DAYS
    )]
    retention_days: i64,

    #[arg(long, env = "DETMIR_GIT_COMMIT", default_value = "unknown")]
    git_commit: String,
}

#[derive(Debug, Serialize)]
struct Report {
    ok: bool,
    status: StatusLevel,
    generated_at_utc: String,
    generated_by: GeneratedBy,
    host: String,
    version: String,
    git_commit: String,
    counts: Counts,
    checks: Vec<Check>,
    limitations: Vec<String>,
}

#[derive(Debug, Serialize)]
struct GeneratedBy {
    name: String,
    version: String,
}

#[derive(Debug, Clone, Default, Serialize)]
struct Counts {
    ok: usize,
    warn: usize,
    fail: usize,
}

#[derive(Debug, Serialize)]
struct Check {
    name: String,
    status: StatusLevel,
    summary: String,
    details: Value,
}

#[derive(Debug, Serialize)]
struct BundleStatus {
    ok: bool,
    status: StatusLevel,
    generated_at_utc: String,
    archive_dir: String,
    latest_dir: String,
    checksum_verified: bool,
    signature: SignatureStatus,
    counts: Counts,
    prometheus_metric_file: String,
}

#[derive(Debug, Clone, Serialize)]
struct SignatureStatus {
    required: bool,
    signed: bool,
    verified: bool,
    method: String,
    summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    public_key_fingerprint_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    signature_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    public_key_file: Option<String>,
}

#[derive(Debug, Clone)]
struct InfluxConfig {
    prefix: &'static str,
    enabled_key: &'static str,
    enabled: bool,
    url: String,
    org: String,
    bucket: String,
    token: String,
    hosts: Vec<String>,
}

fn main() {
    let cli = Cli::parse();
    let code = match run(&cli) {
        Ok(report) => {
            if let Err(err) = write_outputs(&cli, &report) {
                eprintln!("{err:#}");
                exit_codes::ERROR
            } else {
                if cli.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&report).expect("serialize report")
                    );
                } else {
                    print_text(&report);
                }
                readiness_exit_code(report.status)
            }
        }
        Err(err) => {
            eprintln!("{err:#}");
            exit_codes::ERROR
        }
    };
    std::process::exit(code);
}

fn run(cli: &Cli) -> Result<Report> {
    let mut aw_env = load_env_file(&cli.aw_env_file)?;
    overlay_process_env(&mut aw_env);
    let mut grafana_env = load_env_file(&cli.grafana_env_file)?;
    overlay_process_env(&mut grafana_env);

    let client = Client::builder()
        .timeout(Duration::from_secs(cli.timeout_seconds))
        .no_proxy()
        .build()
        .context("build HTTP client")?;

    let mut checks = Vec::new();
    let worktime = influx_config(&aw_env, "AW_WORKTIME_INFLUX");
    let dlp = influx_config(&aw_env, "AW_DLP_INFLUX");

    checks.push(check_influx_env(&worktime, cli.allow_disabled_influx));
    checks.push(check_influx_env(&dlp, cli.allow_disabled_influx));

    if cli.skip_systemd {
        checks.push(warn("systemd", "systemd checks skipped", json!({})));
    } else {
        checks.extend(check_systemd_services(&cli.systemd_services));
    }

    if cli.skip_influx_write {
        checks.push(warn(
            "influx:write",
            "Influx write probes skipped",
            json!({}),
        ));
    } else {
        checks.push(check_influx_write(&client, "worktime", &worktime));
        checks.push(check_influx_write(&client, "dlp", &dlp));
    }

    if cli.skip_grafana {
        checks.push(warn(
            "grafana",
            "Grafana API and datasource checks skipped",
            json!({}),
        ));
    } else {
        checks.push(check_grafana_api(&client, cli, &grafana_env));
        checks.push(check_grafana_datasource(
            &client,
            cli,
            &grafana_env,
            &cli.grafana_datasource_uid,
        ));
    }

    let (status, counts) = summarize(&checks);
    let version = env!("CARGO_PKG_VERSION").to_string();
    Ok(Report {
        ok: status == StatusLevel::Ok,
        status,
        generated_at_utc: now_utc_rfc3339(),
        generated_by: GeneratedBy {
            name: "detmir-readiness".to_string(),
            version: version.clone(),
        },
        host: hostname(),
        version,
        git_commit: cli.git_commit.clone(),
        counts,
        checks,
        limitations: build_limitations(cli),
    })
}

fn load_env_file(path: &Path) -> Result<BTreeMap<String, String>> {
    let mut values = BTreeMap::new();
    let Ok(text) = fs::read_to_string(path) else {
        return Ok(values);
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        values.insert(key.trim().to_string(), unquote(value.trim()));
    }
    Ok(values)
}

fn overlay_process_env(values: &mut BTreeMap<String, String>) {
    for (key, value) in std::env::vars() {
        if !value.trim().is_empty() {
            values.insert(key, value);
        }
    }
}

fn unquote(value: &str) -> String {
    let bytes = value.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        value[1..value.len() - 1].to_string()
    } else {
        value.to_string()
    }
}

fn env_value(env: &BTreeMap<String, String>, key: &str, default: &str) -> String {
    env.get(key)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or(default)
        .to_string()
}

fn env_bool(env: &BTreeMap<String, String>, key: &str, default: bool) -> bool {
    env.get(key)
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .map(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(default)
}

fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn hostname() -> String {
    Command::new("hostname")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

fn build_limitations(cli: &Cli) -> Vec<String> {
    let mut limitations = Vec::new();
    limitations.push(
        "Проверка подтверждает состояние runtime на момент формирования акта и не заменяет аудит конфигурации, нагрузочное тестирование или приемочные испытания заказчика.".to_string(),
    );
    limitations.push(
        "Секреты, токены и пароли не включаются в JSON, Markdown, HTML, PDF и checksum bundle."
            .to_string(),
    );
    if cli.skip_systemd {
        limitations.push("Проверки systemd были пропущены параметром --skip-systemd.".to_string());
    }
    if cli.skip_influx_write {
        limitations.push(
            "Реальная запись тестовой точки в InfluxDB была пропущена параметром --skip-influx-write."
                .to_string(),
        );
    }
    if cli.skip_grafana {
        limitations.push(
            "Проверки Grafana API/datasource были пропущены параметром --skip-grafana.".to_string(),
        );
    }
    if cli.allow_disabled_influx {
        limitations.push(
            "Отключенный Influx тракт допускается как WARN из-за параметра --allow-disabled-influx."
                .to_string(),
        );
    }
    limitations
}

fn influx_config(env: &BTreeMap<String, String>, prefix: &'static str) -> InfluxConfig {
    let enabled_key = match prefix {
        "AW_WORKTIME_INFLUX" => "AW_WORKTIME_INFLUX_ENABLED",
        "AW_DLP_INFLUX" => "AW_DLP_INFLUX_ENABLED",
        _ => unreachable!("unknown Influx prefix"),
    };
    InfluxConfig {
        prefix,
        enabled_key,
        enabled: env_bool(env, enabled_key, false),
        url: env_value(env, &format!("{prefix}_URL"), ""),
        org: env_value(env, &format!("{prefix}_ORG"), ""),
        bucket: env_value(env, &format!("{prefix}_BUCKET"), ""),
        token: env_value(env, &format!("{prefix}_TOKEN"), ""),
        hosts: split_csv(&env_value(env, &format!("{prefix}_HOSTS"), "")),
    }
}

fn check_influx_env(config: &InfluxConfig, allow_disabled: bool) -> Check {
    if !config.enabled {
        let status = if allow_disabled {
            StatusLevel::Warn
        } else {
            StatusLevel::Fail
        };
        return Check {
            name: format!("env:{}", config.prefix),
            status,
            summary: format!("{} is disabled", config.enabled_key),
            details: json!({ "enabled": false, "enabled_key": config.enabled_key }),
        };
    }
    match ensure_influx_runtime_config(
        config.prefix,
        &config.url,
        &config.org,
        &config.bucket,
        &config.token,
        &config.hosts,
    ) {
        Ok(()) => ok(
            format!("env:{}", config.prefix),
            "Influx runtime env is production-ready",
            json!({
                "enabled": true,
                "url_present": !config.url.is_empty(),
                "org": config.org,
                "bucket": config.bucket,
                "token_present": !config.token.is_empty(),
                "host_count": config.hosts.len(),
            }),
        ),
        Err(err) => fail(
            format!("env:{}", config.prefix),
            format!("Influx runtime env failed validation: {err}"),
            json!({ "enabled": true, "token_redacted": true }),
        ),
    }
}

fn check_systemd_services(csv: &str) -> Vec<Check> {
    split_csv(csv)
        .into_iter()
        .map(|service| {
            let output = Command::new("systemctl")
                .arg("is-active")
                .arg(&service)
                .output();
            match output {
                Ok(output) if output.status.success() => ok(
                    format!("systemd:{service}"),
                    "service is active",
                    json!({ "service": service }),
                ),
                Ok(output) => fail(
                    format!("systemd:{service}"),
                    format!(
                        "service is not active: {}",
                        String::from_utf8_lossy(&output.stdout).trim()
                    ),
                    json!({ "service": service, "exit_code": output.status.code() }),
                ),
                Err(err) => fail(
                    format!("systemd:{service}"),
                    format!("systemctl failed: {err}"),
                    json!({ "service": service }),
                ),
            }
        })
        .collect()
}

fn check_influx_write(client: &Client, label: &str, config: &InfluxConfig) -> Check {
    if !config.enabled {
        return fail(
            format!("influx:write:{label}"),
            format!("{} is disabled", config.enabled_key),
            json!({ "enabled": false }),
        );
    }
    if let Err(err) = ensure_influx_runtime_config(
        config.prefix,
        &config.url,
        &config.org,
        &config.bucket,
        &config.token,
        &config.hosts,
    ) {
        return fail(
            format!("influx:write:{label}"),
            format!("Influx write probe skipped because env is invalid: {err}"),
            json!({ "token_redacted": true }),
        );
    }
    let url = format!(
        "{}/api/v2/write?org={}&bucket={}&precision=ns",
        config.url.trim_end_matches('/'),
        urlencoding::encode(&config.org),
        urlencoding::encode(&config.bucket)
    );
    let host = config
        .hosts
        .first()
        .map(String::as_str)
        .unwrap_or("unknown-host");
    let ts = Utc::now().timestamp_nanos_opt().unwrap_or(0);
    let payload = format!(
        "detmir_readiness_heartbeat,channel={},host={} value=1i {}\n",
        escape_tag(label),
        escape_tag(host),
        ts
    );
    match client
        .post(url)
        .header("Authorization", format!("Token {}", config.token))
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(payload)
        .send()
        .and_then(|resp| resp.error_for_status())
    {
        Ok(_) => ok(
            format!("influx:write:{label}"),
            "Influx write probe succeeded",
            json!({ "bucket": config.bucket, "host": host, "token_redacted": true }),
        ),
        Err(err) => fail(
            format!("influx:write:{label}"),
            format!("Influx write probe failed: {err}"),
            json!({ "bucket": config.bucket, "token_redacted": true }),
        ),
    }
}

fn check_grafana_datasource(
    client: &Client,
    cli: &Cli,
    env: &BTreeMap<String, String>,
    uid: &str,
) -> Check {
    let grafana_url = cli
        .grafana_url
        .clone()
        .or_else(|| env.get("DETMIR_GRAFANA_URL").cloned())
        .or_else(|| env.get("GRAFANA_URL").cloned())
        .unwrap_or_else(|| DEFAULT_GRAFANA_URL.to_string())
        .trim_end_matches('/')
        .to_string();
    let user = cli
        .grafana_user
        .clone()
        .or_else(|| env.get("DETMIR_GRAFANA_USER").cloned())
        .or_else(|| env.get("GRAFANA_USER").cloned());
    let password = cli
        .grafana_password
        .clone()
        .or_else(|| env.get("DETMIR_GRAFANA_PASSWORD").cloned())
        .or_else(|| env.get("GRAFANA_PASSWORD").cloned());
    let url = format!("{grafana_url}/api/datasources/uid/{uid}/health");
    let mut request = client.get(url);
    if let Some(user) = user.as_deref() {
        request = request.basic_auth(user, password.as_deref());
    }
    match request.send().and_then(|resp| resp.error_for_status()) {
        Ok(resp) => {
            let value = resp.json::<Value>().unwrap_or_else(|_| json!({}));
            let status = value
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            if status.eq_ignore_ascii_case("ok") {
                ok(
                    "grafana:datasource",
                    "Grafana datasource health is OK",
                    json!({ "uid": uid, "grafana_url": grafana_url, "auth_present": user.is_some() }),
                )
            } else {
                fail(
                    "grafana:datasource",
                    format!("Grafana datasource health is not OK: {status}"),
                    json!({ "uid": uid, "grafana_url": grafana_url }),
                )
            }
        }
        Err(err) => fail(
            "grafana:datasource",
            format!("Grafana datasource health request failed: {err}"),
            json!({ "uid": uid, "grafana_url": grafana_url, "auth_present": user.is_some() }),
        ),
    }
}

fn check_grafana_api(client: &Client, cli: &Cli, env: &BTreeMap<String, String>) -> Check {
    let (grafana_url, user, password) = grafana_auth(cli, env);
    let url = format!("{grafana_url}/api/health");
    let mut request = client.get(url);
    if let Some(user) = user.as_deref() {
        request = request.basic_auth(user, password.as_deref());
    }
    match request.send().and_then(|resp| resp.error_for_status()) {
        Ok(resp) => {
            let value = resp.json::<Value>().unwrap_or_else(|_| json!({}));
            let database = value.get("database").and_then(Value::as_str).unwrap_or("");
            if database.eq_ignore_ascii_case("ok") || value.get("version").is_some() {
                ok(
                    "grafana:api",
                    "Grafana API health is reachable",
                    json!({
                        "grafana_url": grafana_url,
                        "auth_present": user.is_some(),
                        "version": value.get("version").and_then(Value::as_str),
                        "database": database,
                    }),
                )
            } else {
                fail(
                    "grafana:api",
                    "Grafana API health returned unexpected payload",
                    json!({ "grafana_url": grafana_url, "auth_present": user.is_some() }),
                )
            }
        }
        Err(err) => fail(
            "grafana:api",
            format!("Grafana API health request failed: {err}"),
            json!({ "grafana_url": grafana_url, "auth_present": user.is_some() }),
        ),
    }
}

fn grafana_auth(
    cli: &Cli,
    env: &BTreeMap<String, String>,
) -> (String, Option<String>, Option<String>) {
    let grafana_url = cli
        .grafana_url
        .clone()
        .or_else(|| env.get("DETMIR_GRAFANA_URL").cloned())
        .or_else(|| env.get("GRAFANA_URL").cloned())
        .unwrap_or_else(|| DEFAULT_GRAFANA_URL.to_string())
        .trim_end_matches('/')
        .to_string();
    let user = cli
        .grafana_user
        .clone()
        .or_else(|| env.get("DETMIR_GRAFANA_USER").cloned())
        .or_else(|| env.get("GRAFANA_USER").cloned());
    let password = cli
        .grafana_password
        .clone()
        .or_else(|| env.get("DETMIR_GRAFANA_PASSWORD").cloned())
        .or_else(|| env.get("GRAFANA_PASSWORD").cloned());
    (grafana_url, user, password)
}

fn escape_tag(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace(' ', "\\ ")
        .replace(',', "\\,")
        .replace('=', "\\=")
}

fn ok(name: impl Into<String>, summary: impl Into<String>, details: Value) -> Check {
    Check {
        name: name.into(),
        status: StatusLevel::Ok,
        summary: summary.into(),
        details,
    }
}

fn warn(name: impl Into<String>, summary: impl Into<String>, details: Value) -> Check {
    Check {
        name: name.into(),
        status: StatusLevel::Warn,
        summary: summary.into(),
        details,
    }
}

fn fail(name: impl Into<String>, summary: impl Into<String>, details: Value) -> Check {
    Check {
        name: name.into(),
        status: StatusLevel::Fail,
        summary: summary.into(),
        details,
    }
}

fn summarize(checks: &[Check]) -> (StatusLevel, Counts) {
    let mut counts = Counts::default();
    for check in checks {
        match check.status {
            StatusLevel::Ok => counts.ok += 1,
            StatusLevel::Warn => counts.warn += 1,
            StatusLevel::Fail | StatusLevel::Unknown => counts.fail += 1,
        }
    }
    let status = if counts.fail > 0 {
        StatusLevel::Fail
    } else if counts.warn > 0 {
        StatusLevel::Warn
    } else {
        StatusLevel::Ok
    };
    (status, counts)
}

fn print_text(report: &Report) {
    println!("DetMir readiness: {}", report.status);
    for check in &report.checks {
        println!("- {}: {} - {}", check.status, check.name, check.summary);
    }
}

fn readiness_exit_code(status: StatusLevel) -> i32 {
    match status {
        StatusLevel::Ok => exit_codes::OK,
        StatusLevel::Warn => exit_codes::CHECK_FAILED,
        StatusLevel::Fail | StatusLevel::Unknown => exit_codes::POLICY_DENIED,
    }
}

fn write_outputs(cli: &Cli, report: &Report) -> Result<()> {
    if let Some(dir) = &cli.output_dir {
        write_bundle(dir, report, cli)?;
    }
    if let Some(path) = &cli.output_json {
        write_text(path, &serde_json::to_string_pretty(report)?)?;
    }
    if let Some(path) = &cli.output_markdown {
        write_text(path, &render_markdown(report))?;
    }
    if let Some(path) = &cli.output_html {
        write_text(path, &render_html(report))?;
    }
    if let Some(path) = &cli.output_pdf {
        write_pdf(path, report)?;
    }
    Ok(())
}

fn write_bundle(dir: &Path, report: &Report, cli: &Cli) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("create output dir: {}", dir.display()))?;
    let archive_dir = archive_dir_for(dir);
    fs::create_dir_all(&archive_dir)
        .with_context(|| format!("create archive dir: {}", archive_dir.display()))?;

    let files = [
        (
            "detmir-readiness-latest.json",
            serde_json::to_string_pretty(report)?,
        ),
        ("detmir-readiness-act.md", render_markdown(report)),
        ("detmir-readiness-act.html", render_html(report)),
    ];
    let mut sums = Vec::new();
    for (name, content) in files {
        let path = archive_dir.join(name);
        write_text(&path, &content)?;
        sums.push((name.to_string(), sha256_file(&path)?));
    }
    let sums_text = sums
        .into_iter()
        .map(|(name, sum)| format!("{sum}  {name}\n"))
        .collect::<String>();
    write_text(&archive_dir.join("sha256sums.txt"), &sums_text)?;

    let signature = sign_bundle(&archive_dir, cli)?;
    let status = BundleStatus {
        ok: report.ok && (!cli.require_signature || signature.verified),
        status: report.status,
        generated_at_utc: report.generated_at_utc.clone(),
        archive_dir: archive_dir.display().to_string(),
        latest_dir: dir.display().to_string(),
        checksum_verified: true,
        signature,
        counts: report.counts.clone(),
        prometheus_metric_file: dir.join("detmir-readiness.prom").display().to_string(),
    };
    write_text(
        &archive_dir.join("detmir-readiness-status.json"),
        &serde_json::to_string_pretty(&status)?,
    )?;
    write_text(
        &archive_dir.join("detmir-readiness.prom"),
        &render_prometheus_metrics(&status),
    )?;
    copy_latest_bundle(dir, &archive_dir)?;
    prune_old_archives(dir, cli.retention_days)?;
    Ok(())
}

fn archive_dir_for(root: &Path) -> PathBuf {
    let now = Utc::now();
    root.join(now.format("%Y-%m-%d").to_string())
        .join(now.format("%H%M%SZ").to_string())
}

fn copy_latest_bundle(root: &Path, archive_dir: &Path) -> Result<()> {
    for name in [
        "detmir-readiness-latest.json",
        "detmir-readiness-act.md",
        "detmir-readiness-act.html",
        "sha256sums.txt",
        "sha256sums.txt.sig",
        "public-key.pem",
        "detmir-readiness-status.json",
        "detmir-readiness.prom",
    ] {
        let src = archive_dir.join(name);
        if src.is_file() {
            fs::copy(&src, root.join(name))
                .with_context(|| format!("copy latest bundle file: {}", src.display()))?;
        } else {
            let latest = root.join(name);
            if latest.exists() {
                fs::remove_file(&latest)
                    .with_context(|| format!("remove stale latest file: {}", latest.display()))?;
            }
        }
    }
    write_text(
        &root.join("latest-dir.txt"),
        &format!("{}\n", archive_dir.display()),
    )
}

fn sign_bundle(archive_dir: &Path, cli: &Cli) -> Result<SignatureStatus> {
    let sums_path = archive_dir.join("sha256sums.txt");
    let sig_path = archive_dir.join("sha256sums.txt.sig");
    let public_key_path = archive_dir.join("public-key.pem");
    let Some(key_path) = cli.signing_key.as_deref() else {
        if cli.require_signature {
            anyhow::bail!(
                "readiness bundle signature is required but signing key is not configured"
            );
        }
        return Ok(SignatureStatus {
            required: false,
            signed: false,
            verified: false,
            method: "openssl dgst -sha256".to_string(),
            summary: "signature not configured".to_string(),
            public_key_fingerprint_sha256: None,
            signature_file: None,
            public_key_file: None,
        });
    };
    if !key_path.is_file() {
        if cli.require_signature {
            anyhow::bail!("readiness signing key not found: {}", key_path.display());
        }
        return Ok(SignatureStatus {
            required: false,
            signed: false,
            verified: false,
            method: "openssl dgst -sha256".to_string(),
            summary: "signing key not found".to_string(),
            public_key_fingerprint_sha256: None,
            signature_file: None,
            public_key_file: None,
        });
    }
    run_command(
        Command::new("openssl")
            .arg("pkey")
            .arg("-in")
            .arg(key_path)
            .arg("-pubout")
            .arg("-out")
            .arg(&public_key_path),
        "extract readiness public key",
    )?;
    run_command(
        Command::new("openssl")
            .arg("dgst")
            .arg("-sha256")
            .arg("-sign")
            .arg(key_path)
            .arg("-out")
            .arg(&sig_path)
            .arg(&sums_path),
        "sign readiness sha256sums",
    )?;
    run_command(
        Command::new("openssl")
            .arg("dgst")
            .arg("-sha256")
            .arg("-verify")
            .arg(&public_key_path)
            .arg("-signature")
            .arg(&sig_path)
            .arg(&sums_path),
        "verify readiness sha256sums signature",
    )?;
    let fingerprint = sha256_file(&public_key_path)?;
    Ok(SignatureStatus {
        required: cli.require_signature,
        signed: true,
        verified: true,
        method: "openssl dgst -sha256".to_string(),
        summary: "sha256sums detached signature verified".to_string(),
        public_key_fingerprint_sha256: Some(fingerprint),
        signature_file: Some(sig_path.display().to_string()),
        public_key_file: Some(public_key_path.display().to_string()),
    })
}

fn run_command(command: &mut Command, context: &str) -> Result<()> {
    let output = command.output().with_context(|| format!("run {context}"))?;
    if output.status.success() {
        Ok(())
    } else {
        anyhow::bail!(
            "{context} failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn render_prometheus_metrics(status: &BundleStatus) -> String {
    let ready = if status.ok { 1 } else { 0 };
    let signed = if status.signature.verified { 1 } else { 0 };
    let status_value = match status.status {
        StatusLevel::Ok => 0,
        StatusLevel::Warn => 1,
        StatusLevel::Fail | StatusLevel::Unknown => 2,
    };
    format!(
        "# HELP detmir_readiness_ok DetMir readiness result, 1 means OK.\n\
         # TYPE detmir_readiness_ok gauge\n\
         detmir_readiness_ok {ready}\n\
         # HELP detmir_readiness_status DetMir readiness status: 0 OK, 1 WARN, 2 FAIL.\n\
         # TYPE detmir_readiness_status gauge\n\
         detmir_readiness_status {status_value}\n\
         # HELP detmir_readiness_signature_verified DetMir readiness detached signature verification result.\n\
         # TYPE detmir_readiness_signature_verified gauge\n\
         detmir_readiness_signature_verified {signed}\n\
         detmir_readiness_checks_ok {}\n\
         detmir_readiness_checks_warn {}\n\
         detmir_readiness_checks_fail {}\n",
        status.counts.ok, status.counts.warn, status.counts.fail
    )
}

fn prune_old_archives(root: &Path, retention_days: i64) -> Result<()> {
    if retention_days <= 0 {
        return Ok(());
    }
    let cutoff = Utc::now().date_naive() - chrono::Duration::days(retention_days);
    for entry in
        fs::read_dir(root).with_context(|| format!("read output dir: {}", root.display()))?
    {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let Ok(date) = chrono::NaiveDate::parse_from_str(&name, "%Y-%m-%d") else {
            continue;
        };
        if date < cutoff {
            fs::remove_dir_all(entry.path())
                .with_context(|| format!("prune readiness archive: {}", entry.path().display()))?;
        }
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let bytes =
        fs::read(path).with_context(|| format!("read file for sha256: {}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn write_text(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("create output directory: {}", parent.display()))?;
    }
    fs::write(path, text).with_context(|| format!("write output file: {}", path.display()))
}

fn write_pdf(path: &Path, report: &Report) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("create output directory: {}", parent.display()))?;
    }
    let dir = tempfile::tempdir().context("create temporary PDF render directory")?;
    let html_path = dir.path().join("detmir-readiness.html");
    write_text(&html_path, &render_html(report))?;

    if command_exists("weasyprint") {
        run_pdf_command(Command::new("weasyprint").arg(&html_path).arg(path))
    } else if command_exists("chromium") {
        run_chromium_pdf("chromium", &html_path, path)
    } else if command_exists("chromium-browser") {
        run_chromium_pdf("chromium-browser", &html_path, path)
    } else if command_exists("google-chrome") {
        run_chromium_pdf("google-chrome", &html_path, path)
    } else {
        anyhow::bail!(
            "PDF output requires one of: weasyprint, chromium, chromium-browser, google-chrome"
        );
    }
}

fn command_exists(name: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {}", shell_quote(name)))
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn run_chromium_pdf(binary: &str, html_path: &Path, pdf_path: &Path) -> Result<()> {
    let html_uri = format!("file://{}", html_path.display());
    run_pdf_command(
        Command::new(binary)
            .arg("--headless")
            .arg("--disable-gpu")
            .arg("--no-sandbox")
            .arg(format!("--print-to-pdf={}", pdf_path.display()))
            .arg(html_uri),
    )
}

fn run_pdf_command(command: &mut Command) -> Result<()> {
    let output = command.output().context("run PDF renderer")?;
    if output.status.success() {
        Ok(())
    } else {
        anyhow::bail!(
            "PDF renderer failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn render_markdown(report: &Report) -> String {
    let mut out = String::new();
    out.push_str("# Акт готовности стенда DetMir\n\n");
    out.push_str(&format!("- Статус: **{}**\n", report.status));
    out.push_str(&format!(
        "- Готовность: **{}**\n",
        if report.ok { "да" } else { "нет" }
    ));
    out.push_str(&format!(
        "- Сформировано UTC: `{}`\n",
        report.generated_at_utc
    ));
    out.push_str(&format!("- Узел: `{}`\n", report.host));
    out.push_str(&format!("- Утилита: `{}`\n", report.generated_by.name));
    out.push_str(&format!("- Версия: `{}`\n", report.version));
    out.push_str(&format!("- Git commit: `{}`\n", report.git_commit));
    out.push_str(&format!(
        "- Проверки: OK `{}`, WARN `{}`, FAIL `{}`\n\n",
        report.counts.ok, report.counts.warn, report.counts.fail
    ));
    out.push_str("## Результаты проверок\n\n");
    out.push_str("| Статус | Проверка | Результат |\n");
    out.push_str("| --- | --- | --- |\n");
    for check in &report.checks {
        out.push_str(&format!(
            "| {} | `{}` | {} |\n",
            check.status,
            escape_markdown_table(&check.name),
            escape_markdown_table(&check.summary)
        ));
    }
    out.push_str("\n## Решение\n\n");
    match report.status {
        StatusLevel::Ok => {
            out.push_str("Стенд готов к промышленной эксплуатации по проверенным критериям.\n")
        }
        StatusLevel::Warn => out.push_str(
            "Стенд имеет предупреждения. Перед промышленной эксплуатацией требуется управленческое принятие риска или устранение предупреждений.\n",
        ),
        StatusLevel::Fail | StatusLevel::Unknown => {
            out.push_str("Стенд не готов к промышленной эксплуатации до устранения отказов.\n")
        }
    }
    out.push_str("\n## Ограничения проверки\n\n");
    for item in &report.limitations {
        out.push_str(&format!("- {}\n", item));
    }
    out
}

fn render_html(report: &Report) -> String {
    let rows = report
        .checks
        .iter()
        .map(|check| {
            format!(
                "<tr><td class=\"{}\">{}</td><td>{}</td><td>{}</td></tr>",
                html_escape(&check.status.to_string().to_ascii_lowercase()),
                html_escape(&check.status.to_string()),
                html_escape(&check.name),
                html_escape(&check.summary)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"<!doctype html>
<html lang="ru">
<head>
<meta charset="utf-8">
<title>Акт готовности стенда DetMir</title>
<style>
body {{ font-family: sans-serif; margin: 32px; color: #1f2933; }}
h1 {{ font-size: 24px; }}
.status {{ font-size: 20px; font-weight: 700; }}
.ok {{ color: #166534; font-weight: 700; }}
.warn {{ color: #92400e; font-weight: 700; }}
.fail,.unknown {{ color: #991b1b; font-weight: 700; }}
table {{ width: 100%; border-collapse: collapse; margin-top: 16px; }}
th,td {{ border: 1px solid #cbd5e1; padding: 8px; text-align: left; vertical-align: top; }}
th {{ background: #f1f5f9; }}
</style>
</head>
<body>
<h1>Акт готовности стенда DetMir</h1>
<p class="status">Статус: <span class="{status_class}">{status}</span></p>
<p>Готовность: <strong>{ready}</strong></p>
<p>Сформировано UTC: <code>{generated}</code></p>
<p>Узел: <code>{host}</code></p>
<p>Утилита: <code>{generated_by}</code>, версия <code>{version}</code>, git commit <code>{git_commit}</code></p>
<p>Проверки: OK <strong>{ok}</strong>, WARN <strong>{warn}</strong>, FAIL <strong>{fail}</strong></p>
<h2>Результаты проверок</h2>
<table>
<thead><tr><th>Статус</th><th>Проверка</th><th>Результат</th></tr></thead>
<tbody>
{rows}
</tbody>
</table>
<h2>Решение</h2>
<p>{decision}</p>
<h2>Ограничения проверки</h2>
<ul>
{limitations}
</ul>
</body>
</html>"#,
        status_class = html_escape(&report.status.to_string().to_ascii_lowercase()),
        status = html_escape(&report.status.to_string()),
        ready = if report.ok { "да" } else { "нет" },
        generated = html_escape(&report.generated_at_utc),
        host = html_escape(&report.host),
        generated_by = html_escape(&report.generated_by.name),
        version = html_escape(&report.version),
        git_commit = html_escape(&report.git_commit),
        ok = report.counts.ok,
        warn = report.counts.warn,
        fail = report.counts.fail,
        rows = rows,
        limitations = report
            .limitations
            .iter()
            .map(|item| format!("<li>{}</li>", html_escape(item)))
            .collect::<Vec<_>>()
            .join("\n"),
        decision = html_escape(match report.status {
            StatusLevel::Ok => "Стенд готов к промышленной эксплуатации по проверенным критериям.",
            StatusLevel::Warn =>
                "Стенд имеет предупреждения. Требуется принятие риска или устранение предупреждений.",
            StatusLevel::Fail | StatusLevel::Unknown =>
                "Стенд не готов к промышленной эксплуатации до устранения отказов.",
        }),
    )
}

fn escape_markdown_table(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_report(checks: Vec<Check>) -> Report {
        let (status, counts) = summarize(&checks);
        Report {
            ok: status == StatusLevel::Ok,
            status,
            generated_at_utc: "2026-06-03T12:00:00Z".to_string(),
            generated_by: GeneratedBy {
                name: "detmir-readiness".to_string(),
                version: "0.1.0".to_string(),
            },
            host: "HOST-TEST".to_string(),
            version: "0.1.0".to_string(),
            git_commit: "abc123".to_string(),
            counts,
            checks,
            limitations: vec!["Test limitation".to_string()],
        }
    }

    fn test_cli() -> Cli {
        Cli {
            json: false,
            aw_env_file: PathBuf::from("/nonexistent/aw.env"),
            grafana_env_file: PathBuf::from("/nonexistent/grafana.env"),
            systemd_services: DEFAULT_SYSTEMD_SERVICES.to_string(),
            timeout_seconds: 1,
            skip_systemd: true,
            skip_influx_write: true,
            allow_disabled_influx: true,
            skip_grafana: true,
            grafana_url: None,
            grafana_user: None,
            grafana_password: None,
            grafana_datasource_uid: DEFAULT_GRAFANA_DATASOURCE_UID.to_string(),
            output_json: None,
            output_markdown: None,
            output_html: None,
            output_pdf: None,
            output_dir: None,
            signing_key: None,
            require_signature: false,
            retention_days: DEFAULT_RETENTION_DAYS,
            git_commit: "abc123".to_string(),
        }
    }

    #[test]
    fn parses_env_file_without_quotes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("runtime.env");
        fs::write(
            &path,
            "export A=\"one\"\nB='two'\n# skip\nBROKEN\nC=three\n",
        )
        .unwrap();
        let env = load_env_file(&path).unwrap();
        assert_eq!(env.get("A").unwrap(), "one");
        assert_eq!(env.get("B").unwrap(), "two");
        assert_eq!(env.get("C").unwrap(), "three");
    }

    #[test]
    fn validates_influx_env_placeholders() {
        let mut env = BTreeMap::new();
        env.insert("AW_WORKTIME_INFLUX_ENABLED".to_string(), "true".to_string());
        env.insert(
            "AW_WORKTIME_INFLUX_URL".to_string(),
            "http://192.0.2.10:8086".to_string(),
        );
        env.insert("AW_WORKTIME_INFLUX_ORG".to_string(), "proxmox".to_string());
        env.insert(
            "AW_WORKTIME_INFLUX_BUCKET".to_string(),
            "aw_metrics".to_string(),
        );
        env.insert(
            "AW_WORKTIME_INFLUX_TOKEN".to_string(),
            "CHANGE_ME".to_string(),
        );
        env.insert(
            "AW_WORKTIME_INFLUX_HOSTS".to_string(),
            "HOST-EXAMPLE".to_string(),
        );
        let config = influx_config(&env, "AW_WORKTIME_INFLUX");
        let check = check_influx_env(&config, false);
        assert_eq!(check.status, StatusLevel::Fail);
        assert!(check.summary.contains("AW_WORKTIME_INFLUX_URL"));
    }

    #[test]
    fn summarizes_warn_and_fail() {
        let checks = vec![
            ok("a", "a", json!({})),
            warn("b", "b", json!({})),
            fail("c", "c", json!({})),
        ];
        let (status, counts) = summarize(&checks);
        assert_eq!(status, StatusLevel::Fail);
        assert_eq!(counts.ok, 1);
        assert_eq!(counts.warn, 1);
        assert_eq!(counts.fail, 1);
    }

    #[test]
    fn renders_readiness_act_without_secrets() {
        let checks = vec![ok(
            "env:AW_WORKTIME_INFLUX",
            "Influx runtime env is production-ready",
            json!({ "token_redacted": true }),
        )];
        let report = test_report(checks);
        let markdown = render_markdown(&report);
        assert!(markdown.contains("Акт готовности стенда DetMir"));
        assert!(markdown.contains("env:AW_WORKTIME_INFLUX"));
        assert!(markdown.contains("Ограничения проверки"));
        assert!(markdown.contains("abc123"));
        assert!(!markdown.contains("prod-write-token-value"));
        assert_eq!(readiness_exit_code(StatusLevel::Ok), 0);
        assert_eq!(readiness_exit_code(StatusLevel::Warn), 2);
        assert_eq!(readiness_exit_code(StatusLevel::Fail), 3);
    }

    #[test]
    fn writes_bundle_with_sha256sums() {
        let dir = tempfile::tempdir().unwrap();
        let report = test_report(vec![ok("env:test", "ok", json!({}))]);
        let cli = test_cli();
        write_bundle(dir.path(), &report, &cli).unwrap();
        assert!(dir.path().join("detmir-readiness-latest.json").is_file());
        assert!(dir.path().join("detmir-readiness-act.md").is_file());
        assert!(dir.path().join("detmir-readiness-act.html").is_file());
        assert!(dir.path().join("detmir-readiness-status.json").is_file());
        assert!(dir.path().join("detmir-readiness.prom").is_file());
        let sums = fs::read_to_string(dir.path().join("sha256sums.txt")).unwrap();
        assert!(sums.contains("detmir-readiness-latest.json"));
        assert!(sums.contains("detmir-readiness-act.md"));
        assert!(sums.contains("detmir-readiness-act.html"));
        let status: Value = serde_json::from_str(
            &fs::read_to_string(dir.path().join("detmir-readiness-status.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(status["checksum_verified"], true);
        assert_eq!(status["signature"]["signed"], false);
        assert_eq!(status["signature"]["verified"], false);
        let latest_dir = fs::read_to_string(dir.path().join("latest-dir.txt")).unwrap();
        assert!(latest_dir.contains("2026") || latest_dir.contains("20"));
    }

    #[test]
    fn writes_signed_bundle_when_openssl_available() {
        if !command_exists("openssl") {
            eprintln!("skip signed bundle test: openssl is not available");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let key_path = dir.path().join("signing-key.pem");
        run_command(
            Command::new("openssl")
                .arg("genpkey")
                .arg("-algorithm")
                .arg("RSA")
                .arg("-pkeyopt")
                .arg("rsa_keygen_bits:2048")
                .arg("-out")
                .arg(&key_path),
            "generate test signing key",
        )
        .unwrap();
        let mut cli = test_cli();
        cli.signing_key = Some(key_path);
        cli.require_signature = true;
        let report = test_report(vec![ok("env:test", "ok", json!({}))]);
        write_bundle(dir.path(), &report, &cli).unwrap();
        assert!(dir.path().join("sha256sums.txt.sig").is_file());
        assert!(dir.path().join("public-key.pem").is_file());
        let status: Value = serde_json::from_str(
            &fs::read_to_string(dir.path().join("detmir-readiness-status.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(status["signature"]["signed"], true);
        assert_eq!(status["signature"]["verified"], true);
        assert!(
            status["signature"]["public_key_fingerprint_sha256"]
                .as_str()
                .unwrap_or("")
                .len()
                >= 64
        );
    }

    #[test]
    fn prunes_old_readiness_archives() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("2026-01-01").join("000000Z");
        let fresh_date = Utc::now().format("%Y-%m-%d").to_string();
        let fresh = dir.path().join(&fresh_date).join("000000Z");
        fs::create_dir_all(&old).unwrap();
        fs::create_dir_all(&fresh).unwrap();
        fs::write(old.join("marker"), "old").unwrap();
        fs::write(fresh.join("marker"), "fresh").unwrap();
        prune_old_archives(dir.path(), 30).unwrap();
        assert!(!dir.path().join("2026-01-01").exists());
        assert!(dir.path().join(fresh_date).exists());
    }
}
