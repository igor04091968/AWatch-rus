use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use chrono::Local;
use clap::Parser;
use serde::Serialize;

#[derive(Debug, Parser)]
#[command(about = "Safe planner/orchestrator for the production ActivityWatch rollout")]
struct Cli {
    #[arg(long, default_value = ".")]
    root: PathBuf,

    #[arg(long, default_value = "ansible/inventory.ini")]
    inventory: PathBuf,

    #[arg(long)]
    check_inputs: bool,

    #[arg(long)]
    json: bool,

    #[arg(long)]
    apply: bool,

    #[arg(long)]
    skip_quality_gate: bool,

    #[arg(long)]
    timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct Requirement {
    name: String,
    ok: bool,
    detail: String,
}

#[derive(Debug, Clone, Serialize)]
struct Step {
    order: usize,
    name: String,
    command: Vec<String>,
    log_file: String,
    mutation: bool,
}

#[derive(Debug, Serialize)]
struct Plan {
    mode: &'static str,
    root: String,
    branch: String,
    env_file: String,
    inventory: String,
    log_dir: String,
    required_env: Vec<Requirement>,
    required_commands: Vec<Requirement>,
    required_files: Vec<Requirement>,
    steps: Vec<Step>,
    missing_count: usize,
}

#[derive(Debug, Serialize)]
struct StepResult {
    order: usize,
    name: String,
    status: &'static str,
    exit_code: Option<i32>,
    log_file: String,
}

#[derive(Debug, Serialize)]
struct ApplyReport {
    plan: Plan,
    results: Vec<StepResult>,
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
    let root = cli
        .root
        .canonicalize()
        .with_context(|| format!("canonicalize root {}", cli.root.display()))?;
    let timestamp = cli
        .timestamp
        .clone()
        .unwrap_or_else(|| Local::now().format("%Y%m%d-%H%M%S").to_string());
    let env_file = root.join("secrets/runtime.env");
    let env_values = read_env_file(&env_file).unwrap_or_default();
    let inventory = absolute_path(&root, &cli.inventory);
    let log_dir = root.join(".rollout-logs").join(&timestamp);
    let plan = build_plan(
        &root,
        &env_file,
        &env_values,
        &inventory,
        &log_dir,
        cli.apply,
        cli.skip_quality_gate,
    );

    if !cli.apply {
        if cli.json {
            println!("{}", serde_json::to_string_pretty(&plan)?);
        } else {
            print_plan(&plan);
        }
        return if cli.check_inputs && plan.missing_count > 0 {
            Ok(2)
        } else {
            Ok(0)
        };
    }

    if plan.missing_count > 0 {
        if cli.json {
            println!("{}", serde_json::to_string_pretty(&plan)?);
        } else {
            print_plan(&plan);
        }
        bail!("refusing --apply because required inputs are missing");
    }

    let report = apply_plan(plan, env_values)?;
    if cli.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_apply_report(&report);
    }
    Ok(if report.results.iter().all(|item| item.status == "ok") {
        0
    } else {
        1
    })
}

