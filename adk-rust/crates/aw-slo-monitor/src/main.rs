use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use chrono::{DateTime, SecondsFormat, TimeDelta, Utc};
use clap::{Parser, ValueEnum};
use reqwest::{
    blocking::Client,
    header::{ACCEPT, CONNECTION, HeaderMap, HeaderValue},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const ENV_FILE: &str = "/etc/activitywatch/aw-server.env";
const DEFAULT_STATE_DIR: &str = "/var/lib/activitywatch/slo";
const DEFAULT_HEALTHD_CMD: &str = "/usr/local/bin/aw-rus-healthd-rust --json";
const DEFAULT_HEALTHD_STATE_FILE: &str = "/var/lib/activitywatch/health/aw-rus-health.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum HealthdMode {
    State,
    Run,
}

#[derive(Debug, Parser)]
#[command(about = "AW-RUS rolling SLO sampler")]
struct Cli {
    #[arg(long, default_value = DEFAULT_STATE_DIR)]
    state_dir: PathBuf,

    #[arg(long, default_value = DEFAULT_HEALTHD_CMD)]
    healthd_cmd: String,

    #[arg(long, value_enum, default_value_t = HealthdMode::State)]
    healthd_mode: HealthdMode,

    #[arg(long, default_value = DEFAULT_HEALTHD_STATE_FILE)]
    healthd_state_file: PathBuf,

    #[arg(long, default_value_t = 180)]
    healthd_state_max_age_seconds: i64,

    #[arg(long, default_value = "http://127.0.0.1:5600")]
    aw_base: String,

    #[arg(long, default_value = "http://127.0.0.1:5610")]
    worktime_base: String,

    #[arg(long, default_value = "HOST-EXAMPLE")]
    host: String,

    #[arg(long, default_value_t = 99.97)]
    target_percent: f64,

    #[arg(long, default_value_t = 15)]
    sample_interval_seconds: i64,

    #[arg(long, default_value_t = 35)]
    retention_days: i64,

    #[arg(long, default_value_t = 15.0)]
    http_timeout_seconds: f64,

    #[arg(long, default_value_t = 90)]
    health_timeout_seconds: u64,

    #[arg(long, default_value_t = 500)]
    worktime_html_min_bytes: usize,

    #[arg(long)]
    json: bool,
}

