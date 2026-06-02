use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Datelike, SecondsFormat, TimeZone, Utc};
use clap::Parser;
use reqwest::blocking::Client;
use reqwest::header::ACCEPT;
use serde::Serialize;
use serde_json::{Value, json};
use urlencoding::encode;

#[derive(Debug, Parser)]
#[command(about = "Generate DLP compliance reports from AW DLP incidents")]
struct Cli {
    #[arg(long)]
    month: Option<String>,

    #[arg(long)]
    profile: Option<String>,

    #[arg(long, default_value = "152-fz,pci-dss")]
    profiles: String,

    #[arg(long)]
    stdout_json: bool,
}

#[derive(Debug, Clone)]
struct Config {
    aw_api_base: String,
    output_dir: PathBuf,
    base_dir: PathBuf,
    template_152fz: PathBuf,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ReportStats {
    total_incidents: usize,
    high: usize,
    medium: usize,
    low: usize,
    by_host: BTreeMap<String, usize>,
    channels: BTreeMap<String, usize>,
}

#[derive(Debug, Serialize)]
struct Metadata {
    profile: String,
    period: String,
    generated_at: String,
    aw_api_base: String,
    report_path: String,
    stats: MetadataStats,
}

#[derive(Debug, Serialize)]
struct MetadataStats {
    total_incidents: usize,
    high: usize,
    medium: usize,
    low: usize,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let config = Config::from_env()?;
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .no_proxy()
        .build()
        .context("build HTTP client")?;
    let profiles = if let Some(profile) = cli.profile.as_deref() {
        vec![profile.to_string()]
    } else {
        cli.profiles
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>()
    };
    let mut items = Vec::new();
    for profile in profiles {
        items.push(generate_report(
            &client,
            &config,
            cli.month.as_deref(),
            &profile,
        )?);
    }
    if cli.stdout_json {
        if cli.profile.is_some() && items.len() == 1 {
            println!("{}", serde_json::to_string(&items[0])?);
        } else {
            println!("{}", serde_json::to_string(&json!({"items": items}))?);
        }
    }
    Ok(())
}

impl Config {
    fn from_env() -> Result<Self> {
        let aw_raw = env_string("AW_DLP_AW_API_BASE")
            .or_else(|| env_string("AW_SERVER_URL"))
            .unwrap_or_else(|| "http://127.0.0.1:5600".to_string());
        let aw_api_base = build_aw_api_base(&aw_raw);
        let output_dir = env_path("AW_DLP_COMPLIANCE_REPORT_DIR")
            .unwrap_or_else(|| PathBuf::from("/opt/activitywatch/dlp-compliance/reports"));
        let base_dir = env_path("AW_DLP_COMPLIANCE_BASE_DIR")
            .unwrap_or_else(|| PathBuf::from("/opt/activitywatch/dlp-compliance"));
        let template_152fz = env_path("AW_DLP_COMPLIANCE_TEMPLATE")
            .unwrap_or_else(|| base_dir.join("templates/152-fz-report.html"));
        Ok(Self {
            aw_api_base,
            output_dir,
            base_dir,
            template_152fz,
        })
    }
}

fn generate_report(
    client: &Client,
    config: &Config,
    month: Option<&str>,
    profile: &str,
) -> Result<Metadata> {
    let (start, end, period_label) = period_bounds(month)?;
    let incidents = load_incidents(client, config, start, end)?;
    let stats = build_stats(&incidents);
    fs::create_dir_all(&config.output_dir)
        .with_context(|| format!("create output dir {}", config.output_dir.display()))?;
    let generated_at = Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true);
    let html_path = config
        .output_dir
        .join(format!("{profile}-{period_label}.html"));
    let html = render_html(config, profile, &period_label, &stats, &generated_at)?;
    fs::write(&html_path, html).with_context(|| format!("write {}", html_path.display()))?;
    let metadata = Metadata {
        profile: profile.to_string(),
        period: period_label.clone(),
        generated_at,
        aw_api_base: config.aw_api_base.clone(),
        report_path: html_path.to_string_lossy().to_string(),
        stats: MetadataStats {
            total_incidents: stats.total_incidents,
            high: stats.high,
            medium: stats.medium,
            low: stats.low,
        },
    };
    let json_path = config
        .output_dir
        .join(format!("{profile}-{period_label}.json"));
    fs::write(&json_path, serde_json::to_string_pretty(&metadata)?)
        .with_context(|| format!("write {}", json_path.display()))?;
    Ok(metadata)
}

