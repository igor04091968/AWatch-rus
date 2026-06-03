use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use chrono::Local;
use clap::Parser;
use serde::Serialize;

const DEFAULT_SERVER_HOST: &str = "10.10.10.13";
const DEFAULT_SERVER_USER: &str = "igor";
const DEFAULT_LEGACY_DB: &str = "/root/.local/share/activitywatch/aw-server-rust/sqlite.db";
const DEFAULT_TARGET_DB: &str =
    "/var/lib/activitywatch/.local/share/activitywatch/aw-server-rust/sqlite.db";
const DEFAULT_REMOTE_MERGE_BIN: &str = "/tmp/merge-aw-server-dbs";

#[derive(Debug, Parser)]
#[command(about = "Safe planner/checker for the destructive prod backup restore flow")]
struct Cli {
    #[arg(long, default_value = ".")]
    root: PathBuf,

    #[arg(long)]
    check_inputs: bool,

    #[arg(long)]
    json: bool,

    #[arg(long)]
    timestamp: Option<String>,

    #[arg(long)]
    server_host: Option<String>,

    #[arg(long)]
    server_user: Option<String>,

    #[arg(long, default_value = "ansible/inventory.ini")]
    inventory: PathBuf,

    #[arg(long, default_value = DEFAULT_LEGACY_DB)]
    legacy_db: String,

    #[arg(long, default_value = DEFAULT_TARGET_DB)]
    target_db: String,

    #[arg(long, default_value = DEFAULT_REMOTE_MERGE_BIN)]
    remote_merge_bin: String,

    #[arg(long)]
    apply: bool,
}

#[derive(Debug, Serialize)]
struct Plan {
    mode: &'static str,
    root: String,
    env_file: String,
    server_host: String,
    server_user: String,
    timestamp: String,
    remote_backup_dir: String,
    legacy_db: String,
    target_db: String,
    remote_merge_bin: String,
    required_env: Vec<Requirement>,
    required_commands: Vec<Requirement>,
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
    kind: &'static str,
    command: String,
    destructive: bool,
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
    if cli.apply {
        bail!(
            "Rust apply is intentionally disabled for this stage; run without --apply to review the safe restore plan"
        );
    }
    let root = cli
        .root
        .canonicalize()
        .with_context(|| format!("canonicalize root {}", cli.root.display()))?;
    let env_file = root.join("secrets/runtime.env");
    let env_values = read_env_file(&env_file).unwrap_or_default();
    let timestamp = cli
        .timestamp
        .clone()
        .unwrap_or_else(|| Local::now().format("%Y%m%d-%H%M%S").to_string());
    let server_host = cli
        .server_host
        .or_else(|| env_value("AW_SERVER_HOST", &env_values))
        .unwrap_or_else(|| DEFAULT_SERVER_HOST.to_string());
    let server_user = cli
        .server_user
        .or_else(|| env_value("AW_SERVER_USER", &env_values))
        .unwrap_or_else(|| DEFAULT_SERVER_USER.to_string());
    let remote_backup_dir = format!("/var/lib/activitywatch/backups/prod-restore-{timestamp}");
    let inventory = if cli.inventory.is_absolute() {
        cli.inventory.clone()
    } else {
        root.join(&cli.inventory)
    };
    let merge_bin = root.join("adk-rust/target/release/merge-aw-server-dbs");
    let plan = build_plan(
        &root,
        &env_file,
        &env_values,
        &server_host,
        &server_user,
        &timestamp,
        &remote_backup_dir,
        &cli.legacy_db,
        &cli.target_db,
        &cli.remote_merge_bin,
        &inventory,
        &merge_bin,
    );

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&plan)?);
    } else {
        print_plan(&plan);
    }

    if cli.check_inputs && plan.missing_count > 0 {
        Ok(2)
    } else {
        Ok(0)
    }
}

