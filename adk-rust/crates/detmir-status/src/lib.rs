use std::path::PathBuf;

use adk_rust::Content;
use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use detmir_state::{DEFAULT_STATE_FILE, NormalizedStatus, read_state};
use serde_json::json;

#[derive(Debug, Parser)]
#[command(about = "Read-only DetMir status probe with JSON and ADK output.")]
struct Cli {
    #[command(flatten)]
    status: StatusArgs,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Status(StatusArgs),
}

#[derive(Debug, Args)]
struct StatusArgs {
    #[arg(long, default_value = DEFAULT_STATE_FILE)]
    state: PathBuf,

    #[arg(long)]
    json: bool,

    #[arg(long)]
    adk_json: bool,
}

fn render_text(status: &NormalizedStatus) -> String {
    let s = &status.detmir_summary;
    let d = &status.dlp_counts;
    format!(
        "DetMir status: {severity}\n\
         check_ok={check_ok} dlp_ok={dlp_ok} needs_heal={needs_heal}\n\
         buckets: ok={bucket_ok} stale={bucket_stale} dead={bucket_dead}\n\
         services: fail={service_failures} warn={service_warnings}\n\
         dlp: ok={dlp_ok_count} warn={dlp_warn} fail={dlp_fail}\n\
         operator_ok={operator_ok}",
        severity = status.severity,
        check_ok = status.check_ok,
        dlp_ok = status.dlp_ok,
        needs_heal = status.needs_heal,
        bucket_ok = s.bucket_ok.unwrap_or(0),
        bucket_stale = s.bucket_stale.unwrap_or(0),
        bucket_dead = s.bucket_dead.unwrap_or(0),
        service_failures = s.service_failures.unwrap_or(0),
        service_warnings = s.service_warnings.unwrap_or(0),
        dlp_ok_count = d.ok.unwrap_or(0),
        dlp_warn = d.warn.unwrap_or(0),
        dlp_fail = d.fail.unwrap_or(0),
        operator_ok = status.ok_for_operator,
    )
}

fn render_adk_content_json(status: &NormalizedStatus) -> Result<String> {
    let text = render_text(status);
    let content = Content::new("user").with_text(text);
    let envelope = json!({
        "agent": "detmir-status-agent",
        "mode": "read-only",
        "adk_content": content,
        "normalized": status,
    });
    Ok(serde_json::to_string_pretty(&envelope)?)
}

fn run_status(args: StatusArgs) -> Result<i32> {
    let status = read_state(&args.state)?;
    if args.adk_json {
        println!("{}", render_adk_content_json(&status)?);
    } else if args.json {
        println!("{}", serde_json::to_string_pretty(&status)?);
    } else {
        println!("{}", render_text(&status));
    }
    Ok(status.exit_code())
}

pub fn main_entry() -> Result<()> {
    let cli = Cli::parse();
    let code = match cli.command {
        Some(Command::Status(args)) => run_status(args)?,
        None => run_status(cli.status)?,
    };
    std::process::exit(code);
}