impl Cli {
    fn apply_env(mut self) -> Self {
        let file_env = load_env_file(Path::new(ENV_FILE));
        if !cli_arg_present("--state-dir") {
            self.state_dir = env_path(&file_env, "AW_RUS_SLO_STATE_DIR").unwrap_or(self.state_dir);
        }
        if !cli_arg_present("--healthd-cmd") {
            self.healthd_cmd =
                env_string(&file_env, "AW_RUS_SLO_HEALTHD_CMD").unwrap_or(self.healthd_cmd);
        }
        if !cli_arg_present("--healthd-mode") {
            self.healthd_mode = env_string(&file_env, "AW_RUS_SLO_HEALTHD_MODE")
                .and_then(|value| match value.as_str() {
                    "state" => Some(HealthdMode::State),
                    "run" => Some(HealthdMode::Run),
                    _ => None,
                })
                .unwrap_or(self.healthd_mode);
        }
        if !cli_arg_present("--healthd-state-file") {
            self.healthd_state_file = env_path(&file_env, "AW_RUS_SLO_HEALTHD_STATE_FILE")
                .unwrap_or(self.healthd_state_file);
        }
        if !cli_arg_present("--healthd-state-max-age-seconds") {
            self.healthd_state_max_age_seconds = env_i64(
                &file_env,
                "AW_RUS_SLO_HEALTHD_STATE_MAX_AGE_SECONDS",
                self.healthd_state_max_age_seconds,
            );
        }
        if !cli_arg_present("--aw-base") {
            self.aw_base = env_string(&file_env, "AW_RUS_SLO_AW_BASE")
                .or_else(|| env_string(&file_env, "AW_SERVER_URL"))
                .unwrap_or(self.aw_base);
        }
        if !cli_arg_present("--worktime-base") {
            self.worktime_base = env_string(&file_env, "AW_RUS_SLO_WORKTIME_BASE")
                .or_else(|| env_string(&file_env, "AW_RUS_HEALTH_WORKTIME_API"))
                .unwrap_or(self.worktime_base);
        }
        if !cli_arg_present("--host") {
            self.host = env_string(&file_env, "AW_RUS_SLO_HOST")
                .or_else(|| env_string(&file_env, "AW_MONITORED_WINDOWS_HOSTNAME"))
                .unwrap_or(self.host);
        }
        if !cli_arg_present("--target-percent") {
            self.target_percent =
                env_f64(&file_env, "AW_RUS_SLO_TARGET_PERCENT", self.target_percent);
        }
        if !cli_arg_present("--sample-interval-seconds") {
            self.sample_interval_seconds = env_i64(
                &file_env,
                "AW_RUS_SLO_SAMPLE_INTERVAL_SECONDS",
                self.sample_interval_seconds,
            );
        }
        if !cli_arg_present("--retention-days") {
            self.retention_days =
                env_i64(&file_env, "AW_RUS_SLO_RETENTION_DAYS", self.retention_days);
        }
        if !cli_arg_present("--http-timeout-seconds") {
            self.http_timeout_seconds = env_f64(
                &file_env,
                "AW_RUS_SLO_HTTP_TIMEOUT_SECONDS",
                self.http_timeout_seconds,
            );
        }
        if !cli_arg_present("--health-timeout-seconds") {
            self.health_timeout_seconds = env_u64(
                &file_env,
                "AW_RUS_SLO_HEALTH_TIMEOUT_SECONDS",
                self.health_timeout_seconds,
            );
        }
        if !cli_arg_present("--worktime-html-min-bytes") {
            self.worktime_html_min_bytes = env_usize(
                &file_env,
                "AW_RUS_SLO_WORKTIME_HTML_MIN_BYTES",
                self.worktime_html_min_bytes,
            );
        }
        self.aw_base = self.aw_base.trim_end_matches('/').to_string();
        self.worktime_base = self.worktime_base.trim_end_matches('/').to_string();
        self
    }

    fn urls(&self) -> ProbeUrls {
        let host = urlencoding::encode(&self.host);
        ProbeUrls {
            aw_webui: format!("{}/", self.aw_base),
            today_html: format!(
                "{}/reports/worktime/today?format=html&day=today&host={host}&allow_stale=1",
                self.worktime_base
            ),
            management_html: format!(
                "{}/reports/worktime/management?format=html&day=today&host={host}&allow_stale=1",
                self.worktime_base
            ),
            today_csv: format!(
                "{}/reports/worktime/today?format=csv&day=today&host={host}&allow_stale=1",
                self.worktime_base
            ),
            management_json: format!(
                "{}/reports/worktime/management?format=json&day=today&host={host}&allow_stale=1",
                self.worktime_base
            ),
        }
    }
}