fn build_aw_api_base(raw_url: &str) -> String {
    let url = raw_url.trim().trim_end_matches('/').to_string();
    if url.ends_with("/api/0") {
        url
    } else {
        format!("{url}/api/0")
    }
}

fn load_incidents(
    client: &Client,
    config: &Config,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<Value>> {
    let buckets = http_json(client, &format!("{}/buckets", config.aw_api_base))?;
    let Some(map) = buckets.as_object() else {
        return Ok(Vec::new());
    };
    let mut bucket_ids = map
        .keys()
        .filter(|bucket_id| bucket_id.starts_with("aw-dlp-incidents_"))
        .cloned()
        .collect::<Vec<_>>();
    bucket_ids.sort();
    let mut incidents = Vec::new();
    for bucket_id in bucket_ids {
        let encoded = encode(&bucket_id);
        let url = format!("{}/buckets/{encoded}/events?limit=2000", config.aw_api_base);
        let events = http_json(client, &url)?;
        let Some(events) = events.as_array() else {
            continue;
        };
        for event in events {
            let Some(ts) = event
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(parse_ts)
            else {
                continue;
            };
            if ts < start || ts > end {
                continue;
            }
            incidents.push(event.clone());
        }
    }
    Ok(incidents)
}

fn http_json(client: &Client, url: &str) -> Result<Value> {
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

fn build_stats(incidents: &[Value]) -> ReportStats {
    let mut by_host = BTreeMap::new();
    let mut channels = BTreeMap::new();
    let mut high = 0;
    let mut medium = 0;
    let mut low = 0;
    for event in incidents {
        let data = event.get("data").and_then(Value::as_object);
        let host = data
            .and_then(|item| item.get("hostname"))
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        *by_host.entry(host).or_insert(0) += 1;

        match data
            .and_then(|item| item.get("severity"))
            .and_then(Value::as_str)
            .unwrap_or("low")
            .to_lowercase()
            .as_str()
        {
            "high" => high += 1,
            "medium" => medium += 1,
            _ => low += 1,
        }
        let channel = data
            .and_then(|item| item.get("signalType").or_else(|| item.get("source")))
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        *channels.entry(channel).or_insert(0) += 1;
    }
    ReportStats {
        total_incidents: incidents.len(),
        high,
        medium,
        low,
        by_host: sort_counts_desc(by_host),
        channels: sort_counts_desc(channels),
    }
}

fn sort_counts_desc(values: BTreeMap<String, usize>) -> BTreeMap<String, usize> {
    let mut rows = values.into_iter().collect::<Vec<_>>();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    rows.into_iter().collect()
}

fn render_html(
    config: &Config,
    profile: &str,
    period_label: &str,
    stats: &ReportStats,
    generated_at: &str,
) -> Result<String> {
    let template_path = resolve_template_path(config, profile);
    let template = fs::read_to_string(&template_path)
        .with_context(|| format!("read template {}", template_path.display()))?;
    Ok(template
        .replace("{{PERIOD}}", period_label)
        .replace("{{PROFILE}}", profile)
        .replace("{{GENERATED_AT}}", generated_at)
        .replace("{{TOTAL}}", &stats.total_incidents.to_string())
        .replace("{{HIGH}}", &stats.high.to_string())
        .replace("{{MEDIUM}}", &stats.medium.to_string())
        .replace("{{LOW}}", &stats.low.to_string())
        .replace(
            "{{HOST_TABLE}}",
            &render_table("Инциденты по хостам", &stats.by_host),
        )
        .replace(
            "{{CHANNEL_TABLE}}",
            &render_table("Инциденты по каналам", &stats.channels),
        ))
}

fn resolve_template_path(config: &Config, profile: &str) -> PathBuf {
    if profile == "152-fz" {
        config.template_152fz.clone()
    } else {
        let specific = config
            .base_dir
            .join(format!("templates/{profile}-report.html"));
        if specific.exists() {
            specific
        } else {
            config.template_152fz.clone()
        }
    }
}

fn render_table(title: &str, rows: &BTreeMap<String, usize>) -> String {
    if rows.is_empty() {
        return format!("<h3>{title}</h3><p>Нет данных</p>");
    }
    let body = rows
        .iter()
        .map(|(name, count)| format!("<tr><td>{name}</td><td>{count}</td></tr>"))
        .collect::<String>();
    format!(
        "<h3>{title}</h3><table><thead><tr><th>Параметр</th><th>Значение</th></tr></thead><tbody>{body}</tbody></table>"
    )
}

fn period_bounds(month: Option<&str>) -> Result<(DateTime<Utc>, DateTime<Utc>, String)> {
    let start = if let Some(month) = month {
        let (year, month) = month
            .split_once('-')
            .ok_or_else(|| anyhow!("month must be YYYY-MM"))?;
        Utc.with_ymd_and_hms(year.parse()?, month.parse()?, 1, 0, 0, 0)
            .single()
            .ok_or_else(|| anyhow!("invalid month"))?
    } else {
        let now = Utc::now();
        Utc.with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
            .single()
            .ok_or_else(|| anyhow!("invalid current month"))?
    };
    let end = if start.month() == 12 {
        Utc.with_ymd_and_hms(start.year() + 1, 1, 1, 0, 0, 0)
            .single()
            .ok_or_else(|| anyhow!("invalid end month"))?
    } else {
        Utc.with_ymd_and_hms(start.year(), start.month() + 1, 1, 0, 0, 0)
            .single()
            .ok_or_else(|| anyhow!("invalid end month"))?
    };
    Ok((
        start,
        end,
        format!("{:04}-{:02}", start.year(), start.month()),
    ))
}

fn parse_ts(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&value.replace('Z', "+00:00"))
        .map(|ts| ts.with_timezone(&Utc))
        .ok()
}

fn env_string(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

fn env_path(name: &str) -> Option<PathBuf> {
    env_string(name).map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aw_api_base_matches_python_contract() {
        assert_eq!(build_aw_api_base("http://x"), "http://x/api/0");
        assert_eq!(build_aw_api_base("http://x/api/0"), "http://x/api/0");
    }

    #[test]
    fn stats_match_expected_counts() {
        let incidents = vec![
            json!({"data":{"hostname":"a","severity":"high","signalType":"usb"}}),
            json!({"data":{"hostname":"a","severity":"medium","source":"clipboard"}}),
            json!({"data":{"hostname":"b","severity":"low"}}),
        ];
        let stats = build_stats(&incidents);
        assert_eq!(stats.total_incidents, 3);
        assert_eq!(stats.high, 1);
        assert_eq!(stats.medium, 1);
        assert_eq!(stats.low, 1);
        assert_eq!(stats.by_host.get("a"), Some(&2));
        assert_eq!(stats.channels.get("unknown"), Some(&1));
    }

    #[test]
    fn month_bounds_are_utc_month() {
        let (start, end, label) = period_bounds(Some("2026-06")).unwrap();
        assert_eq!(label, "2026-06");
        assert_eq!(start.to_rfc3339(), "2026-06-01T00:00:00+00:00");
        assert_eq!(end.to_rfc3339(), "2026-07-01T00:00:00+00:00");
    }
}
