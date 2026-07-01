use std::io::{self, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::Parser;

const DEFAULT_SSH_TARGET: &str = "igor@192.0.2.13";
const DEFAULT_REMOTE_COMMAND: &str = "sudo -n env AW_DLP_HEALTH_ENDPOINT_SEND_FAILURE_WARN_COUNT=10 AW_DLP_HEALTH_FILEOPS_SEND_FAILURE_WARN_COUNT=10 /usr/local/bin/dlp-health-check --json";

#[derive(Debug, Parser)]
#[command(about = "Run the DetMir DLP health check on the AW server over SSH.")]
struct Cli {
    #[arg(long, default_value = "ssh")]
    ssh_bin: String,

    #[arg(long, default_value = DEFAULT_SSH_TARGET)]
    ssh_target: String,

    #[arg(long, default_value_t = 10)]
    connect_timeout_seconds: u64,

    #[arg(long, default_value_t = 90)]
    timeout_seconds: u64,

    #[arg(long, default_value = DEFAULT_REMOTE_COMMAND)]
    remote_command: String,

    #[arg(long, default_value_t = true)]
    enabled: bool,
}

impl Cli {
    fn apply_env(mut self) -> Self {
        self.ssh_target = env_first(
            &["DETMIR_DLP_SSH_TARGET", "DETMIR_AW_SSH_HOST"],
            &self.ssh_target,
        );
        self.remote_command = env_first(&["DETMIR_DLP_REMOTE_COMMAND"], &self.remote_command);
        self.ssh_bin = env_first(&["DETMIR_SSH_BIN"], &self.ssh_bin);
        self.timeout_seconds = env_u64("DETMIR_DLP_TIMEOUT_SECONDS", self.timeout_seconds);
        self.enabled = env_bool("DETMIR_DLP_ENABLED", self.enabled);
        self
    }
}

fn env_first(names: &[&str], fallback: &str) -> String {
    names
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
        .unwrap_or_else(|| fallback.to_string())
}

fn env_u64(name: &str, fallback: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

fn env_bool(name: &str, fallback: bool) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(fallback)
}

fn ssh_args(cli: &Cli) -> Vec<String> {
    vec![
        "-o".to_string(),
        "BatchMode=yes".to_string(),
        "-o".to_string(),
        format!("ConnectTimeout={}", cli.connect_timeout_seconds),
        "-o".to_string(),
        "StrictHostKeyChecking=accept-new".to_string(),
        cli.ssh_target.clone(),
        cli.remote_command.clone(),
    ]
}

fn run(cli: Cli) -> Result<i32> {
    if !cli.enabled {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": true,
                "counts": {"ok": 1, "warn": 0, "fail": 0},
                "results": [{
                    "name": "dlp:mode",
                    "status": "ok",
                    "summary": "DLP health check disabled by DETMIR_DLP_ENABLED=false",
                    "details": {
                        "mode": "disabled",
                        "load_reduction": ["aw-dlp health ssh probe skipped"]
                    }
                }]
            }))?
        );
        return Ok(0);
    }

    let args = ssh_args(&cli);
    let mut child = Command::new(&cli.ssh_bin)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to execute {}", cli.ssh_bin))?;

    let started = Instant::now();
    let mut timed_out = false;
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if started.elapsed() >= Duration::from_secs(cli.timeout_seconds) {
            timed_out = true;
            terminate_child(&mut child);
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    let output = child
        .wait_with_output()
        .context("failed to collect SSH output")?;
    io::stdout()
        .write_all(&output.stdout)
        .context("failed to write DLP stdout")?;
    io::stderr()
        .write_all(&output.stderr)
        .context("failed to write DLP stderr")?;
    if timed_out {
        writeln!(
            io::stderr(),
            "detmir-dlp timed out after {} seconds",
            cli.timeout_seconds
        )
        .context("failed to write timeout message")?;
        return Ok(124);
    }

    Ok(output.status.code().unwrap_or(1))
}

fn terminate_child(child: &mut std::process::Child) {
    let _ = child.kill();
    std::thread::sleep(Duration::from_secs(2));
    let _ = child.kill();
}

fn main() -> Result<()> {
    let cli = Cli::parse().apply_env();
    let code = run(cli)?;
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_legacy_ssh_args() {
        let cli = Cli {
            ssh_bin: "ssh".to_string(),
            ssh_target: DEFAULT_SSH_TARGET.to_string(),
            connect_timeout_seconds: 10,
            timeout_seconds: 90,
            remote_command: DEFAULT_REMOTE_COMMAND.to_string(),
            enabled: true,
        };
        assert_eq!(
            ssh_args(&cli),
            vec![
                "-o",
                "BatchMode=yes",
                "-o",
                "ConnectTimeout=10",
                "-o",
                "StrictHostKeyChecking=accept-new",
                DEFAULT_SSH_TARGET,
                DEFAULT_REMOTE_COMMAND,
            ]
        );
    }
}
