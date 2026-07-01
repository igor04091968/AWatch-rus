use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use chrono::{SecondsFormat, Utc};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Parser)]
#[command(about = "AWatch-rus containment decision engine")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Evaluate a finding against containment policy without mutating hosts.
    Decide {
        #[arg(
            long,
            env = "AW_CONTAINMENT_POLICY",
            default_value = "/etc/activitywatch/containment-policy.json"
        )]
        policy: PathBuf,

        #[arg(long)]
        finding: PathBuf,

        #[arg(long)]
        pretty: bool,
    },
    /// Print an example conservative policy.
    SamplePolicy {
        #[arg(long)]
        pretty: bool,
    },
    /// Print an example high-confidence workstation finding.
    SampleFinding {
        #[arg(long)]
        pretty: bool,
    },
    /// Windows Firewall executor interface: plan/apply/verify/rollback.
    WindowsFirewall {
        #[command(subcommand)]
        action: WindowsFirewallCommand,
    },
    /// Print an example Windows Firewall containment request.
    SampleWindowsFirewallRequest {
        #[arg(long)]
        pretty: bool,
    },
}

#[derive(Debug, Subcommand)]
enum WindowsFirewallCommand {
    /// Build a Windows Firewall containment plan from a request.
    Plan {
        #[arg(long)]
        request: PathBuf,

        #[arg(long)]
        pretty: bool,
    },
    /// Apply a plan on the local Windows host when explicitly confirmed.
    Apply {
        #[arg(long)]
        plan: PathBuf,

        #[arg(long, default_value = "NO")]
        confirm_apply: String,

        #[arg(long)]
        execute_local: bool,

        #[arg(long)]
        pretty: bool,
    },
    /// Verify the plan on the local Windows host, or print verification commands.
    Verify {
        #[arg(long)]
        plan: PathBuf,

        #[arg(long)]
        execute_local: bool,

        #[arg(long)]
        pretty: bool,
    },
    /// Roll back a previously applied Windows Firewall plan.
    Rollback {
        #[arg(long)]
        plan: PathBuf,

        #[arg(long, default_value = "NO")]
        confirm_rollback: String,

        #[arg(long)]
        execute_local: bool,

        #[arg(long)]
        pretty: bool,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Mode {
    Disabled,
    Shadow,
    ManualApproval,
    Auto,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum HostRole {
    Workstation,
    Server,
    DomainController,
    Unknown,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Confidence {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ContainmentAction {
    WindowsFirewallQuarantine,
    PfsenseHostBlock,
    SwitchVlanQuarantine,
    DisableWorkstationAccount,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    enabled: bool,
    mode: Mode,
    default_ttl_minutes: u32,
    require_admin_channel_check: bool,
    allow_auto_for_servers: bool,
    allowed_actions: Vec<ContainmentAction>,
    management_allowlist: Vec<String>,
    minimum_high_signals_for_auto: usize,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: Mode::Shadow,
            default_ttl_minutes: 60,
            require_admin_channel_check: true,
            allow_auto_for_servers: false,
            allowed_actions: vec![
                ContainmentAction::WindowsFirewallQuarantine,
                ContainmentAction::PfsenseHostBlock,
            ],
            management_allowlist: vec![
                "aw_server".to_string(),
                "velociraptor_server".to_string(),
                "admin_jump_host".to_string(),
            ],
            minimum_high_signals_for_auto: 2,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Signal {
    source: String,
    rule_id: String,
    confidence: Confidence,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Finding {
    host: String,
    host_role: HostRole,
    state: String,
    confidence: Confidence,
    signals: Vec<Signal>,
    recommended_action: Option<ContainmentAction>,
    management_channel_checked: bool,
    manual_operator_flag: bool,
}

#[derive(Debug, Clone, Serialize)]
struct Decision {
    ok: bool,
    generated_at_utc: String,
    host: String,
    host_role: HostRole,
    policy_mode: Mode,
    decision_status: String,
    recommended_action: Option<ContainmentAction>,
    ttl_minutes: u32,
    rollback_plan_id: Option<String>,
    would_mutate: bool,
    blockers: Vec<String>,
    audit: Audit,
}

#[derive(Debug, Clone, Serialize)]
struct Audit {
    signals_total: usize,
    critical_signals: usize,
    high_signals: usize,
    management_channel_checked: bool,
    management_allowlist_count: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct WindowsFirewallRequest {
    target_host: String,
    plan_id: String,
    ttl_minutes: u32,
    reason: String,
    management_allowlist: Vec<String>,
    blocked_remote_addresses: Vec<String>,
    profiles: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct WindowsFirewallPlan {
    executor: String,
    plan_id: String,
    generated_at_utc: String,
    target_host: String,
    ttl_minutes: u32,
    reason: String,
    rule_group: String,
    safety_model: String,
    requires_manual_confirmation: bool,
    allow_rules: Vec<FirewallRule>,
    block_rules: Vec<FirewallRule>,
    apply_commands: Vec<String>,
    verify_commands: Vec<String>,
    rollback_commands: Vec<String>,
    blockers: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct FirewallRule {
    display_name: String,
    direction: FirewallDirection,
    action: FirewallAction,
    remote_address: String,
    profile: String,
    description: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum FirewallDirection {
    Inbound,
    Outbound,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum FirewallAction {
    Allow,
    Block,
}

#[derive(Debug, Clone, Serialize)]
struct ExecutorResult {
    executor: String,
    operation: String,
    generated_at_utc: String,
    plan_id: String,
    target_host: String,
    ok: bool,
    execution_status: String,
    would_mutate: bool,
    execute_local_requested: bool,
    commands: Vec<String>,
    stdout: Vec<String>,
    stderr: Vec<String>,
    blockers: Vec<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Decide {
            policy,
            finding,
            pretty,
        } => {
            let policy = read_json::<Policy>(&policy).with_context(|| {
                format!("failed to read containment policy {}", policy.display())
            })?;
            let finding = read_json::<Finding>(&finding)
                .with_context(|| format!("failed to read finding {}", finding.display()))?;
            print_json(&decide(&policy, &finding), pretty)
        }
        Command::SamplePolicy { pretty } => print_json(&Policy::default(), pretty),
        Command::SampleFinding { pretty } => print_json(&sample_finding(), pretty),
        Command::WindowsFirewall { action } => match action {
            WindowsFirewallCommand::Plan { request, pretty } => {
                let request = read_json::<WindowsFirewallRequest>(&request).with_context(|| {
                    format!(
                        "failed to read Windows Firewall request {}",
                        request.display()
                    )
                })?;
                print_json(&build_windows_firewall_plan(&request), pretty)
            }
            WindowsFirewallCommand::Apply {
                plan,
                confirm_apply,
                execute_local,
                pretty,
            } => {
                let plan = read_json::<WindowsFirewallPlan>(&plan).with_context(|| {
                    format!("failed to read Windows Firewall plan {}", plan.display())
                })?;
                print_json(
                    &run_windows_firewall_executor(&plan, "apply", &confirm_apply, execute_local)?,
                    pretty,
                )
            }
            WindowsFirewallCommand::Verify {
                plan,
                execute_local,
                pretty,
            } => {
                let plan = read_json::<WindowsFirewallPlan>(&plan).with_context(|| {
                    format!("failed to read Windows Firewall plan {}", plan.display())
                })?;
                print_json(
                    &run_windows_firewall_executor(&plan, "verify", "YES", execute_local)?,
                    pretty,
                )
            }
            WindowsFirewallCommand::Rollback {
                plan,
                confirm_rollback,
                execute_local,
                pretty,
            } => {
                let plan = read_json::<WindowsFirewallPlan>(&plan).with_context(|| {
                    format!("failed to read Windows Firewall plan {}", plan.display())
                })?;
                print_json(
                    &run_windows_firewall_executor(
                        &plan,
                        "rollback",
                        &confirm_rollback,
                        execute_local,
                    )?,
                    pretty,
                )
            }
        },
        Command::SampleWindowsFirewallRequest { pretty } => {
            print_json(&sample_windows_firewall_request(), pretty)
        }
    }
}

fn read_json<T>(path: &PathBuf) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let data = fs::read(path)?;
    serde_json::from_slice(&data).context("invalid JSON")
}

fn print_json<T: Serialize>(value: &T, pretty: bool) -> Result<()> {
    if pretty {
        println!("{}", serde_json::to_string_pretty(value)?);
    } else {
        println!("{}", serde_json::to_string(value)?);
    }
    Ok(())
}

fn decide(policy: &Policy, finding: &Finding) -> Decision {
    let mut blockers = Vec::new();
    let audit = audit(policy, finding);
    let action = finding
        .recommended_action
        .clone()
        .or_else(|| policy.allowed_actions.first().cloned());

    if !policy.enabled || policy.mode == Mode::Disabled {
        return decision(policy, finding, "disabled", action, false, blockers, audit);
    }

    if !is_suspected_state(&finding.state) && !finding.manual_operator_flag {
        blockers.push(format!("finding_state_not_actionable:{}", finding.state));
    }
    if !meets_signal_threshold(policy, finding, &audit) {
        blockers.push("signal_threshold_not_met".to_string());
    }
    if let Some(action) = &action {
        if !policy.allowed_actions.contains(action) {
            blockers.push(format!("action_not_allowed:{action:?}"));
        }
    } else {
        blockers.push("no_allowed_action".to_string());
    }
    if policy.require_admin_channel_check && !finding.management_channel_checked {
        blockers.push("management_channel_not_checked".to_string());
    }
    if policy.require_admin_channel_check && policy.management_allowlist.is_empty() {
        blockers.push("management_allowlist_empty".to_string());
    }

    match policy.mode {
        Mode::Disabled => decision(policy, finding, "disabled", action, false, blockers, audit),
        Mode::Shadow => decision(
            policy,
            finding,
            if blockers.is_empty() {
                "shadow_recommended"
            } else {
                "shadow_blocked"
            },
            action,
            false,
            blockers,
            audit,
        ),
        Mode::ManualApproval => decision(
            policy,
            finding,
            if blockers.is_empty() {
                "manual_approval_required"
            } else {
                "manual_approval_blocked"
            },
            action,
            false,
            blockers,
            audit,
        ),
        Mode::Auto => {
            if !policy.allow_auto_for_servers && finding.host_role != HostRole::Workstation {
                blockers.push(format!("auto_refuses_host_role:{:?}", finding.host_role));
            }
            let status = if blockers.is_empty() {
                "auto_ready"
            } else {
                "auto_refused"
            };
            decision(policy, finding, status, action, false, blockers, audit)
        }
    }
}

fn decision(
    policy: &Policy,
    finding: &Finding,
    status: &str,
    action: Option<ContainmentAction>,
    would_mutate: bool,
    blockers: Vec<String>,
    audit: Audit,
) -> Decision {
    Decision {
        ok: blockers.is_empty(),
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        host: finding.host.clone(),
        host_role: finding.host_role.clone(),
        policy_mode: policy.mode.clone(),
        decision_status: status.to_string(),
        recommended_action: action,
        ttl_minutes: policy.default_ttl_minutes,
        rollback_plan_id: Some(rollback_plan_id(policy, finding)),
        would_mutate,
        blockers,
        audit,
    }
}

fn audit(policy: &Policy, finding: &Finding) -> Audit {
    Audit {
        signals_total: finding.signals.len(),
        critical_signals: finding
            .signals
            .iter()
            .filter(|signal| signal.confidence == Confidence::Critical)
            .count(),
        high_signals: finding
            .signals
            .iter()
            .filter(|signal| matches!(signal.confidence, Confidence::High | Confidence::Critical))
            .count(),
        management_channel_checked: finding.management_channel_checked,
        management_allowlist_count: policy.management_allowlist.len(),
    }
}

fn meets_signal_threshold(policy: &Policy, finding: &Finding, audit: &Audit) -> bool {
    finding.manual_operator_flag
        || matches!(finding.confidence, Confidence::Critical)
        || audit.critical_signals > 0
        || audit.high_signals >= policy.minimum_high_signals_for_auto
}

fn is_suspected_state(state: &str) -> bool {
    matches!(
        state.trim().to_ascii_lowercase().as_str(),
        "suspected_infected" | "confirmed_infected"
    )
}

fn rollback_plan_id(policy: &Policy, finding: &Finding) -> String {
    let mut hasher = Sha256::new();
    hasher.update(finding.host.as_bytes());
    hasher.update(format!("{:?}", finding.host_role).as_bytes());
    hasher.update(format!("{:?}", policy.mode).as_bytes());
    hasher.update(policy.default_ttl_minutes.to_be_bytes());
    format!("rollback-{:x}", hasher.finalize())[..25].to_string()
}

fn build_windows_firewall_plan(request: &WindowsFirewallRequest) -> WindowsFirewallPlan {
    let mut blockers = validate_windows_firewall_request(request);
    let plan_id = request.plan_id.trim().to_ascii_lowercase();
    let target_host = request.target_host.trim().to_string();
    let rule_group = format!("AWatch-rus containment {plan_id}");
    let profiles = normalized_profiles(&request.profiles);

    let mut allow_rules = Vec::new();
    for remote in &request.management_allowlist {
        for profile in &profiles {
            allow_rules.push(firewall_rule(
                &plan_id,
                "allow-admin-in",
                FirewallDirection::Inbound,
                FirewallAction::Allow,
                remote,
                profile,
                "Keep the management channel reachable during containment.",
            ));
            allow_rules.push(firewall_rule(
                &plan_id,
                "allow-admin-out",
                FirewallDirection::Outbound,
                FirewallAction::Allow,
                remote,
                profile,
                "Keep the management channel reachable during containment.",
            ));
        }
    }

    let mut block_rules = Vec::new();
    for remote in &request.blocked_remote_addresses {
        for profile in &profiles {
            block_rules.push(firewall_rule(
                &plan_id,
                "block-suspect-in",
                FirewallDirection::Inbound,
                FirewallAction::Block,
                remote,
                profile,
                "Block suspected lateral movement to or from explicit remote ranges.",
            ));
            block_rules.push(firewall_rule(
                &plan_id,
                "block-suspect-out",
                FirewallDirection::Outbound,
                FirewallAction::Block,
                remote,
                profile,
                "Block suspected lateral movement to or from explicit remote ranges.",
            ));
        }
    }

    if !blockers.is_empty() {
        blockers.sort();
        blockers.dedup();
    }

    let apply_commands = allow_rules
        .iter()
        .chain(block_rules.iter())
        .map(|rule| new_firewall_rule_command(&rule_group, rule))
        .collect::<Vec<_>>();
    let verify_commands = vec![format!(
        "Get-NetFirewallRule -Group {} | Select-Object DisplayName,Enabled,Direction,Action,Profile",
        ps_quote(&rule_group)
    )];
    let rollback_commands = vec![format!(
        "Remove-NetFirewallRule -Group {}",
        ps_quote(&rule_group)
    )];

    WindowsFirewallPlan {
        executor: "windows_firewall".to_string(),
        plan_id,
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        target_host,
        ttl_minutes: request.ttl_minutes,
        reason: request.reason.trim().to_string(),
        rule_group,
        safety_model:
            "explicit_allow_management_then_explicit_block_ranges_no_default_profile_change"
                .to_string(),
        requires_manual_confirmation: true,
        allow_rules,
        block_rules,
        apply_commands,
        verify_commands,
        rollback_commands,
        blockers,
    }
}

fn validate_windows_firewall_request(request: &WindowsFirewallRequest) -> Vec<String> {
    let mut blockers = Vec::new();
    if !is_safe_identifier(&request.target_host, 1, 128) {
        blockers.push("target_host_invalid".to_string());
    }
    if !is_safe_identifier(&request.plan_id, 8, 64) {
        blockers.push("plan_id_invalid".to_string());
    }
    if request.ttl_minutes == 0 || request.ttl_minutes > 24 * 60 {
        blockers.push("ttl_minutes_must_be_1_to_1440".to_string());
    }
    if request.reason.trim().len() < 8 || request.reason.len() > 240 {
        blockers.push("reason_length_invalid".to_string());
    }
    if request.management_allowlist.is_empty() {
        blockers.push("management_allowlist_empty".to_string());
    }
    if request.blocked_remote_addresses.is_empty() {
        blockers.push("blocked_remote_addresses_empty".to_string());
    }
    if request.management_allowlist.len() > 32 {
        blockers.push("management_allowlist_too_large".to_string());
    }
    if request.blocked_remote_addresses.len() > 64 {
        blockers.push("blocked_remote_addresses_too_large".to_string());
    }

    for remote in &request.management_allowlist {
        if !is_safe_remote_address(remote) {
            blockers.push(format!("management_address_invalid:{remote}"));
        }
    }
    for remote in &request.blocked_remote_addresses {
        if !is_safe_remote_address(remote) {
            blockers.push(format!("blocked_address_invalid:{remote}"));
        }
        if is_broad_block(remote) {
            blockers.push(format!("broad_block_refused:{remote}"));
        }
    }
    for profile in &request.profiles {
        if !is_allowed_profile(profile) {
            blockers.push(format!("profile_invalid:{profile}"));
        }
    }
    if request.blocked_remote_addresses.iter().any(|blocked| {
        request
            .management_allowlist
            .iter()
            .any(|allow| remote_addresses_overlap(blocked, allow))
    }) {
        blockers.push("management_allowlist_overlaps_blocked_remote_addresses".to_string());
    }
    blockers
}

fn normalized_profiles(profiles: &[String]) -> Vec<String> {
    if profiles.is_empty() {
        return vec!["Any".to_string()];
    }
    profiles
        .iter()
        .map(|profile| {
            let profile = profile.trim();
            let mut chars = profile.chars();
            match chars.next() {
                Some(first) => {
                    first.to_ascii_uppercase().to_string() + &chars.as_str().to_ascii_lowercase()
                }
                None => "Any".to_string(),
            }
        })
        .collect()
}

fn firewall_rule(
    plan_id: &str,
    label: &str,
    direction: FirewallDirection,
    action: FirewallAction,
    remote_address: &str,
    profile: &str,
    description: &str,
) -> FirewallRule {
    let direction_label = match direction {
        FirewallDirection::Inbound => "in",
        FirewallDirection::Outbound => "out",
    };
    let action_label = match action {
        FirewallAction::Allow => "allow",
        FirewallAction::Block => "block",
    };
    let remote_label = safe_display_token(remote_address);
    FirewallRule {
        display_name: format!(
            "AWatch containment {plan_id} {action_label}-{direction_label} {label} {remote_label}"
        ),
        direction,
        action,
        remote_address: remote_address.trim().to_string(),
        profile: profile.to_string(),
        description: description.to_string(),
    }
}

fn new_firewall_rule_command(group: &str, rule: &FirewallRule) -> String {
    format!(
        "New-NetFirewallRule -DisplayName {} -Group {} -Direction {} -Action {} -RemoteAddress {} -Profile {} -Enabled True -Description {}",
        ps_quote(&rule.display_name),
        ps_quote(group),
        match rule.direction {
            FirewallDirection::Inbound => "Inbound",
            FirewallDirection::Outbound => "Outbound",
        },
        match rule.action {
            FirewallAction::Allow => "Allow",
            FirewallAction::Block => "Block",
        },
        ps_quote(&rule.remote_address),
        ps_quote(&rule.profile),
        ps_quote(&rule.description),
    )
}

fn run_windows_firewall_executor(
    plan: &WindowsFirewallPlan,
    operation: &str,
    confirmation: &str,
    execute_local: bool,
) -> Result<ExecutorResult> {
    validate_windows_firewall_plan(plan)?;
    let commands = match operation {
        "apply" => &plan.apply_commands,
        "verify" => &plan.verify_commands,
        "rollback" => &plan.rollback_commands,
        _ => bail!("unsupported Windows Firewall executor operation: {operation}"),
    };
    let mut blockers = plan.blockers.clone();
    if operation == "apply" && confirmation != "YES" {
        blockers.push("confirm_apply_must_be_YES".to_string());
    }
    if operation == "rollback" && confirmation != "YES" {
        blockers.push("confirm_rollback_must_be_YES".to_string());
    }

    if !blockers.is_empty() {
        return Ok(executor_result(
            plan,
            operation,
            ExecutorRunData {
                execution_status: "refused".to_string(),
                would_mutate: false,
                execute_local_requested: execute_local,
                commands: commands.clone(),
                stdout: Vec::new(),
                stderr: Vec::new(),
                blockers,
            },
        ));
    }

    if !execute_local {
        return Ok(executor_result(
            plan,
            operation,
            ExecutorRunData {
                execution_status: "dry_run_commands_ready".to_string(),
                would_mutate: false,
                execute_local_requested: execute_local,
                commands: commands.clone(),
                stdout: Vec::new(),
                stderr: Vec::new(),
                blockers: Vec::new(),
            },
        ));
    }

    let executed = execute_powershell_commands(commands, operation != "verify")?;
    Ok(executor_result(
        plan,
        operation,
        ExecutorRunData {
            execution_status: executed.status,
            would_mutate: executed.would_mutate,
            execute_local_requested: execute_local,
            commands: commands.clone(),
            stdout: executed.stdout,
            stderr: executed.stderr,
            blockers: executed.blockers,
        },
    ))
}

struct LocalExecution {
    status: String,
    would_mutate: bool,
    stdout: Vec<String>,
    stderr: Vec<String>,
    blockers: Vec<String>,
}

struct ExecutorRunData {
    execution_status: String,
    would_mutate: bool,
    execute_local_requested: bool,
    commands: Vec<String>,
    stdout: Vec<String>,
    stderr: Vec<String>,
    blockers: Vec<String>,
}

#[cfg(windows)]
fn execute_powershell_commands(
    commands: &[String],
    mutation_expected: bool,
) -> Result<LocalExecution> {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    for command in commands {
        let output = std::process::Command::new("powershell.exe")
            .arg("-NoProfile")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-Command")
            .arg(command)
            .output()
            .with_context(|| format!("failed to execute PowerShell command: {command}"))?;
        stdout.push(String::from_utf8_lossy(&output.stdout).trim().to_string());
        stderr.push(String::from_utf8_lossy(&output.stderr).trim().to_string());
        if !output.status.success() {
            return Ok(LocalExecution {
                status: "failed".to_string(),
                would_mutate: mutation_expected,
                stdout,
                stderr,
                blockers: vec![format!("powershell_exit_status:{}", output.status)],
            });
        }
    }
    Ok(LocalExecution {
        status: "executed_local_windows".to_string(),
        would_mutate: mutation_expected,
        stdout,
        stderr,
        blockers: Vec::new(),
    })
}

#[cfg(not(windows))]
fn execute_powershell_commands(
    _commands: &[String],
    _mutation_expected: bool,
) -> Result<LocalExecution> {
    Ok(LocalExecution {
        status: "refused_non_windows_host".to_string(),
        would_mutate: false,
        stdout: Vec::new(),
        stderr: Vec::new(),
        blockers: vec!["execute_local_requires_windows_host".to_string()],
    })
}

fn executor_result(
    plan: &WindowsFirewallPlan,
    operation: &str,
    run: ExecutorRunData,
) -> ExecutorResult {
    ExecutorResult {
        executor: plan.executor.clone(),
        operation: operation.to_string(),
        generated_at_utc: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        plan_id: plan.plan_id.clone(),
        target_host: plan.target_host.clone(),
        ok: run.blockers.is_empty(),
        execution_status: run.execution_status,
        would_mutate: run.would_mutate,
        execute_local_requested: run.execute_local_requested,
        commands: run.commands,
        stdout: run.stdout,
        stderr: run.stderr,
        blockers: run.blockers,
    }
}

fn validate_windows_firewall_plan(plan: &WindowsFirewallPlan) -> Result<()> {
    if plan.executor != "windows_firewall" {
        bail!("plan executor must be windows_firewall");
    }
    if !is_safe_identifier(&plan.plan_id, 8, 64) {
        bail!("plan_id_invalid");
    }
    if !is_safe_identifier(&plan.target_host, 1, 128) {
        bail!("target_host_invalid");
    }
    if plan.rule_group.trim().is_empty()
        || plan.rule_group.contains('\r')
        || plan.rule_group.contains('\n')
    {
        bail!("rule_group_invalid");
    }
    for rule in plan.allow_rules.iter().chain(plan.block_rules.iter()) {
        if rule.display_name.contains('\r')
            || rule.display_name.contains('\n')
            || rule.display_name.len() > 180
        {
            bail!("rule_display_name_invalid");
        }
        if !is_safe_remote_address(&rule.remote_address) {
            bail!("rule_remote_address_invalid:{}", rule.remote_address);
        }
        if rule.action == FirewallAction::Block && is_broad_block(&rule.remote_address) {
            bail!("broad block refused in plan: {}", rule.remote_address);
        }
        if !is_allowed_profile(&rule.profile) {
            bail!("rule_profile_invalid:{}", rule.profile);
        }
    }
    Ok(())
}

fn is_safe_identifier(value: &str, min: usize, max: usize) -> bool {
    let trimmed = value.trim();
    (min..=max).contains(&trimmed.len())
        && trimmed
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn is_safe_remote_address(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && trimmed.len() <= 96
        && trimmed
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b':' | b'/' | b'-'))
}

fn is_allowed_profile(profile: &str) -> bool {
    matches!(
        profile.trim().to_ascii_lowercase().as_str(),
        "any" | "domain" | "private" | "public"
    )
}

fn is_broad_block(remote: &str) -> bool {
    matches!(
        remote.trim().to_ascii_lowercase().as_str(),
        "any" | "*" | "localsubnet" | "internet" | "intranet"
    )
}

fn same_token(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

fn remote_addresses_overlap(blocked: &str, allowed: &str) -> bool {
    if same_token(blocked, allowed) {
        return true;
    }
    match (ipv4_network(blocked), ipv4_network(allowed)) {
        (Some((blocked_addr, blocked_prefix)), Some((allowed_addr, allowed_prefix))) => {
            let prefix = blocked_prefix.min(allowed_prefix);
            ipv4_network_base(blocked_addr, prefix) == ipv4_network_base(allowed_addr, prefix)
        }
        _ => false,
    }
}

fn ipv4_network(value: &str) -> Option<(u32, u8)> {
    let trimmed = value.trim();
    let (addr, prefix) = if let Some((addr, prefix)) = trimmed.split_once('/') {
        let prefix = prefix.parse::<u8>().ok()?;
        if prefix > 32 {
            return None;
        }
        (addr, prefix)
    } else {
        (trimmed, 32)
    };
    let addr = addr.parse::<std::net::Ipv4Addr>().ok()?;
    Some((u32::from(addr), prefix))
}

fn ipv4_network_base(addr: u32, prefix: u8) -> u32 {
    if prefix == 0 {
        return 0;
    }
    let mask = u32::MAX << (32 - prefix);
    addr & mask
}

fn safe_display_token(value: &str) -> String {
    value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn sample_finding() -> Finding {
    Finding {
        host: "HOST-EXAMPLE".to_string(),
        host_role: HostRole::Workstation,
        state: "suspected_infected".to_string(),
        confidence: Confidence::High,
        signals: vec![
            Signal {
                source: "hayabusa".to_string(),
                rule_id: "sigma-placeholder-critical".to_string(),
                confidence: Confidence::Critical,
            },
            Signal {
                source: "velociraptor".to_string(),
                rule_id: "Windows.Hayabusa.Monitoring".to_string(),
                confidence: Confidence::High,
            },
        ],
        recommended_action: Some(ContainmentAction::WindowsFirewallQuarantine),
        management_channel_checked: true,
        manual_operator_flag: false,
    }
}

fn sample_windows_firewall_request() -> WindowsFirewallRequest {
    WindowsFirewallRequest {
        target_host: "HOST-EXAMPLE".to_string(),
        plan_id: "rollback-host-example-001".to_string(),
        ttl_minutes: 60,
        reason: "High-confidence Hayabusa and Velociraptor containment drill".to_string(),
        management_allowlist: vec![
            "10.10.10.10".to_string(),
            "10.10.10.11".to_string(),
            "10.10.10.12".to_string(),
        ],
        blocked_remote_addresses: vec!["10.10.20.0/24".to_string(), "10.10.30.0/24".to_string()],
        profiles: vec!["Domain".to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled_policy(mode: Mode) -> Policy {
        Policy {
            enabled: true,
            mode,
            ..Policy::default()
        }
    }

    #[test]
    fn disabled_policy_never_mutates() {
        let policy = Policy::default();
        let decision = decide(&policy, &sample_finding());
        assert_eq!(decision.decision_status, "disabled");
        assert!(!decision.would_mutate);
        assert!(decision.ok);
    }

    #[test]
    fn shadow_recommends_without_mutation() {
        let decision = decide(&enabled_policy(Mode::Shadow), &sample_finding());
        assert_eq!(decision.decision_status, "shadow_recommended");
        assert!(!decision.would_mutate);
        assert!(decision.ok);
    }

    #[test]
    fn auto_refuses_server_by_default() {
        let mut finding = sample_finding();
        finding.host_role = HostRole::Server;
        let decision = decide(&enabled_policy(Mode::Auto), &finding);
        assert_eq!(decision.decision_status, "auto_refused");
        assert!(
            decision
                .blockers
                .iter()
                .any(|blocker| blocker.starts_with("auto_refuses_host_role"))
        );
    }

    #[test]
    fn auto_requires_management_channel() {
        let mut finding = sample_finding();
        finding.management_channel_checked = false;
        let decision = decide(&enabled_policy(Mode::Auto), &finding);
        assert_eq!(decision.decision_status, "auto_refused");
        assert!(
            decision
                .blockers
                .contains(&"management_channel_not_checked".to_string())
        );
    }

    #[test]
    fn weak_signal_is_blocked() {
        let mut finding = sample_finding();
        finding.confidence = Confidence::Medium;
        finding.signals = vec![Signal {
            source: "hayabusa".to_string(),
            rule_id: "weak".to_string(),
            confidence: Confidence::Medium,
        }];
        let decision = decide(&enabled_policy(Mode::ManualApproval), &finding);
        assert_eq!(decision.decision_status, "manual_approval_blocked");
        assert!(
            decision
                .blockers
                .contains(&"signal_threshold_not_met".to_string())
        );
    }

    #[test]
    fn unknown_policy_field_is_rejected() {
        let json = r#"{
          "enabled": false,
          "mode": "shadow",
          "default_ttl_minutes": 60,
          "require_admin_channel_check": true,
          "allow_auto_for_servers": false,
          "allowed_actions": ["windows_firewall_quarantine"],
          "management_allowlist": ["aw_server"],
          "minimum_high_signals_for_auto": 2,
          "unexpected_auto_bypass": true
        }"#;
        assert!(serde_json::from_str::<Policy>(json).is_err());
    }

    #[test]
    fn windows_firewall_plan_generates_commands_and_rollback() {
        let request = sample_windows_firewall_request();
        let plan = build_windows_firewall_plan(&request);

        assert!(plan.blockers.is_empty());
        assert_eq!(plan.executor, "windows_firewall");
        assert!(!plan.allow_rules.is_empty());
        assert!(!plan.block_rules.is_empty());
        assert!(
            plan.apply_commands
                .iter()
                .any(|command| command.contains("New-NetFirewallRule"))
        );
        assert!(
            plan.rollback_commands
                .iter()
                .any(|command| command.contains("Remove-NetFirewallRule"))
        );
    }

    #[test]
    fn windows_firewall_plan_requires_management_allowlist() {
        let mut request = sample_windows_firewall_request();
        request.management_allowlist.clear();
        let plan = build_windows_firewall_plan(&request);

        assert!(
            plan.blockers
                .contains(&"management_allowlist_empty".to_string())
        );
    }

    #[test]
    fn windows_firewall_plan_refuses_broad_block() {
        let mut request = sample_windows_firewall_request();
        request.blocked_remote_addresses = vec!["Any".to_string()];
        let plan = build_windows_firewall_plan(&request);

        assert!(
            plan.blockers
                .iter()
                .any(|blocker| blocker == "broad_block_refused:Any")
        );
    }

    #[test]
    fn windows_firewall_plan_refuses_management_subnet_overlap() {
        let mut request = sample_windows_firewall_request();
        request.management_allowlist = vec!["10.10.10.10".to_string()];
        request.blocked_remote_addresses = vec!["10.10.10.0/24".to_string()];
        let plan = build_windows_firewall_plan(&request);

        assert!(
            plan.blockers
                .contains(&"management_allowlist_overlaps_blocked_remote_addresses".to_string())
        );
    }

    #[test]
    fn windows_firewall_apply_without_confirmation_is_refused() {
        let request = sample_windows_firewall_request();
        let plan = build_windows_firewall_plan(&request);
        let result = run_windows_firewall_executor(&plan, "apply", "NO", false).unwrap();

        assert!(!result.ok);
        assert!(!result.would_mutate);
        assert_eq!(result.execution_status, "refused");
        assert!(
            result
                .blockers
                .contains(&"confirm_apply_must_be_YES".to_string())
        );
    }

    #[test]
    fn windows_firewall_apply_confirmed_without_execute_is_dry_run() {
        let request = sample_windows_firewall_request();
        let plan = build_windows_firewall_plan(&request);
        let result = run_windows_firewall_executor(&plan, "apply", "YES", false).unwrap();

        assert!(result.ok);
        assert!(!result.would_mutate);
        assert_eq!(result.execution_status, "dry_run_commands_ready");
        assert!(!result.commands.is_empty());
    }
}
