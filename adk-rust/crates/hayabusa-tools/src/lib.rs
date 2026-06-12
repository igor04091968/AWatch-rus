use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use serde::Serialize;
use serde_json::{Value, json};

pub const SEVERITY_ORDER: &[(&str, i64)] =
    &[("low", 1), ("medium", 2), ("high", 3), ("critical", 4)];

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Summary {
    pub severity: String,
    pub score: i64,
    pub events_total: usize,
    pub level_counts: BTreeMap<String, usize>,
    pub top_rules: Vec<TopRule>,
    pub first_timestamp: Option<String>,
    pub last_timestamp: Option<String>,
    pub failed_logon_rows: usize,
    pub successful_logon_rows: usize,
    pub suspicious_pwsh: usize,
    pub credential_events: usize,
    pub timestomp_events: usize,
    pub logon_failure_events: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TopRule {
    pub title: String,
    pub count: usize,
}

pub fn http_client() -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .no_proxy()
        .build()
        .context("build HTTP client")
}

pub fn get_json(client: &Client, url: &str) -> Result<Value> {
    client
        .get(url)
        .header(ACCEPT, "application/json")
        .send()
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("GET {url} status"))?
        .json::<Value>()
        .with_context(|| format!("decode JSON from {url}"))
}

pub fn post_json(client: &Client, url: &str, payload: &Value) -> Result<Value> {
    let response = client
        .post(url)
        .header(ACCEPT, "application/json")
        .header(CONTENT_TYPE, "application/json")
        .json(payload)
        .send()
        .with_context(|| format!("POST {url}"))?
        .error_for_status()
        .with_context(|| format!("POST {url} status"))?;
    decode_optional_json(response.text().context("read POST response")?)
}

pub fn patch_json(client: &Client, url: &str, payload: &Value) -> Result<Value> {
    let response = client
        .patch(url)
        .header(ACCEPT, "application/json")
        .header(CONTENT_TYPE, "application/json")
        .json(payload)
        .send()
        .with_context(|| format!("PATCH {url}"))?
        .error_for_status()
        .with_context(|| format!("PATCH {url} status"))?;
    decode_optional_json(response.text().context("read PATCH response")?)
}

fn decode_optional_json(body: String) -> Result<Value> {
    if body.trim().is_empty() {
        Ok(json!({}))
    } else {
        serde_json::from_str(&body).context("decode JSON response")
    }
}

pub fn read_json_file(path: &Path) -> Result<Value> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .with_context(|| format!("decode {}", path.display()))
}

pub fn write_json_pretty(value: &Value) -> Result<String> {
    serde_json::to_string_pretty(value).context("encode JSON")
}

pub fn normalize_host(value: Option<&str>) -> String {
    value.unwrap_or("").trim().to_lowercase()
}

pub fn normalize_case_api_base(raw: &str) -> String {
    let mut base = raw.trim().trim_end_matches('/').to_string();
    if base.ends_with("/api/0/dlp/cases") {
        let cut = base.len() - "/api/0/dlp/cases".len();
        base.truncate(cut);
    }
    base
}

pub fn build_hayabusa_payload(intake: &Value, mode: &str, link_source: &str) -> Result<Value> {
    let report_dir = required_str(intake, "report_dir")?;
    let report_dir_path = PathBuf::from(report_dir);
    Ok(json!({
        "tool": "hayabusa",
        "host": required_str(intake, "host")?,
        "mode": mode,
        "status": required_str(intake, "status")?,
        "intake_id": required_str(intake, "intake_id")?,
        "package_path": required_str(intake, "package_path")?,
        "sha256": required_str(intake, "sha256")?,
        "report_dir": report_dir,
        "summary_html": report_dir_path.join("summary.html").to_string_lossy(),
        "timeline_path": report_dir_path.join("timeline.jsonl").to_string_lossy(),
        "manifest_path": report_dir_path.join("manifest.json").to_string_lossy(),
        "link_source": link_source,
    }))
}

