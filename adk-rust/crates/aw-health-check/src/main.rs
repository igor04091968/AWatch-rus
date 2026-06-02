use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use reqwest::blocking::Client;
use serde_json::Value;

const ENV_PATH: &str = "/etc/activitywatch/aw-server.env";
const SERVICES: &[&str] = &[
    "activitywatch-server",
    "aw-worktime-api",
    "aw-worktime-ui-bridge",
];
const DLP_TRANSPORT_CHECKS: &[&str] = &[
    "buckets:endpoint-signals",
    "buckets:file-operations",
    "endpoint-self-test-metrics",
];
const AW_DB_HEALTH_CHECKS: &[&str] = &[
    "sqlite:file-size",
    "sqlite:wal-size",
    "aw-session-events:rows",
    "aw-session-events:recent-process-events",
    "windows-config:process-events",
];

#[derive(Debug, Default)]
struct HealthState {
    unhealthy: Vec<String>,
    warnings: Vec<String>,
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
    let env = load_effective_env(Path::new(ENV_PATH))?;
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .no_proxy()
        .build()
        .context("build HTTP client")?;
    let mut state = HealthState::default();

    println!("=== AW Services Health Check ===");
    println!(
        "Timestamp: {}",
        command_stdout("date", &[]).unwrap_or_else(|_| "unknown".to_string())
    );
    println!();

    for service in SERVICES {
        check_service(&mut state, service)?;
    }
    println!();

    let worktime_base = env_first(
        &env,
        "AW_RUS_HEALTH_WORKTIME_API",
        &env_first(&env, "AW_WORKTIME_REPORT_BASE", "http://127.0.0.1:5610"),
    );
    let worktime_url = format!("{}/health", worktime_base.trim_end_matches('/'));
    let worktime_timeout = env_i64(&env, "AW_RUS_HEALTH_WORKTIME_TIMEOUT_SECONDS", 15);
    let aw_timeout = env_i64(&env, "AW_RUS_HEALTH_AW_TIMEOUT_SECONDS", 15);
    let aw_attempts = env_i64(&env, "AW_RUS_HEALTH_AW_ATTEMPTS", 3);
    let settings_timeout = env_i64(&env, "AW_RUS_HEALTH_SETTINGS_TIMEOUT_SECONDS", 15);
    let settings_attempts = env_i64(&env, "AW_RUS_HEALTH_SETTINGS_ATTEMPTS", 3);

    check_api_endpoint(
        &client,
        &mut state,
        "http://127.0.0.1:5600/api/0/info",
        "activitywatch-server",
        aw_timeout,
        aw_attempts,
    );
    check_api_endpoint(
        &client,
        &mut state,
        &worktime_url,
        "aw-worktime-api",
        worktime_timeout,
        2,
    );
    check_dlp_transport_freshness(&mut state, &env);
    check_aw_db_health(&mut state, &env);
    check_expected_setting(
        &client,
        &mut state,
        "startOfDay",
        env_first(&env, "AW_EXPECT_START_OF_DAY", ""),
        "startOfDay",
        settings_timeout,
        settings_attempts,
    );
    check_expected_setting(
        &client,
        &mut state,
        "always_active_pattern",
        env_first(&env, "AW_EXPECT_ALWAYS_ACTIVE_PATTERN", ""),
        "always_active_pattern",
        settings_timeout,
        settings_attempts,
    );
    check_expected_setting(
        &client,
        &mut state,
        "landingpage",
        env_first(&env, "AW_EXPECT_LANDINGPAGE", ""),
        "landingpage",
        settings_timeout,
        settings_attempts,
    );

    println!();
    if state.unhealthy.is_empty() {
        println!("✓ All services are healthy");
        if !state.warnings.is_empty() {
            println!("⚠ Warnings: {}", state.warnings.join(" "));
        }
        Ok(0)
    } else {
        println!("✗ Unhealthy services: {}", state.unhealthy.join(" "));
        Ok(1)
    }
}

fn check_service(state: &mut HealthState, service: &str) -> Result<()> {
    if service == "aw-worktime-ui-bridge" {
        let active = systemctl_success(&["is-active", "--quiet", "aw-worktime-ui-bridge.timer"])?;
        let enabled = systemctl_success(&["is-enabled", "--quiet", "aw-worktime-ui-bridge.timer"])?;
        if active && enabled {
            println!("✓ aw-worktime-ui-bridge.timer is running and enabled");
        } else {
            println!("✗ aw-worktime-ui-bridge.timer is not active/enabled");
            state
                .unhealthy
                .push("aw-worktime-ui-bridge.timer".to_string());
        }
        return Ok(());
    }

    if systemctl_success(&["is-active", "--quiet", service])? {
        println!("✓ {service} is running");
    } else {
        println!("✗ {service} is not running");
        state.unhealthy.push(service.to_string());
    }
    Ok(())
}

