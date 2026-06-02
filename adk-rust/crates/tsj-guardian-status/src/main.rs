use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use clap::Parser;
use detmir_state::{DEFAULT_STATE_FILE, NormalizedStatus, read_state};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Parser)]
#[command(about = "Read-only backend helpers for TSJ Guardian Telegram status.")]
struct Cli {
    #[arg(long, default_value = DEFAULT_STATE_FILE)]
    state: PathBuf,

    #[arg(long)]
    json: bool,

    #[arg(long)]
    aw_slo_status_line: bool,

    #[arg(long)]
    status_text: bool,

    #[arg(long)]
    incident_suggestions: bool,

    #[arg(long)]
    incident_defer_decision: bool,

    #[arg(long)]
    escalation_decision: bool,

    #[arg(long)]
    operator_action_decision: bool,

    #[arg(long)]
    dlp_policy_decision: bool,

    #[arg(long)]
    confirmation_decision: bool,

    #[arg(long)]
    autoheal_plan_decision: bool,

    #[arg(long, default_value_t = 1)]
    incident_failure_quorum_checks: i64,

    #[arg(long, default_value_t = 900)]
    operator_timeout_seconds: i64,

    #[arg(long, default_value_t = 900)]
    confirmation_ttl_seconds: i64,

    #[arg(long)]
    aw_slo_summary_command: Option<String>,

    #[arg(long, default_value = "24h")]
    aw_slo_alert_window: String,

    #[arg(long)]
    bot_state: Option<PathBuf>,

    #[arg(long)]
    rollback_file: Option<PathBuf>,

    #[arg(long)]
    pfsense_status_command: Option<String>,

    #[arg(long)]
    now_epoch: Option<i64>,
}

fn detmir_auto_line(status: &NormalizedStatus) -> String {
    let summary = &status.detmir_summary;
    let dlp = &status.dlp_counts;
    format!(
        "- detmir_auto: {severity} check_ok={check_ok} dlp_ok={dlp_ok} \
         bucket_stale={bucket_stale} bucket_dead={bucket_dead} \
         service_fail={service_failures} service_warn={service_warnings} \
         dlp_warn={dlp_warn} dlp_fail={dlp_fail}",
        severity = status.severity,
        check_ok = status.check_ok,
        dlp_ok = status.dlp_ok,
        bucket_stale = summary.bucket_stale.unwrap_or(0),
        bucket_dead = summary.bucket_dead.unwrap_or(0),
        service_failures = summary.service_failures.unwrap_or(0),
        service_warnings = summary.service_warnings.unwrap_or(0),
        dlp_warn = dlp.warn.unwrap_or(0),
        dlp_fail = dlp.fail.unwrap_or(0),
    )
}

fn run_shell_json(command: &str) -> Result<Value> {
    let stdout = run_shell_text(command)?;
    serde_json::from_str(&stdout).context("parse AW SLO summary JSON")
}

fn run_shell_text(command: &str) -> Result<String> {
    let output = Command::new("sh")
        .arg("-lc")
        .arg(command)
        .output()
        .with_context(|| format!("run command: {command}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("command failed rc={:?}: {}", output.status.code(), stderr);
    }
    String::from_utf8(output.stdout).context("decode command stdout as UTF-8")
}

fn value_i64(value: Option<&Value>) -> Option<i64> {
    match value {
        Some(Value::Number(n)) => n.as_i64().or_else(|| n.as_f64().map(|v| v as i64)),
        Some(Value::String(s)) => s.parse::<i64>().ok(),
        _ => None,
    }
}

fn value_f64(value: Option<&Value>) -> Option<f64> {
    match value {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.parse::<f64>().ok(),
        _ => None,
    }
}

fn value_string(value: Option<&Value>, fallback: &str) -> String {
    match value {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(v)) => v.to_string(),
        Some(Value::Null) | None => fallback.to_string(),
        Some(v) => v.to_string(),
    }
}

