use std::{thread, time::Duration};

use anyhow::{Context, Result};
use chrono::Local;
use clap::Parser;
use reqwest::blocking::Client;
use serde::Serialize;

const DEFAULT_BASE_URL: &str = "http://127.0.0.1:5610";
const DEFAULT_HOST: &str = "HOST-EXAMPLE";

#[derive(Debug, Parser)]
#[command(about = "AW Worktime report cache prewarm")]
struct Cli {
    #[arg(long)]
    dry_run: bool,

    #[arg(long)]
    json: bool,
}

#[derive(Debug, Clone)]
struct Config {
    base_url: String,
    timeout_seconds: f64,
    health_timeout_seconds: f64,
    ready_timeout_seconds: f64,
    ready_interval_seconds: f64,
    host: String,
    profile: String,
}

#[derive(Debug, Serialize)]
struct ProbeResult {
    url: String,
    ok: bool,
    code: Option<u16>,
    cache: Option<String>,
    reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct RunSummary {
    ok: bool,
    dry_run: bool,
    profile: String,
    host: String,
    urls: Vec<String>,
    failures: usize,
    readiness_ok: bool,
    probes: Vec<ProbeResult>,
}

fn env(name: &str, fallback: &str) -> String {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

fn env_f64(name: &str, fallback: f64) -> f64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .filter(|value| *value > 0.0)
        .unwrap_or(fallback)
}

fn load_config() -> Config {
    Config {
        base_url: env("WORKTIME_BASE_URL", DEFAULT_BASE_URL)
            .trim_end_matches('/')
            .to_string(),
        timeout_seconds: env_f64("WORKTIME_PREWARM_TIMEOUT_SECONDS", 45.0),
        health_timeout_seconds: env_f64("WORKTIME_PREWARM_HEALTH_TIMEOUT_SECONDS", 10.0),
        ready_timeout_seconds: env_f64("WORKTIME_PREWARM_READY_TIMEOUT_SECONDS", 60.0),
        ready_interval_seconds: env_f64("WORKTIME_PREWARM_READY_INTERVAL_SECONDS", 2.0),
        host: env(
            "WORKTIME_PREWARM_HOST",
            &env("AW_WORKTIME_HOST", DEFAULT_HOST),
        ),
        profile: env("WORKTIME_PREWARM_PROFILE", "full"),
    }
}

fn log(message: &str) {
    println!("{} {}", Local::now().format("%F %T"), message);
}

fn duration(seconds: f64) -> Duration {
    Duration::from_secs_f64(seconds.max(0.001))
}

fn profile_urls(config: &Config) -> Option<Vec<String>> {
    let host = urlencoding::encode(&config.host);
    let base = &config.base_url;
    let full = vec![
        format!("{base}/reports/worktime/today?day=today&format=csv&host={host}"),
        format!("{base}/reports/worktime/today?day=today&format=json&host={host}"),
        format!("{base}/reports/worktime/today?day=today&format=html&host={host}"),
        format!("{base}/reports/worktime/management?day=today&format=csv&host={host}"),
        format!("{base}/reports/worktime/management?day=today&format=json&host={host}"),
        format!("{base}/reports/worktime/management?day=today&format=html&host={host}"),
    ];
    let startup = vec![
        format!("{base}/reports/worktime/today?day=today&format=csv&host={host}"),
        format!("{base}/reports/worktime/today?day=today&format=json&host={host}"),
        format!("{base}/reports/worktime/management?day=today&format=json&host={host}"),
        format!("{base}/reports/worktime/management?day=today&format=csv&host={host}"),
    ];
    match config.profile.as_str() {
        "full" => Some(full),
        "startup" => Some(startup),
        _ => None,
    }
}

fn build_client(timeout_seconds: f64) -> Result<Client> {
    Client::builder()
        .timeout(duration(timeout_seconds))
        .no_proxy()
        .build()
        .context("build HTTP client")
}

fn probe(url: &str, timeout_seconds: f64) -> ProbeResult {
    let client = match build_client(timeout_seconds) {
        Ok(client) => client,
        Err(_) => {
            return ProbeResult {
                url: url.to_string(),
                ok: false,
                code: None,
                cache: None,
                reason: None,
            };
        }
    };
    match client.get(url).send() {
        Ok(resp) => {
            let code = resp.status().as_u16();
            let cache = resp
                .headers()
                .get("x-aw-worktime-cache")
                .and_then(|value| value.to_str().ok())
                .map(ToOwned::to_owned);
            let reason = resp
                .headers()
                .get("x-aw-worktime-cache-reason")
                .and_then(|value| value.to_str().ok())
                .map(ToOwned::to_owned);
            ProbeResult {
                url: url.to_string(),
                ok: (200..300).contains(&code),
                code: Some(code),
                cache,
                reason,
            }
        }
        Err(_) => ProbeResult {
            url: url.to_string(),
            ok: false,
            code: None,
            cache: None,
            reason: None,
        },
    }
}

fn log_probe(result: &ProbeResult) {
    if result.ok {
        log(&format!(
            "ok code={} cache={} reason={} url={}",
            result.code.unwrap_or(0),
            result.cache.as_deref().unwrap_or("none"),
            result.reason.as_deref().unwrap_or("none"),
            result.url
        ));
    } else {
        log(&format!(
            "warn code={} url={}",
            result
                .code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "000".to_string()),
            result.url
        ));
    }
}