fn systemctl_success(args: &[&str]) -> Result<bool> {
    Ok(Command::new("systemctl")
        .args(args)
        .status()
        .with_context(|| format!("run systemctl {}", args.join(" ")))?
        .success())
}

fn check_api_endpoint(
    client: &Client,
    state: &mut HealthState,
    url: &str,
    service_name: &str,
    timeout_seconds: i64,
    attempts: i64,
) {
    for attempt in 1..=attempts.max(1) {
        let result = client
            .get(url)
            .timeout(Duration::from_secs(timeout_seconds.max(1) as u64))
            .send()
            .and_then(|resp| resp.error_for_status())
            .map(|_| ());
        if result.is_ok() {
            println!("✓ {service_name} API endpoint is responding");
            return;
        }
        if attempt < attempts {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    println!("✗ {service_name} API endpoint is not responding");
    state.unhealthy.push(format!("{service_name}-api"));
}

fn check_expected_setting(
    client: &Client,
    state: &mut HealthState,
    key: &str,
    expected: String,
    label: &str,
    timeout_seconds: i64,
    attempts: i64,
) {
    if expected.is_empty() {
        println!("⚠ expected value for {label} is not configured, skipping drift check");
        state.warnings.push(format!("{key}-expected-missing"));
        return;
    }

    match read_setting_value(client, key, timeout_seconds, attempts) {
        Ok(actual) if actual == expected => {
            println!("✓ {label} matches expected value ({expected})");
        }
        Ok(actual) => {
            println!("✗ {label} drift detected: actual='{actual}' expected='{expected}'");
            state.unhealthy.push(format!("setting-{key}"));
        }
        Err(_) => {
            println!("✗ failed to read setting {label}");
            state.unhealthy.push(format!("setting-{key}"));
        }
    }
}

fn read_setting_value(
    client: &Client,
    key: &str,
    timeout_seconds: i64,
    attempts: i64,
) -> Result<String> {
    let url = format!("http://127.0.0.1:5600/api/0/settings/{key}");
    for attempt in 1..=attempts.max(1) {
        let result = client
            .get(&url)
            .timeout(Duration::from_secs(timeout_seconds.max(1) as u64))
            .send()
            .and_then(|resp| resp.error_for_status())
            .and_then(|resp| resp.json::<Value>());
        match result {
            Ok(value) => return Ok(json_value_to_shell_print(value)),
            Err(err) if attempt >= attempts => return Err(anyhow!(err)),
            Err(_) => std::thread::sleep(Duration::from_secs(1)),
        }
    }
    Err(anyhow!("setting read exhausted"))
}

fn check_dlp_transport_freshness(state: &mut HealthState, env: &HashMap<String, String>) {
    let dlp_health = env_first(env, "DLP_HEALTH_BIN", "/usr/local/bin/dlp-health-check");
    if !is_executable(Path::new(&dlp_health)) {
        println!("⚠ dlp-health-check is not available, skipping DLP transport freshness checks");
        state.warnings.push("dlp-health-check-missing".to_string());
        return;
    }

    let output = match Command::new(&dlp_health).arg("--json").output() {
        Ok(output) => output,
        Err(_) => {
            println!(
                "⚠ dlp-health-check did not return JSON, skipping DLP transport freshness checks"
            );
            state.warnings.push("dlp-health-check-empty".to_string());
            return;
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        println!("⚠ dlp-health-check did not return JSON, skipping DLP transport freshness checks");
        state.warnings.push("dlp-health-check-empty".to_string());
        return;
    }
    let payload = match serde_json::from_str::<Value>(&stdout) {
        Ok(payload) => payload,
        Err(_) => {
            println!(
                "⚠ dlp-health-check did not return JSON, skipping DLP transport freshness checks"
            );
            state.warnings.push("dlp-health-check-empty".to_string());
            return;
        }
    };

    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    for result in payload
        .get("results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let name = result.get("name").and_then(Value::as_str).unwrap_or("");
        if !DLP_TRANSPORT_CHECKS.contains(&name) {
            continue;
        }
        let status = result.get("status").and_then(Value::as_str).unwrap_or("");
        let summary = result.get("summary").and_then(Value::as_str).unwrap_or("");
        if status == "fail" {
            errors.push(format!("{name}:{summary}"));
        } else if status == "warn" {
            warnings.push(format!("{name}:{summary}"));
        }
    }

    if errors.is_empty() {
        println!("✓ DLP transport freshness check passed");
    } else {
        println!("✗ DLP transport freshness check failed");
        state.unhealthy.push("dlp-transport".to_string());
    }
    if !errors.is_empty() {
        println!("  errors: {}", errors.join(", "));
    }
    if !warnings.is_empty() {
        let text = warnings.join(", ");
        println!("  warnings: {text}");
        state.warnings.push(text);
    }
}

fn check_aw_db_health(state: &mut HealthState, env: &HashMap<String, String>) {
    let aw_db_health = env_first(env, "AW_DB_HEALTH_BIN", "/usr/local/bin/aw-db-health");
    if !is_executable(Path::new(&aw_db_health)) {
        println!("⚠ aw-db-health is not available, skipping AW DB growth checks");
        state.warnings.push("aw-db-health-missing".to_string());
        return;
    }

    let output = match Command::new(&aw_db_health).arg("--json").output() {
        Ok(output) => output,
        Err(_) => {
            println!("⚠ aw-db-health did not return JSON, skipping AW DB growth checks");
            state.warnings.push("aw-db-health-empty".to_string());
            return;
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        println!("⚠ aw-db-health did not return JSON, skipping AW DB growth checks");
        state.warnings.push("aw-db-health-empty".to_string());
        return;
    }
    let payload = match serde_json::from_str::<Value>(&stdout) {
        Ok(payload) => payload,
        Err(_) => {
            println!("⚠ aw-db-health did not return JSON, skipping AW DB growth checks");
            state.warnings.push("aw-db-health-empty".to_string());
            return;
        }
    };

    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    for result in payload
        .get("results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let name = result.get("name").and_then(Value::as_str).unwrap_or("");
        if !AW_DB_HEALTH_CHECKS.contains(&name) {
            continue;
        }
        let status = result.get("status").and_then(Value::as_str).unwrap_or("");
        let summary = result.get("summary").and_then(Value::as_str).unwrap_or("");
        if status == "fail" {
            errors.push(format!("{name}:{summary}"));
        } else if status == "warn" {
            warnings.push(format!("{name}:{summary}"));
        }
    }

    if errors.is_empty() {
        println!("✓ AW DB growth guard passed");
    } else {
        println!("✗ AW DB growth guard failed");
        state.unhealthy.push("aw-db-health".to_string());
    }
    if !errors.is_empty() {
        println!("  errors: {}", errors.join(", "));
    }
    if !warnings.is_empty() {
        let text = warnings.join(", ");
        println!("  warnings: {text}");
        state.warnings.push(text);
    }
}

fn command_stdout(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program).args(args).output()?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn is_executable(path: &Path) -> bool {
    path.is_file()
}

fn load_effective_env(path: &Path) -> Result<HashMap<String, String>> {
    let mut env = std::env::vars().collect::<HashMap<_, _>>();
    if path.is_file() {
        for (key, value) in parse_env_file(&fs::read_to_string(path)?) {
            env.insert(key, value);
        }
    }
    Ok(env)
}

fn parse_env_file(text: &str) -> HashMap<String, String> {
    let mut values = HashMap::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || !line.contains('=') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty()
            || !key
                .chars()
                .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
        {
            continue;
        }
        values.insert(key.to_string(), unquote_env_value(value.trim()));
    }
    values
}

fn unquote_env_value(value: &str) -> String {
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        if (bytes[0] == b'"' && bytes[value.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[value.len() - 1] == b'\'')
        {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}

fn env_first(env: &HashMap<String, String>, key: &str, default: &str) -> String {
    env.get(key)
        .filter(|value| !value.is_empty())
        .cloned()
        .unwrap_or_else(|| default.to_string())
}

fn env_i64(env: &HashMap<String, String>, key: &str, default: i64) -> i64 {
    env.get(key)
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(default)
}

fn json_value_to_shell_print(value: Value) -> String {
    match value {
        Value::String(value) => value,
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_env_file() {
        let parsed = parse_env_file(
            r#"
            # comment
            AW_EXPECT_START_OF_DAY="00:00"
            AW_EXPECT_LANDINGPAGE=/#/activity/SHARKON2025/view/
            BAD KEY=value
            "#,
        );
        assert_eq!(parsed.get("AW_EXPECT_START_OF_DAY").unwrap(), "00:00");
        assert_eq!(
            parsed.get("AW_EXPECT_LANDINGPAGE").unwrap(),
            "/#/activity/SHARKON2025/view/"
        );
        assert!(!parsed.contains_key("BAD KEY"));
    }

    #[test]
    fn formats_json_setting_like_python_print_json_load() {
        assert_eq!(
            json_value_to_shell_print(Value::String("00:00".to_string())),
            "00:00"
        );
        assert_eq!(json_value_to_shell_print(Value::Bool(true)), "true");
    }
}
