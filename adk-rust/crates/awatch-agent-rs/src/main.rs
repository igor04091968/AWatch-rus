mod collectors;
mod config;
mod telemetry;
mod transport;

use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use config::{AgentConfig, AgentRole, default_config_path};
use transport::{TelemetryTransport, spool_health};

#[derive(Debug, Parser)]
#[command(about = "AWatch-rus Rust telemetry agent")]
struct Cli {
    #[arg(long, env = "AWATCH_AGENT_CONFIG")]
    config: Option<PathBuf>,

    #[arg(long, env = "AWATCH_AGENT_SERVER_URL")]
    server_url: Option<String>,

    #[arg(long, env = "AWATCH_AGENT_API_KEY")]
    api_key: Option<String>,

    #[arg(long, env = "AWATCH_AGENT_ROLE")]
    role: Option<String>,

    #[arg(long)]
    once: bool,

    #[arg(long)]
    print_json: bool,

    #[arg(long)]
    flush_spool: bool,

    #[arg(long)]
    spool_health: bool,
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
    if let Some(api_key) = cli.api_key {
        config.api_key = api_key;
    }
    if let Some(role) = cli.role {
        config.role = AgentRole::parse(&role);
    }
    if cli.spool_health {
        println!(
            "{}",
            serde_json::to_string_pretty(&spool_health(&config.spool_dir))?
        );
        return Ok(0);
    }

    let transport = TelemetryTransport::new(&config);
    if cli.flush_spool {
        let flushed = transport.flush_spool()?;
        println!("{}", serde_json::json!({"ok": true, "flushed": flushed}));
        return Ok(0);
    }

    loop {
        let collector = collectors::platform_collector(config.role)?;
        let record = collector.collect_all()?;
        if cli.print_json {
            println!("{}", serde_json::to_string_pretty(&record)?);
        } else if let Err(err) = transport.send_or_spool(&record) {
            eprintln!("{err:#}");
        }
        if cli.once {
            break;
        }
        thread::sleep(Duration::from_secs(config.collect_interval_seconds));
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
