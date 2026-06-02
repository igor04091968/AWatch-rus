use std::time::Duration;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use serde_json::{Map, Value, json};
use urlencoding::encode;

#[derive(Debug, Parser)]
#[command(about = "AWatch DLP admin CLI")]
struct Cli {
    #[arg(long, default_value = "http://127.0.0.1:5601")]
    policy_server: String,

    #[arg(long, default_value = "http://127.0.0.1:5602")]
    case_server: String,

    #[arg(long, default_value = "http://127.0.0.1:5600")]
    aw_server: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Policies {
        #[command(subcommand)]
        command: PoliciesCommand,
    },
    Incidents {
        #[command(subcommand)]
        command: IncidentsCommand,
    },
    Cases {
        #[command(subcommand)]
        command: CasesCommand,
    },
    Health {
        #[command(subcommand)]
        command: HealthCommand,
    },
}

#[derive(Debug, Subcommand)]
enum PoliciesCommand {
    List,
    Active,
}

#[derive(Debug, Subcommand)]
enum IncidentsCommand {
    List {
        #[arg(long)]
        host: Option<String>,

        #[arg(long)]
        severity: Option<String>,

        #[arg(long, default_value_t = 100)]
        limit: usize,

        #[arg(long, default_value_t = 24)]
        since_hours: i64,
    },
}

#[derive(Debug, Subcommand)]
enum CasesCommand {
    List {
        #[arg(long)]
        host: Option<String>,

        #[arg(long)]
        status: Option<String>,

        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    Create {
        #[arg(long)]
        incident_id: String,

        #[arg(long)]
        title: String,

        #[arg(long)]
        host: Option<String>,

        #[arg(long, default_value = "medium")]
        severity: String,
    },
}

#[derive(Debug, Subcommand)]
enum HealthCommand {
    Check,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .no_proxy()
        .build()
        .context("build HTTP client")?;

    let output = match cli.command {
        Command::Policies { command } => match command {
            PoliciesCommand::List => get_json(
                &client,
                &format!("{}/api/0/dlp/policies", trim_url(&cli.policy_server)),
            )?,
            PoliciesCommand::Active => get_json(
                &client,
                &format!("{}/api/0/dlp/policies/active", trim_url(&cli.policy_server)),
            )?,
        },
        Command::Incidents { command } => match command {
            IncidentsCommand::List {
                host,
                severity,
                limit,
                since_hours,
            } => list_incidents(&client, &cli.aw_server, host, severity, limit, since_hours)?,
        },
        Command::Cases { command } => match command {
            CasesCommand::List {
                host,
                status,
                limit,
            } => list_cases(&client, &cli.case_server, host, status, limit)?,
            CasesCommand::Create {
                incident_id,
                title,
                host,
                severity,
            } => create_case(
                &client,
                &cli.case_server,
                incident_id,
                title,
                host,
                severity,
            )?,
        },
        Command::Health { command } => match command {
            HealthCommand::Check => health_check(
                &client,
                &cli.policy_server,
                &cli.case_server,
                &cli.aw_server,
            ),
        },
    };