fn absolute_path(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn build_plan(
    root: &Path,
    env_file: &Path,
    env_values: &HashMap<String, String>,
    inventory: &Path,
    log_dir: &Path,
    apply: bool,
    skip_quality_gate: bool,
) -> Plan {
    let required_env = ["AW_SSH_PASSWORD", "AW_WINRM_PASSWORD"]
        .into_iter()
        .map(|name| {
            let ok = env_value(name, env_values).is_some();
            Requirement {
                name: name.to_string(),
                ok,
                detail: if ok {
                    "present (value hidden)".to_string()
                } else {
                    "missing".to_string()
                },
            }
        })
        .collect::<Vec<_>>();

    let required_commands = ["git", "ansible", "ansible-playbook"]
        .into_iter()
        .map(|name| {
            let ok = command_exists(name);
            Requirement {
                name: name.to_string(),
                ok,
                detail: if ok {
                    "found in PATH".to_string()
                } else {
                    "missing in PATH".to_string()
                },
            }
        })
        .collect::<Vec<_>>();

    let quality_gate = root.join("scripts/quality-gate.sh");
    let deploy_aw_server = root.join("ansible/deploy_aw_server.yml");
    let deploy_aw_windows = root.join("ansible/deploy_aw_windows.yml");
    let post_validate_aw_windows = root.join("ansible/post_validate_aw_windows.yml");
    let required_files = [
        ("inventory", inventory),
        ("scripts/quality-gate.sh", &quality_gate),
        ("ansible/deploy_aw_server.yml", &deploy_aw_server),
        ("ansible/deploy_aw_windows.yml", &deploy_aw_windows),
        (
            "ansible/post_validate_aw_windows.yml",
            &post_validate_aw_windows,
        ),
    ]
    .into_iter()
    .map(|(name, path)| Requirement {
        name: name.to_string(),
        ok: path.is_file(),
        detail: path.display().to_string(),
    })
    .collect::<Vec<_>>();

    let mut steps = Vec::new();
    if !skip_quality_gate {
        push_step(
            &mut steps,
            "quality-gate",
            vec!["./scripts/quality-gate.sh".to_string()],
            "quality-gate.log",
            false,
        );
    }
    push_step(
        &mut steps,
        "ping-aw-server",
        vec![
            "ansible".to_string(),
            "-i".to_string(),
            "ansible/inventory.ini".to_string(),
            "aw_server".to_string(),
            "-m".to_string(),
            "ping".to_string(),
        ],
        "ping_aw_server.log",
        false,
    );
    push_step(
        &mut steps,
        "ping-aw-windows",
        vec![
            "ansible".to_string(),
            "-i".to_string(),
            "ansible/inventory.ini".to_string(),
            "aw_windows".to_string(),
            "-m".to_string(),
            "win_ping".to_string(),
        ],
        "ping_aw_windows.log",
        false,
    );
    push_step(
        &mut steps,
        "check-aw-server",
        vec![
            "ansible-playbook".to_string(),
            "-i".to_string(),
            "ansible/inventory.ini".to_string(),
            "ansible/deploy_aw_server.yml".to_string(),
            "--check".to_string(),
            "--diff".to_string(),
        ],
        "check_aw_server.log",
        false,
    );
    push_step(
        &mut steps,
        "deploy-aw-server",
        vec![
            "ansible-playbook".to_string(),
            "-i".to_string(),
            "ansible/inventory.ini".to_string(),
            "ansible/deploy_aw_server.yml".to_string(),
        ],
        "deploy_aw_server.log",
        true,
    );
    push_step(
        &mut steps,
        "check-aw-windows",
        vec![
            "ansible-playbook".to_string(),
            "-i".to_string(),
            "ansible/inventory.ini".to_string(),
            "ansible/deploy_aw_windows.yml".to_string(),
            "--check".to_string(),
            "--diff".to_string(),
        ],
        "check_aw_windows.log",
        false,
    );
    push_step(
        &mut steps,
        "deploy-aw-windows",
        vec![
            "ansible-playbook".to_string(),
            "-i".to_string(),
            "ansible/inventory.ini".to_string(),
            "ansible/deploy_aw_windows.yml".to_string(),
        ],
        "deploy_aw_windows.log",
        true,
    );
    push_step(
        &mut steps,
        "post-validate-aw-windows",
        vec![
            "ansible-playbook".to_string(),
            "-i".to_string(),
            "ansible/inventory.ini".to_string(),
            "ansible/post_validate_aw_windows.yml".to_string(),
        ],
        "post_validate_aw_windows.log",
        false,
    );

    let missing_count = required_env
        .iter()
        .chain(required_commands.iter())
        .chain(required_files.iter())
        .filter(|item| !item.ok)
        .count();

    Plan {
        mode: if apply { "apply" } else { "plan-only" },
        root: root.display().to_string(),
        branch: git_branch(root).unwrap_or_else(|| "unknown".to_string()),
        env_file: env_file.display().to_string(),
        inventory: inventory.display().to_string(),
        log_dir: log_dir.display().to_string(),
        required_env,
        required_commands,
        required_files,
        steps,
        missing_count,
    }
}

fn push_step(
    steps: &mut Vec<Step>,
    name: &str,
    command: Vec<String>,
    log_file: &str,
    mutation: bool,
) {
    steps.push(Step {
        order: steps.len() + 1,
        name: name.to_string(),
        command,
        log_file: log_file.to_string(),
        mutation,
    });
}

fn apply_plan(plan: Plan, env_values: HashMap<String, String>) -> Result<ApplyReport> {
    let log_dir = PathBuf::from(&plan.log_dir);
    fs::create_dir_all(&log_dir).with_context(|| format!("create {}", log_dir.display()))?;
    let mut results = Vec::new();
    for step in &plan.steps {
        log_line(&log_dir, &format!("START {} {}", step.order, step.name))?;
        let result = run_step(&plan, step, &env_values)?;
        log_line(
            &log_dir,
            &format!(
                "END {} {} status={} exit={:?}",
                step.order, step.name, result.status, result.exit_code
            ),
        )?;
        let ok = result.status == "ok";
        results.push(result);
        if !ok {
            break;
        }
    }
    Ok(ApplyReport { plan, results })
}

fn run_step(plan: &Plan, step: &Step, env_values: &HashMap<String, String>) -> Result<StepResult> {
    let Some(program) = step.command.first() else {
        bail!("empty command for step {}", step.name);
    };
    let args = &step.command[1..];
    let output = Command::new(program)
        .args(args)
        .current_dir(&plan.root)
        .envs(env_values)
        .output()
        .with_context(|| format!("run {}", shell_join(&step.command)))?;
    let log_file = PathBuf::from(&plan.log_dir).join(&step.log_file);
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)
        .with_context(|| format!("open {}", log_file.display()))?;
    file.write_all(&output.stdout)?;
    file.write_all(&output.stderr)?;
    std::io::stdout().write_all(&output.stdout)?;
    std::io::stderr().write_all(&output.stderr)?;
    Ok(StepResult {
        order: step.order,
        name: step.name.clone(),
        status: if output.status.success() {
            "ok"
        } else {
            "failed"
        },
        exit_code: output.status.code(),
        log_file: log_file.display().to_string(),
    })
}