fn aw_slo_status_line(summary: Option<&Value>, alert_window: &str) -> String {
    let Some(summary) = summary else {
        return "- aw_rus_slo: unavailable".to_string();
    };
    let windows = summary.get("windows").and_then(Value::as_object);
    let window_name = if windows
        .and_then(|w| w.get(alert_window))
        .and_then(Value::as_object)
        .is_some()
    {
        alert_window
    } else {
        "24h"
    };
    let window = windows
        .and_then(|w| w.get(window_name))
        .and_then(Value::as_object);
    let availability = window.and_then(|w| value_f64(w.get("availability_percent")));
    let remaining_raw = window.and_then(|w| w.get("budget_remaining_seconds"));
    let remaining_value = value_i64(remaining_raw).unwrap_or(0);
    let remaining_text = value_string(remaining_raw, "None");
    let samples_text = value_string(window.and_then(|w| w.get("samples")), "None");
    let status = value_string(window.and_then(|w| w.get("status")), "unknown");
    let current = summary.get("current_sample").and_then(Value::as_object);
    let current_ok = current
        .and_then(|c| c.get("ok"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let display_status = if remaining_value < 0 && current_ok {
        "recovered".to_string()
    } else if remaining_value < 0 && !current_ok {
        "fail".to_string()
    } else {
        status
    };
    let availability_text = availability
        .map(|v| format!("{v:.5}%"))
        .unwrap_or_else(|| "n/a".to_string());
    format!(
        "- aw_rus_slo: {display_status} {window_name} current_sample={} availability={availability_text} samples={samples_text} budget_remaining_seconds={remaining_text}",
        if current_ok { "OK" } else { "FAIL" }
    )
}

fn parse_generated_age_seconds(summary: &Value) -> Option<i64> {
    let generated = summary.get("generated_at_utc")?.as_str()?;
    let ts = DateTime::parse_from_rfc3339(&generated.replace('Z', "+00:00"))
        .ok()?
        .with_timezone(&Utc);
    Some((Utc::now() - ts).num_seconds())
}

#[derive(Debug, Serialize)]
struct StatusTextPayload {
    status_text: String,
    pfsense_status: String,
    aw_rus_slo_line: String,
    detmir_auto_line: String,
    rollback_pending_items: usize,
}

#[derive(Debug, Deserialize)]
struct IncidentInput {
    #[serde(default)]
    failures: Vec<String>,
    #[serde(default)]
    state: Value,
}

#[derive(Debug, Serialize)]
struct SuggestionsPayload {
    suggestions: Vec<String>,
}

#[derive(Debug, Serialize)]
struct DeferDecisionPayload {
    defer: bool,
    signature: String,
    failure_streak_signature: String,
    failure_streak_count: i64,
    failure_streak_first_ts: i64,
    reset_failure_streak: bool,
    log_line: Option<String>,
}

#[derive(Debug, Serialize)]
struct EscalationDecisionPayload {
    should_escalate: bool,
    should_fallback: bool,
    timed_out: bool,
    operator_acked: bool,
    age_seconds: i64,
    reason: String,
}

#[derive(Debug, Deserialize)]
struct OperatorActionInput {
    #[serde(default)]
    action: String,
    #[serde(default)]
    state: Value,
}

#[derive(Debug, Serialize)]
struct OperatorActionDecisionPayload {
    requested_action: String,
    canonical_action: String,
    handler: String,
    allowed: bool,
    requires_confirmation: bool,
    risk_level: String,
    reason: String,
    message: Option<String>,
    state_update_hints: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct DlpPolicyInput {
    #[serde(default)]
    policy: Value,
    #[serde(default)]
    target_mode: String,
}

#[derive(Debug, Serialize)]
struct DlpRuleGroupSummary {
    name: String,
    total: usize,
    blocked: usize,
}

#[derive(Debug, Serialize)]
struct DlpPolicyDecisionPayload {
    current_mode: String,
    target_mode: Option<String>,
    changed_count: usize,
    changed_rules: Vec<String>,
    groups: Vec<DlpRuleGroupSummary>,
    updated_policy: Option<Value>,
    reason: String,
}

#[derive(Debug, Deserialize)]
struct ConfirmationInput {
    #[serde(default)]
    kind: String,
    #[serde(default)]
    action: String,
    #[serde(default)]
    code: String,
    #[serde(default)]
    state: Value,
}

#[derive(Debug, Serialize)]
struct ConfirmationDecisionPayload {
    kind: String,
    action: String,
    present: bool,
    expired: bool,
    allowed: bool,
    clear_pending: bool,
    next_stage: Option<String>,
    first_confirmed_ts: Option<i64>,
    reason: String,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AutohealPlanInput {
    #[serde(default)]
    failures: Vec<String>,
    #[serde(default)]
    slo_stale: bool,
}

#[derive(Debug, Serialize)]
struct AutohealPlanPayload {
    failures: Vec<String>,
    slo_only: bool,
    slo_stale: bool,
    include_watchers: bool,
    include_worktime: bool,
    include_windows_dlp: bool,
    server_dlp_failures: Vec<String>,
    run_windows_heal: bool,
    run_server_dlp_heal: bool,
    run_worktime_heal: bool,
    sleep_after_seconds: i64,
    report_triggers: Vec<String>,
    direct_autoheal_target: bool,
}

fn read_json_file(path: &Path) -> Option<Value> {
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn read_stdin_value() -> Result<Value> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .context("read stdin JSON")?;
    serde_json::from_str(&input).context("parse stdin JSON")
}

fn read_stdin_json() -> Result<IncidentInput> {
    serde_json::from_value(read_stdin_value()?).context("parse incident stdin JSON")
}

fn read_operator_action_input() -> Result<OperatorActionInput> {
    serde_json::from_value(read_stdin_value()?).context("parse operator action stdin JSON")
}

fn read_dlp_policy_input() -> Result<DlpPolicyInput> {
    serde_json::from_value(read_stdin_value()?).context("parse DLP policy stdin JSON")
}

fn read_confirmation_input() -> Result<ConfirmationInput> {
    serde_json::from_value(read_stdin_value()?).context("parse confirmation stdin JSON")
}

fn read_autoheal_plan_input() -> Result<AutohealPlanInput> {
    serde_json::from_value(read_stdin_value()?).context("parse autoheal plan stdin JSON")
}

fn failure_signature(failures: &[String]) -> String {
    let mut unique = failures.to_vec();
    unique.sort();
    unique.dedup();
    unique.join("\n")
}

fn has_filesystem_critical(failures: &[String]) -> bool {
    failures
        .iter()
        .any(|line| line.to_ascii_lowercase().contains("filesystem_usage"))
}

fn suggestions_from_failures(failures: &[String]) -> Vec<String> {
    let text = failures.join("\n").to_ascii_lowercase();
    let mut suggestions = Vec::new();
    if text.contains("proxmox_api") {
        suggestions.push("Перезапустить pveproxy/pvedaemon/pve-cluster и проверить порт 8006.");
    }
    if text.contains("pfsense_web") {
        suggestions
            .push("Проверить доступность pfSense 10.10.10.1:8443, перезапустить WebGUI/nginx.");
    }
    if text.contains("pfsense_mcp") {
        suggestions.push("Проверить локальный pfsense-mcp-server.service, bearer token и endpoint 127.0.0.1:3010/mcp.");
    }
    if text.contains("influxdb") {
        suggestions.push("Проверить контейнер InfluxDB и restart сервиса influxdb.");
    }
    if text.contains("grafana") {
        suggestions.push("Проверить grafana-server и NO_PROXY для 10.10.10.0/24.");
    }
    if text.contains("loki") || text.contains("alloy") {
        suggestions.push("Проверить LXC логов и restart сервисов loki/alloy.");
    }
    if text.contains("aw-rus:watcher-") || text.contains("aw-rus:worktime:") {
        suggestions.push("Проверить Windows collector recovery: worktime-session-collector, ActivityWatch Recovery и Launch tasks на 192.168.100.18.");
        suggestions.push("После Windows recovery проверить server-side aw-worktime-autoheal/ui-bridge для пересборки afk/window bucket'ов.");
    }
    if text.contains("aw-rus:dlp-") {
        suggestions.push(
            "Проверить DLP endpoint/fileops collectors и server-side DLP transport на 10.10.10.13.",
        );
    }
    if text.contains("filesystem_usage") {
        suggestions.push("Проверить самые большие каталоги: du -x /var /srv /home, журналы в /var/log и apt cache.");
        suggestions.push("Проверить давление по снапшотам/хранилищу Proxmox и решить: очистка, ротация или расширение диска.");
    }
    if suggestions.is_empty() {
        suggestions.push("Запустить расширенную диагностику: /run check");
    }
    suggestions.into_iter().map(ToOwned::to_owned).collect()
}

fn incident_defer_decision(
    failures: &[String],
    state: &Value,
    threshold: i64,
    now_epoch: i64,
) -> DeferDecisionPayload {
    let signature = failure_signature(failures);
    if threshold <= 1 || has_filesystem_critical(failures) {
        return DeferDecisionPayload {
            defer: false,
            signature,
            failure_streak_signature: value_string(state.get("failure_streak_signature"), ""),
            failure_streak_count: value_i64(state.get("failure_streak_count")).unwrap_or(0),
            failure_streak_first_ts: value_i64(state.get("failure_streak_first_ts")).unwrap_or(0),
            reset_failure_streak: false,
            log_line: None,
        };
    }

    let current_signature = value_string(state.get("failure_streak_signature"), "");
    let mut count = value_i64(state.get("failure_streak_count")).unwrap_or(0);
    let first_ts;
    if signature == current_signature {
        count += 1;
        first_ts = value_i64(state.get("failure_streak_first_ts")).unwrap_or(now_epoch);
    } else {
        count = 1;
        first_ts = now_epoch;
    }
    let defer = count < threshold;
    DeferDecisionPayload {
        defer,
        signature: signature.clone(),
        failure_streak_signature: signature,
        failure_streak_count: count,
        failure_streak_first_ts: first_ts,
        reset_failure_streak: !defer,
        log_line: defer.then(|| {
            format!(
                "Suppressing transient incident streak={count}/{threshold} failures={}",
                failures.len()
            )
        }),
    }
}

fn escalation_decision(
    state: &Value,
    operator_timeout_seconds: i64,
    now_epoch: i64,
) -> EscalationDecisionPayload {
    let Some(pi) = state
        .get("pending_incident")
        .filter(|value| value.is_object())
    else {
        return EscalationDecisionPayload {
            should_escalate: false,
            should_fallback: false,
            timed_out: false,
            operator_acked: false,
            age_seconds: 0,
            reason: "no_pending_incident".to_string(),
        };
    };
    let operator_acked = pi
        .get("operator_acked")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let created_ts = value_i64(pi.get("created_ts")).unwrap_or(now_epoch);
    let age_seconds = (now_epoch - created_ts).max(0);
    if operator_acked {
        return EscalationDecisionPayload {
            should_escalate: false,
            should_fallback: false,
            timed_out: false,
            operator_acked,
            age_seconds,
            reason: "operator_acked".to_string(),
        };
    }
    if age_seconds < operator_timeout_seconds {
        return EscalationDecisionPayload {
            should_escalate: false,
            should_fallback: false,
            timed_out: false,
            operator_acked,
            age_seconds,
            reason: "timeout_not_reached".to_string(),
        };
    }
    let escalated_to_ai = pi
        .get("escalated_to_ai")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let fallback_executed = pi
        .get("fallback_executed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    EscalationDecisionPayload {
        should_escalate: !escalated_to_ai,
        should_fallback: !fallback_executed,
        timed_out: true,
        operator_acked,
        age_seconds,
        reason: "operator_timeout_reached".to_string(),
    }
}

fn strip_run_prefix(action: &str) -> String {
    let trimmed = action.trim();
    if let Some(rest) = trimmed.strip_prefix("/run ") {
        rest.trim().to_string()
    } else {
        trimmed.to_string()
    }
}

fn canonical_operator_action(action: &str) -> Option<&'static str> {
    match strip_run_prefix(action).to_ascii_lowercase().as_str() {
        "check" => Some("check"),
        "aw-dlp-check" | "awrus-dlp-check" => Some("aw-dlp-check"),
        "dlp-mode" | "dlp-policy-mode" => Some("dlp-mode"),
        "dlp-mode-toggle" | "dlp-policy-toggle" => Some("dlp-mode-toggle"),
        "heal" => Some("heal"),
        "ai" | "support" | "techsupport" | "техподдержка" | "тех.поддержка" => {
            Some("support")
        }
        "fallback" => Some("fallback"),
        "updates-check" => Some("updates-check"),
        "updates-install-request" => Some("updates-install-request"),
        "updates-install-confirm" => Some("updates-install-confirm"),
        "updates-rollback-confirm" => Some("updates-rollback-confirm"),
        _ => None,
    }
}

fn operator_action_risk(action: &str) -> &'static str {
    match action {
        "check" | "dlp-mode" => "low",
        "aw-dlp-check" | "updates-check" | "support" => "medium",
        "heal"
        | "fallback"
        | "dlp-mode-toggle"
        | "updates-install-request"
        | "updates-install-confirm" => "high",
        "updates-rollback-confirm" => "critical",
        _ => "unknown",
    }
}

fn operator_action_handler(action: &str) -> &'static str {
    match action {
        "check" => "check_script",
        "aw-dlp-check" => "aw_rus_dlp_check_and_heal",
        "dlp-mode" => "aw_dlp_policy_mode",
        "dlp-mode-toggle" => "aw_dlp_policy_toggle",
        "heal" => "autoheal",
        "support" => "ai_escalation",
        "fallback" => "server_fallback",
        "updates-check" => "updates_check",
        "updates-install-request" => "updates_install_request",
        "updates-install-confirm" => "updates_install_apply",
        "updates-rollback-confirm" => "updates_rollback",
        _ => "unknown",
    }
}

fn operator_action_requires_confirmation(action: &str) -> bool {
    matches!(
        action,
        "updates-install-confirm" | "updates-rollback-confirm"
    )
}

fn state_bool(state: &Value, key: &str) -> bool {
    state.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn operator_action_decision(input: &OperatorActionInput) -> OperatorActionDecisionPayload {
    let requested_action = input.action.trim().to_string();
    let Some(canonical) = canonical_operator_action(&requested_action) else {
        return OperatorActionDecisionPayload {
            requested_action,
            canonical_action: String::new(),
            handler: "unknown".to_string(),
            allowed: false,
            requires_confirmation: false,
            risk_level: "unknown".to_string(),
            reason: "unknown_action".to_string(),
            message: Some("Неизвестное действие.".to_string()),
            state_update_hints: Vec::new(),
        };
    };

    if canonical == "updates-install-confirm"
        && !state_bool(&input.state, "pending_update_install_confirm")
    {
        return OperatorActionDecisionPayload {
            requested_action,
            canonical_action: canonical.to_string(),
            handler: operator_action_handler(canonical).to_string(),
            allowed: false,
            requires_confirmation: true,
            risk_level: operator_action_risk(canonical).to_string(),
            reason: "missing_update_install_confirmation".to_string(),
            message: Some(
                "Нет ожидающего запроса на установку. Сначала нажмите \"Установить критичные и важные обновления\"."
                    .to_string(),
            ),
            state_update_hints: Vec::new(),
        };
    }

    if canonical == "updates-rollback-confirm"
        && !state_bool(&input.state, "pending_rollback_confirm")
    {
        return OperatorActionDecisionPayload {
            requested_action,
            canonical_action: canonical.to_string(),
            handler: operator_action_handler(canonical).to_string(),
            allowed: false,
            requires_confirmation: true,
            risk_level: operator_action_risk(canonical).to_string(),
            reason: "missing_update_rollback_confirmation".to_string(),
            message: Some(
                "Нет ожидающего отката. Откат доступен только после неуспешного ручного обновления."
                    .to_string(),
            ),
            state_update_hints: Vec::new(),
        };
    }

    let state_update_hints = match canonical {
        "updates-install-request" => vec!["set_pending_update_install_confirm".to_string()],
        "updates-install-confirm" => vec![
            "clear_pending_update_install_confirm".to_string(),
            "set_pending_rollback_confirm_if_rollback_items_exist".to_string(),
        ],
        "updates-rollback-confirm" => vec!["clear_pending_rollback_confirm".to_string()],
        _ => Vec::new(),
    };

    OperatorActionDecisionPayload {
        requested_action,
        canonical_action: canonical.to_string(),
        handler: operator_action_handler(canonical).to_string(),
        allowed: true,
        requires_confirmation: operator_action_requires_confirmation(canonical),
        risk_level: operator_action_risk(canonical).to_string(),
        reason: "allowed".to_string(),
        message: None,
        state_update_hints,
    }
}

fn dlp_toggle_group_names() -> [&'static str; 4] {
    ["clipboard", "usb", "print", "email"]
}

fn dlp_group_summaries(policy: &Value) -> Vec<DlpRuleGroupSummary> {
    let Some(endpoint) = policy.get("endpoint").and_then(Value::as_object) else {
        return Vec::new();
    };
    dlp_toggle_group_names()
        .into_iter()
        .filter_map(|key| {
            let rules = endpoint.get(key)?.as_array()?;
            let mut total = 0;
            let mut blocked = 0;
            for rule in rules {
                let Some(rule) = rule.as_object() else {
                    continue;
                };
                if rule.get("enabled").and_then(Value::as_bool) == Some(false) {
                    continue;
                }
                let action = value_string(rule.get("action"), "")
                    .trim()
                    .to_ascii_lowercase();
                if action.is_empty() {
                    continue;
                }
                total += 1;
                if action == "block" {
                    blocked += 1;
                }
            }
            Some(DlpRuleGroupSummary {
                name: format!("endpoint.{key}"),
                total,
                blocked,
            })
        })
        .collect()
}

fn dlp_mode_from_policy(policy: &Value) -> String {
    if let Some(mode) = policy
        .get("_tsj_meta")
        .and_then(Value::as_object)
        .and_then(|meta| meta.get("dlp_mode"))
        .and_then(Value::as_str)
        .map(|mode| mode.trim().to_ascii_lowercase())
        .filter(|mode| mode == "monitor" || mode == "enforce")
    {
        return mode;
    }

    let groups = dlp_group_summaries(policy);
    let total: usize = groups.iter().map(|group| group.total).sum();
    let blocked: usize = groups.iter().map(|group| group.blocked).sum();
    if blocked == 0 {
        "monitor".to_string()
    } else if blocked == total {
        "enforce".to_string()
    } else {
        "mixed".to_string()
    }
}

fn dlp_updated_at_utc(now_epoch: i64) -> String {
    DateTime::<Utc>::from_timestamp(now_epoch, 0)
        .unwrap_or_else(Utc::now)
        .to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn dlp_policy_for_mode(
    policy: &Value,
    target_mode: &str,
    now_epoch: i64,
) -> Result<(Value, usize, Vec<String>)> {
    if !matches!(target_mode, "monitor" | "enforce") {
        anyhow::bail!("unsupported DLP mode: {target_mode}");
    }

    let mut updated = policy.clone();
    let mut changed = 0;
    let mut changed_rules = Vec::new();
    if let Some(endpoint) = updated.get_mut("endpoint").and_then(Value::as_object_mut) {
        for key in dlp_toggle_group_names() {
            let Some(rules) = endpoint.get_mut(key).and_then(Value::as_array_mut) else {
                continue;
            };
            for rule in rules {
                let Some(rule_obj) = rule.as_object_mut() else {
                    continue;
                };
                if rule_obj.get("enabled").and_then(Value::as_bool) == Some(false) {
                    continue;
                }
                let old_action = value_string(rule_obj.get("action"), "alert")
                    .trim()
                    .to_ascii_lowercase();
                let new_action = if target_mode == "monitor" {
                    if old_action == "block" {
                        "alert"
                    } else {
                        old_action.as_str()
                    }
                } else if matches!(old_action.as_str(), "log" | "alert") {
                    "block"
                } else {
                    old_action.as_str()
                };
                if new_action == old_action {
                    continue;
                }
                rule_obj.insert("action".to_string(), Value::String(new_action.to_string()));
                changed += 1;
                if changed_rules.len() < 10 {
                    let rule_id = value_string(rule_obj.get("id"), "?");
                    changed_rules.push(format!(
                        "endpoint.{key}:{rule_id} {old_action}->{new_action}"
                    ));
                }
            }
        }
    }

    let root = updated
        .as_object_mut()
        .context("DLP policy root must be a JSON object")?;
    let meta = root.entry("_tsj_meta").or_insert_with(|| json!({}));
    if !meta.is_object() {
        *meta = json!({});
    }
    let meta_obj = meta.as_object_mut().expect("meta object created above");
    meta_obj.insert(
        "dlp_mode".to_string(),
        Value::String(target_mode.to_string()),
    );
    meta_obj.insert(
        "updated_by".to_string(),
        Value::String("tsj-guardian-bot".to_string()),
    );
    meta_obj.insert(
        "updated_at_utc".to_string(),
        Value::String(dlp_updated_at_utc(now_epoch)),
    );

    Ok((updated, changed, changed_rules))
}

fn dlp_policy_decision(input: &DlpPolicyInput, now_epoch: i64) -> Result<DlpPolicyDecisionPayload> {
    let current_mode = dlp_mode_from_policy(&input.policy);
    let requested = input.target_mode.trim().to_ascii_lowercase();
    let target_mode = match requested.as_str() {
        "" => None,
        "toggle" => Some(if current_mode == "enforce" || current_mode == "mixed" {
            "monitor".to_string()
        } else {
            "enforce".to_string()
        }),
        "monitor" | "enforce" => Some(requested),
        other => anyhow::bail!("unsupported DLP target mode: {other}"),
    };

    let groups = dlp_group_summaries(&input.policy);
    let Some(target_mode) = target_mode else {
        return Ok(DlpPolicyDecisionPayload {
            current_mode,
            target_mode: None,
            changed_count: 0,
            changed_rules: Vec::new(),
            groups,
            updated_policy: None,
            reason: "mode_only".to_string(),
        });
    };

    let (updated_policy, changed_count, changed_rules) =
        dlp_policy_for_mode(&input.policy, &target_mode, now_epoch)?;
    let reason = if changed_count == 0 {
        "no_toggleable_changes"
    } else {
        "policy_plan_ready"
    };
    Ok(DlpPolicyDecisionPayload {
        current_mode,
        target_mode: Some(target_mode),
        changed_count,
        changed_rules,
        groups,
        updated_policy: Some(updated_policy),
        reason: reason.to_string(),
    })
}

fn confirmation_no_pending_message(kind: &str) -> &'static str {
    match kind {
        "pfsense" => "Нет ожидающего изменения pfSense.",
        "openvpn" => "Нет ожидающего запроса на OpenVPN конфиг.",
        "proxmox_restore" => "Нет ожидающего восстановления Proxmox.",
        "proxmox_selection" => "Нет ожидающего выбора узла Proxmox.",
        _ => "Нет ожидающего подтверждения.",
    }
}

fn confirmation_wrong_stage_message(kind: &str) -> &'static str {
    match kind {
        "pfsense" => {
            "Второе подтверждение пока недоступно. Сначала выполните первый шаг подтверждения."
        }
        "openvpn" => {
            "Второе подтверждение пока недоступно. Сначала выполните первый шаг подтверждения."
        }
        _ => "Подтверждение пока недоступно.",
    }
}

fn confirmation_wrong_code_message(kind: &str) -> &'static str {
    match kind {
        "pfsense" => "Неверный код второго подтверждения pfSense.",
        "openvpn" => "Неверный код второго подтверждения OpenVPN-конфига.",
        "proxmox_restore" => "Неверный код подтверждения восстановления Proxmox.",
        _ => "Неверный код подтверждения.",
    }
}

