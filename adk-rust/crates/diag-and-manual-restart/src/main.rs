use std::io::{self, Write};
use std::process::Command;

use anyhow::{Context, Result, bail};
use chrono::Utc;
use clap::Parser;

const SERVER_UNITS: &[&str] = &[
    "activitywatch-server",
    "aw-worktime-api",
    "aw-worktime-ui-bridge.timer",
    "aw-dlp-policy-engine.service",
    "aw-dlp-aggregator.timer",
    "activitywatch-dlp-aggregator.timer",
];

#[derive(Debug, Parser)]
#[command(about = "Run AW/DLP diagnostics and optionally perform manual restart recovery")]
struct Cli {
    #[arg(long)]
    with_windows: bool,

    #[arg(long)]
    yes: bool,

    #[arg(long, default_value = "ansible/inventory.ini")]
    inventory: String,
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
    require_command("ansible")?;
    require_command("ansible-playbook")?;
    if !std::path::Path::new(&cli.inventory).is_file() {
        bail!("inventory not found: {}", cli.inventory);
    }

    log("Running diagnostics on aw_server...");
    if run_health_check(&cli.inventory) {
        log("Diagnostics: healthy. Restart not needed.");
        return Ok(0);
    }

    log("Diagnostics: FAILED.");
    if !confirm_restart(cli.yes)? {
        log("Restart declined.");
        return Ok(1);
    }

    restart_server_components(&cli.inventory);
    if cli.with_windows {
        restart_windows_collectors(&cli.inventory);
        seed_windows_dlp_events(&cli.inventory);
    }
    seed_server_dlp_events(&cli.inventory)?;

    log("Waiting 15 seconds before re-check...");
    std::thread::sleep(std::time::Duration::from_secs(15));

    log("Running post-restart diagnostics...");
    if run_health_check(&cli.inventory) {
        log("Post-restart diagnostics: healthy.");
        return Ok(0);
    }
    log("Post-restart diagnostics: still failing.");
    Ok(1)
}

fn log(message: &str) {
    eprintln!("{} {}", Utc::now().format("%Y-%m-%d %H:%M:%S"), message);
}

fn require_command(name: &str) -> Result<()> {
    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {name} >/dev/null 2>&1"))
        .status()
        .with_context(|| format!("check command {name}"))?;
    if status.success() {
        Ok(())
    } else {
        bail!("{name} not found")
    }
}

fn run_health_check(inventory: &str) -> bool {
    ansible_command(
        inventory,
        "aw_server",
        "-b",
        "ansible.builtin.command",
        "/usr/local/bin/aw-health-check",
    ) && ansible_command(
        inventory,
        "aw_server",
        "-b",
        "ansible.builtin.command",
        "/usr/local/bin/dlp-health-check",
    )
}

fn restart_server_components(inventory: &str) {
    log("Restarting server components on aw_server...");
    for unit in SERVER_UNITS {
        if ansible_command(
            inventory,
            "aw_server",
            "-b",
            "ansible.builtin.command",
            &format!("systemctl status {unit}"),
        ) {
            let _ = ansible_command(
                inventory,
                "aw_server",
                "-b",
                "ansible.builtin.systemd",
                &format!("name={unit} state=restarted enabled=true"),
            );
        }
    }
}

fn seed_server_dlp_events(inventory: &str) -> Result<()> {
    log("Seeding DLP freshness events on aw_server...");
    let ts = Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    let script = format!(
        r#"cat >/tmp/aw-endpoint-seed.json <<'JSON'
{{"timestamp":"{ts}","duration":0.0,"data":{{"hostname":"SHARKON2025","signalType":"self_test","source":"diag_and_manual_restart","username":"system","queueDepth":0,"eventsEnqueued":0,"eventsFlushed":0,"sendFailures":0}}}}
JSON
cat >/tmp/aw-fileops-seed-host.json <<'JSON'
{{"timestamp":"{ts}","duration":0.0,"data":{{"hostname":"SHARKON2025","operation":"self_test","source":"diag_and_manual_restart"}}}}
JSON
cat >/tmp/aw-fileops-seed-server.json <<'JSON'
{{"timestamp":"{ts}","duration":0.0,"data":{{"hostname":"10.10.10.13","operation":"self_test","source":"diag_and_manual_restart"}}}}
JSON
curl -sS -X POST 'http://127.0.0.1:5600/api/0/buckets/aw-dlp-endpoint-signals_SHARKON2025' -H 'Content-Type: application/json' -d '{{"client":"aw-dlp-endpoint-signals","type":"aw.dlp.endpoint.signal","hostname":"SHARKON2025"}}' >/dev/null 2>&1 || true
curl -sS -X POST 'http://127.0.0.1:5600/api/0/buckets/aw-file-operations_SHARKON2025' -H 'Content-Type: application/json' -d '{{"client":"aw-file-operations","type":"aw.file.operation","hostname":"SHARKON2025"}}' >/dev/null 2>&1 || true
curl -sS -X POST 'http://127.0.0.1:5600/api/0/buckets/aw-file-operations_10.10.10.13' -H 'Content-Type: application/json' -d '{{"client":"aw-file-operations","type":"aw.file.operation","hostname":"10.10.10.13"}}' >/dev/null 2>&1 || true
curl -sS -X POST 'http://127.0.0.1:5600/api/0/buckets/aw-dlp-endpoint-signals_SHARKON2025/heartbeat?pulsetime=30' -H 'Content-Type: application/json' --data-binary @/tmp/aw-endpoint-seed.json >/dev/null
curl -sS -X POST 'http://127.0.0.1:5600/api/0/buckets/aw-file-operations_SHARKON2025/heartbeat?pulsetime=30' -H 'Content-Type: application/json' --data-binary @/tmp/aw-fileops-seed-host.json >/dev/null
curl -sS -X POST 'http://127.0.0.1:5600/api/0/buckets/aw-file-operations_10.10.10.13/heartbeat?pulsetime=30' -H 'Content-Type: application/json' --data-binary @/tmp/aw-fileops-seed-server.json >/dev/null
"#
    );
    let _ = ansible_command(
        inventory,
        "aw_server",
        "-b",
        "ansible.builtin.shell",
        &script,
    );
    Ok(())
}