fn print_plan(plan: &Plan) {
    println!("prod-rollout: {}", plan.mode);
    println!("root: {}", plan.root);
    println!("branch: {}", plan.branch);
    println!("inventory: {}", plan.inventory);
    println!("log_dir: {}", plan.log_dir);
    println!("missing_inputs: {}", plan.missing_count);
    println!();
    println!("Required env:");
    for item in &plan.required_env {
        println!("  [{}] {} - {}", ok_mark(item.ok), item.name, item.detail);
    }
    println!("Required commands:");
    for item in &plan.required_commands {
        println!("  [{}] {} - {}", ok_mark(item.ok), item.name, item.detail);
    }
    println!("Required files:");
    for item in &plan.required_files {
        println!("  [{}] {} - {}", ok_mark(item.ok), item.name, item.detail);
    }
    println!();
    println!("Planned steps:");
    for step in &plan.steps {
        let risk = if step.mutation { "MUTATION" } else { "check" };
        println!(
            "  {:02}. {:<24} {:<8} {}",
            step.order,
            step.name,
            risk,
            shell_join(&step.command)
        );
    }
    if plan.mode == "plan-only" {
        println!();
        println!("No steps executed. Use --apply for explicit rollout execution.");
    }
}

fn print_apply_report(report: &ApplyReport) {
    println!("prod-rollout apply result:");
    println!("log_dir: {}", report.plan.log_dir);
    for item in &report.results {
        println!(
            "  {:02}. {:<24} {:<6} exit={:?} log={}",
            item.order, item.name, item.status, item.exit_code, item.log_file
        );
    }
}

fn log_line(log_dir: &Path, line: &str) -> Result<()> {
    fs::create_dir_all(log_dir).with_context(|| format!("create {}", log_dir.display()))?;
    let path = log_dir.join("rollout.log");
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("open {}", path.display()))?;
    writeln!(
        file,
        "{} {}",
        Local::now().format("%Y-%m-%d %H:%M:%S"),
        line
    )?;
    Ok(())
}