fn confirmation_cancel_message(kind: &str) -> &'static str {
    match kind {
        "pfsense" => "Ожидающее изменение pfSense отменено.",
        "openvpn" => "Ожидающий запрос на OpenVPN конфиг отменён.",
        "proxmox_restore" => "Ожидающее восстановление Proxmox отменено.",
        "proxmox_selection" => "Выбор узла Proxmox отменён.",
        _ => "Ожидающее подтверждение отменено.",
    }
}

fn confirmation_first_already_message(kind: &str, code: &str) -> String {
    match kind {
        "pfsense" => format!(
            "Первое подтверждение уже принято.\nДля второго подтверждения отправьте `/pfsense_apply {code}`."
        ),
        "openvpn" => format!(
            "Первое подтверждение уже принято.\nДля второго подтверждения отправьте `/openvpn_config_apply {code}`."
        ),
        _ => "Первое подтверждение уже принято.".to_string(),
    }
}

fn confirmation_state_object(state: &Value) -> Option<&serde_json::Map<String, Value>> {
    state.as_object().filter(|object| !object.is_empty())
}

fn confirmation_decision(
    input: &ConfirmationInput,
    ttl_seconds: i64,
    now_epoch: i64,
) -> ConfirmationDecisionPayload {
    let kind = input.kind.trim().to_ascii_lowercase();
    let action = input.action.trim().to_ascii_lowercase();
    let state = confirmation_state_object(&input.state);
    let present = state.is_some();
    let created_ts = state
        .and_then(|object| value_i64(object.get("created_ts")))
        .unwrap_or(now_epoch);
    let expired = present && ttl_seconds >= 0 && now_epoch - created_ts > ttl_seconds;
    let stage = state
        .and_then(|object| object.get("stage"))
        .map(|value| value_string(Some(value), ""))
        .unwrap_or_default();
    let confirm_code = state
        .and_then(|object| object.get("confirm_code"))
        .map(|value| value_string(Some(value), ""))
        .unwrap_or_default();

    let base = |allowed: bool,
                clear_pending: bool,
                next_stage: Option<String>,
                first_confirmed_ts: Option<i64>,
                reason: &str,
                message: Option<String>| ConfirmationDecisionPayload {
        kind: kind.clone(),
        action: action.clone(),
        present,
        expired,
        allowed,
        clear_pending,
        next_stage,
        first_confirmed_ts,
        reason: reason.to_string(),
        message,
    };

    if action == "cancel" {
        return base(
            true,
            true,
            None,
            None,
            "cancelled",
            Some(confirmation_cancel_message(&kind).to_string()),
        );
    }

    if !present {
        return base(
            false,
            false,
            None,
            None,
            "no_pending",
            Some(confirmation_no_pending_message(&kind).to_string()),
        );
    }

    if expired {
        return base(
            false,
            true,
            None,
            None,
            "expired",
            Some(confirmation_no_pending_message(&kind).to_string()),
        );
    }

    match action.as_str() {
        "expire" => base(false, false, None, None, "active", None),
        "first_confirm" => {
            if stage != "awaiting_first_confirm" {
                return base(
                    false,
                    false,
                    None,
                    None,
                    "already_first_confirmed",
                    Some(confirmation_first_already_message(&kind, &confirm_code)),
                );
            }
            base(
                true,
                false,
                Some("awaiting_second_confirm".to_string()),
                Some(now_epoch),
                "first_confirmed",
                None,
            )
        }
        "apply" => {
            if kind != "proxmox_restore" && stage != "awaiting_second_confirm" {
                return base(
                    false,
                    false,
                    None,
                    None,
                    "wrong_stage",
                    Some(confirmation_wrong_stage_message(&kind).to_string()),
                );
            }
            if input.code.trim() != confirm_code {
                return base(
                    false,
                    false,
                    None,
                    None,
                    "wrong_code",
                    Some(confirmation_wrong_code_message(&kind).to_string()),
                );
            }
            base(true, false, None, None, "apply_allowed", None)
        }
        _ => base(
            false,
            false,
            None,
            None,
            "unknown_action",
            Some("Неизвестное действие подтверждения.".to_string()),
        ),
    }
}

