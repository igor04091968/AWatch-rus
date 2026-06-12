use std::net::{TcpStream, ToSocketAddrs};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use reqwest::blocking::Client;
use reqwest::redirect::Policy;
use serde_json::Value;

#[derive(Debug, Parser)]
#[command(about = "ActivityWatch-Russian contour smoke checks")]
struct Cli {
    #[arg(long, value_enum, default_value_t = Mode::ProxmoxRemote)]
    mode: Mode,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Mode {
    ProxmoxRemote,
}

#[derive(Debug, Default)]
struct Counts {
    ok: usize,
    warn: usize,
    fail: usize,
    skip: usize,
}

impl Counts {
    fn pass(&mut self, msg: impl AsRef<str>) {
        self.ok += 1;
        println!("[OK]   {}", msg.as_ref());
    }

    fn fail(&mut self, msg: impl AsRef<str>) {
        self.fail += 1;
        println!("[FAIL] {}", msg.as_ref());
    }

    fn skip(&mut self, msg: impl AsRef<str>) {
        self.skip += 1;
        println!("[SKIP] {}", msg.as_ref());
    }
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
    match cli.mode {
        Mode::ProxmoxRemote => run_proxmox_remote(),
    }
}

fn run_proxmox_remote() -> Result<i32> {
    let mut counts = Counts::default();
    let http = Client::builder()
        .no_proxy()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(15))
        .build()
        .context("build HTTP client")?;
    let no_redirect_http = Client::builder()
        .no_proxy()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(15))
        .redirect(Policy::none())
        .build()
        .context("build no-redirect HTTP client")?;

    section("Host");
    print_command("hostnamectl", &["hostnamectl"]);
    print_command("date", &["date", "-Is"]);
    print_command("uptime", &["uptime"]);

    section("Core Services");
    for unit in [
        "nginx.service",
        "pveproxy.service",
        "pvedaemon.service",
        "pvestatd.service",
        "pve-cluster.service",
        "docker.service",
        "aw-1c-company-api.service",
        "aw-pve-webadmin-logger.service",
    ] {
        check_service(&mut counts, unit);
    }

    section("Timers");
    for unit in [
        "aw-1c-ingest.timer",
        "aw-1c-proofcheck.timer",
        "aw-1c-manager-brief.timer",
        "aw-1c-recovery-brief.timer",
        "aw-1c-weekly-digest.timer",
    ] {
        check_timer(&mut counts, unit);
    }
    if let Ok(out) = command_output("systemctl", &["list-timers", "--all", "--no-pager"]) {
        print_filtered_lines(&out, &["aw-1c", "NEXT", "LEFT", "PASSED"], 40);
    }

    section("Ports");
    check_tcp(&mut counts, "nginx http", "127.0.0.1", 80);
    check_tcp(&mut counts, "nginx https", "127.0.0.1", 443);
    check_tcp(&mut counts, "proxmox web", "127.0.0.1", 8006);
    check_tcp(&mut counts, "1C company API", "10.10.10.2", 8710);
    check_tcp(&mut counts, "clickhouse native", "127.0.0.1", 9000);
    check_tcp(&mut counts, "clickhouse http", "127.0.0.1", 8123);
    if let Ok(out) = command_output("ss", &["-tulpn"]) {
        print_filtered_lines(
            &out,
            &[":80", ":443", ":8006", ":8710", ":8123", ":9000"],
            40,
        );
    }

    section("Gateway HTTP");
    check_http_code(
        &mut counts,
        &http,
        "nginx healthz",
        "https://127.0.0.1/healthz",
        &[200],
    );
    check_http_code(
        &mut counts,
        &http,
        "go proxmox gui protected",
        "https://127.0.0.1/go/proxmox-gui",
        &[401],
    );
    check_http_code(
        &mut counts,
        &http,
        "go file1c brief protected",
        "https://127.0.0.1/go/file1c-brief",
        &[401],
    );
    check_http_code(
        &mut counts,
        &http,
        "go file1c actions protected",
        "https://127.0.0.1/go/file1c-actions",
        &[401],
    );

    section("1C Company API");
    check_http_code(
        &mut counts,
        &no_redirect_http,
        "1C root redirect",
        "http://10.10.10.2:8710/",
        &[307],
    );
    for (name, url) in [
        ("1C /health", "http://10.10.10.2:8710/health"),
        ("1C /api/health", "http://10.10.10.2:8710/api/health"),
        ("1C manager brief", "http://10.10.10.2:8710/manager/brief"),
        (
            "1C manager actions",
            "http://10.10.10.2:8710/manager/actions",
        ),
        (
            "1C manager recovery",
            "http://10.10.10.2:8710/manager/recovery",
        ),
        (
            "1C weekly digest",
            "http://10.10.10.2:8710/manager/digest/weekly",
        ),
    ] {
        check_http_code(&mut counts, &http, name, url, &[200]);
    }

    section("ClickHouse");
    check_docker_container(&mut counts, "aw-rus-1c-clickhouse");
    check_http_code(
        &mut counts,
        &http,
        "ClickHouse ping",
        "http://127.0.0.1:8123/ping",
        &[200],
    );
    if command_exists("docker") && docker_container_running("aw-rus-1c-clickhouse") {
        check_command(
            &mut counts,
            "ClickHouse SELECT 1",
            "docker",
            &[
                "exec",
                "aw-rus-1c-clickhouse",
                "clickhouse-client",
                "--query",
                "SELECT 1",
            ],
        );
    }

    section("System Capacity");
    print_command("df", &["df", "-h", "/", "/var", "/opt"]);
    print_command("free", &["free", "-h"]);

    section("Summary");
    println!(
        "OK={} WARN={} FAIL={} SKIP={}",
        counts.ok, counts.warn, counts.fail, counts.skip
    );
    Ok(if counts.fail > 0 { 2 } else { 0 })
}

