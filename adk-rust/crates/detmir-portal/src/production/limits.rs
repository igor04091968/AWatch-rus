//! Configuration and request-bound validation for production portal routes.
//!
//! RATIONALE: the portal can aggregate reports, evidence and external service
//! payloads. Query and body limits keep pilot installations responsive and make
//! expensive report routes fail closed instead of exhausting memory or blocking
//! the single-process runtime.

use std::collections::BTreeSet;

use anyhow::{Result, anyhow};
use chrono::NaiveDate;
use serde_json::{Value, json};
use tiny_http::StatusCode;
use url::Url;

use crate::{
    Cli, MAX_ALLOWED_PAGE_SIZE, MAX_ALLOWED_REPORT_DATE_RANGE_DAYS, MAX_ALLOWED_REQUEST_BODY_BYTES,
    MAX_ALLOWED_REQUEST_TIMEOUT_SECONDS, query_param,
};

#[derive(Debug)]
pub(crate) struct ApiLimitError {
    pub(crate) status: StatusCode,
    pub(crate) payload: Value,
}

pub(crate) fn validate_portal_config(args: &Cli) -> Result<()> {
    let (host, port) = args
        .bind
        .rsplit_once(':')
        .ok_or_else(|| anyhow!("invalid config bind: expected host:port"))?;
    if host.trim().is_empty() {
        return Err(anyhow!("invalid config host: value is empty"));
    }
    let port = port
        .parse::<u16>()
        .map_err(|_| anyhow!("invalid config port: expected 1..65535"))?;
    if port == 0 {
        return Err(anyhow!("invalid config port: expected 1..65535"));
    }

    // RATIONALE: page and date limits protect heavy report endpoints while
    // preserving monthly pilot reporting. Hard upper bounds prevent accidental
    // production overrides from turning the portal into an unbounded exporter.
    if args.max_page_size == 0 || args.max_page_size > MAX_ALLOWED_PAGE_SIZE {
        return Err(anyhow!(
            "invalid config max_page_size: expected 1..={MAX_ALLOWED_PAGE_SIZE}"
        ));
    }
    if args.default_page_size == 0 || args.default_page_size > args.max_page_size {
        return Err(anyhow!(
            "invalid config default_page_size: expected 1..=max_page_size"
        ));
    }
    if args.max_report_date_range_days <= 0
        || args.max_report_date_range_days > MAX_ALLOWED_REPORT_DATE_RANGE_DAYS
    {
        return Err(anyhow!(
            "invalid config max_report_date_range_days: expected 1..={MAX_ALLOWED_REPORT_DATE_RANGE_DAYS}"
        ));
    }
    if args.request_timeout_seconds == 0
        || args.request_timeout_seconds > MAX_ALLOWED_REQUEST_TIMEOUT_SECONDS
    {
        return Err(anyhow!(
            "invalid config request_timeout_seconds: expected 1..={MAX_ALLOWED_REQUEST_TIMEOUT_SECONDS}"
        ));
    }
    if args.timeout_seconds == 0 || args.timeout_seconds > MAX_ALLOWED_REQUEST_TIMEOUT_SECONDS {
        return Err(anyhow!(
            "invalid config timeout_seconds: expected 1..={MAX_ALLOWED_REQUEST_TIMEOUT_SECONDS}"
        ));
    }
    if args.max_request_body_bytes < 1024
        || args.max_request_body_bytes > MAX_ALLOWED_REQUEST_BODY_BYTES
    {
        return Err(anyhow!(
            "invalid config max_request_body_bytes: expected 1024..={MAX_ALLOWED_REQUEST_BODY_BYTES}"
        ));
    }
    validate_runtime_url("worktime_url", &args.worktime_url)?;
    validate_runtime_url("one_c_url", &args.one_c_url)?;
    validate_probe_command("status_cmd", &args.status_cmd)?;
    validate_probe_command("check_cmd", &args.check_cmd)?;
    validate_probe_command("failed_units_cmd", &args.failed_units_cmd)?;

    // SECURITY: environment and module names can reach metrics/log labels.
    // Restrict them to short ASCII tokens to avoid label injection and runaway
    // cardinality from free-form deployment names.
    if !is_safe_environment_name(&args.environment) {
        return Err(anyhow!(
            "invalid config environment: use 1..32 chars from A-Z, a-z, 0-9, _, -"
        ));
    }
    let modules = enabled_modules(&args.enabled_modules);
    if modules.is_empty() {
        return Err(anyhow!(
            "invalid config enabled_modules: no modules enabled"
        ));
    }
    let allowed = [
        "executive",
        "workforce",
        "security",
        "forensics",
        "admin",
        "ueba",
        "pfsense",
        "reports",
    ];
    for module in modules {
        if !allowed.contains(&module.as_str()) {
            return Err(anyhow!(
                "invalid config enabled_modules: unsupported module {module}"
            ));
        }
    }
    Ok(())
}