    println!(
        "{}",
        serde_json::to_string_pretty(&output).context("serialize output")?
    );
    Ok(())
}

fn get_json(client: &Client, url: &str) -> Result<Value> {
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

fn send_json(client: &Client, url: &str, method: &str, payload: &Value) -> Result<Value> {
    let request = match method {
        "POST" => client.post(url),
        "PUT" => client.put(url),
        other => bail!("unsupported method: {other}"),
    };
    let response = request
        .header(ACCEPT, "application/json")
        .header(CONTENT_TYPE, "application/json")
        .json(payload)
        .send()
        .with_context(|| format!("{method} {url}"))?
        .error_for_status()
        .with_context(|| format!("{method} {url} status"))?;
    let raw = response.text().context("read response body")?;
    if raw.trim().is_empty() {
        Ok(json!({}))
    } else {
        serde_json::from_str(&raw).with_context(|| format!("decode JSON from {url}"))
    }
}

fn list_incidents(
    client: &Client,
    aw_server: &str,
    host: Option<String>,
    severity: Option<String>,
    limit: usize,
    since_hours: i64,
) -> Result<Value> {
    let aw_server = trim_url(aw_server);
    let bucket_map = get_json(client, &format!("{aw_server}/api/0/buckets"))?;
    let Some(buckets) = bucket_map.as_object() else {
        return Ok(json!([]));
    };

    let mut bucket_ids: Vec<String> = buckets
        .keys()
        .filter(|bucket_id| bucket_id.starts_with("aw-dlp-incidents_"))
        .cloned()
        .collect();
    if let Some(host) = host.as_deref() {
        let suffix = format!("_{host}");
        bucket_ids.retain(|bucket_id| bucket_id.ends_with(&suffix));
    }
    bucket_ids.sort();

    let after = Utc::now() - chrono::Duration::hours(since_hours.max(1));
    let severity = severity.map(|value| value.to_lowercase());
    let mut rows = Vec::new();
    for bucket_id in bucket_ids {
        let encoded = encode(&bucket_id);
        let events_url = format!(
            "{aw_server}/api/0/buckets/{encoded}/events?limit={}",
            limit.max(1)
        );
        let events = get_json(client, &events_url)?;
        let Some(events) = events.as_array() else {
            continue;
        };
        for event in events {
            let Some(ts) = event
                .get("timestamp")
                .and_then(Value::as_str)
                .and_then(parse_iso)
            else {
                continue;
            };
            if ts < after {
                continue;
            }
            if let Some(expected) = severity.as_deref() {
                let actual = event
                    .get("data")
                    .and_then(|data| data.get("severity"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_lowercase();
                if actual != expected {
                    continue;
                }
            }
            rows.push(event.clone());
        }
    }
    Ok(Value::Array(rows))
}

fn list_cases(
    client: &Client,
    case_server: &str,
    host: Option<String>,
    status: Option<String>,
    limit: usize,
) -> Result<Value> {
    let mut query = Vec::new();
    if let Some(host) = host {
        query.push(format!("host={}", encode(&host)));
    }
    if let Some(status) = status {
        query.push(format!("status={}", encode(&status)));
    }
    query.push(format!("limit={}", limit.max(1)));
    get_json(
        client,
        &format!(
            "{}/api/0/dlp/cases?{}",
            trim_url(case_server),
            query.join("&")
        ),
    )
}

fn create_case(
    client: &Client,
    case_server: &str,
    incident_id: String,
    title: String,
    host: Option<String>,
    severity: String,
) -> Result<Value> {
    let payload = json!({
        "incident_id": incident_id,
        "title": title,
        "host": host,
        "severity": severity,
        "evidence": {
            "source": "dlp-admin-cli"
        }
    });
    send_json(
        client,
        &format!("{}/api/0/dlp/cases", trim_url(case_server)),
        "POST",
        &payload,
    )
}

fn health_check(client: &Client, policy_server: &str, case_server: &str, aw_server: &str) -> Value {
    let mut out = Map::new();
    out.insert(
        "policy".to_string(),
        get_json(client, &format!("{}/healthz", trim_url(policy_server)))
            .unwrap_or_else(error_payload),
    );
    out.insert(
        "cases".to_string(),
        get_json(client, &format!("{}/health", trim_url(case_server)))
            .unwrap_or_else(error_payload),
    );
    out.insert(
        "aw".to_string(),
        get_json(client, &format!("{}/api/0/info", trim_url(aw_server)))
            .unwrap_or_else(error_payload),
    );
    Value::Object(out)
}

fn parse_iso(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|ts| ts.with_timezone(&Utc))
        .ok()
}

fn trim_url(url: &str) -> String {
    url.trim_end_matches('/').to_string()
}

fn error_payload(err: anyhow::Error) -> Value {
    json!({
        "status": "error",
        "error": err.to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_urls() {
        assert_eq!(trim_url("http://127.0.0.1:5600/"), "http://127.0.0.1:5600");
    }

    #[test]
    fn parses_rfc3339_z_timestamp() {
        assert!(parse_iso("2026-06-01T10:11:12Z").is_some());
    }
}
