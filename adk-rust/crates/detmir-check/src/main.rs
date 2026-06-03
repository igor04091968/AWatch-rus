use std::net::{SocketAddr, TcpStream};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use clap::Parser;
use detmir_aw_client::ActivityWatchClient;
use detmir_core::{exit_codes, now_utc_rfc3339};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
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
            "https://127.0.0.1/healthz".to_string(),
            true,
            true,
            build_headers(&[("Host", DEFAULT_GATEWAY_HOST)]).unwrap_or_default(),
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
    checks
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
    args.grafana_check_json = env_or_default("DETMIR_GRAFANA_CHECK_JSON", &args.grafana_check_json);

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
}