struct ProbeUrls {
    aw_webui: String,
    today_html: String,
    management_html: String,
    today_csv: String,
    management_json: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Sample {
    ts: String,
    ok: bool,
    healthd_ok: bool,
    #[serde(default)]
    healthd_counts: Value,
    probes: BTreeMap<String, Value>,
}

#[derive(Debug, Serialize)]
struct Summary {
    generated_at_utc: String,
    target_percent: f64,
    sample_interval_seconds: i64,
    current_sample: Sample,
    windows: Windows,
}

#[derive(Debug, Serialize)]
struct Windows {
    #[serde(rename = "24h")]
    day_24h: WindowSummary,
    #[serde(rename = "7d")]
    day_7d: WindowSummary,
    #[serde(rename = "30d")]
    day_30d: WindowSummary,
}

#[derive(Debug, Serialize, PartialEq)]
struct WindowSummary {
    window_seconds: i64,
    samples: usize,
    good_samples: usize,
    bad_samples: usize,
    availability_percent: Option<f64>,
    target_percent: f64,
    observed_bad_seconds: i64,
    budget_seconds: i64,
    budget_remaining_seconds: i64,
    status: String,
}

fn main() {
    match run() {
        Ok(ok) => std::process::exit(if ok { 0 } else { 1 }),
        Err(error) => {
            eprintln!("error: {error:#}");
            std::process::exit(2);
        }
    }
}

fn run() -> Result<bool> {
    let cli = Cli::parse().apply_env();
    let client = build_client(cli.http_timeout_seconds)?;
    let sample = build_sample(&cli, &client);
    let sample_path = cli.state_dir.join("aw-slo-samples.jsonl");
    let samples =
        append_and_trim_sample(&sample_path, &sample, cli.retention_days.max(1) * 86_400)?;
    let generated_at = Utc::now();
    let summary = build_summary(&cli, sample, &samples, generated_at);
    let summary_text = render_summary_text(&summary);

    write_atomic(
        &cli.state_dir.join("aw-slo-summary.json"),
        &(serde_json::to_string_pretty(&summary)? + "\n"),
    )?;
    write_atomic(
        &cli.state_dir.join("aw-slo-summary.txt"),
        &(summary_text.clone() + "\n"),
    )?;

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        println!("{summary_text}");
    }
    Ok(summary.current_sample.ok)
}

fn build_client(timeout_seconds: f64) -> Result<Client> {
    let mut headers = HeaderMap::new();
    headers.insert(CONNECTION, HeaderValue::from_static("close"));
    Client::builder()
        .timeout(Duration::from_secs_f64(timeout_seconds.max(0.001)))
        .no_proxy()
        .pool_max_idle_per_host(0)
        .default_headers(headers)
        .build()
        .context("build HTTP client")
}

fn build_sample(cli: &Cli, client: &Client) -> Sample {
    let health = match cli.healthd_mode {
        HealthdMode::Run => run_healthd(&cli.healthd_cmd, cli.health_timeout_seconds),
        HealthdMode::State => {
            read_healthd_state(&cli.healthd_state_file, cli.healthd_state_max_age_seconds)
        }
    };
    let urls = cli.urls();
    let mut probes = BTreeMap::new();
    probes.insert(
        "aw_webui_index".to_string(),
        html_probe(
            client,
            &urls.aw_webui,
            1_000,
            &["ActivityWatch", "id=\"app\"", "ru-patch-v5.js"],
        ),
    );
    probes.insert(
        "worktime_today_html".to_string(),
        html_probe(
            client,
            &urls.today_html,
            cli.worktime_html_min_bytes,
            &["AW-rus", "<html", "</html>"],
        ),
    );
    probes.insert(
        "worktime_management_html".to_string(),
        html_probe(
            client,
            &urls.management_html,
            cli.worktime_html_min_bytes,
            &["AW-rus", "<html", "</html>"],
        ),
    );
    probes.insert(
        "worktime_today_csv".to_string(),
        http_probe(client, &urls.today_csv),
    );
    probes.insert(
        "worktime_management_json".to_string(),
        json_probe(
            client,
            &urls.management_json,
            &[("host", json!(cli.host))],
            &["generated_at_utc", "host", "summary", "rows", "workday"],
        ),
    );

    let health_ok = value_bool(&health, "ok");
    let ok = health_ok && probes.values().all(|probe| value_bool(probe, "ok"));
    Sample {
        ts: iso(Utc::now()),
        ok,
        healthd_ok: health_ok,
        healthd_counts: health.get("counts").cloned().unwrap_or_else(|| json!({})),
        probes,
    }
}