fn autoheal_plan_decision(input: &AutohealPlanInput) -> AutohealPlanPayload {
    let mut failures: Vec<String> = input
        .failures
        .iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect();
    failures.sort();
    failures.dedup();

    let slo_only = failures.iter().any(|item| item == "slo");
    if slo_only {
        let trigger = if input.slo_stale {
            "- heal trigger: SLO summary stale, check aw-slo-monitor.timer/service"
        } else {
            "- heal trigger: SLO error budget exhausted, no direct autoheal target"
        };
        return AutohealPlanPayload {
            failures,
            slo_only: true,
            slo_stale: input.slo_stale,
            include_watchers: false,
            include_worktime: false,
            include_windows_dlp: false,
            server_dlp_failures: Vec::new(),
            run_windows_heal: false,
            run_server_dlp_heal: false,
            run_worktime_heal: false,
            sleep_after_seconds: 0,
            report_triggers: vec![trigger.to_string()],
            direct_autoheal_target: false,
        };
    }

    let watcher_failures: Vec<String> = failures
        .iter()
        .filter(|item| item.starts_with("watcher-"))
        .cloned()
        .collect();
    let dlp_failures: Vec<String> = failures
        .iter()
        .filter(|item| item.starts_with("dlp-"))
        .cloned()
        .collect();
    let include_worktime = failures.iter().any(|item| item == "worktime");
    let windows_dlp_failures: Vec<String> = dlp_failures
        .iter()
        .filter(|item| matches!(item.as_str(), "dlp-endpoint" | "dlp-fileops-host"))
        .cloned()
        .collect();
    let server_dlp_failures: Vec<String> = dlp_failures
        .iter()
        .filter(|item| !matches!(item.as_str(), "dlp-endpoint" | "dlp-fileops-host"))
        .cloned()
        .collect();

    let include_watchers = !watcher_failures.is_empty();
    let include_windows_dlp = !windows_dlp_failures.is_empty();
    let run_windows_heal = include_watchers || include_worktime || include_windows_dlp;
    let run_server_dlp_heal = !server_dlp_failures.is_empty();
    let run_worktime_heal = include_watchers || include_worktime;
    let sleep_after_seconds = if include_watchers || include_windows_dlp {
        30
    } else {
        5
    };
    let mut report_triggers = Vec::new();
    if run_windows_heal {
        report_triggers.push(
            "- heal trigger: Windows session collectors degraded, starting remediation".to_string(),
        );
    }
    if run_server_dlp_heal {
        report_triggers
            .push("- heal trigger: server-side DLP degraded, starting remediation".to_string());
    }
    if run_worktime_heal {
        report_triggers.push(
            "- heal trigger: worktime/watchers degraded, rebuilding server-side worktime views"
                .to_string(),
        );
    }

    AutohealPlanPayload {
        failures,
        slo_only: false,
        slo_stale: false,
        include_watchers,
        include_worktime,
        include_windows_dlp,
        server_dlp_failures,
        run_windows_heal,
        run_server_dlp_heal,
        run_worktime_heal,
        sleep_after_seconds,
        report_triggers,
        direct_autoheal_target: run_windows_heal || run_server_dlp_heal || run_worktime_heal,
    }
}