fn wait_until_ready(config: &Config) -> bool {
    let deadline = std::time::Instant::now() + duration(config.ready_timeout_seconds);
    loop {
        let result = probe(
            &format!("{}/health", config.base_url),
            config.health_timeout_seconds,
        );
        log_probe(&result);
        if result.ok {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        thread::sleep(duration(config.ready_interval_seconds));
    }
}

fn run(cli: &Cli) -> RunSummary {
    let config = load_config();
    let Some(urls) = profile_urls(&config) else {
        log(&format!("unknown profile={}", config.profile));
        return RunSummary {
            ok: true,
            dry_run: cli.dry_run,
            profile: config.profile,
            host: config.host,
            urls: Vec::new(),
            failures: 0,
            readiness_ok: false,
            probes: Vec::new(),
        };
    };

    if cli.dry_run {
        return RunSummary {
            ok: true,
            dry_run: true,
            profile: config.profile,
            host: config.host,
            urls,
            failures: 0,
            readiness_ok: false,
            probes: Vec::new(),
        };
    }

    if !wait_until_ready(&config) {
        log("health readiness timed out; skip prewarm");
        return RunSummary {
            ok: true,
            dry_run: false,
            profile: config.profile,
            host: config.host,
            urls,
            failures: 0,
            readiness_ok: false,
            probes: Vec::new(),
        };
    }

    let mut probes = Vec::new();
    let mut failures = 0;
    for url in &urls {
        let result = probe(url, config.timeout_seconds);
        if !result.ok {
            failures += 1;
        }
        log_probe(&result);
        probes.push(result);
    }

    if failures > 0 {
        log(&format!(
            "completed profile={} with failures={failures}",
            config.profile
        ));
    } else {
        log(&format!(
            "completed profile={} successfully",
            config.profile
        ));
    }

    RunSummary {
        ok: true,
        dry_run: false,
        profile: config.profile,
        host: config.host,
        urls,
        failures,
        readiness_ok: true,
        probes,
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let summary = run(&cli);
    if cli.json || cli.dry_run {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_profile_has_legacy_urls() {
        let config = Config {
            base_url: DEFAULT_BASE_URL.to_string(),
            timeout_seconds: 45.0,
            health_timeout_seconds: 10.0,
            ready_timeout_seconds: 60.0,
            ready_interval_seconds: 2.0,
            host: DEFAULT_HOST.to_string(),
            profile: "full".to_string(),
        };
        let urls = profile_urls(&config).unwrap();
        assert_eq!(urls.len(), 6);
        assert!(urls[0].contains("format=csv"));
        assert!(urls[5].contains("/reports/worktime/management"));
    }

    #[test]
    fn startup_profile_has_legacy_urls() {
        let config = Config {
            base_url: DEFAULT_BASE_URL.to_string(),
            timeout_seconds: 45.0,
            health_timeout_seconds: 10.0,
            ready_timeout_seconds: 60.0,
            ready_interval_seconds: 2.0,
            host: DEFAULT_HOST.to_string(),
            profile: "startup".to_string(),
        };
        let urls = profile_urls(&config).unwrap();
        assert_eq!(urls.len(), 4);
        assert!(urls[2].contains("format=json"));
        assert!(urls[3].contains("format=csv"));
    }

    #[test]
    fn unknown_profile_is_ignored_like_legacy() {
        let config = Config {
            base_url: DEFAULT_BASE_URL.to_string(),
            timeout_seconds: 45.0,
            health_timeout_seconds: 10.0,
            ready_timeout_seconds: 60.0,
            ready_interval_seconds: 2.0,
            host: DEFAULT_HOST.to_string(),
            profile: "bad".to_string(),
        };
        assert!(profile_urls(&config).is_none());
    }
}