#[allow(clippy::too_many_arguments)]
fn build_plan(
    root: &Path,
    env_file: &Path,
    env_values: &HashMap<String, String>,
    server_host: &str,
    server_user: &str,
    timestamp: &str,
    remote_backup_dir: &str,
    legacy_db: &str,
    target_db: &str,
    remote_merge_bin: &str,
    inventory: &Path,
    merge_bin: &Path,
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
    let required_commands = ["sshpass", "ansible-playbook"]
        .into_iter()
        .map(|name| Requirement {
            name: name.to_string(),
            ok: command_exists(name),
            detail: if command_exists(name) {
                "found in PATH".to_string()
            } else {
                "missing in PATH".to_string()
            },
        })
        .collect::<Vec<_>>();
    let required_files = [("inventory", inventory), ("merge-aw-server-dbs", merge_bin)]
        .into_iter()
        .map(|(name, path)| Requirement {
            name: name.to_string(),
            ok: path.is_file(),
            detail: path.display().to_string(),
        })
        .collect::<Vec<_>>();

    let mut steps = Vec::new();
    push_step(
        &mut steps,
        "scp",
        format!(
            "sshpass scp adk-rust/target/release/merge-aw-server-dbs {server_user}@{server_host}:{remote_merge_bin}"
        ),
        false,
    );
    push_step(
        &mut steps,
        "ssh",
        format!(
            "sudo mkdir -p '{remote_backup_dir}' && sudo chown root:root '{remote_backup_dir}'"
        ),
        false,
    );
    push_step(
        &mut steps,
        "ssh",
        format!("sudo test -f '{legacy_db}'"),
        false,
    );
    push_step(
        &mut steps,
        "ssh",
        format!("sudo test -f '{target_db}'"),
        false,
    );
    push_step(
        &mut steps,
        "ssh",
        format!(
            "sudo cp -a '{legacy_db}' '{remote_backup_dir}/legacy-root-sqlite.db' && sudo cp -a '{target_db}' '{remote_backup_dir}/target-before-merge-sqlite.db'"
        ),
        false,
    );
    push_step(
        &mut steps,
        "ssh",
        "sudo systemctl stop activitywatch-server.service || true".to_string(),
        true,
    );
    push_step(
        &mut steps,
        "ssh",
        format!(
            "sudo chmod 0755 '{remote_merge_bin}' && sudo '{remote_merge_bin}' --base '{legacy_db}' --overlay '{target_db}' --output '{remote_backup_dir}/sqlite.merged.db'"
        ),
        true,
    );
    push_step(
        &mut steps,
        "ssh",
        format!(
            "sudo install -o activitywatch -g activitywatch -m 0644 '{remote_backup_dir}/sqlite.merged.db' '{target_db}'"
        ),
        true,
    );
    for playbook in [
        "ansible/deploy_aw_server.yml",
        "ansible/deploy_aw_windows.yml",
        "ansible/post_validate_aw_windows.yml",
    ] {
        push_step(
            &mut steps,
            "ansible-playbook",
            format!("ansible-playbook -i {} {playbook}", inventory.display()),
            true,
        );
    }
    push_step(
        &mut steps,
        "validate",
        "query AW historical window data for 2026-04-29 and settings always_active_pattern"
            .to_string(),
        false,
    );

    let missing_count = required_env
        .iter()
        .chain(required_commands.iter())
        .chain(required_files.iter())
        .filter(|item| !item.ok)
        .count();

    Plan {
        mode: "plan-only",
        root: root.display().to_string(),
        env_file: env_file.display().to_string(),
        server_host: server_host.to_string(),
        server_user: server_user.to_string(),
        timestamp: timestamp.to_string(),
        remote_backup_dir: remote_backup_dir.to_string(),
        legacy_db: legacy_db.to_string(),
        target_db: target_db.to_string(),
        remote_merge_bin: remote_merge_bin.to_string(),
        required_env,
        required_commands,
        required_files,
        steps,
        missing_count,
    }
}

fn push_step(steps: &mut Vec<Step>, kind: &'static str, command: String, destructive: bool) {
    steps.push(Step {
        order: steps.len() + 1,
        kind,
        command,
        destructive,
    });
}

fn print_plan(plan: &Plan) {
    println!("prod-backup-restore: {}", plan.mode);
    println!("root: {}", plan.root);
    println!("server: {}@{}", plan.server_user, plan.server_host);
    println!("remote_backup_dir: {}", plan.remote_backup_dir);
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
    println!("Planned steps, not executed:");
    for step in &plan.steps {
        let risk = if step.destructive {
            "MUTATION"
        } else {
            "read/prepare"
        };
        println!(
            "  {:02}. {:<16} {:<12} {}",
            step.order, step.kind, risk, step.command
        );
    }
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
        if key.is_empty() || key.starts_with("export ") {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_simple_shell_quotes() {
        assert_eq!(strip_shell_quotes("'secret'"), "secret");
        assert_eq!(strip_shell_quotes("\"secret\""), "secret");
        assert_eq!(strip_shell_quotes("plain"), "plain");
    }

    #[test]
    fn reads_simple_env_file_without_exposing_values() {
        let dir = tempfile::tempdir().unwrap();
        let env = dir.path().join("runtime.env");
        fs::write(
            &env,
            "AW_SSH_PASSWORD='one'\nAW_WINRM_PASSWORD=\"two\"\n# ignored\n",
        )
        .unwrap();
        let values = read_env_file(&env).unwrap();
        assert_eq!(values.get("AW_SSH_PASSWORD").unwrap(), "one");
        assert_eq!(values.get("AW_WINRM_PASSWORD").unwrap(), "two");
    }

    #[test]
    fn plan_marks_destructive_steps() {
        let dir = tempfile::tempdir().unwrap();
        let inventory = dir.path().join("ansible/inventory.ini");
        let merge = dir
            .path()
            .join("adk-rust/target/release/merge-aw-server-dbs");
        fs::create_dir_all(inventory.parent().unwrap()).unwrap();
        fs::create_dir_all(merge.parent().unwrap()).unwrap();
        fs::write(&inventory, "").unwrap();
        fs::write(&merge, "").unwrap();
        let mut env = HashMap::new();
        env.insert("AW_SSH_PASSWORD".to_string(), "hidden".to_string());
        env.insert("AW_WINRM_PASSWORD".to_string(), "hidden".to_string());
        let plan = build_plan(
            dir.path(),
            &dir.path().join("secrets/runtime.env"),
            &env,
            "10.10.10.13",
            "igor",
            "20260602-000000",
            "/var/lib/activitywatch/backups/prod-restore-20260602-000000",
            DEFAULT_LEGACY_DB,
            DEFAULT_TARGET_DB,
            DEFAULT_REMOTE_MERGE_BIN,
            &inventory,
            &merge,
        );
        assert!(plan.steps.iter().any(|step| step.destructive));
        assert!(
            plan.steps
                .iter()
                .any(|step| step.command.contains("systemctl stop activitywatch-server"))
        );
        assert_eq!(plan.required_env.iter().filter(|item| item.ok).count(), 2);
    }
}
