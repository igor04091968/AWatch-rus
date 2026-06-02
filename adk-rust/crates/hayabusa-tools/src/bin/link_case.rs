use anyhow::Result;
use clap::Parser;
use hayabusa_tools::{build_hayabusa_payload, http_client, link_hayabusa_to_case, read_json_file};
use serde_json::json;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(about = "Link Hayabusa intake metadata to AW-rus case management")]
struct Cli {
    #[arg(long)]
    case_id: i64,

    #[arg(long, default_value = "/opt/hayabusa/state/latest-intake.json")]
    intake_json: PathBuf,

    #[arg(long, default_value = "http://127.0.0.1:5602")]
    case_api_base: String,

    #[arg(long, default_value = "incident")]
    mode: String,

    #[arg(long, default_value = "aw-rus-ops")]
    link_source: String,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let intake = read_json_file(&cli.intake_json)?;
    let client = http_client()?;
    let case = link_hayabusa_to_case(
        &client,
        &cli.case_api_base,
        cli.case_id,
        &intake,
        &cli.mode,
        &cli.link_source,
    )?;
    let _payload = build_hayabusa_payload(&intake, &cli.mode, &cli.link_source)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "case_id": cli.case_id,
            "intake": intake,
            "forensics": case.get("forensics").cloned().unwrap_or(serde_json::Value::Null),
        }))?
    );
    Ok(())
}