pub fn link_hayabusa_to_case(
    client: &Client,
    case_api_base: &str,
    case_id: i64,
    intake: &Value,
    mode: &str,
    link_source: &str,
) -> Result<Value> {
    let case_api_base = normalize_case_api_base(case_api_base);
    let case = get_json(
        client,
        &format!("{case_api_base}/api/0/dlp/cases/{case_id}"),
    )?;
    let case_host = normalize_host(case.get("host").and_then(Value::as_str));
    let intake_host = normalize_host(intake.get("host").and_then(Value::as_str));
    if !case_host.is_empty() && !intake_host.is_empty() && case_host != intake_host {
        bail!(
            "hayabusa host mismatch: case host={} intake host={}",
            case.get("host").and_then(Value::as_str).unwrap_or(""),
            intake.get("host").and_then(Value::as_str).unwrap_or("")
        );
    }
    let payload = build_hayabusa_payload(intake, mode, link_source)?;
    post_json(
        client,
        &format!("{case_api_base}/api/0/dlp/cases/{case_id}/forensics/hayabusa"),
        &payload,
    )?;
    get_json(
        client,
        &format!("{case_api_base}/api/0/dlp/cases/{case_id}"),
    )
}

pub fn analyze_report(report_dir: &Path) -> Result<Summary> {
    let timeline_path = report_dir.join("timeline.jsonl");
    let mut level_counts: HashMap<String, usize> = HashMap::new();
    let mut title_counts: HashMap<String, (usize, usize)> = HashMap::new();
    let mut first_ts: Option<DateTime<Utc>> = None;
    let mut last_ts: Option<DateTime<Utc>> = None;
    let mut total_events = 0usize;

    if timeline_path.is_file() {
        let text = fs::read_to_string(&timeline_path)
            .with_context(|| format!("read {}", timeline_path.display()))?;
        for raw_line in text.lines() {
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }
            let Ok(event) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            total_events += 1;
            let level = normalize_level(event.get("Level").and_then(Value::as_str));
            *level_counts.entry(level).or_insert(0) += 1;
            let title = event
                .get("RuleTitle")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|title| !title.is_empty())
                .unwrap_or("Unknown rule")
                .to_string();
            let next_index = title_counts.len();
            let entry = title_counts.entry(title).or_insert((0, next_index));
            entry.0 += 1;
            if let Some(ts) = event
                .get("Timestamp")
                .and_then(Value::as_str)
                .and_then(parse_ts)
            {
                first_ts = Some(first_ts.map_or(ts, |old| old.min(ts)));
                last_ts = Some(last_ts.map_or(ts, |old| old.max(ts)));
            }
        }
    }

    let failed_logons = read_csv_rows(&report_dir.join("logon-summary-failed.csv"))?;
    let successful_logons = read_csv_rows(&report_dir.join("logon-summary-successful.csv"))?;
    let suspicious_pwsh = title_counts
        .iter()
        .filter(|(title, _)| {
            let text = title.to_lowercase();
            text.contains("pwsh") || text.contains("powershell") || text.contains("obfuscation")
        })
        .map(|(_, (count, _))| *count)
        .sum();
    let credential_events = title_counts
        .iter()
        .filter(|(title, _)| title.to_lowercase().contains("credential"))
        .map(|(_, (count, _))| *count)
        .sum();
    let timestomp_events = title_counts
        .iter()
        .filter(|(title, _)| title.to_lowercase().contains("timestomp"))
        .map(|(_, (count, _))| *count)
        .sum();
    let logon_failure_events = title_counts
        .iter()
        .filter(|(title, _)| title.to_lowercase().contains("logon failure"))
        .map(|(_, (count, _))| *count)
        .sum();
    let score = level_counts
        .iter()
        .map(|(level, count)| level_weight(level) * (*count as i64))
        .sum::<i64>()
        + (failed_logons.min(200) as i64) * 2
        + (suspicious_pwsh as i64) * 6
        + (credential_events as i64) * 8
        + (timestomp_events as i64) * 12
        + (logon_failure_events as i64) * 2;

    let crit_count = *level_counts.get("crit").unwrap_or(&0);
    let high_count = *level_counts.get("high").unwrap_or(&0);
    let med_count = *level_counts.get("med").unwrap_or(&0);
    let severity =
        if crit_count >= 1 || score >= 240 || (high_count >= 4 && suspicious_pwsh >= 4) {
            "critical"
        } else if high_count >= 1 || score >= 120 || suspicious_pwsh >= 8 || credential_events >= 5
        {
            "high"
        } else if med_count >= 1 || score >= 40 || failed_logons >= 10 {
            "medium"
        } else {
            "low"
        }
        .to_string();

    Ok(Summary {
        severity,
        score,
        events_total: total_events,
        level_counts: sort_map(level_counts),
        top_rules: top_rules(title_counts),
        first_timestamp: first_ts.map(format_ts),
        last_timestamp: last_ts.map(format_ts),
        failed_logon_rows: failed_logons,
        successful_logon_rows: successful_logons,
        suspicious_pwsh,
        credential_events,
        timestomp_events,
        logon_failure_events,
    })
}

