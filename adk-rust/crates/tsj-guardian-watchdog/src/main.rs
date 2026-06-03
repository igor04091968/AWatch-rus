use std::fs;
use std::process::Command;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use detmir_core::{exit_codes, now_utc_rfc3339};
use serde::Serialize;

const DEFAULT_HEARTBEAT_FILE: &str = "/opt/infra-admin/.state/tsj_guardian_heartbeat";
const DEFAULT_SERVICE_NAME: &str = "tsj-guardian-bot.service";
const DEFAULT_GOST_SERVICE_NAME: &str = "gost-tg.service";
const DEFAULT_GOST_PATTERN: &str =
    "/usr/local/bin/gost -L http+socks5://127.0.0.1:11090 -F socks5+wss://gw.example.local:4443";

#[derive(Debug, Parser)]
#[command(about = "TSJ Guardian bot heartbeat watchdog and gost duplicate guard.")]
struct Cli {
    #[arg(long, default_value = DEFAULT_HEARTBEAT_FILE)]
    heartbeat_file: String,

    #[arg(long, default_value_t = 180)]
    max_age_seconds: i64,

    #[arg(long, default_value = DEFAULT_SERVICE_NAME)]
    service_name: String,

    #[arg(long, default_value = DEFAULT_GOST_SERVICE_NAME)]
    gost_service_name: String,

    #[arg(long, default_value = DEFAULT_GOST_PATTERN)]
    gost_dup_pattern: String,

    #[arg(long)]
    apply: bool,

    #[arg(long)]
    json: bool,

    #[arg(long)]
    loop_forever: bool,

    #[arg(long, default_value_t = 60)]
    interval_seconds: u64,
}