fn fetch_url(client: &Client, url: &str, accept: Option<&str>, attempts: usize) -> Value {
    let started = Instant::now();
    let attempts = attempts.max(1);
    let mut last_error = String::new();
    for attempt in 1..=attempts {
        let mut request = client.get(url);
        if let Some(accept) = accept {
            request = request.header(ACCEPT, accept);
        }
        match request.send() {
            Ok(response) => {
                let status = response.status().as_u16();
                let content_type = response
                    .headers()
                    .get("content-type")
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("")
                    .to_string();
                match response.bytes() {
                    Ok(body) => {
                        let ok = (200..300).contains(&status);
                        return json!({
                            "ok": ok,
                            "status": status,
                            "body_bytes": body.len(),
                            "content_type": content_type,
                            "body": String::from_utf8_lossy(&body),
                            "attempts": attempt,
                            "latency_ms": started.elapsed().as_millis() as i64,
                            "url": url,
                        });
                    }
                    Err(error) => last_error = error.to_string(),
                }
            }
            Err(error) => last_error = error.to_string(),
        }
        if attempt < attempts {
            thread::sleep(Duration::from_secs(1));
        }
    }
    json!({
        "ok": false,
        "error": last_error,
        "body_bytes": 0,
        "attempts": attempts,
        "latency_ms": started.elapsed().as_millis() as i64,
        "url": url,
    })
}

fn public_probe_result(mut result: Value) -> Value {
    if let Some(object) = result.as_object_mut() {
        object.remove("body");
    }
    result
}

fn http_probe(client: &Client, url: &str) -> Value {
    public_probe_result(fetch_url(client, url, None, 2))
}

fn html_probe(client: &Client, url: &str, min_bytes: usize, required_markers: &[&str]) -> Value {
    let mut result = fetch_url(client, url, Some("text/html"), 2);
    if !value_bool(&result, "ok") {
        return public_probe_result(result);
    }
    let body = result
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let content_type = value_string(&result, "content_type");
    let body_bytes = result
        .get("body_bytes")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let missing: Vec<&str> = required_markers
        .iter()
        .copied()
        .filter(|marker| !body.contains(marker))
        .collect();
    if body_bytes < min_bytes {
        set_probe_error(
            &mut result,
            format!("body too small: {body_bytes} < {min_bytes}"),
        );
    } else if !content_type.to_ascii_lowercase().contains("text/html") {
        set_probe_error(
            &mut result,
            format!(
                "unexpected content-type: {}",
                if content_type.is_empty() {
                    "unknown"
                } else {
                    &content_type
                }
            ),
        );
    } else if !missing.is_empty() {
        set_probe_error(
            &mut result,
            format!("missing markers: {}", missing.join(", ")),
        );
        if let Some(object) = result.as_object_mut() {
            object.insert("missing_markers".to_string(), json!(missing));
        }
    }
    public_probe_result(result)
}

fn json_probe(
    client: &Client,
    url: &str,
    expected_values: &[(&str, Value)],
    required_keys: &[&str],
) -> Value {
    let mut result = fetch_url(client, url, Some("application/json"), 2);
    if !value_bool(&result, "ok") {
        return public_probe_result(result);
    }
    let body = result.get("body").and_then(Value::as_str).unwrap_or("");
    let payload: Value = match serde_json::from_str(body) {
        Ok(payload) => payload,
        Err(error) => {
            set_probe_error(&mut result, format!("invalid json: {error}"));
            return public_probe_result(result);
        }
    };
    let Some(object) = payload.as_object() else {
        set_probe_error(&mut result, "json root is not object".to_string());
        return public_probe_result(result);
    };

    let missing_keys: Vec<&str> = required_keys
        .iter()
        .copied()
        .filter(|key| !object.contains_key(*key))
        .collect();
    let mismatched: BTreeMap<&str, Value> = expected_values
        .iter()
        .filter_map(|(key, expected)| {
            let actual = object.get(*key)?;
            if actual == expected {
                None
            } else {
                Some((*key, json!({"expected": expected, "actual": actual})))
            }
        })
        .collect();
    if !missing_keys.is_empty() {
        set_probe_error(
            &mut result,
            format!("missing json keys: {}", missing_keys.join(", ")),
        );
        if let Some(result_object) = result.as_object_mut() {
            result_object.insert("missing_keys".to_string(), json!(missing_keys));
        }
    } else if !mismatched.is_empty() {
        set_probe_error(&mut result, "unexpected json values".to_string());
        if let Some(result_object) = result.as_object_mut() {
            result_object.insert("mismatched_values".to_string(), json!(mismatched));
        }
    } else if let Some(result_object) = result.as_object_mut() {
        let mut keys: Vec<&String> = object.keys().collect();
        keys.sort();
        result_object.insert("json_keys".to_string(), json!(keys));
    }
    public_probe_result(result)
}