fn section(name: &str) {
    println!();
    println!("== {name} ==");
}

fn command_exists(cmd: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {cmd} >/dev/null 2>&1"))
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn command_output(cmd: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .with_context(|| format!("run {cmd}"))?;
    let mut text = String::new();
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    if output.status.success() {
        Ok(text)
    } else {
        Err(anyhow::anyhow!("{cmd} failed: {text}"))
    }
}

fn print_command(label: &str, command: &[&str]) {
    if let Some((cmd, args)) = command.split_first() {
        match command_output(cmd, args) {
            Ok(out) => print_indented(&out, 80),
            Err(err) => println!("       {label}: {err:#}"),
        }
    }
}

fn print_indented(text: &str, max_lines: usize) {
    for line in text.lines().take(max_lines) {
        println!("       {line}");
    }
}

fn print_filtered_lines(text: &str, patterns: &[&str], max_lines: usize) {
    for line in text
        .lines()
        .filter(|line| patterns.iter().any(|pattern| line.contains(pattern)))
        .take(max_lines)
    {
        println!("       {line}");
    }
}

fn check_service(counts: &mut Counts, unit: &str) {
    check_systemd_unit(counts, unit, "service");
}

fn check_timer(counts: &mut Counts, unit: &str) {
    check_systemd_unit(counts, unit, "timer");
}

fn check_systemd_unit(counts: &mut Counts, unit: &str, kind: &str) {
    if !Command::new("systemctl")
        .args(["list-unit-files", unit])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
    {
        counts.skip(format!("{unit} is not installed"));
        return;
    }
    if Command::new("systemctl")
        .args(["is-active", "--quiet", unit])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
    {
        counts.pass(format!("{unit} active"));
    } else {
        counts.fail(format!("{unit} inactive or failed"));
        if let Ok(out) = command_output("systemctl", &["--no-pager", "--lines=8", "status", unit]) {
            print_indented(&out, 30);
        } else {
            let _ = kind;
        }
    }
}

fn check_tcp(counts: &mut Counts, name: &str, host: &str, port: u16) {
    let addr = format!("{host}:{port}");
    let ok = addr
        .to_socket_addrs()
        .ok()
        .and_then(|mut addrs| addrs.next())
        .and_then(|addr| TcpStream::connect_timeout(&addr, Duration::from_secs(4)).ok())
        .is_some();
    if ok {
        counts.pass(format!("{name} TCP {host}:{port}"));
    } else {
        counts.fail(format!("{name} TCP {host}:{port}"));
    }
}

fn check_http_code(counts: &mut Counts, client: &Client, name: &str, url: &str, expected: &[u16]) {
    match client.get(url).send() {
        Ok(response) => {
            let code = response.status().as_u16();
            if expected.contains(&code) {
                counts.pass(format!("{name} HTTP {code} {url}"));
            } else {
                counts.fail(format!("{name} HTTP {code} {url}"));
                if let Ok(text) = response.text() {
                    print_indented(&text, 40);
                }
            }
        }
        Err(err) => counts.fail(format!("{name} HTTP error {url}: {err}")),
    }
}

fn check_command(counts: &mut Counts, name: &str, cmd: &str, args: &[&str]) {
    match command_output(cmd, args) {
        Ok(out) => {
            counts.pass(name);
            print_indented(&out, 40);
        }
        Err(err) => counts.fail(format!("{name}: {err:#}")),
    }
}

fn check_docker_container(counts: &mut Counts, name: &str) {
    if !command_exists("docker") {
        counts.skip("docker command unavailable");
        return;
    }
    if docker_container_running(name) {
        counts.pass(format!("docker container {name} running"));
        if let Ok(out) = command_output(
            "docker",
            &[
                "ps",
                "--filter",
                &format!("name=^/{name}$"),
                "--format",
                "{{.Names}} {{.Status}} {{.Ports}}",
            ],
        ) {
            print_indented(&out, 20);
        }
    } else {
        counts.fail(format!("docker container {name} not running"));
        if let Ok(out) = command_output(
            "docker",
            &[
                "ps",
                "-a",
                "--filter",
                &format!("name=^/{name}$"),
                "--format",
                "{{.Names}} {{.Status}} {{.Ports}}",
            ],
        ) {
            print_indented(&out, 20);
        }
    }
}

fn docker_container_running(name: &str) -> bool {
    command_output("docker", &["ps", "--format", "{{.Names}}"])
        .map(|out| out.lines().any(|line| line == name))
        .unwrap_or(false)
}

#[allow(dead_code)]
fn parse_json_key_present(value: &Value, key: &str) -> bool {
    value.get(key).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_exit_code_matches_failures() {
        let counts = Counts {
            ok: 1,
            warn: 1,
            fail: 0,
            skip: 1,
        };
        assert_eq!(if counts.fail > 0 { 2 } else { 0 }, 0);
        let counts = Counts { fail: 1, ..counts };
        assert_eq!(if counts.fail > 0 { 2 } else { 0 }, 2);
    }

    #[test]
    fn default_counts_are_zero() {
        let counts = Counts::default();
        assert_eq!(counts.ok + counts.warn + counts.fail + counts.skip, 0);
    }
}
