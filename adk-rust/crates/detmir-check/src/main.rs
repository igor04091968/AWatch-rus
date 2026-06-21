use std::io::Read;
use std::net::{SocketAddr, TcpStream};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use clap::Parser;
use detmir_aw_client::ActivityWatchClient;
use detmir_core::{exit_codes, now_utc_rfc3339};
use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderName, HeaderValue};
use serde::Serialize;
use serde_json::Value;

const DEFAULT_AW_API: &str = "http://192.0.2.13:5600/api/0";
const DEFAULT_WORKTIME_URL: &str = "http://192.0.2.13:5610";
const DEFAULT_ONE_C_URL: &str = "http://192.0.2.2:8710";
const DEFAULT_RDP_HOST: &str = "198.51.100.18";
const DEFAULT_HOSTNAME: &str = "HOST-EXAMPLE";
const DEFAULT_GATEWAY_HOST: &str = "detmir.example.local";

#[derive(Debug, Parser)]
#[command(about = "Autonomous read-only DetMir contour check from Proxmox.")]
struct Cli {
    #[arg(long)]
    json: bool,

    #[arg(long, default_value = DEFAULT_AW_API)]
    aw_api: String,

    #[arg(long, default_value = DEFAULT_WORKTIME_URL)]
    worktime_url: String,

    #[arg(long, default_value = DEFAULT_ONE_C_URL)]
    one_c_url: String,

    #[arg(long, default_value = DEFAULT_RDP_HOST)]
    rdp_host: String,

    #[arg(long, default_value = DEFAULT_HOSTNAME)]
    hostname: String,

    #[arg(long, default_value = DEFAULT_GATEWAY_HOST)]
    gateway_host: String,

    #[arg(long, default_value = "https://127.0.0.1")]
    portal_url: String,

    #[arg(long, default_value_t = 5)]
    service_timeout_seconds: u64,

    #[arg(long, default_value_t = 8)]
    bucket_timeout_seconds: u64,

    #[arg(long, default_value_t = 3.0)]
    tcp_timeout_seconds: f64,

    #[arg(long, default_value_t = 201)]
    grafana_ct_id: u32,

    #[arg(long, default_value = "/var/lib/detmir-grafana-check/latest.json")]
    grafana_check_json: String,

    #[arg(long, default_value_t = 30 * 60)]
    grafana_check_max_age_seconds: i64,

    #[arg(long, default_value_t = false)]
    disable_grafana_check: bool,

    #[arg(long, default_value = "disabled")]
    security_events_backend: String,

    #[arg(long, default_value = "http://127.0.0.1:8123")]
    clickhouse_url: String,

    #[arg(long, default_value = "analytics_1c")]
    clickhouse_database: String,

    #[arg(long, default_value = "default")]
    clickhouse_user: String,

    #[arg(long, default_value = "")]
    clickhouse_password: String,

    #[arg(long, default_value = "detmir-dlp")]
    dlp_command: String,

    #[arg(long, default_value_t = 45)]
    dlp_timeout_seconds: u64,

    #[arg(long, default_value_t = false)]
    disable_dlp_health_check: bool,

    #[arg(long, default_value_t = false)]
    disable_portal_check: bool,
}

#[derive(Debug, Clone, Copy)]
enum BucketMode {
    Fresh,
    InteractiveFresh,
    InactiveOk,
    EventDriven,
}

impl BucketMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::InteractiveFresh => "interactive_fresh",
            Self::InactiveOk => "inactive_ok",
            Self::EventDriven => "event_driven",
        }
    }
}

#[derive(Debug)]
struct BucketSpec {
    label: &'static str,
    bucket: String,
    max_age_seconds: Option<i64>,
    mode: BucketMode,
}

#[derive(Debug, Serialize)]
struct CheckReport {
    ok: bool,
    #[serde(rename = "generatedAtUtc")]
    generated_at_utc: String,
    services: Vec<ServiceCheck>,
    buckets: Vec<BucketCheck>,
    summary: CheckSummary,
}