pub fn build_case_payload(intake: &Value, summary: &Summary) -> Result<Value> {
    Ok(json!({
        "incident_id": format!("hayabusa:{}:{}", required_str(intake, "host")?, required_str(intake, "intake_id")?),
        "host": required_str(intake, "host")?,
        "title": build_case_title(required_str(intake, "host")?, summary),
        "severity": summary.severity,
        "evidence": {
            "hayabusa": {
                "intake_id": required_str(intake, "intake_id")?,
                "package_path": required_str(intake, "package_path")?,
                "sha256": required_str(intake, "sha256")?,
                "report_dir": required_str(intake, "report_dir")?,
                "summary": serde_json::to_value(summary)?,
            }
        }
    }))
}

pub fn build_case_title(host: &str, summary: &Summary) -> String {
    let suffix = summary
        .top_rules
        .first()
        .map(|item| item.title.as_str())
        .unwrap_or("No dominant rule");
    format!(
        "Hayabusa {} · {host} · {suffix}",
        summary.severity.to_uppercase()
    )
}

pub fn build_comment(summary: &Summary, intake: &Value) -> Result<String> {
    let top = summary
        .top_rules
        .iter()
        .take(3)
        .map(|item| format!("{} ({})", item.title, item.count))
        .collect::<Vec<_>>()
        .join(", ");
    Ok(format!(
        "Hayabusa auto-summary\nSeverity: {} (score={})\nHost: {}\nIntake: {}\nEvents: {}, failed_logons={}, suspicious_pwsh={}, credential_events={}\nTop rules: {}\nReport: {}",
        summary.severity,
        summary.score,
        required_str(intake, "host")?,
        required_str(intake, "intake_id")?,
        summary.events_total,
        summary.failed_logon_rows,
        summary.suspicious_pwsh,
        summary.credential_events,
        if top.is_empty() {
            "n/a".to_string()
        } else {
            top
        },
        required_str(intake, "report_dir")?,
    ))
}

pub fn build_telegram_text(
    case_id: Option<i64>,
    intake: &Value,
    summary: &Summary,
) -> Result<String> {
    let severity_label = match summary.severity.as_str() {
        "critical" => "критичное событие",
        "high" => "опасное событие",
        "medium" => "подозрительное событие",
        "low" => "слабый сигнал",
        other => other,
    };
    let top_rule = summary
        .top_rules
        .first()
        .map(|item| item.title.as_str())
        .unwrap_or("нет явного доминирующего правила");
    let top_count = summary
        .top_rules
        .first()
        .map(|item| item.count)
        .unwrap_or(0);
    let mut lines = vec![
        format!("Hayabusa: {severity_label}"),
        String::new(),
        format!("Хост: {}", required_str(intake, "host")?),
    ];
    if let Some(case_id) = case_id {
        lines.push(format!("Кейс: {case_id}"));
    }
    lines.extend([
        format!("Уровень: {}", summary.severity),
        String::new(),
        "Что найдено:".to_string(),
        format!("- {top_rule}: {top_count}"),
        format!("- подозрительный PowerShell: {}", summary.suspicious_pwsh),
        format!("- ошибок входа: {}", summary.logon_failure_events),
        format!("- событий по учётным данным: {}", summary.credential_events),
    ]);
    if summary.timestomp_events > 0 {
        lines.push(format!(
            "- timestomp-подобных событий: {}",
            summary.timestomp_events
        ));
    }
    lines.extend([
        String::new(),
        "Главный риск:".to_string(),
        "возможная активность вокруг учётных данных и PowerShell".to_string(),
        String::new(),
        "Отчёт:".to_string(),
        required_str(intake, "report_dir")?.to_string(),
    ]);
    Ok(lines.join("\n"))
}