fn set_probe_error(result: &mut Value, message: String) {
    if let Some(object) = result.as_object_mut() {
        object.insert("ok".to_string(), json!(false));
        object.insert("error".to_string(), json!(message));
    }
}

fn run_healthd(command: &str, timeout_seconds: u64) -> Value {
    let mut child = match Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => return json!({"ok": false, "returncode": null, "error": error.to_string()}),
    };
    let deadline = Instant::now() + Duration::from_secs(timeout_seconds.max(1));
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut output = String::new();
                if let Some(mut stdout) = child.stdout.take() {
                    let _ = stdout.read_to_string(&mut output);
                }
                if let Some(mut stderr) = child.stderr.take() {
                    let mut err = String::new();
                    let _ = stderr.read_to_string(&mut err);
                    output.push_str(&err);
                }
                let payload: Value = serde_json::from_str(&output).unwrap_or_else(|_| json!({}));
                let ok = status.success() && value_bool(&payload, "ok");
                return json!({
                    "ok": ok,
                    "returncode": status.code(),
                    "counts": payload.get("counts").cloned().unwrap_or_else(|| json!({})),
                    "payload": payload,
                    "output_tail": output_tail(&output),
                });
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return json!({
                        "ok": false,
                        "returncode": null,
                        "error": format!("timeout after {timeout_seconds}s"),
                        "output_tail": "",
                    });
                }
                thread::sleep(Duration::from_millis(100));
            }
            Err(error) => {
                let _ = child.kill();
                return json!({"ok": false, "returncode": null, "error": error.to_string()});
            }
        }
    }
}

fn read_healthd_state(path: &Path, max_age_seconds: i64) -> Value {
    let payload: Value = match fs::read_to_string(path)
        .with_context(|| format!("read {}", path.display()))
        .and_then(|content| serde_json::from_str(&content).context("parse healthd state json"))
    {
        Ok(payload) => payload,
        Err(error) => return json!({"ok": false, "error": error.to_string(), "path": path}),
    };
    let generated = payload
        .get("generated_at_utc")
        .and_then(Value::as_str)
        .and_then(parse_ts);
    let Some(generated) = generated else {
        return json!({"ok": false, "error": "missing or invalid generated_at_utc", "path": path});
    };
    let age_seconds = (Utc::now() - generated).num_seconds().max(0);
    json!({
        "ok": value_bool(&payload, "ok") && age_seconds <= max_age_seconds,
        "counts": payload.get("counts").cloned().unwrap_or_else(|| json!({})),
        "age_seconds": age_seconds,
        "path": path,
        "payload": payload,
    })
}

fn append_and_trim_sample(
    path: &Path,
    sample: &Sample,
    retention_seconds: i64,
) -> Result<Vec<Sample>> {
    let sample_ts = parse_ts(&sample.ts).unwrap_or_else(Utc::now);
    let cutoff = sample_ts - TimeDelta::seconds(retention_seconds.max(1));
    let mut samples = load_samples(path, cutoff);
    samples.push(serde_json::from_value(serde_json::to_value(sample)?)?);

    let mut content = String::new();
    for item in &samples {
        content.push_str(&serde_json::to_string(item)?);
        content.push('\n');
    }
    write_atomic(path, &content)?;
    Ok(samples)
}

