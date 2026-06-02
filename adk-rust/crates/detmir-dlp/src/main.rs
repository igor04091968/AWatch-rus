use std::io::{self, Write};
use std::process::Command;

use anyhow::{Context, Result};
use clap::Parser;

const DEFAULT_SSH_TARGET: &str = "igor@10.10.10.13";
const DEFAULT_REMOTE_COMMAND: &str = "sudo -n /usr/local/bin/dlp-health-check --json";

#[derive(Debug, Parser)]
#[command(about = "Run the DetMir DLP health check on the AW server over SSH.")]
struct Cli {
    #[arg(long, default_value = "ssh")]
    ssh_bin: String,

    #[arg(long, default_value = DEFAULT_SSH_TARGET)]
    ssh_target: String,

    #[arg(long, default_value_t = 10)]
    connect_timeout_seconds: u64,

    #[arg(long, default_value = DEFAULT_REMOTE_COMMAND)]
    remote_command: String,
}

impl Cli {
    fn apply_env(mut self) -> Self {
        self.ssh_target = env_first(
            &["DETMIR_DLP_SSH_TARGET", "DETMIR_AW_SSH_HOST"],
            &self.ssh_target,
        );
        self.remote_command = env_first(&["DETMIR_DLP_REMOTE_COMMAND"], &self.remote_command);
        self.ssh_bin = env_first(&["DETMIR_SSH_BIN"], &self.ssh_bin);
        self
    }
}

fn env_first(names: &[&str], fallback: &str) -> String {
    names
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
        .unwrap_or_else(|| fallback.to_string())
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
    let args = ssh_args(&cli);
    let output = Command::new(&cli.ssh_bin)
        .args(&args)
        .output()
        .with_context(|| format!("failed to execute {}", cli.ssh_bin))?;

    io::stdout()
        .write_all(&output.stdout)
        .context("failed to write DLP stdout")?;
    io::stderr()
        .write_all(&output.stderr)
        .context("failed to write DLP stderr")?;

    Ok(output.status.code().unwrap_or(1))
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
            remote_command: DEFAULT_REMOTE_COMMAND.to_string(),
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
