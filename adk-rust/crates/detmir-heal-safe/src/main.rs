use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use clap::Parser;
use detmir_core::{exit_codes, now_utc_rfc3339};
use serde::Serialize;

const DEFAULT_SSH_TARGET: &str = "igor@192.0.2.13";
const REQUIRED_UNITS: &[&str] = &[
    "activitywatch-server.service",
    "aw-worktime-api.service",
    "aw-worktime-ui-bridge.timer",
];
const OPTIONAL_UNITS: &[&str] = &["activitywatch-dlp-aggregator.timer"];

#[derive(Debug, Parser)]
#[command(about = "Safely heal allowlisted DetMir server-side services over SSH.")]
struct Cli {
    #[arg(long, default_value = "ssh")]
    ssh_bin: String,

    #[arg(long, default_value = DEFAULT_SSH_TARGET)]
    ssh_target: String,

    #[arg(long, default_value_t = 10)]
    connect_timeout_seconds: u64,

    #[arg(long)]
    json: bool,

    #[arg(long)]
    apply: bool,

    #[arg(long)]
    start_optional: bool,
}

impl Cli {
    fn apply_env(mut self) -> Self {
        self.ssh_target = env_first(
            &["DETMIR_HEAL_SSH_TARGET", "DETMIR_AW_SSH_HOST"],
            &self.ssh_target,
        );
        self.ssh_bin = env_first(&["DETMIR_SSH_BIN"], &self.ssh_bin);
        self
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct HealAction {
    action: String,
    unit: Option<String>,
    required: bool,
    status: String,
}

#[derive(Debug, Serialize)]
struct HealReport {
    ok: bool,
    dry_run: bool,
    applied: bool,
    generated_at_utc: String,
    ssh_target: String,
    actions: Vec<HealAction>,
    raw_stdout: String,
    raw_stderr: String,
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
        "bash".to_string(),
        "-s".to_string(),
        "--".to_string(),
        if cli.apply { "apply" } else { "dry-run" }.to_string(),
        if cli.start_optional {
            "start-optional"
        } else {
            "skip-optional"
        }
        .to_string(),
    ]
}

fn remote_script() -> String {
    let required = REQUIRED_UNITS.join(" ");
    let optional = OPTIONAL_UNITS.join(" ");
    format!(
        r#"set -euo pipefail
mode="${{1:-dry-run}}"
optional_mode="${{2:-skip-optional}}"
required_units="{required}"
optional_units="{optional}"

emit() {{
  action="$1"
  unit="${{2:-}}"
  required="${{3:-false}}"
  status="${{4:-ok}}"
  printf '%s\t%s\t%s\t%s\n' "$action" "$unit" "$required" "$status"
}}

if [ "$mode" = "apply" ]; then
  sudo -n systemctl reset-failed $required_units $optional_units >/dev/null 2>&1 || true
  emit reset-failed "" false applied
else
  emit reset-failed "" false planned
fi

for service in $required_units; do
  if systemctl is-active --quiet "$service"; then
    emit active "$service" true ok
  elif [ "$mode" = "apply" ]; then
    emit restart "$service" true started
    sudo -n systemctl restart "$service"
    emit active "$service" true ok
  else
    emit restart "$service" true planned
  fi
done

for service in $optional_units; do
  if ! systemctl list-unit-files "$service" >/dev/null 2>&1; then
    emit absent "$service" false skipped
  elif systemctl is-active --quiet "$service"; then
    emit active "$service" false ok
  elif [ "$mode" = "apply" ]; then
    if [ "$optional_mode" = "start-optional" ]; then
      emit start "$service" false started
      sudo -n systemctl start "$service" || true
    else
      emit inactive "$service" false skipped
    fi
  else
    if [ "$optional_mode" = "start-optional" ]; then
      emit start "$service" false planned
    else
      emit inactive "$service" false skipped
    fi
  fi
done

if [ "$mode" = "apply" ]; then
  sudo -n /usr/local/bin/dlp-health-check --json >/tmp/detmir-heal-dlp-health.json || true
  emit dlp-health-check "/tmp/detmir-heal-dlp-health.json" false written
else
  emit dlp-health-check "/tmp/detmir-heal-dlp-health.json" false planned
fi
"#
    )
}

fn parse_actions(raw: &str) -> Vec<HealAction> {
    raw.lines()
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let action = parts.next()?;
            let unit = parts.next().unwrap_or_default();
            let required = parts.next().unwrap_or("false") == "true";
            let status = parts.next().unwrap_or("unknown");
            Some(HealAction {
                action: action.to_string(),
                unit: if unit.is_empty() {
                    None
                } else {
                    Some(unit.to_string())
                },
                required,
                status: status.to_string(),
            })
        })
        .collect()
}

fn run(cli: Cli) -> Result<i32> {
    let args = ssh_args(&cli);
    let mut child = Command::new(&cli.ssh_bin)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to execute {}", cli.ssh_bin))?;

    child
        .stdin
        .as_mut()
        .context("failed to open SSH stdin")?
        .write_all(remote_script().as_bytes())
        .context("failed to write remote heal script")?;

    let output = child
        .wait_with_output()
        .context("SSH heal command failed")?;
    let raw_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let raw_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(exit_codes::ERROR);
    let actions = parse_actions(&raw_stdout);
    let report = HealReport {
        ok: code == exit_codes::OK,
        dry_run: !cli.apply,
        applied: cli.apply && code == exit_codes::OK,
        generated_at_utc: now_utc_rfc3339(),
        ssh_target: cli.ssh_target.clone(),
        actions,
        raw_stdout,
        raw_stderr,
    };

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        if report.dry_run {
            println!("detmir-heal-safe: dry-run");
        } else {
            println!("detmir-heal-safe: apply");
        }
        for action in &report.actions {
            match &action.unit {
                Some(unit) => println!(
                    "{} {} required={} status={}",
                    action.action, unit, action.required, action.status
                ),
                None => println!("{} status={}", action.action, action.status),
            }
        }
        if !report.raw_stderr.trim().is_empty() {
            eprint!("{}", report.raw_stderr);
        }
    }

    Ok(code)
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
    fn builds_dry_run_ssh_args() {
        let cli = Cli {
            ssh_bin: "ssh".to_string(),
            ssh_target: DEFAULT_SSH_TARGET.to_string(),
            connect_timeout_seconds: 10,
            json: false,
            apply: false,
            start_optional: false,
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
                "bash",
                "-s",
                "--",
                "dry-run",
                "skip-optional",
            ]
        );
    }

    #[test]
    fn parses_remote_actions() {
        let actions = parse_actions(
            "reset-failed\t\tfalse\tplanned\nactive\tactivitywatch-server.service\ttrue\tok\n",
        );
        assert_eq!(
            actions,
            vec![
                HealAction {
                    action: "reset-failed".to_string(),
                    unit: None,
                    required: false,
                    status: "planned".to_string(),
                },
                HealAction {
                    action: "active".to_string(),
                    unit: Some("activitywatch-server.service".to_string()),
                    required: true,
                    status: "ok".to_string(),
                },
            ]
        );
    }

    #[test]
    fn remote_script_uses_allowlisted_units() {
        let script = remote_script();
        assert!(script.contains("activitywatch-server.service"));
        assert!(script.contains("aw-worktime-api.service"));
        assert!(script.contains("aw-worktime-ui-bridge.timer"));
        assert!(script.contains("activitywatch-dlp-aggregator.timer"));
        assert!(!script.contains("*"));
    }
}