fn load_samples(path: &Path, cutoff: DateTime<Utc>) -> Vec<Sample> {
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };
    content
        .lines()
        .filter_map(|line| {
            let sample: Sample = serde_json::from_str(line).ok()?;
            let ts = parse_ts(&sample.ts)?;
            (ts >= cutoff).then_some(sample)
        })
        .collect()
}

fn build_summary(
    cli: &Cli,
    sample: Sample,
    samples: &[Sample],
    generated_at: DateTime<Utc>,
) -> Summary {
    let windows = Windows {
        day_24h: summarize_window(
            samples,
            generated_at,
            86_400,
            cli.sample_interval_seconds,
            cli.target_percent,
        ),
        day_7d: summarize_window(
            samples,
            generated_at,
            7 * 86_400,
            cli.sample_interval_seconds,
            cli.target_percent,
        ),
        day_30d: summarize_window(
            samples,
            generated_at,
            30 * 86_400,
            cli.sample_interval_seconds,
            cli.target_percent,
        ),
    };
    Summary {
        generated_at_utc: iso(generated_at),
        target_percent: cli.target_percent,
        sample_interval_seconds: cli.sample_interval_seconds,
        current_sample: sample,
        windows,
    }
}

fn summarize_window(
    samples: &[Sample],
    now: DateTime<Utc>,
    window_seconds: i64,
    sample_interval_seconds: i64,
    target_percent: f64,
) -> WindowSummary {
    let cutoff = now - TimeDelta::seconds(window_seconds);
    let window: Vec<&Sample> = samples
        .iter()
        .filter(|sample| parse_ts(&sample.ts).is_some_and(|ts| ts >= cutoff))
        .collect();
    let total = window.len();
    let good = window.iter().filter(|sample| sample.ok).count();
    let bad = total - good;
    let availability_percent = if total == 0 {
        None
    } else {
        Some(round5((good as f64 / total as f64) * 100.0))
    };
    let observed_bad_seconds = bad as i64 * sample_interval_seconds;
    let budget_seconds = (window_seconds as f64 * ((100.0 - target_percent) / 100.0)) as i64;
    let budget_remaining_seconds = budget_seconds - observed_bad_seconds;
    let status = if total == 0 {
        "unknown"
    } else if budget_remaining_seconds >= 0 {
        "ok"
    } else {
        "burning"
    }
    .to_string();
    WindowSummary {
        window_seconds,
        samples: total,
        good_samples: good,
        bad_samples: bad,
        availability_percent,
        target_percent,
        observed_bad_seconds,
        budget_seconds,
        budget_remaining_seconds,
        status,
    }
}

fn render_summary_text(summary: &Summary) -> String {
    let mut lines = vec![
        "=== AW-RUS SLO ===".to_string(),
        format!("Timestamp: {}", summary.generated_at_utc),
        format!("Target: {}%", summary.target_percent),
        String::new(),
    ];
    for (name, data) in [
        ("24h", &summary.windows.day_24h),
        ("7d", &summary.windows.day_7d),
        ("30d", &summary.windows.day_30d),
    ] {
        let availability = data
            .availability_percent
            .map(|value| format!("{value:.5}%"))
            .unwrap_or_else(|| "n/a".to_string());
        lines.push(format!(
            "{name}: {} availability={} samples={} bad={} bad_seconds={} budget_remaining_seconds={}",
            data.status,
            availability,
            data.samples,
            data.bad_samples,
            data.observed_bad_seconds,
            data.budget_remaining_seconds
        ));
    }
    lines.push(String::new());
    lines.push(format!(
        "Current sample: {}",
        if summary.current_sample.ok {
            "OK"
        } else {
            "FAIL"
        }
    ));
    for name in [
        "aw_webui_index",
        "worktime_today_html",
        "worktime_management_html",
        "worktime_today_csv",
        "worktime_management_json",
    ] {
        let Some(probe) = summary.current_sample.probes.get(name) else {
            continue;
        };
        let marker = if value_bool(probe, "ok") {
            "OK"
        } else {
            "FAIL"
        };
        let detail = probe
            .get("status")
            .and_then(Value::as_u64)
            .map(|status| status.to_string())
            .or_else(|| {
                probe
                    .get("error")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
            })
            .unwrap_or_default();
        lines.push(format!("- {name}: {marker} {detail}"));
    }
    lines.join("\n")
}