#[derive(Debug, Serialize)]
struct CheckSummary {
    bucket_ok: usize,
    bucket_stale: usize,
    bucket_dead: usize,
    service_failures: usize,
    service_warnings: usize,
}

#[derive(Debug, Serialize)]
struct ServiceCheck {
    name: String,
    required: bool,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct BucketCheck {
    label: String,
    bucket: String,
    mode: String,
    status: String,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    event_count_sample: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    latest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    age_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn env_or_default(name: &str, default: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn env_flag_enabled(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| parse_env_flag(&value))
        .unwrap_or(false)
}

fn parse_env_flag(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn bucket_specs(hostname: &str) -> Vec<BucketSpec> {
    vec![
        BucketSpec {
            label: "AFK watcher",
            bucket: format!("aw-watcher-afk_{hostname}"),
            max_age_seconds: Some(15 * 60),
            mode: BucketMode::InteractiveFresh,
        },
        BucketSpec {
            label: "Window watcher",
            bucket: format!("aw-watcher-window_{hostname}"),
            max_age_seconds: Some(2 * 60 * 60),
            mode: BucketMode::InactiveOk,
        },
        BucketSpec {
            label: "Worktime sessions",
            bucket: format!("aw-worktime-sessions_{hostname}"),
            max_age_seconds: Some(5 * 60),
            mode: BucketMode::Fresh,
        },
        BucketSpec {
            label: "Session events",
            bucket: format!("aw-session-events_{hostname}"),
            max_age_seconds: None,
            mode: BucketMode::EventDriven,
        },
        BucketSpec {
            label: "DLP signals",
            bucket: format!("aw-dlp-endpoint-signals_{hostname}"),
            max_age_seconds: Some(10 * 60),
            mode: BucketMode::InteractiveFresh,
        },
        BucketSpec {
            label: "DLP incidents",
            bucket: format!("aw-dlp-incidents_{hostname}"),
            max_age_seconds: None,
            mode: BucketMode::EventDriven,
        },
        BucketSpec {
            label: "DLP review",
            bucket: format!("aw-dlp-review_{hostname}"),
            max_age_seconds: None,
            mode: BucketMode::EventDriven,
        },
        BucketSpec {
            label: "DLP rules",
            bucket: format!("aw-dlp-rules_{hostname}"),
            max_age_seconds: None,
            mode: BucketMode::EventDriven,
        },
    ]
}

fn build_headers(items: &[(&str, &str)]) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    for (name, value) in items {
        headers.insert(
            HeaderName::from_bytes(name.as_bytes())?,
            HeaderValue::from_str(value)?,
        );
    }
    Ok(headers)
}

fn portal_headers(args: &Cli) -> HeaderMap {
    let mut headers = build_headers(&[("Host", args.gateway_host.as_str())]).unwrap_or_default();
    if let Some(value) = std::env::var("DETMIR_PORTAL_AUTH_HEADER")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        if let Ok(value) = HeaderValue::from_str(&value) {
            headers.insert(AUTHORIZATION, value);
        }
    }
    headers
}

fn fetch_text(
    url: &str,
    timeout: Duration,
    insecure: bool,
    headers: HeaderMap,
    attempts: usize,
) -> Result<String> {
    let client = Client::builder()
        .timeout(timeout)
        .danger_accept_invalid_certs(insecure)
        .no_proxy()
        .build()
        .context("failed to build HTTP client")?;
    let mut last_error = None;
    for attempt in 0..attempts.max(1) {
        match client.get(url).headers(headers.clone()).send() {
            Ok(response) => match response.error_for_status() {
                Ok(response) => {
                    return response.text().context("failed to read HTTP response body");
                }
                Err(err) => last_error = Some(err.into()),
            },
            Err(err) => last_error = Some(anyhow::anyhow!("{err:#}")),
        }
        if attempt + 1 < attempts.max(1) {
            std::thread::sleep(Duration::from_millis(500));
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("HTTP request failed")))
}

fn service_checks(args: &Cli) -> Vec<ServiceCheck> {
    let timeout = Duration::from_secs(args.service_timeout_seconds);
    let one_c_url = args.one_c_url.trim_end_matches('/');
    let portal_url = args.portal_url.trim_end_matches('/');
    let services = [
        (
            "aw-info",
            format!("{}/info", args.aw_api.trim_end_matches('/')),
            false,
            false,
            HeaderMap::new(),
        ),
        (
            "worktime-today",
            format!(
                "{}/reports/worktime/today",
                args.worktime_url.trim_end_matches('/')
            ),
            false,
            false,
            HeaderMap::new(),
        ),
        (
            "1c-api-health",
            format!("{one_c_url}/api/health"),
            false,
            false,
            HeaderMap::new(),
        ),
        (
            "gateway-healthz",
            format!("{portal_url}/healthz"),
            true,
            true,
            portal_headers(args),
        ),
    ];

    let mut checks = Vec::new();
    for (name, url, insecure, required, headers) in services {
        match fetch_text(&url, timeout, insecure, headers, 2) {
            Ok(raw) => {
                let payload = serde_json::from_str::<Value>(&raw)
                    .unwrap_or_else(|_| Value::String(raw.trim().to_string()));
                checks.push(ServiceCheck {
                    name: name.to_string(),
                    required,
                    ok: true,
                    url: Some(url),
                    payload: Some(payload),
                    error: None,
                });
            }
            Err(err) => checks.push(ServiceCheck {
                name: name.to_string(),
                required,
                ok: false,
                url: Some(url),
                payload: None,
                error: Some(err.to_string()),
            }),
        }
    }

    checks.push(tcp_check(
        &args.rdp_host,
        5985,
        args.tcp_timeout_seconds,
        true,
    ));
    checks.push(tcp_check(
        &args.rdp_host,
        22,
        args.tcp_timeout_seconds,
        true,
    ));
    if !args.disable_grafana_check {
        checks.push(grafana_data_check(args));
    }
    if !args.disable_portal_check {
        checks.extend(portal_checks(args));
    }
    if security_events_clickhouse_enabled(args) {
        checks.push(clickhouse_security_events_check(args));
    }
    if !args.disable_dlp_health_check {
        checks.push(dlp_health_check(args));
    }
    checks
}

fn portal_checks(args: &Cli) -> Vec<ServiceCheck> {
    let base = args.portal_url.trim_end_matches('/');
    [
        ("portal-healthz", "/healthz", true),
        ("portal-readyz", "/readyz", true),
        ("portal-version", "/version", true),
        ("portal-metrics", "/metrics", true),
    ]
    .into_iter()
    .map(|(name, path, required)| {
        let url = format!("{base}{path}");
        let headers = portal_headers(args);
        match fetch_text(
            &url,
            Duration::from_secs(args.service_timeout_seconds),
            true,
            headers,
            2,
        ) {
            Ok(raw) => {
                let payload = serde_json::from_str::<Value>(&raw).unwrap_or_else(|_| {
                    Value::String(raw.lines().next().unwrap_or("").to_string())
                });
                ServiceCheck {
                    name: name.to_string(),
                    required,
                    ok: true,
                    url: Some(url),
                    payload: Some(payload),
                    error: None,
                }
            }
            Err(err) => ServiceCheck {
                name: name.to_string(),
                required,
                ok: false,
                url: Some(url),
                payload: None,
                error: Some(err.to_string()),
            },
        }
    })
    .collect()
}

fn dlp_health_check(args: &Cli) -> ServiceCheck {
    let name = "aw-dlp-health".to_string();
    match run_shell_command_timeout(
        &args.dlp_command,
        Duration::from_secs(args.dlp_timeout_seconds),
    ) {
        Ok(output) if output.timed_out => ServiceCheck {
            name,
            required: true,
            ok: false,
            url: None,
            payload: None,
            error: Some(format!(
                "DLP health command timed out after {} seconds",
                args.dlp_timeout_seconds
            )),
        },
        Ok(output) => {
            let payload = serde_json::from_str::<Value>(&output.stdout).unwrap_or_else(|_| {
                Value::String(output.stdout.lines().next().unwrap_or("").to_string())
            });
            let ok = output.code == Some(0);
            ServiceCheck {
                name,
                required: true,
                ok,
                url: None,
                payload: Some(payload),
                error: if ok {
                    None
                } else {
                    Some(format!(
                        "DLP health command exited with {:?}: {}",
                        output.code,
                        sanitize_command_stderr(&output.stderr)
                    ))
                },
            }
        }
        Err(err) => ServiceCheck {
            name,
            required: true,
            ok: false,
            url: None,
            payload: None,
            error: Some(format!("cannot execute DLP health command: {err:#}")),
        },
    }
}

#[derive(Debug)]
struct CommandOutput {
    code: Option<i32>,
    stdout: String,
    stderr: String,
    timed_out: bool,
}

fn run_shell_command_timeout(command: &str, timeout: Duration) -> Result<CommandOutput> {
    let mut child = Command::new("/bin/sh")
        .arg("-lc")
        .arg(command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn command: {command}"))?;
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().context("command wait failed")? {
            return read_command_output(child, status.code(), false);
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return read_command_output(child, None, true);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn read_command_output(
    mut child: std::process::Child,
    code: Option<i32>,
    timed_out: bool,
) -> Result<CommandOutput> {
    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        pipe.read_to_string(&mut stdout)
            .context("failed to read command stdout")?;
    }
    if let Some(mut pipe) = child.stderr.take() {
        pipe.read_to_string(&mut stderr)
            .context("failed to read command stderr")?;
    }
    Ok(CommandOutput {
        code,
        stdout,
        stderr,
        timed_out,
    })
}

fn sanitize_command_stderr(stderr: &str) -> String {
    let trimmed = stderr.trim();
    if trimmed.is_empty() {
        return "no stderr".to_string();
    }
    trimmed
        .lines()
        .take(3)
        .map(|line| {
            if line.chars().count() > 240 {
                format!("{}...", line.chars().take(240).collect::<String>())
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn security_events_clickhouse_enabled(args: &Cli) -> bool {
    args.security_events_backend
        .trim()
        .eq_ignore_ascii_case("clickhouse")
}

fn clickhouse_security_events_check(args: &Cli) -> ServiceCheck {
    let name = "security-events-clickhouse".to_string();
    let Some(database) = clickhouse_identifier(&args.clickhouse_database) else {
        return ServiceCheck {
            name,
            required: false,
            ok: false,
            url: None,
            payload: None,
            error: Some("invalid ClickHouse database identifier".to_string()),
        };
    };
    let timeout = Duration::from_secs(args.service_timeout_seconds.min(5));
    let sql = format!(
        "SELECT toUInt64(count()) AS events_24h FROM {database}.entity_timeline WHERE ts >= now() - INTERVAL 24 HOUR FORMAT JSONEachRow"
    );
    match clickhouse_query_first(args, &sql, timeout) {
        Ok(payload) => ServiceCheck {
            name,
            required: false,
            ok: true,
            url: Some(args.clickhouse_url.trim_end_matches('/').to_string()),
            payload: Some(payload.unwrap_or_else(|| serde_json::json!({"events_24h": 0}))),
            error: None,
        },
        Err(err) => ServiceCheck {
            name,
            required: false,
            ok: false,
            url: Some(args.clickhouse_url.trim_end_matches('/').to_string()),
            payload: None,
            error: Some(err.to_string()),
        },
    }
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

fn clickhouse_query_first(args: &Cli, sql: &str, timeout: Duration) -> Result<Option<Value>> {
    let client = Client::builder()
        .timeout(timeout)
        .no_proxy()
        .build()
        .context("ClickHouse HTTP client")?;
    let mut request = client
        .post(args.clickhouse_url.trim_end_matches('/'))
        .query(&[("database", args.clickhouse_database.trim())])
        .body(sql.to_string());
    if !args.clickhouse_user.trim().is_empty() {
        request = request.basic_auth(
            args.clickhouse_user.trim().to_string(),
            Some(args.clickhouse_password.clone()),
        );
    }
    let text = request
        .send()
        .context("ClickHouse request")?
        .error_for_status()
        .context("ClickHouse HTTP status")?
        .text()
        .context("ClickHouse response body")?;
    let Some(line) = text.lines().map(str::trim).find(|line| !line.is_empty()) else {
        return Ok(None);
    };
    Ok(Some(
        serde_json::from_str::<Value>(line).context("ClickHouse JSONEachRow")?,
    ))
}

fn grafana_data_check(args: &Cli) -> ServiceCheck {
    let name = "grafana-data".to_string();
    let output = read_grafana_check_json_from_ct(args);
    let output = match output {
        Ok(output) => output,
        Err(err) => {
            return ServiceCheck {
                name,
                required: true,
                ok: false,
                url: None,
                payload: None,
                error: Some(format!("cannot execute pct for Grafana check: {err}")),
            };
        }
    };
    if !output.status.success() {
        return ServiceCheck {
            name,
            required: true,
            ok: false,
            url: None,
            payload: None,
            error: Some(format!(
                "pct grafana check read failed with status {:?}: {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr).trim()
            )),
        };
    }
    let raw = String::from_utf8_lossy(&output.stdout);
    let payload = match serde_json::from_str::<Value>(&raw) {
        Ok(payload) => payload,
        Err(err) => {
            return ServiceCheck {
                name,
                required: true,
                ok: false,
                url: None,
                payload: None,
                error: Some(format!("cannot parse Grafana check JSON: {err}")),
            };
        }
    };
    let check_ok = payload.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let fail_count = payload
        .pointer("/counts/fail")
        .and_then(Value::as_u64)
        .unwrap_or(1);
    let generated_at = payload
        .get("generated_at_utc")
        .and_then(Value::as_str)
        .unwrap_or("");
    let age_seconds = DateTime::parse_from_rfc3339(generated_at)
        .map(|ts| (Utc::now() - ts.with_timezone(&Utc)).num_seconds())
        .unwrap_or(i64::MAX);
    let fresh = (0..=args.grafana_check_max_age_seconds).contains(&age_seconds);
    let ok = check_ok && fail_count == 0 && fresh;
    ServiceCheck {
        name,
        required: true,
        ok,
        url: None,
        payload: Some(serde_json::json!({
            "generated_at_utc": generated_at,
            "age_seconds": age_seconds,
            "max_age_seconds": args.grafana_check_max_age_seconds,
            "check_ok": check_ok,
            "fail_count": fail_count,
            "dashboard_uid": payload.get("dashboard_uid"),
            "counts": payload.get("counts"),
        })),
        error: if ok {
            None
        } else {
            Some(format!(
                "Grafana check unhealthy: ok={check_ok} fail_count={fail_count} age_seconds={age_seconds}"
            ))
        },
    }
}

fn read_grafana_check_json_from_ct(args: &Cli) -> std::io::Result<std::process::Output> {
    let ct_id = args.grafana_ct_id.to_string();
    let pct_args = [
        "exec",
        ct_id.as_str(),
        "--",
        "cat",
        args.grafana_check_json.as_str(),
    ];
    if let Some(custom_pct) = std::env::var("DETMIR_PCT_BIN")
        .ok()
        .filter(|value| !value.is_empty())
    {
        return Command::new(custom_pct).args(pct_args).output();
    }
    let sudo_output = Command::new("/usr/bin/sudo")
        .args(["-n", "/usr/sbin/pct"])
        .args(pct_args)
        .output();
    match sudo_output {
        Ok(output) if output.status.success() => Ok(output),
        Ok(output) if output.status.code() != Some(127) => Ok(output),
        _ => Command::new("/usr/sbin/pct").args(pct_args).output(),
    }
}

fn tcp_check(host: &str, port: u16, timeout_seconds: f64, required: bool) -> ServiceCheck {
    let name = format!("tcp:{host}:{port}");
    let timeout = Duration::from_secs_f64(timeout_seconds);
    let addr = format!("{host}:{port}");
    match addr.parse::<SocketAddr>() {
        Ok(addr) => match TcpStream::connect_timeout(&addr, timeout) {
            Ok(_) => ServiceCheck {
                name,
                required,
                ok: true,
                url: None,
                payload: None,
                error: None,
            },
            Err(err) => ServiceCheck {
                name,
                required,
                ok: false,
                url: None,
                payload: None,
                error: Some(err.to_string()),
            },
        },
        Err(err) => ServiceCheck {
            name,
            required,
            ok: false,
            url: None,
            payload: None,
            error: Some(err.to_string()),
        },
    }
}

fn bucket_health(args: &Cli) -> Result<Vec<BucketCheck>> {
    let client = ActivityWatchClient::new(
        args.aw_api.trim_end_matches('/'),
        Duration::from_secs(args.bucket_timeout_seconds),
    )?;
    let now = Utc::now();
    let interactive_required = interactive_required(&client, &args.hostname, now);
    let mut out = Vec::new();

    for spec in bucket_specs(&args.hostname) {
        if matches!(spec.mode, BucketMode::EventDriven) {
            out.push(BucketCheck {
                label: spec.label.to_string(),
                bucket: spec.bucket,
                mode: spec.mode.as_str().to_string(),
                status: "EVENT-DRIVEN".to_string(),
                ok: true,
                event_count_sample: None,
                latest: None,
                age_seconds: None,
                error: None,
            });
            continue;
        }

        match client.latest_event(&spec.bucket) {
            Ok(Some(event)) => {
                let ts = event.timestamp_utc()?;
                let age = (now - ts).num_seconds();
                let (status, ok) =
                    classify_bucket(spec.mode, spec.max_age_seconds, age, interactive_required);
                out.push(BucketCheck {
                    label: spec.label.to_string(),
                    bucket: spec.bucket,
                    mode: spec.mode.as_str().to_string(),
                    status: status.to_string(),
                    ok,
                    event_count_sample: Some(1),
                    latest: Some(ts.to_rfc3339_opts(SecondsFormat::Secs, true)),
                    age_seconds: Some(age),
                    error: None,
                });
            }
            Ok(None) => {
                let (status, ok) = classify_missing_bucket(spec.mode, interactive_required);
                out.push(BucketCheck {
                    label: spec.label.to_string(),
                    bucket: spec.bucket,
                    mode: spec.mode.as_str().to_string(),
                    status: status.to_string(),
                    ok,
                    event_count_sample: Some(0),
                    latest: None,
                    age_seconds: None,
                    error: None,
                });
            }
            Err(err) => out.push(BucketCheck {
                label: spec.label.to_string(),
                bucket: spec.bucket,
                mode: spec.mode.as_str().to_string(),
                status: "DEAD".to_string(),
                ok: false,
                event_count_sample: None,
                latest: None,
                age_seconds: None,
                error: Some(err.to_string()),
            }),
        }
    }
    Ok(out)
}

fn interactive_required(client: &ActivityWatchClient, hostname: &str, now: DateTime<Utc>) -> bool {
    let bucket = format!("aw-worktime-sessions_{hostname}");
    let Ok(Some(event)) = client.latest_event(&bucket) else {
        return false;
    };
    let Ok(ts) = event.timestamp_utc() else {
        return false;
    };
    let age = (now - ts).num_seconds();
    let fresh = (0..=5 * 60).contains(&age);
    let active = event
        .data
        .get("active")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    fresh && active
}

fn classify_bucket(
    mode: BucketMode,
    max_age_seconds: Option<i64>,
    age_seconds: i64,
    interactive_required: bool,
) -> (&'static str, bool) {
    match mode {
        BucketMode::InactiveOk => {
            if max_age_seconds.is_some_and(|max_age| age_seconds <= max_age) {
                ("FRESH", true)
            } else {
                ("INACTIVE", true)
            }
        }
        BucketMode::InteractiveFresh if !interactive_required => ("INACTIVE", true),
        BucketMode::InteractiveFresh | BucketMode::Fresh => {
            if max_age_seconds.is_some_and(|max_age| age_seconds <= max_age) {
                ("FRESH", true)
            } else {
                ("STALE", false)
            }
        }
        BucketMode::EventDriven => ("EVENT-DRIVEN", true),
    }
}

fn classify_missing_bucket(mode: BucketMode, interactive_required: bool) -> (&'static str, bool) {
    match mode {
        BucketMode::InteractiveFresh if !interactive_required => ("INACTIVE", true),
        BucketMode::EventDriven => ("EVENT-DRIVEN", true),
        _ => ("DEAD", false),
    }
}

fn build_report(args: &Cli) -> Result<CheckReport> {
    let services = service_checks(args);
    let buckets = bucket_health(args)?;
    let bucket_ok = buckets.iter().filter(|bucket| bucket.ok).count();
    let bucket_stale = buckets
        .iter()
        .filter(|bucket| !bucket.ok && bucket.status == "STALE")
        .count();
    let bucket_dead = buckets
        .iter()
        .filter(|bucket| !bucket.ok && bucket.status == "DEAD")
        .count();
    let service_failures = services
        .iter()
        .filter(|service| service.required && !service.ok)
        .count();
    let service_warnings = services
        .iter()
        .filter(|service| !service.required && service.error.is_some())
        .count();
    let ok = service_failures == 0 && bucket_stale == 0 && bucket_dead == 0;

    Ok(CheckReport {
        ok,
        generated_at_utc: now_utc_rfc3339(),
        services,
        buckets,
        summary: CheckSummary {
            bucket_ok,
            bucket_stale,
            bucket_dead,
            service_failures,
            service_warnings,
        },
    })
}

fn render_text(report: &CheckReport) -> String {
    let mut lines = vec![
        "=== DetMir Autonomous Check ===".to_string(),
        format!("OK: {}", if report.ok { "True" } else { "False" }),
        String::new(),
        "Services:".to_string(),
    ];
    for service in &report.services {
        let mark = if service.ok && service.error.is_none() {
            "OK"
        } else if service.required {
            "FAIL"
        } else {
            "WARN"
        };
        lines.push(format!("  {:<18} {}", service.name, mark));
    }
    lines.push(String::new());
    lines.push(format!("{:<20} {:<13} {:>8}", "Bucket", "Status", "Age(s)"));
    lines.push("-".repeat(44));
    for bucket in &report.buckets {
        let age = bucket
            .age_seconds
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_string());
        lines.push(format!(
            "{:<20} {:<13} {:>8}",
            bucket.label, bucket.status, age
        ));
    }
    lines.push(String::new());
    lines.push(format!(
        "Summary: OK={} STALE={} DEAD={} SERVICE_FAIL={} SERVICE_WARN={}",
        report.summary.bucket_ok,
        report.summary.bucket_stale,
        report.summary.bucket_dead,
        report.summary.service_failures,
        report.summary.service_warnings,
    ));
    lines.join("\n")
}

fn main() -> Result<()> {
    let mut args = Cli::parse();
    args.aw_api = env_or_default("DETMIR_AW_API", &args.aw_api);
    args.worktime_url = env_or_default("DETMIR_WORKTIME_URL", &args.worktime_url);
    args.one_c_url = env_or_default("DETMIR_ONE_C_URL", &args.one_c_url);
    args.rdp_host = env_or_default("DETMIR_RDP_HOST", &args.rdp_host);
    args.hostname = env_or_default("DETMIR_HOSTNAME", &args.hostname);
    args.gateway_host = env_or_default("DETMIR_GATEWAY_HOST", &args.gateway_host);
    args.portal_url = env_or_default("DETMIR_PORTAL_URL", &args.portal_url);
    args.grafana_check_json = env_or_default("DETMIR_GRAFANA_CHECK_JSON", &args.grafana_check_json);
    args.security_events_backend =
        env_or_default("SECURITY_EVENTS_BACKEND", &args.security_events_backend);
    args.clickhouse_url = env_or_default("CLICKHOUSE_URL", &args.clickhouse_url);
    args.clickhouse_database = env_or_default("CLICKHOUSE_DATABASE", &args.clickhouse_database);
    args.clickhouse_user = env_or_default("CLICKHOUSE_USER", &args.clickhouse_user);
    args.clickhouse_password = env_or_default("CLICKHOUSE_PASSWORD", &args.clickhouse_password);
    args.dlp_command = env_or_default("DETMIR_DLP_COMMAND", &args.dlp_command);
    if env_flag_enabled("DETMIR_DISABLE_DLP_HEALTH_CHECK") {
        args.disable_dlp_health_check = true;
    }
    if env_flag_enabled("DETMIR_DISABLE_PORTAL_CHECK") {
        args.disable_portal_check = true;
    }

    let report = build_report(&args)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{}", render_text(&report));
    }
    std::process::exit(if report.ok {
        exit_codes::OK
    } else {
        exit_codes::CHECK_FAILED
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interactive_bucket_is_inactive_when_no_interactive_session() {
        let (status, ok) = classify_bucket(
            BucketMode::InteractiveFresh,
            Some(15 * 60),
            3 * 60 * 60,
            false,
        );
        assert_eq!(status, "INACTIVE");
        assert!(ok);
    }

    #[test]
    fn interactive_bucket_is_stale_when_session_is_active() {
        let (status, ok) = classify_bucket(
            BucketMode::InteractiveFresh,
            Some(15 * 60),
            3 * 60 * 60,
            true,
        );
        assert_eq!(status, "STALE");
        assert!(!ok);
    }

    #[test]
    fn missing_interactive_bucket_is_inactive_when_no_interactive_session() {
        let (status, ok) = classify_missing_bucket(BucketMode::InteractiveFresh, false);
        assert_eq!(status, "INACTIVE");
        assert!(ok);
    }

    #[test]
    fn security_events_backend_disabled_by_default() {
        let args = Cli::parse_from(["detmir-check"]);
        assert!(!security_events_clickhouse_enabled(&args));
    }

    #[test]
    fn security_events_backend_clickhouse_is_optional() {
        let args = Cli::parse_from([
            "detmir-check",
            "--security-events-backend",
            "clickhouse",
            "--clickhouse-database",
            "analytics_1c",
        ]);
        assert!(security_events_clickhouse_enabled(&args));
        assert_eq!(
            clickhouse_identifier(&args.clickhouse_database).as_deref(),
            Some("analytics_1c")
        );
    }

    #[test]
    fn env_flag_accepts_true_values_only() {
        assert!(parse_env_flag("true"));
        assert!(parse_env_flag("1"));
        assert!(parse_env_flag("yes"));
        assert!(parse_env_flag("on"));
        assert!(!parse_env_flag("0"));
        assert!(!parse_env_flag("false"));
    }

    #[test]
    fn clickhouse_database_identifier_rejects_injection() {
        assert_eq!(
            clickhouse_identifier("analytics_1c").as_deref(),
            Some("analytics_1c")
        );
        assert!(clickhouse_identifier("analytics_1c;DROP TABLE x").is_none());
        assert!(clickhouse_identifier("").is_none());
    }
}