fn pfsense_status_lines(command: Option<&str>) -> String {
    let Some(command) = command.map(str::trim).filter(|value| !value.is_empty()) else {
        return "- pfsense_security: status unavailable (command not configured)".to_string();
    };
    match run_shell_text(command) {
        Ok(output) => {
            let lines: Vec<String> = output
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(ToOwned::to_owned)
                .collect();
            if lines.is_empty() {
                "- pfsense_security: status unavailable (empty output)".to_string()
            } else {
                lines.join("\n")
            }
        }
        Err(error) => {
            let text = error.to_string();
            let tail = text
                .chars()
                .rev()
                .take(500)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<String>();
            format!("- pfsense_security: status unavailable ({tail})")
        }
    }
}

fn rollback_pending_count(path: Option<&PathBuf>) -> usize {
    let Some(path) = path else {
        return 0;
    };
    read_json_file(path)
        .and_then(|payload| {
            payload
                .get("pending_rollback")
                .and_then(Value::as_array)
                .map(Vec::len)
        })
        .unwrap_or(0)
}

fn pending_line(state: &Value, key: &str, none: &str) -> String {
    let Some(value) = state.get(key).filter(|value| value.is_object()) else {
        return none.to_string();
    };
    match key {
        "pending_pfsense_change" => format!(
            "- pending_pfsense_change: {} stage={}",
            value_string(value.get("request_id"), "unknown"),
            value_string(value.get("stage"), "unknown")
        ),
        "pending_proxmox_selection" => format!(
            "- pending_proxmox_selection: mode={}",
            value_string(value.get("mode"), "unknown")
        ),
        "pending_proxmox_restore" => format!(
            "- pending_proxmox_restore: {}:{} snapshot={}",
            value_string(value.get("kind"), "unknown"),
            value_string(value.get("guest_id"), "unknown"),
            value_string(value.get("snapshot"), "unknown")
        ),
        "pending_openvpn_config" => format!(
            "- pending_openvpn_config: {} cn={} stage={}",
            value_string(value.get("request_id"), "unknown"),
            value_string(value.get("common_name"), "unknown"),
            value_string(value.get("stage"), "unknown")
        ),
        _ => none.to_string(),
    }
}

fn status_bool_line(state: &Value, key: &str) -> String {
    format!(
        "- {key}: {}",
        state.get(key).and_then(Value::as_bool).unwrap_or(false)
    )
}

fn status_text_payload(
    detmir_state: &Path,
    bot_state_path: Option<&PathBuf>,
    rollback_path: Option<&PathBuf>,
    pfsense_command: Option<&str>,
    slo_command: Option<&str>,
    slo_alert_window: &str,
    now_epoch: i64,
) -> StatusTextPayload {
    let detmir_auto_line = match read_state(detmir_state) {
        Ok(status) => detmir_auto_line(&status),
        Err(error) => format!("- detmir_auto: unavailable ({error})"),
    };
    let summary = match slo_command.map(str::trim) {
        Some(command) if !command.is_empty() => run_shell_json(command).ok(),
        _ => None,
    };
    let aw_rus_slo_line = aw_slo_status_line(summary.as_ref(), slo_alert_window);
    let pfsense_status = pfsense_status_lines(pfsense_command);
    let bot_state = bot_state_path
        .and_then(|path| read_json_file(path))
        .unwrap_or_else(|| json!({}));
    let rollback_pending_items = rollback_pending_count(rollback_path);
    let pending_incident = bot_state
        .get("pending_incident")
        .filter(|value| value.is_object());

    let ppc_line = pending_line(
        &bot_state,
        "pending_pfsense_change",
        "- pending_pfsense_change: none",
    );
    let ovpn_warn_line = if bot_state
        .get("last_openvpn_expiry_signature")
        .and_then(Value::as_str)
        .unwrap_or("")
        .is_empty()
    {
        "- openvpn_expiry_warning_signature: none".to_string()
    } else {
        "- openvpn_expiry_warning_signature: set".to_string()
    };
    let pps_line = pending_line(
        &bot_state,
        "pending_proxmox_selection",
        "- pending_proxmox_selection: none",
    );
    let ppr_line = pending_line(
        &bot_state,
        "pending_proxmox_restore",
        "- pending_proxmox_restore: none",
    );
    let povpn_line = pending_line(
        &bot_state,
        "pending_openvpn_config",
        "- pending_openvpn_config: none",
    );
    let update_line = status_bool_line(&bot_state, "pending_update_install_confirm");
    let rollback_line = status_bool_line(&bot_state, "pending_rollback_confirm");

    let status_text = if let Some(pi) = pending_incident {
        let created_ts = value_i64(pi.get("created_ts")).unwrap_or(now_epoch);
        let age = (now_epoch - created_ts).max(0);
        let failures = pi
            .get("failures")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(|item| value_string(Some(item), ""))
                    .filter(|item| !item.is_empty())
                    .collect::<Vec<_>>()
                    .join(" | ")
            })
            .unwrap_or_default();
        [
            format!(
                "Статус: активный инцидент {}",
                value_string(pi.get("incident_id"), "unknown")
            ),
            pfsense_status.clone(),
            aw_rus_slo_line.clone(),
            detmir_auto_line.clone(),
            format!("- возраст: {age}s"),
            format!(
                "- autoheal attempts: {}",
                value_string(pi.get("autoheal_attempts"), "0")
            ),
            format!(
                "- operator_acked: {}",
                value_string(pi.get("operator_acked"), "false")
            ),
            format!(
                "- escalated_to_ai: {}",
                value_string(pi.get("escalated_to_ai"), "false")
            ),
            format!(
                "- fallback_executed: {}",
                value_string(pi.get("fallback_executed"), "false")
            ),
            format!("- failures: {failures}"),
            ppc_line,
            ovpn_warn_line,
            pps_line,
            ppr_line,
            povpn_line,
            update_line,
            rollback_line,
            format!("- rollback_pending_items: {rollback_pending_items}"),
        ]
        .join("\n")
    } else {
        [
            "Статус: инцидентов нет.".to_string(),
            pfsense_status.clone(),
            aw_rus_slo_line.clone(),
            detmir_auto_line.clone(),
            ppc_line,
            ovpn_warn_line,
            pps_line,
            ppr_line,
            povpn_line,
            update_line,
            rollback_line,
            format!("- rollback_pending_items: {rollback_pending_items}"),
        ]
        .join("\n")
    };

    StatusTextPayload {
        status_text,
        pfsense_status,
        aw_rus_slo_line,
        detmir_auto_line,
        rollback_pending_items,
    }
}

fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.incident_suggestions {
        let input = read_stdin_json()?;
        println!(
            "{}",
            serde_json::to_string_pretty(&SuggestionsPayload {
                suggestions: suggestions_from_failures(&input.failures),
            })?
        );
        return Ok(());
    }
    if cli.incident_defer_decision {
        let input = read_stdin_json()?;
        let payload = incident_defer_decision(
            &input.failures,
            &input.state,
            cli.incident_failure_quorum_checks,
            cli.now_epoch.unwrap_or_else(now_epoch),
        );
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }
    if cli.escalation_decision {
        let input = read_stdin_json()?;
        let payload = escalation_decision(
            &input.state,
            cli.operator_timeout_seconds,
            cli.now_epoch.unwrap_or_else(now_epoch),
        );
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }
    if cli.operator_action_decision {
        let input = read_operator_action_input()?;
        let payload = operator_action_decision(&input);
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }
    if cli.dlp_policy_decision {
        let input = read_dlp_policy_input()?;
        let payload = dlp_policy_decision(&input, cli.now_epoch.unwrap_or_else(now_epoch))?;
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }
    if cli.confirmation_decision {
        let input = read_confirmation_input()?;
        let payload = confirmation_decision(
            &input,
            cli.confirmation_ttl_seconds,
            cli.now_epoch.unwrap_or_else(now_epoch),
        );
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }
    if cli.autoheal_plan_decision {
        let input = read_autoheal_plan_input()?;
        let payload = autoheal_plan_decision(&input);
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }
    if cli.status_text {
        let payload = status_text_payload(
            &cli.state,
            cli.bot_state.as_ref(),
            cli.rollback_file.as_ref(),
            cli.pfsense_status_command.as_deref(),
            cli.aw_slo_summary_command.as_deref(),
            &cli.aw_slo_alert_window,
            cli.now_epoch.unwrap_or_else(now_epoch),
        );
        if cli.json {
            println!("{}", serde_json::to_string_pretty(&payload)?);
        } else {
            println!("{}", payload.status_text);
        }
        return Ok(());
    }
    if cli.aw_slo_status_line {
        let summary = match cli.aw_slo_summary_command.as_deref().map(str::trim) {
            Some(command) if !command.is_empty() => run_shell_json(command).ok(),
            _ => None,
        };
        let line = aw_slo_status_line(summary.as_ref(), &cli.aw_slo_alert_window);
        if cli.json {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "aw_rus_slo_line": line,
                    "generated_age_seconds": summary.as_ref().and_then(parse_generated_age_seconds),
                    "summary": summary,
                }))?
            );
        } else {
            println!("{line}");
        }
        return Ok(());
    }
    let status = read_state(&cli.state)?;
    let line = detmir_auto_line(&status);
    if cli.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "detmir_auto_line": line,
                "normalized": status,
            }))?
        );
    } else {
        println!("{line}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_python_compatible_detmir_line() {
        let raw = r#"{
          "severity": "OK",
          "check_ok": true,
          "dlp_ok": true,
          "needs_heal": false,
          "detmir_summary": {
            "bucket_ok": 8,
            "bucket_stale": 0,
            "bucket_dead": 0,
            "service_failures": 0,
            "service_warnings": 0
          },
          "dlp_counts": {"ok": 22, "warn": 0, "fail": 0}
        }"#;
        let state: detmir_state::DetmirState = serde_json::from_str(raw).unwrap();
        let line = detmir_auto_line(&state.normalize());
        assert_eq!(
            line,
            "- detmir_auto: OK check_ok=true dlp_ok=true bucket_stale=0 bucket_dead=0 service_fail=0 service_warn=0 dlp_warn=0 dlp_fail=0"
        );
    }

    #[test]
    fn json_mode_includes_line_and_normalized_status() {
        let dir = tempfile::tempdir().unwrap();
        let state_path = dir.path().join("state.json");
        std::fs::write(
            &state_path,
            r#"{
              "severity": "FAIL",
              "check_ok": false,
              "dlp_ok": true,
              "needs_heal": true,
              "detmir_summary": {"bucket_stale": 1, "service_failures": 1},
              "dlp_counts": {"warn": 2, "fail": 0}
            }"#,
        )
        .unwrap();
        let status = read_state(&state_path).unwrap();
        let payload = json!({
            "detmir_auto_line": detmir_auto_line(&status),
            "normalized": status,
        });
        assert_eq!(payload["normalized"]["severity"], "FAIL");
        assert!(
            payload["detmir_auto_line"]
                .as_str()
                .unwrap()
                .contains("bucket_stale=1")
        );
    }

    #[test]
    fn renders_aw_slo_recovered_status_line() {
        let summary = json!({
            "generated_at_utc": "2026-06-01T00:00:00Z",
            "windows": {
                "24h": {
                    "status": "fail",
                    "availability_percent": 98.123456,
                    "samples": 42,
                    "budget_remaining_seconds": -12
                }
            },
            "current_sample": {"ok": true}
        });
        assert_eq!(
            aw_slo_status_line(Some(&summary), "24h"),
            "- aw_rus_slo: recovered 24h current_sample=OK availability=98.12346% samples=42 budget_remaining_seconds=-12"
        );
    }

    #[test]
    fn renders_aw_slo_unavailable_without_summary() {
        assert_eq!(aw_slo_status_line(None, "24h"), "- aw_rus_slo: unavailable");
    }

    #[test]
    fn renders_full_status_without_incident() {
        let dir = tempfile::tempdir().unwrap();
        let detmir_path = dir.path().join("detmir.json");
        let bot_state_path = dir.path().join("bot.json");
        let rollback_path = dir.path().join("rollback.json");
        std::fs::write(
            &detmir_path,
            r#"{
              "severity": "OK",
              "check_ok": true,
              "dlp_ok": true,
              "needs_heal": false,
              "detmir_summary": {"bucket_stale": 0, "bucket_dead": 0, "service_failures": 0, "service_warnings": 0},
              "dlp_counts": {"ok": 22, "warn": 0, "fail": 0}
            }"#,
        )
        .unwrap();
        std::fs::write(
            &bot_state_path,
            r#"{
              "pending_pfsense_change": null,
              "pending_openvpn_config": {"request_id": "ovpn-1", "common_name": "user1", "stage": "created"},
              "pending_update_install_confirm": true,
              "pending_rollback_confirm": false,
              "last_openvpn_expiry_signature": "sig"
            }"#,
        )
        .unwrap();
        std::fs::write(&rollback_path, r#"{"pending_rollback":[{"id":"101"}]}"#).unwrap();

        let payload = status_text_payload(
            &detmir_path,
            Some(&bot_state_path),
            Some(&rollback_path),
            None,
            None,
            "24h",
            1000,
        );

        assert!(payload.status_text.starts_with("Статус: инцидентов нет."));
        assert!(payload.status_text.contains("- aw_rus_slo: unavailable"));
        assert!(payload.status_text.contains("- detmir_auto: OK"));
        assert!(
            payload
                .status_text
                .contains("- pending_openvpn_config: ovpn-1 cn=user1 stage=created")
        );
        assert!(
            payload
                .status_text
                .contains("- pending_update_install_confirm: true")
        );
        assert!(payload.status_text.contains("- rollback_pending_items: 1"));
    }

    #[test]
    fn renders_full_status_with_incident() {
        let dir = tempfile::tempdir().unwrap();
        let detmir_path = dir.path().join("detmir.json");
        let bot_state_path = dir.path().join("bot.json");
        std::fs::write(
            &detmir_path,
            r#"{
              "severity": "FAIL",
              "check_ok": false,
              "dlp_ok": true,
              "needs_heal": true,
              "detmir_summary": {"bucket_stale": 2},
              "dlp_counts": {"warn": 1, "fail": 0}
            }"#,
        )
        .unwrap();
        std::fs::write(
            &bot_state_path,
            r#"{
              "pending_incident": {
                "incident_id": "inc-1",
                "created_ts": 900,
                "failures": ["f1", "f2"],
                "suggestions": [],
                "last_autoheal_ts": 0,
                "autoheal_attempts": 2,
                "operator_acked": true,
                "escalated_to_ai": false,
                "fallback_executed": true
              }
            }"#,
        )
        .unwrap();

        let payload = status_text_payload(
            &detmir_path,
            Some(&bot_state_path),
            None,
            None,
            None,
            "24h",
            1000,
        );

        assert!(
            payload
                .status_text
                .contains("Статус: активный инцидент inc-1")
        );
        assert!(payload.status_text.contains("- возраст: 100s"));
        assert!(payload.status_text.contains("- autoheal attempts: 2"));
        assert!(payload.status_text.contains("- operator_acked: true"));
        assert!(payload.status_text.contains("- fallback_executed: true"));
        assert!(payload.status_text.contains("- failures: f1 | f2"));
    }

    #[test]
    fn incident_suggestions_match_filesystem_and_aw_paths() {
        let suggestions = suggestions_from_failures(&[
            "[FAIL] filesystem_usage: /var 96%".to_string(),
            "[FAIL] aw-rus:watcher-window: stale".to_string(),
        ]);

        assert!(
            suggestions
                .iter()
                .any(|line| line.contains("самые большие каталоги"))
        );
        assert!(
            suggestions
                .iter()
                .any(|line| line.contains("Windows collector recovery"))
        );
    }

    #[test]
    fn incident_defer_decision_suppresses_until_quorum() {
        let state = json!({
            "failure_streak_signature": "",
            "failure_streak_count": 0,
            "failure_streak_first_ts": 0
        });
        let failures = vec!["[FAIL] node_13: unavailable".to_string()];

        let first = incident_defer_decision(&failures, &state, 2, 1000);
        assert!(first.defer);
        assert_eq!(first.failure_streak_count, 1);
        assert_eq!(first.failure_streak_first_ts, 1000);

        let state = json!({
            "failure_streak_signature": first.failure_streak_signature,
            "failure_streak_count": first.failure_streak_count,
            "failure_streak_first_ts": first.failure_streak_first_ts
        });
        let second = incident_defer_decision(&failures, &state, 2, 1060);
        assert!(!second.defer);
        assert_eq!(second.failure_streak_count, 2);
        assert!(second.reset_failure_streak);
    }

    #[test]
    fn incident_defer_decision_never_suppresses_filesystem_critical() {
        let decision = incident_defer_decision(
            &["[FAIL] filesystem_usage: /var 96%".to_string()],
            &json!({}),
            3,
            1000,
        );

        assert!(!decision.defer);
        assert!(!decision.reset_failure_streak);
    }

    #[test]
    fn escalation_decision_waits_for_timeout_and_ack() {
        let state = json!({
            "pending_incident": {
                "created_ts": 1000,
                "operator_acked": false,
                "escalated_to_ai": false,
                "fallback_executed": false
            }
        });
        let early = escalation_decision(&state, 900, 1200);
        assert!(!early.should_escalate);
        assert_eq!(early.reason, "timeout_not_reached");

        let timed_out = escalation_decision(&state, 900, 2000);
        assert!(timed_out.timed_out);
        assert!(timed_out.should_escalate);
        assert!(timed_out.should_fallback);

        let acked = escalation_decision(
            &json!({"pending_incident": {"created_ts": 1000, "operator_acked": true}}),
            900,
            2000,
        );
        assert!(!acked.should_escalate);
        assert_eq!(acked.reason, "operator_acked");
    }

    #[test]
    fn operator_action_decision_normalizes_aliases() {
        let decision = operator_action_decision(&OperatorActionInput {
            action: "techsupport".to_string(),
            state: json!({}),
        });

        assert!(decision.allowed);
        assert_eq!(decision.canonical_action, "support");
        assert_eq!(decision.handler, "ai_escalation");
        assert_eq!(decision.risk_level, "medium");
    }

    #[test]
    fn operator_action_decision_blocks_unknown_action() {
        let decision = operator_action_decision(&OperatorActionInput {
            action: "format-disk".to_string(),
            state: json!({}),
        });

        assert!(!decision.allowed);
        assert_eq!(decision.reason, "unknown_action");
        assert_eq!(decision.message.as_deref(), Some("Неизвестное действие."));
    }

    #[test]
    fn operator_action_decision_guards_update_install_confirmation() {
        let blocked = operator_action_decision(&OperatorActionInput {
            action: "updates-install-confirm".to_string(),
            state: json!({"pending_update_install_confirm": false}),
        });
        assert!(!blocked.allowed);
        assert!(blocked.requires_confirmation);
        assert_eq!(blocked.reason, "missing_update_install_confirmation");

        let allowed = operator_action_decision(&OperatorActionInput {
            action: "updates-install-confirm".to_string(),
            state: json!({"pending_update_install_confirm": true}),
        });
        assert!(allowed.allowed);
        assert_eq!(allowed.canonical_action, "updates-install-confirm");
        assert!(
            allowed
                .state_update_hints
                .contains(&"clear_pending_update_install_confirm".to_string())
        );
    }

    #[test]
    fn operator_action_decision_guards_update_rollback_confirmation() {
        let blocked = operator_action_decision(&OperatorActionInput {
            action: "/run updates-rollback-confirm".to_string(),
            state: json!({"pending_rollback_confirm": false}),
        });
        assert!(!blocked.allowed);
        assert_eq!(blocked.reason, "missing_update_rollback_confirmation");

        let allowed = operator_action_decision(&OperatorActionInput {
            action: "/run updates-rollback-confirm".to_string(),
            state: json!({"pending_rollback_confirm": true}),
        });
        assert!(allowed.allowed);
        assert_eq!(allowed.canonical_action, "updates-rollback-confirm");
        assert_eq!(allowed.risk_level, "critical");
    }

    #[test]
    fn dlp_policy_decision_detects_monitor_and_group_counts() {
        let policy = json!({
            "endpoint": {
                "clipboard": [{"id": "c1", "enabled": true, "action": "alert"}],
                "usb": [{"id": "u1", "enabled": true, "action": "log"}],
                "print": [{"id": "p1", "enabled": false, "action": "block"}]
            }
        });

        let decision = dlp_policy_decision(
            &DlpPolicyInput {
                policy,
                target_mode: String::new(),
            },
            1000,
        )
        .unwrap();

        assert_eq!(decision.current_mode, "monitor");
        assert_eq!(decision.reason, "mode_only");
        assert_eq!(decision.groups.len(), 3);
        assert_eq!(decision.groups[0].name, "endpoint.clipboard");
        assert_eq!(decision.groups[0].blocked, 0);
        assert!(decision.updated_policy.is_none());
    }

    #[test]
    fn dlp_policy_decision_toggle_promotes_endpoint_rules_only() {
        let policy = json!({
            "rules": [{"id": "web1", "enabled": true, "action": "alert"}],
            "endpoint": {
                "clipboard": [{"id": "c1", "enabled": true, "action": "alert"}],
                "usb": [{"id": "u1", "enabled": true, "action": "log"}],
                "print": [{"id": "p1", "enabled": true, "action": "block"}],
                "email": [{"id": "e1", "enabled": false, "action": "alert"}]
            }
        });

        let decision = dlp_policy_decision(
            &DlpPolicyInput {
                policy,
                target_mode: "toggle".to_string(),
            },
            1000,
        )
        .unwrap();

        let updated = decision.updated_policy.as_ref().unwrap();
        assert_eq!(decision.current_mode, "mixed");
        assert_eq!(decision.target_mode.as_deref(), Some("monitor"));
        assert_eq!(decision.changed_count, 1);
        assert_eq!(
            updated["endpoint"]["print"][0]["action"].as_str(),
            Some("alert")
        );
        assert_eq!(updated["rules"][0]["action"].as_str(), Some("alert"));
        assert_eq!(
            updated["_tsj_meta"]["updated_at_utc"].as_str(),
            Some("1970-01-01T00:16:40Z")
        );
    }

    #[test]
    fn dlp_policy_decision_enforce_sets_alert_and_log_to_block() {
        let policy = json!({
            "endpoint": {
                "clipboard": [{"id": "c1", "enabled": true, "action": "alert"}],
                "usb": [{"id": "u1", "enabled": true, "action": "log"}],
                "print": [{"id": "p1", "enabled": true, "action": "block"}]
            }
        });

        let decision = dlp_policy_decision(
            &DlpPolicyInput {
                policy,
                target_mode: "enforce".to_string(),
            },
            1000,
        )
        .unwrap();

        let updated = decision.updated_policy.as_ref().unwrap();
        assert_eq!(decision.target_mode.as_deref(), Some("enforce"));
        assert_eq!(decision.changed_count, 2);
        assert_eq!(
            updated["endpoint"]["clipboard"][0]["action"].as_str(),
            Some("block")
        );
        assert_eq!(
            updated["endpoint"]["usb"][0]["action"].as_str(),
            Some("block")
        );
        assert!(
            decision
                .changed_rules
                .contains(&"endpoint.clipboard:c1 alert->block".to_string())
        );
    }

    #[test]
    fn confirmation_decision_expires_pending_request() {
        let decision = confirmation_decision(
            &ConfirmationInput {
                kind: "pfsense".to_string(),
                action: "expire".to_string(),
                code: String::new(),
                state: json!({"created_ts": 1000, "stage": "awaiting_first_confirm", "confirm_code": "123456"}),
            },
            900,
            2000,
        );

        assert!(decision.present);
        assert!(decision.expired);
        assert!(decision.clear_pending);
        assert!(!decision.allowed);
        assert_eq!(decision.reason, "expired");
    }

    #[test]
    fn confirmation_decision_first_confirm_advances_stage() {
        let decision = confirmation_decision(
            &ConfirmationInput {
                kind: "openvpn".to_string(),
                action: "first_confirm".to_string(),
                code: String::new(),
                state: json!({"created_ts": 1000, "stage": "awaiting_first_confirm", "confirm_code": "654321"}),
            },
            900,
            1200,
        );

        assert!(decision.allowed);
        assert_eq!(
            decision.next_stage.as_deref(),
            Some("awaiting_second_confirm")
        );
        assert_eq!(decision.first_confirmed_ts, Some(1200));
        assert_eq!(decision.reason, "first_confirmed");
    }

    #[test]
    fn confirmation_decision_apply_guards_stage_and_code() {
        let wrong_stage = confirmation_decision(
            &ConfirmationInput {
                kind: "pfsense".to_string(),
                action: "apply".to_string(),
                code: "123456".to_string(),
                state: json!({"created_ts": 1000, "stage": "awaiting_first_confirm", "confirm_code": "123456"}),
            },
            900,
            1200,
        );
        assert!(!wrong_stage.allowed);
        assert_eq!(wrong_stage.reason, "wrong_stage");

        let wrong_code = confirmation_decision(
            &ConfirmationInput {
                kind: "pfsense".to_string(),
                action: "apply".to_string(),
                code: "000000".to_string(),
                state: json!({"created_ts": 1000, "stage": "awaiting_second_confirm", "confirm_code": "123456"}),
            },
            900,
            1200,
        );
        assert!(!wrong_code.allowed);
        assert_eq!(wrong_code.reason, "wrong_code");

        let allowed = confirmation_decision(
            &ConfirmationInput {
                kind: "pfsense".to_string(),
                action: "apply".to_string(),
                code: "123456".to_string(),
                state: json!({"created_ts": 1000, "stage": "awaiting_second_confirm", "confirm_code": "123456"}),
            },
            900,
            1200,
        );
        assert!(allowed.allowed);
        assert_eq!(allowed.reason, "apply_allowed");
    }

    #[test]
    fn confirmation_decision_proxmox_restore_apply_uses_code_only() {
        let allowed = confirmation_decision(
            &ConfirmationInput {
                kind: "proxmox_restore".to_string(),
                action: "apply".to_string(),
                code: "222333".to_string(),
                state: json!({"created_ts": 1000, "confirm_code": "222333"}),
            },
            900,
            1200,
        );

        assert!(allowed.allowed);
        assert_eq!(allowed.reason, "apply_allowed");
    }

    #[test]
    fn autoheal_plan_routes_endpoint_failure_to_windows_heal_only() {
        let plan = autoheal_plan_decision(&AutohealPlanInput {
            failures: vec!["dlp-endpoint".to_string()],
            slo_stale: false,
        });

        assert!(plan.run_windows_heal);
        assert!(plan.include_windows_dlp);
        assert!(!plan.include_watchers);
        assert!(!plan.include_worktime);
        assert!(!plan.run_server_dlp_heal);
        assert!(!plan.run_worktime_heal);
        assert_eq!(plan.sleep_after_seconds, 30);
        assert!(plan.direct_autoheal_target);
    }

    #[test]
    fn autoheal_plan_routes_server_dlp_failure_to_server_dlp_heal() {
        let plan = autoheal_plan_decision(&AutohealPlanInput {
            failures: vec!["dlp-fileops-server".to_string()],
            slo_stale: false,
        });

        assert!(!plan.run_windows_heal);
        assert!(plan.run_server_dlp_heal);
        assert_eq!(plan.server_dlp_failures, vec!["dlp-fileops-server"]);
        assert_eq!(plan.sleep_after_seconds, 5);
    }

    #[test]
    fn autoheal_plan_routes_watcher_and_worktime_to_windows_and_worktime() {
        let plan = autoheal_plan_decision(&AutohealPlanInput {
            failures: vec!["watcher-window".to_string(), "worktime".to_string()],
            slo_stale: false,
        });

        assert!(plan.run_windows_heal);
        assert!(plan.include_watchers);
        assert!(plan.include_worktime);
        assert!(plan.run_worktime_heal);
        assert_eq!(plan.sleep_after_seconds, 30);
        assert_eq!(plan.report_triggers.len(), 2);
    }

    #[test]
    fn autoheal_plan_slo_has_no_direct_target() {
        let plan = autoheal_plan_decision(&AutohealPlanInput {
            failures: vec!["slo".to_string()],
            slo_stale: true,
        });

        assert!(plan.slo_only);
        assert!(plan.slo_stale);
        assert!(!plan.direct_autoheal_target);
        assert_eq!(
            plan.report_triggers[0],
            "- heal trigger: SLO summary stale, check aw-slo-monitor.timer/service"
        );
    }
}
