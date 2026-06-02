use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::{Parser, ValueEnum};
use serde::Serialize;

#[derive(Debug, Clone, Copy, ValueEnum, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum InstallKind {
    Client,
    RemoteWorker,
    ConsoleSsh,
    WebCategory,
    PveWebadmin,
}

#[derive(Debug, Parser)]
#[command(about = "Safe planner/apply wrapper for AW Linux install scripts")]
struct Cli {
    #[arg(long, value_enum)]
    kind: InstallKind,

    #[arg(long)]
    legacy_script: PathBuf,

    #[arg(long, default_value = "10.10.10.13")]
    server_host: String,

    #[arg(long, default_value = "5600")]
    server_port: String,

    #[arg(long, default_value = "5")]
    poll_interval: String,

    #[arg(long, default_value = "0.13.2")]
    version: String,

    #[arg(long)]
    install_base: Option<PathBuf>,

    #[arg(long, default_value_t = false)]
    force: bool,

    #[arg(long, default_value_t = false)]
    apply: bool,

    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Debug, Serialize)]
struct Plan {
    apply: bool,
    kind: InstallKind,
    legacy_script: PathBuf,
    required_files: Vec<Requirement>,
    steps: Vec<Step>,
    missing_count: usize,
}

#[derive(Debug, Serialize)]
struct Requirement {
    name: String,
    ok: bool,
    detail: String,
}

#[derive(Debug, Serialize)]
struct Step {
    order: usize,
    name: String,
    mutation: bool,
    command: Vec<String>,
    summary: String,
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
    let plan = build_plan(&cli);
    if !cli.apply {
        print_plan(&plan, cli.json)?;
        return Ok(if plan.missing_count == 0 { 0 } else { 2 });
    }
    if plan.missing_count > 0 {
        print_plan(&plan, cli.json)?;
        bail!("refusing --apply because required files are missing");
    }
    let Some(step) = plan.steps.first() else {
        bail!("empty install plan");
    };
    let Some(program) = step.command.first() else {
        bail!("empty legacy command");
    };
    let status = Command::new(program)
        .args(&step.command[1..])
        .status()
        .with_context(|| format!("run {}", shell_join(&step.command)))?;
    Ok(status.code().unwrap_or(1))
}

fn build_plan(cli: &Cli) -> Plan {
    let required_files = vec![Requirement {
        name: "legacy_script".to_string(),
        ok: cli.legacy_script.is_file(),
        detail: cli.legacy_script.display().to_string(),
    }];
    let command = legacy_command(cli);
    let steps = vec![Step {
        order: 1,
        name: format!("{:?}", cli.kind).to_lowercase(),
        mutation: true,
        summary: summary(cli.kind).to_string(),
        command,
    }];
    let missing_count = required_files.iter().filter(|item| !item.ok).count();
    Plan {
        apply: cli.apply,
        kind: cli.kind,
        legacy_script: cli.legacy_script.clone(),
        required_files,
        steps,
        missing_count,
    }
}

fn legacy_command(cli: &Cli) -> Vec<String> {
    let mut command = vec![
        "sh".to_string(),
        cli.legacy_script.display().to_string(),
        "--apply-legacy".to_string(),
        "--server-host".to_string(),
        cli.server_host.clone(),
        "--server-port".to_string(),
        cli.server_port.clone(),
    ];
    match cli.kind {
        InstallKind::Client => {
            command.extend(["--version".to_string(), cli.version.clone()]);
            if let Some(path) = &cli.install_base {
                command.extend(["--install-base".to_string(), path.display().to_string()]);
            }
            if cli.force {
                command.push("--force".to_string());
            }
        }
        InstallKind::RemoteWorker => {
            command.extend([
                "--poll-interval".to_string(),
                cli.poll_interval.clone(),
                "--version".to_string(),
                cli.version.clone(),
            ]);
        }
        InstallKind::ConsoleSsh | InstallKind::WebCategory | InstallKind::PveWebadmin => {
            command.extend(["--poll-interval".to_string(), cli.poll_interval.clone()]);
        }
    }
    command
}

fn summary(kind: InstallKind) -> &'static str {
    match kind {
        InstallKind::Client => {
            "Install ActivityWatch Linux GUI watcher bundle and remote server config"
        }
        InstallKind::RemoteWorker => {
            "Install Linux client, console/SSH logger, and web category logger"
        }
        InstallKind::ConsoleSsh => "Install console command and SSH session logger",
        InstallKind::WebCategory => "Install Linux web category logger",
        InstallKind::PveWebadmin => "Install Proxmox webadmin logger service",
    }
}

fn print_plan(plan: &Plan, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(plan)?);
        return Ok(());
    }
    println!(
        "aw-linux-install: {}",
        if plan.apply { "apply" } else { "dry-run" }
    );
    println!("kind: {:?}", plan.kind);
    println!("legacy_script: {}", plan.legacy_script.display());
    println!("missing_inputs: {}", plan.missing_count);
    println!("required files:");
    for item in &plan.required_files {
        println!(
            "  [{}] {} - {}",
            if item.ok { "OK" } else { "MISS" },
            item.name,
            item.detail
        );
    }
    println!("planned steps:");
    for step in &plan.steps {
        println!(
            "  {:02}. MUTATION {} :: {}",
            step.order,
            step.summary,
            shell_join(&step.command)
        );
    }
    if !plan.apply {
        println!("No install executed. Use --apply for explicit legacy install execution.");
    }
    Ok(())
}

fn shell_join(command: &[String]) -> String {
    command
        .iter()
        .map(|part| {
            if part
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || "-_./:=,".contains(ch))
            {
                part.clone()
            } else {
                format!("'{}'", part.replace('\'', "'\\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_plan_preserves_version_and_force() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("install.sh");
        std::fs::write(&script, "#!/bin/sh\n").unwrap();
        let cli = Cli {
            kind: InstallKind::Client,
            legacy_script: script,
            server_host: "10.10.10.13".to_string(),
            server_port: "5600".to_string(),
            poll_interval: "5".to_string(),
            version: "0.13.2".to_string(),
            install_base: Some(PathBuf::from("/tmp/aw")),
            force: true,
            apply: false,
            json: false,
        };
        let plan = build_plan(&cli);
        let cmd = &plan.steps[0].command;
        assert!(cmd.contains(&"--apply-legacy".to_string()));
        assert!(cmd.contains(&"--install-base".to_string()));
        assert!(cmd.contains(&"--force".to_string()));
    }

    #[test]
    fn remote_worker_plan_includes_poll_and_version() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("install.sh");
        std::fs::write(&script, "#!/bin/sh\n").unwrap();
        let cli = Cli {
            kind: InstallKind::RemoteWorker,
            legacy_script: script,
            server_host: "host".to_string(),
            server_port: "5600".to_string(),
            poll_interval: "9".to_string(),
            version: "0.13.3".to_string(),
            install_base: None,
            force: false,
            apply: false,
            json: false,
        };
        let plan = build_plan(&cli);
        let joined = shell_join(&plan.steps[0].command);
        assert!(joined.contains("--poll-interval 9"));
        assert!(joined.contains("--version 0.13.3"));
    }
}