fn write_atomic(path: &Path, content: &str) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("resolve parent for {}", path.display()))?;
    fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    let tmp = parent.join(format!(
        ".{}.tmp.{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("aw-slo"),
        std::process::id()
    ));
    {
        let mut file = File::create(&tmp).with_context(|| format!("create {}", tmp.display()))?;
        file.write_all(content.as_bytes())
            .with_context(|| format!("write {}", tmp.display()))?;
        file.sync_all().ok();
    }
    fs::set_permissions(&tmp, fs::Permissions::from_mode_ext(0o644)).ok();
    fs::rename(&tmp, path)
        .with_context(|| format!("rename {} to {}", tmp.display(), path.display()))?;
    Ok(())
}

trait PermissionsExtCompat {
    fn from_mode_ext(mode: u32) -> Self;
}

impl PermissionsExtCompat for fs::Permissions {
    #[cfg(unix)]
    fn from_mode_ext(mode: u32) -> Self {
        use std::os::unix::fs::PermissionsExt;
        fs::Permissions::from_mode(mode)
    }

    #[cfg(not(unix))]
    fn from_mode_ext(_mode: u32) -> Self {
        fs::metadata(".")
            .map(|metadata| metadata.permissions())
            .unwrap_or_else(|_| fs::Permissions::readonly())
    }
}

fn load_env_file(path: &Path) -> BTreeMap<String, String> {
    let Ok(content) = fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    content
        .lines()
        .filter_map(|raw| {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            Some((
                key.trim().to_string(),
                value
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string(),
            ))
        })
        .collect()
}

fn cli_arg_present(name: &str) -> bool {
    std::env::args_os().skip(1).any(|arg| {
        let Some(value) = arg.to_str() else {
            return false;
        };
        value == name
            || value
                .strip_prefix(name)
                .is_some_and(|rest| rest.starts_with('='))
    })
}