fn ok_mark(ok: bool) -> &'static str {
    if ok { "OK" } else { "MISS" }
}

fn env_value(name: &str, env_values: &HashMap<String, String>) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| env_values.get(name).cloned())
        .filter(|value| !value.trim().is_empty())
}

fn read_env_file(path: &Path) -> Result<HashMap<String, String>> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut values = HashMap::new();
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || !line.contains('=') {
            continue;
        }
        let mut parts = line.splitn(2, '=');
        let Some(key) = parts.next().map(str::trim) else {
            continue;
        };
        let Some(value) = parts.next().map(str::trim) else {
            continue;
        };
        if key.is_empty() {
            continue;
        }
        let key = key.strip_prefix("export ").unwrap_or(key).trim();
        if key.is_empty() {
            continue;
        }
        values.insert(key.to_string(), strip_shell_quotes(value).to_string());
    }
    Ok(values)
}

fn strip_shell_quotes(value: &str) -> &str {
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

fn command_exists(name: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {name} >/dev/null 2>&1"))
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn git_branch(root: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch.is_empty() {
        None
    } else {
        Some(branch)
    }
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
    fn reads_exported_env_and_strips_quotes() {
        let dir = tempfile::tempdir().unwrap();
        let env = dir.path().join("runtime.env");
        fs::write(
            &env,
            "export AW_SSH_PASSWORD='one'\nAW_WINRM_PASSWORD=\"two\"\n",
        )
        .unwrap();
        let values = read_env_file(&env).unwrap();
        assert_eq!(values.get("AW_SSH_PASSWORD").unwrap(), "one");
        assert_eq!(values.get("AW_WINRM_PASSWORD").unwrap(), "two");
    }

    #[test]
    fn plan_marks_rollout_mutations() {
        let dir = tempfile::tempdir().unwrap();
        create_file(dir.path().join("scripts/quality-gate.sh"));
        create_file(dir.path().join("ansible/inventory.ini"));
        create_file(dir.path().join("ansible/deploy_aw_server.yml"));
        create_file(dir.path().join("ansible/deploy_aw_windows.yml"));
        create_file(dir.path().join("ansible/post_validate_aw_windows.yml"));
        let mut env = HashMap::new();
        env.insert("AW_SSH_PASSWORD".to_string(), "hidden".to_string());
        env.insert("AW_WINRM_PASSWORD".to_string(), "hidden".to_string());
        let plan = build_plan(
            dir.path(),
            &dir.path().join("secrets/runtime.env"),
            &env,
            &dir.path().join("ansible/inventory.ini"),
            &dir.path().join(".rollout-logs/test"),
            false,
            false,
        );
        assert!(
            plan.steps
                .iter()
                .any(|step| step.name == "deploy-aw-server")
        );
        assert!(
            plan.steps
                .iter()
                .any(|step| step.name == "deploy-aw-windows")
        );
        assert!(
            plan.steps
                .iter()
                .filter(|step| step.mutation)
                .all(|step| step.name.starts_with("deploy-"))
        );
        assert_eq!(plan.required_env.iter().filter(|item| item.ok).count(), 2);
    }

    #[test]
    fn skip_quality_gate_removes_first_step() {
        let dir = tempfile::tempdir().unwrap();
        create_file(dir.path().join("scripts/quality-gate.sh"));
        create_file(dir.path().join("ansible/inventory.ini"));
        create_file(dir.path().join("ansible/deploy_aw_server.yml"));
        create_file(dir.path().join("ansible/deploy_aw_windows.yml"));
        create_file(dir.path().join("ansible/post_validate_aw_windows.yml"));
        let plan = build_plan(
            dir.path(),
            &dir.path().join("secrets/runtime.env"),
            &HashMap::new(),
            &dir.path().join("ansible/inventory.ini"),
            &dir.path().join(".rollout-logs/test"),
            false,
            true,
        );
        assert_ne!(plan.steps.first().unwrap().name, "quality-gate");
    }

    fn create_file(path: PathBuf) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "").unwrap();
    }
}