fn validate_runtime_url(name: &str, value: &str) -> Result<()> {
    let url = Url::parse(value).map_err(|err| anyhow!("invalid config {name}: {err}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(anyhow!("invalid config {name}: expected http or https URL"));
    }
    let Some(host) = url.host_str() else {
        return Err(anyhow!("invalid config {name}: missing host"));
    };
    if is_placeholder_host(host) {
        return Err(anyhow!(
            "invalid config {name}: placeholder/documentation host is not allowed in production"
        ));
    }
    Ok(())
}

fn is_placeholder_host(host: &str) -> bool {
    let host = host.trim().to_ascii_lowercase();
    host.is_empty()
        || host == "host-example"
        || host.ends_with(".example")
        || host.starts_with("192.0.2.")
        || host.starts_with("198.51.100.")
        || host.starts_with("203.0.113.")
}

fn validate_probe_command(name: &str, command: &str) -> Result<()> {
    let command = command.trim();
    if command.is_empty() {
        return Err(anyhow!("invalid config {name}: command is empty"));
    }
    let forbidden = ['\n', '\r', '\0', ';', '|', '&', '<', '>', '`'];
    if command.contains("$(") || command.chars().any(|ch| forbidden.contains(&ch)) {
        return Err(anyhow!(
            "invalid config {name}: shell control operators are not allowed"
        ));
    }
    Ok(())
}

fn is_safe_environment_name(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && value.len() <= 32
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
}

fn enabled_modules(value: &str) -> BTreeSet<String> {
    value
        .split(',')
        .map(|item| item.trim().to_ascii_lowercase())
        .filter(|item| !item.is_empty())
        .collect()
}

pub(crate) fn is_limited_api_route(path: &str) -> bool {
    matches!(
        path,
        "/api/reports"
            | "/api/executive"
            | "/api/workforce"
            | "/api/security"
            | "/api/forensics"
            | "/api/ueba"
            | "/api/pfsense"
            | "/api/workforce/kpi/explain"
            | "/api/risk/narrative"
            | "/api/actions"
    )
}

pub(crate) fn validate_api_query_limits(
    url: &str,
    args: &Cli,
) -> std::result::Result<(), ApiLimitError> {
    for key in ["page_size", "limit"] {
        if let Some(value) = query_param(url, key) {
            let parsed = value.parse::<u32>().ok();
            if parsed.is_none_or(|page_size| page_size == 0 || page_size > args.max_page_size) {
                return Err(api_limit_error(
                    StatusCode(400),
                    "invalid_page_size",
                    &format!("{key} must be between 1 and {}", args.max_page_size),
                ));
            }
        }
    }

    for (from_key, to_key) in [("date_from", "date_to"), ("from", "to"), ("start", "end")] {
        let Some(from) = query_param(url, from_key).and_then(|value| parse_query_date(&value))
        else {
            continue;
        };
        let Some(to) = query_param(url, to_key).and_then(|value| parse_query_date(&value)) else {
            continue;
        };
        let days = (to - from).num_days().abs() + 1;
        if days > args.max_report_date_range_days {
            return Err(api_limit_error(
                StatusCode(400),
                "report_range_too_large",
                &format!(
                    "report date range must be <= {} days",
                    args.max_report_date_range_days
                ),
            ));
        }
    }
    Ok(())
}

pub(crate) fn parse_query_date(value: &str) -> Option<NaiveDate> {
    let date = value.split('T').next().unwrap_or(value);
    NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()
}

fn api_limit_error(status: StatusCode, code: &str, message: &str) -> ApiLimitError {
    ApiLimitError {
        status,
        payload: json!({
            "ok": false,
            "error_code": code,
            "message": message,
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::{
        DEFAULT_MAX_PAGE_SIZE, DEFAULT_MAX_REPORT_DATE_RANGE_DAYS, DEFAULT_MAX_REQUEST_BODY_BYTES,
        DEFAULT_PAGE_SIZE, DEFAULT_REQUEST_TIMEOUT_SECONDS, DEFAULT_SLOW_REQUEST_LOG_MS,
    };

    fn test_cli(dir: &Path) -> Cli {
        Cli {
            bind: "127.0.0.1:8720".to_string(),
            status_cmd: "true".to_string(),
            check_cmd: "true".to_string(),
            failed_units_cmd: "true".to_string(),
            worktime_url: "http://127.0.0.1".to_string(),
            one_c_url: "http://127.0.0.1".to_string(),
            workforce_policy_path: dir.join("workforce-policy.json"),
            ueba_policy_path: dir.join("ueba-policy.yaml"),
            timeout_seconds: 1,
            max_page_size: DEFAULT_MAX_PAGE_SIZE,
            default_page_size: DEFAULT_PAGE_SIZE,
            max_report_date_range_days: DEFAULT_MAX_REPORT_DATE_RANGE_DAYS,
            request_timeout_seconds: DEFAULT_REQUEST_TIMEOUT_SECONDS,
            max_request_body_bytes: DEFAULT_MAX_REQUEST_BODY_BYTES,
            slow_request_log_ms: DEFAULT_SLOW_REQUEST_LOG_MS,
            environment: "test".to_string(),
            enabled_modules: "executive,workforce,security,forensics,admin".to_string(),
            dlp_module_enabled: true,
            state_dir: dir.join("state"),
            dlp_db_path: dir.join("dlp.sqlite"),
            evidence_root: dir.to_path_buf(),
            readiness_bundle_dir: dir.join("readiness-bundle"),
            evidence_limit: 10,
            evidence_max_bytes: 1024,
            json_smoke: false,
            evidence_only: false,
            evidence_upload_token: None,
            telemetry_api_key: "dummy".to_string(),
            telemetry_store_path: dir.join("telemetry.jsonl"),
            expected_nodes_path: dir.join("expected_nodes.json"),
            security_events_backend: "disabled".to_string(),
            clickhouse_url: "http://127.0.0.1:8123".to_string(),
            clickhouse_database: "analytics_1c".to_string(),
            clickhouse_user: "default".to_string(),
            clickhouse_password: String::new(),
        }
    }

    #[test]
    fn config_validation_rejects_bad_values() {
        let dir = tempfile::tempdir().unwrap();
        let args = test_cli(dir.path());
        assert!(validate_portal_config(&args).is_ok());

        let mut invalid = args.clone();
        invalid.bind = "127.0.0.1:bad".to_string();
        assert!(
            validate_portal_config(&invalid)
                .unwrap_err()
                .to_string()
                .contains("port")
        );

        let mut invalid = args.clone();
        invalid.max_page_size = 0;
        assert!(
            validate_portal_config(&invalid)
                .unwrap_err()
                .to_string()
                .contains("max_page_size")
        );

        let mut invalid = args.clone();
        invalid.max_report_date_range_days = 0;
        assert!(
            validate_portal_config(&invalid)
                .unwrap_err()
                .to_string()
                .contains("max_report_date_range_days")
        );

        let mut invalid = args.clone();
        invalid.request_timeout_seconds = 0;
        assert!(
            validate_portal_config(&invalid)
                .unwrap_err()
                .to_string()
                .contains("request_timeout_seconds")
        );
    }

    #[test]
    fn config_validation_rejects_placeholder_endpoints_and_shell_operators() {
        let dir = tempfile::tempdir().unwrap();
        let args = test_cli(dir.path());

        let mut invalid = args.clone();
        invalid.worktime_url = "http://192.0.2.13:5610".to_string();
        assert!(
            validate_portal_config(&invalid)
                .unwrap_err()
                .to_string()
                .contains("placeholder")
        );

        let mut invalid = args.clone();
        invalid.one_c_url = "http://198.51.100.2:8710".to_string();
        assert!(
            validate_portal_config(&invalid)
                .unwrap_err()
                .to_string()
                .contains("placeholder")
        );

        let mut invalid = args.clone();
        invalid.check_cmd = "detmir-check --json; curl http://127.0.0.1".to_string();
        assert!(
            validate_portal_config(&invalid)
                .unwrap_err()
                .to_string()
                .contains("shell control")
        );
    }

    #[test]
    fn query_limits_reject_page_size_and_report_range() {
        let dir = tempfile::tempdir().unwrap();
        let args = test_cli(dir.path());
        assert!(validate_api_query_limits("/api/reports?page_size=100", &args).is_ok());
        let page_error =
            validate_api_query_limits("/api/reports?page_size=999999", &args).unwrap_err();
        assert_eq!(page_error.status.0, 400);
        assert_eq!(page_error.payload["error_code"], "invalid_page_size");

        let range_error = validate_api_query_limits(
            "/api/reports?date_from=2026-01-01&date_to=2026-12-31",
            &args,
        )
        .unwrap_err();
        assert_eq!(range_error.status.0, 400);
        assert_eq!(range_error.payload["error_code"], "report_range_too_large");
    }
}