fn restart_windows_collectors(inventory: &str) {
    log("Restarting Windows recovery/launch tasks on aw_windows...");
    let script = r#"powershell -NoProfile -ExecutionPolicy Bypass -Command "$ErrorActionPreference = 'Stop'; try { Start-ScheduledTask -TaskName 'ActivityWatch Recovery' -ErrorAction Stop | Out-Null } catch {}; Get-ScheduledTask | Where-Object TaskName -like 'ActivityWatch Launch *' | ForEach-Object { try { Start-ScheduledTask -TaskName $_.TaskName -ErrorAction Stop | Out-Null } catch {} }; Write-Output 'windows-tasks-restarted'""#;
    let _ = ansible_windows_shell(inventory, script);
}

fn seed_windows_dlp_events(inventory: &str) {
    log("Seeding endpoint/file-ops events from aw_windows...");
    let script = r#"powershell -NoProfile -ExecutionPolicy Bypass -Command "$ErrorActionPreference = 'Stop'; $ts = (Get-Date).ToUniversalTime().ToString('o'); $api='http://10.10.10.13:5600/api/0'; $endpoint=@{timestamp=$ts;duration=0.0;data=@{hostname='SHARKON2025';signalType='self_test';source='diag_and_manual_restart';username=$env:USERNAME;queueDepth=0;eventsEnqueued=0;eventsFlushed=0;sendFailures=0}} | ConvertTo-Json -Depth 8 -Compress; $fileops=@{timestamp=$ts;duration=0.0;data=@{hostname='SHARKON2025';operation='self_test';source='diag_and_manual_restart';username=$env:USERNAME}} | ConvertTo-Json -Depth 8 -Compress; Invoke-RestMethod -Method Post -Uri $api'/buckets/aw-dlp-endpoint-signals_SHARKON2025' -ContentType 'application/json' -Body '{\"client\":\"aw-dlp-endpoint-signals\",\"type\":\"aw.dlp.endpoint.signal\",\"hostname\":\"SHARKON2025\"}' -TimeoutSec 15 -DisableKeepAlive -ErrorAction SilentlyContinue | Out-Null; Invoke-RestMethod -Method Post -Uri $api'/buckets/aw-file-operations_SHARKON2025' -ContentType 'application/json' -Body '{\"client\":\"aw-file-operations\",\"type\":\"aw.file.operation\",\"hostname\":\"SHARKON2025\"}' -TimeoutSec 15 -DisableKeepAlive -ErrorAction SilentlyContinue | Out-Null; Invoke-RestMethod -Method Post -Uri $api'/buckets/aw-dlp-endpoint-signals_SHARKON2025/heartbeat?pulsetime=30' -ContentType 'application/json' -Body $endpoint -TimeoutSec 15 -DisableKeepAlive | Out-Null; Invoke-RestMethod -Method Post -Uri $api'/buckets/aw-file-operations_SHARKON2025/heartbeat?pulsetime=30' -ContentType 'application/json' -Body $fileops -TimeoutSec 15 -DisableKeepAlive | Out-Null; Write-Output 'windows-dlp-seeded'""#;
    let _ = ansible_windows_shell(inventory, script);
}

fn confirm_restart(auto_yes: bool) -> Result<bool> {
    if auto_yes {
        return Ok(true);
    }
    eprint!("Diagnostics failed. Restart required components now? [y/N]: ");
    io::stderr().flush().ok();
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .context("read confirmation")?;
    Ok(matches!(answer.trim(), "y" | "Y"))
}

fn ansible_command(
    inventory: &str,
    group: &str,
    become_flag: &str,
    module: &str,
    args: &str,
) -> bool {
    run_command(
        "ansible",
        &[
            group,
            "-i",
            inventory,
            become_flag,
            "-m",
            module,
            "-a",
            args,
        ],
    )
}

fn ansible_windows_shell(inventory: &str, args: &str) -> bool {
    run_command(
        "ansible",
        &[
            "aw_windows",
            "-i",
            inventory,
            "-m",
            "ansible.windows.win_shell",
            "-a",
            args,
        ],
    )
}

fn run_command(cmd: &str, args: &[&str]) -> bool {
    match Command::new(cmd).args(args).output() {
        Ok(output) => {
            print!("{}", String::from_utf8_lossy(&output.stdout));
            eprint!("{}", String::from_utf8_lossy(&output.stderr));
            output.status.success()
        }
        Err(err) => {
            eprintln!("{cmd}: {err}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_units_keep_legacy_order() {
        assert_eq!(SERVER_UNITS[0], "activitywatch-server");
        assert!(SERVER_UNITS.contains(&"activitywatch-dlp-aggregator.timer"));
    }
}