impl Cli {
    fn apply_env(mut self) -> Self {
        self.heartbeat_file = env_string("HEARTBEAT_FILE").unwrap_or(self.heartbeat_file);
        self.max_age_seconds = env_string("MAX_AGE_SEC")
            .and_then(|value| value.parse().ok())
            .unwrap_or(self.max_age_seconds);
        self.service_name = env_string("SERVICE_NAME").unwrap_or(self.service_name);
        self.gost_service_name = env_string("GOST_SERVICE_NAME").unwrap_or(self.gost_service_name);
        self.gost_dup_pattern = env_string("GOST_DUP_PATTERN").unwrap_or(self.gost_dup_pattern);
        self
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct WatchdogAction {
    action: String,
    target: String,
    status: String,
}

#[derive(Debug, Serialize)]
struct WatchdogReport {
    ok: bool,
    dry_run: bool,
    generated_at_utc: String,
    heartbeat_file: String,
    heartbeat_age_seconds: Option<i64>,
    heartbeat_status: String,
    gost_main_pid: Option<u32>,
    gost_pids: Vec<u32>,
    actions: Vec<WatchdogAction>,
}

fn env_string(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

fn main_pid(service_name: &str) -> Option<u32> {
    let output = Command::new("systemctl")
        .args(["show", "-p", "MainPID", "--value", service_name])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|pid| *pid > 1)
}

fn matching_pids(pattern: &str) -> Vec<u32> {
    let output = match Command::new("ps").args(["-eo", "pid=,args="]).output() {
        Ok(output) => output,
        Err(_) => return Vec::new(),
    };
    if !output.status.success() {
        return Vec::new();
    }
    let self_pid = std::process::id();
    let mut pids = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let (pid, args) = line.split_once(char::is_whitespace)?;
            let pid = pid.parse::<u32>().ok()?;
            if pid != self_pid && args.contains(pattern) {
                Some(pid)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    pids.sort_unstable();
    pids.dedup();
    pids
}

fn choose_keep_pid(main_pid: Option<u32>, pids: &[u32]) -> Option<u32> {
    if pids.is_empty() {
        return None;
    }
    match main_pid {
        Some(pid) if pids.contains(&pid) => Some(pid),
        _ => pids.first().copied(),
    }
}

fn signal_pid(pid: u32, signal: &str) -> Result<()> {
    let status = Command::new("kill")
        .args([format!("-{signal}"), pid.to_string()])
        .status()
        .with_context(|| format!("failed to signal pid {pid}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow::anyhow!("kill -{signal} {pid} exited with {status}"))
    }
}

fn pid_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn restart_service(service_name: &str) -> Result<()> {
    let status = Command::new("systemctl")
        .args(["restart", service_name])
        .status()
        .with_context(|| format!("failed to restart {service_name}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "systemctl restart {service_name} exited with {status}"
        ))
    }
}

fn heartbeat_status(path: &str, max_age_seconds: i64, now: i64) -> (String, Option<i64>) {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(_) => return ("missing".to_string(), None),
    };
    let heartbeat = match raw.trim().parse::<i64>() {
        Ok(value) => value,
        Err(_) => return ("invalid".to_string(), None),
    };
    let age = now.saturating_sub(heartbeat);
    if age > max_age_seconds {
        ("stale".to_string(), Some(age))
    } else {
        ("fresh".to_string(), Some(age))
    }
}

fn run_once(cli: &Cli) -> Result<WatchdogReport> {
    let mut actions = Vec::new();
    let gost_main_pid = main_pid(&cli.gost_service_name);
    let gost_pids = matching_pids(&cli.gost_dup_pattern);
    if let Some(keep_pid) = choose_keep_pid(gost_main_pid, &gost_pids) {
        for pid in gost_pids.iter().copied().filter(|pid| *pid != keep_pid) {
            if cli.apply {
                let status = signal_pid(pid, "TERM")
                    .map(|_| "terminated")
                    .unwrap_or("term-failed");
                actions.push(WatchdogAction {
                    action: "kill-term".to_string(),
                    target: pid.to_string(),
                    status: status.to_string(),
                });
            } else {
                actions.push(WatchdogAction {
                    action: "kill-term".to_string(),
                    target: pid.to_string(),
                    status: "planned".to_string(),
                });
            }
        }
        if cli.apply && gost_pids.len() > 1 {
            thread::sleep(Duration::from_secs(2));
            for pid in gost_pids.iter().copied().filter(|pid| *pid != keep_pid) {
                if pid_alive(pid) {
                    let status = signal_pid(pid, "KILL")
                        .map(|_| "killed")
                        .unwrap_or("kill-failed");
                    actions.push(WatchdogAction {
                        action: "kill-kill".to_string(),
                        target: pid.to_string(),
                        status: status.to_string(),
                    });
                }
            }
        }
    }

    let now = chrono_like_now_epoch();
    let (hb_status, hb_age) = heartbeat_status(&cli.heartbeat_file, cli.max_age_seconds, now);
    if hb_status != "fresh" {
        if cli.apply {
            let status = restart_service(&cli.service_name)
                .map(|_| "restarted")
                .unwrap_or("restart-failed");
            actions.push(WatchdogAction {
                action: "restart-service".to_string(),
                target: cli.service_name.clone(),
                status: status.to_string(),
            });
        } else {
            actions.push(WatchdogAction {
                action: "restart-service".to_string(),
                target: cli.service_name.clone(),
                status: "planned".to_string(),
            });
        }
    }

    Ok(WatchdogReport {
        ok: true,
        dry_run: !cli.apply,
        generated_at_utc: now_utc_rfc3339(),
        heartbeat_file: cli.heartbeat_file.clone(),
        heartbeat_age_seconds: hb_age,
        heartbeat_status: hb_status,
        gost_main_pid,
        gost_pids,
        actions,
    })
}

fn chrono_like_now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn print_report(report: &WatchdogReport, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(report)?);
    } else {
        println!(
            "tsj-guardian-watchdog: heartbeat={} age={:?} dry_run={}",
            report.heartbeat_status, report.heartbeat_age_seconds, report.dry_run
        );
        for action in &report.actions {
            println!("{} {} {}", action.action, action.target, action.status);
        }
    }
    Ok(())
}

fn run(cli: Cli) -> Result<()> {
    if cli.loop_forever {
        loop {
            let report = run_once(&cli)?;
            print_report(&report, cli.json)?;
            thread::sleep(Duration::from_secs(cli.interval_seconds));
        }
    }
    let report = run_once(&cli)?;
    print_report(&report, cli.json)?;
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse().apply_env();
    match run(cli) {
        Ok(()) => Ok(()),
        Err(err) => {
            eprintln!("{err:#}");
            std::process::exit(exit_codes::ERROR);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_main_pid_when_present() {
        assert_eq!(choose_keep_pid(Some(20), &[10, 20, 30]), Some(20));
    }

    #[test]
    fn keeps_first_pid_when_main_missing() {
        assert_eq!(choose_keep_pid(Some(99), &[10, 20, 30]), Some(10));
    }

    #[test]
    fn detects_heartbeat_states() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("heartbeat");
        assert_eq!(
            heartbeat_status(path.to_str().unwrap(), 180, 1_000),
            ("missing".to_string(), None)
        );
        fs::write(&path, "bad\n").unwrap();
        assert_eq!(
            heartbeat_status(path.to_str().unwrap(), 180, 1_000),
            ("invalid".to_string(), None)
        );
        fs::write(&path, "900\n").unwrap();
        assert_eq!(
            heartbeat_status(path.to_str().unwrap(), 180, 1_000),
            ("fresh".to_string(), Some(100))
        );
        fs::write(&path, "700\n").unwrap();
        assert_eq!(
            heartbeat_status(path.to_str().unwrap(), 180, 1_000),
            ("stale".to_string(), Some(300))
        );
    }
}
