use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Parser;
use serde::Serialize;

const LOGROTATE_CONTENT: &str = include_str!("../../../../aw-server/logrotate.conf");
const HEALTH_TIMER_CONTENT: &str = r#"[Unit]
Description=AW Health Check Timer
Requires=aw-health-check.service

[Timer]
OnCalendar=*:0/5:00
Persistent=true

[Install]
WantedBy=timers.target
"#;
const HEALTH_SERVICE_CONTENT: &str = r#"[Unit]
Description=AW Health Check
After=network.target

[Service]
Type=oneshot
ExecStart=/usr/local/bin/aw-health-check
User=root
Group=root
"#;

#[derive(Debug, Parser)]
#[command(about = "Plan or apply AW service reliability hardening")]
struct Cli {
    #[arg(long, default_value = "/etc/activitywatch/aw-server.env")]
    env_file: PathBuf,

    #[arg(long, default_value = "/var/lib/activitywatch")]
    data_dir: PathBuf,

    #[arg(long, default_value = "/var/log/activitywatch")]
    log_dir: PathBuf,

    #[arg(long, default_value = "/opt/activitywatch")]
    opt_dir: PathBuf,

    #[arg(long, default_value = "/etc/logrotate.d/activitywatch")]
    logrotate_target: PathBuf,

    #[arg(long, default_value = "/usr/local/bin/aw-health-check")]
    health_script_target: PathBuf,

    #[arg(long, default_value = "/etc/systemd/system/aw-health-check.timer")]
    health_timer_target: PathBuf,

    #[arg(long, default_value = "/etc/systemd/system/aw-health-check.service")]
    health_service_target: PathBuf,