fn env_string(file_env: &BTreeMap<String, String>, name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .or_else(|| file_env.get(name).cloned())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn env_path(file_env: &BTreeMap<String, String>, name: &str) -> Option<PathBuf> {
    env_string(file_env, name).map(PathBuf::from)
}

fn env_i64(file_env: &BTreeMap<String, String>, name: &str, fallback: i64) -> i64 {
    env_string(file_env, name)
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

fn env_u64(file_env: &BTreeMap<String, String>, name: &str, fallback: u64) -> u64 {
    env_string(file_env, name)
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

fn env_usize(file_env: &BTreeMap<String, String>, name: &str, fallback: usize) -> usize {
    env_string(file_env, name)
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

fn env_f64(file_env: &BTreeMap<String, String>, name: &str, fallback: f64) -> f64 {
    env_string(file_env, name)
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

fn parse_ts(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&value.replace('Z', "+00:00"))
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn iso(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(SecondsFormat::Micros, true)
}

fn round5(value: f64) -> f64 {
    (value * 100_000.0).round() / 100_000.0
}

fn value_bool(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn value_string(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn output_tail(output: &str) -> String {
    let chars: Vec<char> = output.chars().collect();
    let start = chars.len().saturating_sub(1000);
    chars[start..].iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(ts: &str, ok: bool) -> Sample {
        Sample {
            ts: ts.to_string(),
            ok,
            healthd_ok: ok,
            healthd_counts: json!({}),
            probes: BTreeMap::new(),
        }
    }

    #[test]
    fn summarize_window_calculates_9997_budget() {
        let now = parse_ts("2026-05-30T12:00:00Z").unwrap();
        let samples = vec![
            sample("2026-05-30T11:58:00Z", true),
            sample("2026-05-30T11:59:00Z", false),
            sample("2026-05-30T12:00:00Z", true),
        ];

        let summary = summarize_window(&samples, now, 86_400, 60, 99.97);

        assert_eq!(summary.samples, 3);
        assert_eq!(summary.good_samples, 2);
        assert_eq!(summary.bad_samples, 1);
        assert_eq!(summary.availability_percent, Some(66.66667));
        assert_eq!(summary.budget_seconds, 25);
        assert_eq!(summary.budget_remaining_seconds, -35);
        assert_eq!(summary.status, "burning");
    }

    #[test]
    fn summarize_window_status_uses_remaining_error_budget_for_partial_window() {
        let now = parse_ts("2026-05-30T12:00:00Z").unwrap();
        let samples = vec![
            sample("2026-05-30T11:59:30Z", false),
            sample("2026-05-30T11:59:45Z", true),
            sample("2026-05-30T12:00:00Z", true),
        ];

        let summary = summarize_window(&samples, now, 86_400, 15, 99.97);

        assert_eq!(summary.availability_percent, Some(66.66667));
        assert_eq!(summary.budget_seconds, 25);
        assert_eq!(summary.budget_remaining_seconds, 10);
        assert_eq!(summary.status, "ok");
    }

    #[test]
    fn render_summary_text_includes_budget_remaining() {
        let mut probes = BTreeMap::new();
        probes.insert(
            "worktime_today_csv".to_string(),
            json!({"ok": true, "status": 200}),
        );
        let sample = Sample {
            ts: "2026-05-30T12:00:00Z".to_string(),
            ok: true,
            healthd_ok: true,
            healthd_counts: json!({}),
            probes,
        };
        let windows = Windows {
            day_24h: WindowSummary {
                window_seconds: 86_400,
                samples: 10,
                good_samples: 10,
                bad_samples: 0,
                availability_percent: Some(100.0),
                target_percent: 99.97,
                observed_bad_seconds: 0,
                budget_seconds: 25,
                budget_remaining_seconds: 25,
                status: "ok".to_string(),
            },
            day_7d: WindowSummary {
                window_seconds: 604_800,
                samples: 10,
                good_samples: 10,
                bad_samples: 0,
                availability_percent: Some(100.0),
                target_percent: 99.97,
                observed_bad_seconds: 0,
                budget_seconds: 181,
                budget_remaining_seconds: 181,
                status: "ok".to_string(),
            },
            day_30d: WindowSummary {
                window_seconds: 2_592_000,
                samples: 10,
                good_samples: 10,
                bad_samples: 0,
                availability_percent: Some(100.0),
                target_percent: 99.97,
                observed_bad_seconds: 0,
                budget_seconds: 777,
                budget_remaining_seconds: 777,
                status: "ok".to_string(),
            },
        };
        let summary = Summary {
            generated_at_utc: "2026-05-30T12:00:00Z".to_string(),
            target_percent: 99.97,
            sample_interval_seconds: 15,
            current_sample: sample,
            windows,
        };

        let text = render_summary_text(&summary);

        assert!(text.contains("Target: 99.97%"));
        assert!(text.contains("24h: ok availability=100.00000%"));
        assert!(text.contains("budget_remaining_seconds=25"));
        assert!(text.contains("- worktime_today_csv: OK 200"));
    }

    #[test]
    fn load_samples_skips_bad_and_old_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("samples.jsonl");
        fs::write(
            &path,
            concat!(
                "{\"ts\":\"2026-05-01T00:00:00Z\",\"ok\":true,\"healthd_ok\":true,\"probes\":{}}\n",
                "not-json\n",
                "{\"ts\":\"2026-05-30T11:59:00Z\",\"ok\":true,\"healthd_ok\":true,\"probes\":{}}\n"
            ),
        )
        .unwrap();

        let samples = load_samples(&path, parse_ts("2026-05-30T11:00:00Z").unwrap());

        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].ts, "2026-05-30T11:59:00Z");
    }
}
