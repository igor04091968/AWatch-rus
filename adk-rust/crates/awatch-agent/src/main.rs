mod config;
mod envelope;
mod health;
mod logging;
mod metrics;
mod spool;
mod transport;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

use config::{AgentConfig, default_config_path};
use envelope::TelemetryEnvelope;
use logging::log_json;
use spool::LocalSpool;

#[derive(Debug, Parser)]
#[command(about = "AWatch-rus Rust agent baseline scaffold")]
struct Cli {
    #[arg(long, env = "AWATCH_AGENT_CONFIG")]
    config: Option<PathBuf>,

    #[arg(long, env = "AWATCH_AGENT_SERVER_URL")]
    server_url: Option<String>,

    #[arg(long, env = "AWATCH_AGENT_SPOOL_DIR")]
    spool_dir: Option<PathBuf>,

    #[arg(long)]
    enqueue_heartbeat: bool,

    #[arg(long)]
    flush_spool: bool,

    #[arg(long)]
    metrics: bool,

    #[arg(long)]
    healthz: bool,

    #[arg(long)]
    print_envelope: bool,

    #[arg(long)]
    max_health_requests: Option<usize>,
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
    let cli = Cli::parse();
    let mut config = load_config(cli.config.as_ref())?;
    if let Some(server_url) = cli.server_url {
        config.server_url = server_url;
    }
    if let Some(spool_dir) = cli.spool_dir {
        config.spool_dir = spool_dir;
    }
    let spool = LocalSpool::new(config.spool_dir.clone());
    let mut metrics = spool.metrics().unwrap_or_default();

    if cli.print_envelope {
        println!(
            "{}",
            serde_json::to_string_pretty(&TelemetryEnvelope::heartbeat(&config))?
        );
        return Ok(0);
    }

    if cli.enqueue_heartbeat {
        spool.enqueue(TelemetryEnvelope::heartbeat(&config))?;
        metrics.heartbeat_sent = metrics.heartbeat_sent.saturating_add(1);
        log_json(
            &config.agent_id,
            "INFO",
            "heartbeat",
            "heartbeat envelope queued",
        );
    }

    if cli.flush_spool {
        let summary = transport::flush_with_retry(&config, &spool, &mut metrics)?;
        println!("{}", serde_json::to_string_pretty(&summary)?);
        return Ok(0);
    }

    if cli.metrics {
        let mut current = spool.metrics()?;
        current.heartbeat_sent = metrics.heartbeat_sent;
        current.retry_count = metrics.retry_count;
        print!("{}", current.render_prometheus());
        return Ok(0);
    }

    if cli.healthz {
        health::serve_health(
            &config.health_bind,
            spool.metrics()?,
            cli.max_health_requests,
        )?;
        return Ok(0);
    }

    if !cli.enqueue_heartbeat {
        log_json(&config.agent_id, "INFO", "agent", "no action requested");
    }
    Ok(0)
}

fn load_config(path: Option<&PathBuf>) -> Result<AgentConfig> {
    let path = path.cloned().unwrap_or_else(default_config_path);
    if path.exists() {
        AgentConfig::load(&path)
    } else {
        AgentConfig::parse_toml_like("")
            .with_context(|| format!("load default config because {} is absent", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_run_has_no_monitoring_side_effect() {
        let config = AgentConfig::parse_toml_like("").unwrap();
        let envelope = TelemetryEnvelope::heartbeat(&config);
        assert_eq!(envelope.records.len(), 1);
        assert!(envelope.records[0].get("processes").is_none());
        assert!(envelope.records[0].get("screenshots").is_none());
    }
}