    #[arg(long, default_value_t = false)]
    apply: bool,

    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum StepKind {
    Check,
    Chown,
    Chmod,
    Write,
    Systemd,
    Sleep,
}

#[derive(Debug, Clone, Serialize)]
struct Step {
    order: usize,
    name: String,
    kind: StepKind,
    command: String,
    mutation: bool,
    needed: bool,
    reason: String,
}

#[derive(Debug, Serialize)]
struct Report {
    apply: bool,
    ok: bool,
    env_file: PathBuf,
    missing_required: Vec<String>,
    steps: Vec<Step>,
    executed: Vec<ExecResult>,
}

#[derive(Debug, Serialize)]
struct ExecResult {
    order: usize,
    name: String,
    ok: bool,
    exit_code: Option<i32>,
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
    let mut report = build_report(&cli);
    if cli.apply {
        if !report.missing_required.is_empty() {
            report.ok = false;
            print_report(&report, cli.json)?;
            bail!("refusing --apply because required inputs are missing");
        }
        apply_steps(&mut report)?;
    }
    print_report(&report, cli.json)?;
    Ok(if report.ok { 0 } else { 1 })
}

fn build_report(cli: &Cli) -> Report {
    let mut missing_required = Vec::new();
    if !cli.env_file.is_file() {
        missing_required.push(format!("env file missing: {}", cli.env_file.display()));
    }
    if !cli.health_script_target.is_file() {
        missing_required.push(format!(
            "health script target missing: {}",
            cli.health_script_target.display()
        ));
    }

    let mut steps = Vec::new();
    push_step(
        &mut steps,
        "check-env-file",
        StepKind::Check,
        format!("test -f {}", shell_quote(&cli.env_file)),
        false,
        !cli.env_file.is_file(),
        "required before reliability actions".to_string(),
    );
    for dir in [&cli.data_dir, &cli.log_dir, &cli.opt_dir] {
        push_step(
            &mut steps,
            format!("chown-{}", dir.display()),
            StepKind::Chown,
            format!("chown -R activitywatch:activitywatch {}", shell_quote(dir)),
            true,
            true,
            "preserve legacy ownership repair".to_string(),
        );
    }
    for dir in [&cli.data_dir, &cli.log_dir, &cli.opt_dir] {
        push_step(
            &mut steps,
            format!("chmod-{}", dir.display()),
            StepKind::Chmod,
            format!("chmod 755 {}", shell_quote(dir)),
            true,
            true,
            "preserve legacy directory mode repair".to_string(),
        );
    }
    push_step(
        &mut steps,
        "install-logrotate",
        StepKind::Write,
        format!("write {}", shell_quote(&cli.logrotate_target)),
        true,
        !cli.logrotate_target.is_file(),
        if cli.logrotate_target.is_file() {
            "logrotate already configured".to_string()
        } else {
            "logrotate target missing".to_string()
        },
    );
    push_step(
        &mut steps,
        "check-health-script",
        StepKind::Check,
        format!("test -x {}", shell_quote(&cli.health_script_target)),
        false,
        !cli.health_script_target.is_file(),
        if cli.health_script_target.is_file() {
            "health script already installed by Ansible".to_string()
        } else {
            "health script target missing".to_string()
        },
    );
    for (name, path, content_name) in [
        (
            "install-health-timer",
            &cli.health_timer_target,
            "aw-health-check.timer",
        ),
        (
            "install-health-service",
            &cli.health_service_target,
            "aw-health-check.service",
        ),
    ] {
        push_step(
            &mut steps,
            name,
            StepKind::Write,
            format!("write {} ({content_name})", shell_quote(path)),
            true,
            !path.is_file(),
            if path.is_file() {
                format!("{content_name} already installed")
            } else {
                format!("{content_name} target missing")
            },
        );
    }
    for (name, command) in [
        ("daemon-reload-before-restart", "systemctl daemon-reload"),
        (
            "stop-services",
            "systemctl stop aw-worktime-api aw-worktime-ui-bridge activitywatch-server || true",
        ),
        ("sleep-after-stop", "sleep 2"),
        (
            "start-activitywatch-server",
            "systemctl start activitywatch-server",
        ),
        ("sleep-after-server-start", "sleep 3"),
        ("start-worktime-api", "systemctl start aw-worktime-api"),
        ("sleep-after-api-start", "sleep 2"),
        (
            "start-worktime-ui-bridge",
            "systemctl start aw-worktime-ui-bridge",
        ),
        (
            "enable-activitywatch-server",
            "systemctl enable activitywatch-server",
        ),
        ("enable-worktime-api", "systemctl enable aw-worktime-api"),
        (
            "enable-worktime-ui-bridge",
            "systemctl enable aw-worktime-ui-bridge",
        ),
        ("daemon-reload-health", "systemctl daemon-reload"),
        (
            "enable-health-timer",
            "systemctl enable aw-health-check.timer",
        ),
        (
            "start-health-timer",
            "systemctl start aw-health-check.timer",
        ),
    ] {
        let kind = if command.starts_with("sleep") {
            StepKind::Sleep
        } else {
            StepKind::Systemd
        };
        push_step(
            &mut steps,
            name,
            kind,
            command.to_string(),
            true,
            true,
            "preserve legacy reliability action".to_string(),
        );
    }

    Report {
        apply: cli.apply,
        ok: missing_required.is_empty(),
        env_file: cli.env_file.clone(),
        missing_required,
        steps,
        executed: Vec::new(),
    }
}

fn push_step(
    steps: &mut Vec<Step>,
    name: impl Into<String>,
    kind: StepKind,
    command: String,
    mutation: bool,
    needed: bool,
    reason: String,
) {
    steps.push(Step {
        order: steps.len() + 1,
        name: name.into(),
        kind,
        command,
        mutation,
        needed,
        reason,
    });
}

fn apply_steps(report: &mut Report) -> Result<()> {
    let steps = report.steps.clone();
    for step in steps.iter().filter(|step| step.needed) {
        let result = match step.name.as_str() {
            "check-env-file" => ExecResult {
                order: step.order,
                name: step.name.clone(),
                ok: Path::new(&report.env_file).is_file(),
                exit_code: Some(if Path::new(&report.env_file).is_file() {
                    0
                } else {
                    1
                }),
            },
            "check-health-script" => {
                let target = Path::new("/usr/local/bin/aw-health-check");
                ExecResult {
                    order: step.order,
                    name: step.name.clone(),
                    ok: target.is_file(),
                    exit_code: Some(if target.is_file() { 0 } else { 1 }),
                }
            }
            "install-logrotate" => write_file_result(step, report, LOGROTATE_CONTENT, 0o644)?,
            "install-health-timer" => write_file_result(step, report, HEALTH_TIMER_CONTENT, 0o644)?,
            "install-health-service" => {
                write_file_result(step, report, HEALTH_SERVICE_CONTENT, 0o644)?
            }
            _ => run_shell_step(step)?,
        };
        let ok = result.ok;
        report.executed.push(result);
        if !ok {
            report.ok = false;
            return Ok(());
        }
    }
    report.ok = true;
    Ok(())
}

fn write_file_result(step: &Step, report: &Report, content: &str, mode: u32) -> Result<ExecResult> {
    let target = match step.name.as_str() {
        "install-logrotate" => target_from_command(&step.command)?,
        "install-health-timer" => target_from_command(&step.command)?,
        "install-health-service" => target_from_command(&step.command)?,
        _ => bail!("unsupported write step {}", step.name),
    };
    let _ = report;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    fs::write(&target, content).with_context(|| format!("write {}", target.display()))?;
    set_mode(&target, mode).with_context(|| format!("chmod {:o} {}", mode, target.display()))?;
    Ok(ExecResult {
        order: step.order,
        name: step.name.clone(),
        ok: true,
        exit_code: Some(0),
    })
}

fn run_shell_step(step: &Step) -> Result<ExecResult> {
    let status = Command::new("sh")
        .arg("-c")
        .arg(&step.command)
        .status()
        .with_context(|| format!("run {}", step.command))?;
    Ok(ExecResult {
        order: step.order,
        name: step.name.clone(),
        ok: status.success(),
        exit_code: status.code(),
    })
}

fn target_from_command(command: &str) -> Result<PathBuf> {
    let raw = command
        .split_whitespace()
        .nth(1)
        .or_else(|| command.split_whitespace().nth(2))
        .context("parse target from command")?;
    Ok(PathBuf::from(raw.trim_matches('\'')))
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(mode);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

fn print_report(report: &Report, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }
    println!(
        "aw-ensure-reliability: {}",
        if report.apply { "apply" } else { "dry-run" }
    );
    println!("env_file: {}", report.env_file.display());
    println!("ok: {}", report.ok);
    if !report.missing_required.is_empty() {
        println!("missing_required:");
        for item in &report.missing_required {
            println!("  - {item}");
        }
    }
    println!("planned steps:");
    for step in &report.steps {
        let risk = if step.mutation { "MUTATION" } else { "check" };
        let needed = if step.needed { "needed" } else { "skip" };
        println!(
            "  {:02}. {:<28} {:<8} {:<6} {}",
            step.order, step.name, risk, needed, step.command
        );
    }
    if report.executed.is_empty() {
        println!("No mutation executed. Use --apply for explicit reliability fix.");
    } else {
        println!("executed:");
        for item in &report.executed {
            println!(
                "  {:02}. {:<28} ok={} exit={:?}",
                item.order, item.name, item.ok, item.exit_code
            );
        }
    }
    Ok(())
}

fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_run_marks_legacy_mutations() {
        let dir = tempfile::tempdir().unwrap();
        let env_file = dir.path().join("aw-server.env");
        let health_target = dir.path().join("bin/aw-health-check");
        fs::write(&env_file, "AW_BASE_URL=http://127.0.0.1:5600\n").unwrap();
        fs::create_dir_all(health_target.parent().unwrap()).unwrap();
        fs::write(&health_target, "#!/bin/sh\nexit 0\n").unwrap();
        let cli = Cli {
            env_file,
            data_dir: dir.path().join("data"),
            log_dir: dir.path().join("log"),
            opt_dir: dir.path().join("opt"),
            logrotate_target: dir.path().join("logrotate/activitywatch"),
            health_script_target: health_target,
            health_timer_target: dir.path().join("systemd/aw-health-check.timer"),
            health_service_target: dir.path().join("systemd/aw-health-check.service"),
            apply: false,
            json: true,
        };
        let report = build_report(&cli);
        assert!(report.ok);
        assert!(report.steps.iter().any(|step| step.name == "stop-services"));
        assert!(
            report
                .steps
                .iter()
                .any(|step| step.name == "install-logrotate")
        );
        assert!(report.steps.iter().any(|step| step.mutation));
    }

    #[test]
    fn missing_env_blocks_apply() {
        let dir = tempfile::tempdir().unwrap();
        let health_target = dir.path().join("bin/aw-health-check");
        fs::create_dir_all(health_target.parent().unwrap()).unwrap();
        fs::write(&health_target, "#!/bin/sh\nexit 0\n").unwrap();
        let cli = Cli {
            env_file: dir.path().join("missing.env"),
            data_dir: dir.path().join("data"),
            log_dir: dir.path().join("log"),
            opt_dir: dir.path().join("opt"),
            logrotate_target: dir.path().join("logrotate/activitywatch"),
            health_script_target: health_target,
            health_timer_target: dir.path().join("systemd/aw-health-check.timer"),
            health_service_target: dir.path().join("systemd/aw-health-check.service"),
            apply: true,
            json: true,
        };
        let report = build_report(&cli);
        assert!(!report.ok);
        assert_eq!(report.missing_required.len(), 1);
    }
}