pub fn severity_meets(actual: &str, threshold: &str) -> bool {
    severity_order(actual) >= severity_order(threshold)
}

pub fn env_bool(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Ok(value) => matches!(
            value.trim().to_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => default,
    }
}

pub fn env_string(name: &str, default: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

pub fn required_str<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("{key} missing"))
}

pub fn windows_filename(path: &str) -> String {
    path.rsplit(['\\', '/'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
        .to_string()
}

pub fn guess_host_from_filename(filename: &str) -> String {
    let stem = filename.strip_suffix(".zip").unwrap_or(filename);
    stem.split_once('-')
        .map(|(host, _)| host)
        .unwrap_or(stem)
        .to_string()
}

fn read_csv_rows(path: &Path) -> Result<usize> {
    if !path.is_file() {
        return Ok(0);
    }
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let rows = text.lines().count();
    Ok(rows.saturating_sub(1))
}

fn normalize_level(level: Option<&str>) -> String {
    match level.unwrap_or("").trim().to_lowercase().as_str() {
        "" => "info",
        "informational" | "info" => "info",
        "low" => "low",
        "med" | "medium" => "med",
        "high" => "high",
        "crit" | "critical" => "crit",
        other => other,
    }
    .to_string()
}

fn level_weight(level: &str) -> i64 {
    match level {
        "info" => 1,
        "low" => 4,
        "med" => 12,
        "high" => 40,
        "crit" => 100,
        _ => 0,
    }
}

fn severity_order(value: &str) -> i64 {
    SEVERITY_ORDER
        .iter()
        .find_map(|(name, order)| (*name == value).then_some(*order))
        .unwrap_or(0)
}

fn parse_ts(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&value.replace('Z', "+00:00"))
        .map(|ts| ts.with_timezone(&Utc))
        .ok()
}

fn format_ts(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
}

fn sort_map(values: HashMap<String, usize>) -> BTreeMap<String, usize> {
    values.into_iter().collect()
}

fn top_rules(values: HashMap<String, (usize, usize)>) -> Vec<TopRule> {
    let mut rows = values.into_iter().collect::<Vec<_>>();
    rows.sort_by(|a, b| b.1.0.cmp(&a.1.0).then_with(|| a.1.1.cmp(&b.1.1)));
    rows.into_iter()
        .take(5)
        .map(|(title, (count, _))| TopRule { title, count })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn severity_rules_match_expected_thresholds() {
        assert!(severity_meets("high", "medium"));
        assert!(!severity_meets("low", "medium"));
    }

    #[test]
    fn analyzes_timeline_and_csv_counts() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("timeline.jsonl"),
            r#"{"Level":"high","RuleTitle":"PowerShell Credential Dump","Timestamp":"2026-06-01T00:00:00Z"}"#,
        )
        .unwrap();
        fs::write(dir.path().join("logon-summary-failed.csv"), "h\n1\n2\n").unwrap();
        let summary = analyze_report(dir.path()).unwrap();
        assert_eq!(summary.severity, "high");
        assert_eq!(summary.events_total, 1);
        assert_eq!(summary.failed_logon_rows, 2);
        assert_eq!(summary.credential_events, 1);
        assert_eq!(summary.suspicious_pwsh, 1);
    }
}
