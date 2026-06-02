use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use hayabusa_tools::{
    analyze_report, build_case_payload, build_comment, build_hayabusa_payload, build_telegram_text,
    env_bool, env_string, http_client, normalize_case_api_base, patch_json, post_json,
    read_json_file, required_str, severity_meets,
};
use reqwest::blocking::Client;
use serde_json::{Value, json};

#[derive(Debug, Parser)]
#[command(
    about = "Auto-create/update AW-rus case, compute Hayabusa severity, and send Telegram alerts"
)]
struct Cli {
    #[arg(long)]
    case_id: Option<i64>,

    #[arg(long, default_value = "/opt/hayabusa/state/latest-intake.json")]
    intake_json: PathBuf,

    #[arg(long)]
    case_api_base: Option<String>,

    #[arg(long, default_value = "incident")]
    mode: String,

    #[arg(long, default_value = "aw-rus-drop-autoprocess")]
    link_source: String,

    #[arg(long, default_value_t = false)]
    auto_create: bool,

    #[arg(long)]
    auto_create_min_severity: Option<String>,

    #[arg(long, default_value_t = false)]
    telegram_enabled: bool,

    #[arg(long)]
    telegram_min_severity: Option<String>,

    #[arg(long)]
    telegram_bot_token: Option<String>,

    #[arg(long)]
    telegram_chat_ids: Option<String>,
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
    let intake = read_json_file(&cli.intake_json)?;
    let summary = analyze_report(PathBuf::from(required_str(&intake, "report_dir")?).as_path())?;
    let case_api_base = normalize_case_api_base(cli.case_api_base.as_deref().unwrap_or(
        &env_string("AW_HAYABUSA_CASE_API_BASE", "http://127.0.0.1:5602"),
    ));
    let auto_create = cli.auto_create || env_bool("AW_HAYABUSA_AUTO_CASE_ENABLED", true);
    let auto_create_min_severity = cli
        .auto_create_min_severity
        .unwrap_or_else(|| env_string("AW_HAYABUSA_AUTO_CASE_MIN_SEVERITY", "medium"));
    let telegram_enabled = cli.telegram_enabled || env_bool("AW_HAYABUSA_TELEGRAM_ENABLED", false);
    let telegram_min_severity = cli
        .telegram_min_severity
        .unwrap_or_else(|| env_string("AW_HAYABUSA_TELEGRAM_MIN_SEVERITY", "high"));
    let telegram_bot_token = cli
        .telegram_bot_token
        .unwrap_or_else(|| env_string("AW_HAYABUSA_TELEGRAM_BOT_TOKEN", ""));
    let telegram_chat_ids = cli
        .telegram_chat_ids
        .unwrap_or_else(|| env_string("AW_HAYABUSA_TELEGRAM_CHAT_IDS", ""));
    let client = http_client()?;

    let mut case_id = cli.case_id;
    let mut created_case = Value::Null;
    let mut case_error: Option<String> = None;
    let mut linked = false;
    let mut comment_added = false;

    if let Err(err) = (|| -> Result<()> {
        if case_id.is_none()
            && auto_create
            && severity_meets(&summary.severity, &auto_create_min_severity)
        {
            created_case = post_json(
                &client,
                &format!("{case_api_base}/api/0/dlp/cases"),
                &build_case_payload(&intake, &summary)?,
            )?;
            case_id = created_case.get("id").and_then(Value::as_i64);
        }
        if let Some(id) = case_id {
            patch_json(
                &client,
                &format!("{case_api_base}/api/0/dlp/cases/{id}"),
                &json!({"severity": summary.severity}),
            )?;
            post_json(
                &client,
                &format!("{case_api_base}/api/0/dlp/cases/{id}/forensics/hayabusa"),
                &build_hayabusa_payload(&intake, &cli.mode, &cli.link_source)?,
            )?;
            linked = true;
            post_json(
                &client,
                &format!("{case_api_base}/api/0/dlp/cases/{id}/comments"),
                &json!({"comment": build_comment(&summary, &intake)?, "author": "aw-hayabusa-auto"}),
            )?;
            comment_added = true;
        }
        Ok(())
    })() {
        case_error = Some(err.to_string());
    }

    let telegram_results = if telegram_enabled
        && !telegram_bot_token.is_empty()
        && severity_meets(&summary.severity, &telegram_min_severity)
    {
        let chat_ids = telegram_chat_ids
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>();
        send_telegram(
            &client,
            &telegram_bot_token,
            &chat_ids,
            &build_telegram_text(case_id, &intake, &summary)?,
        )
    } else {
        Vec::new()
    };

    let result = json!({
        "summary": summary,
        "case_id": case_id,
        "case_created": if created_case.is_null() { Value::Null } else { created_case },
        "case_linked": linked,
        "case_comment_added": comment_added,
        "case_error": case_error,
        "telegram_results": telegram_results,
    });
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(if result.get("case_error").is_some_and(|v| !v.is_null()) {
        1
    } else {
        0
    })
}

fn send_telegram(client: &Client, bot_token: &str, chat_ids: &[&str], text: &str) -> Vec<Value> {
    let mut results = Vec::new();
    for chat_id in chat_ids {
        let url = format!("https://api.telegram.org/bot{bot_token}/sendMessage");
        let response = client
            .post(&url)
            .form(&[("chat_id", *chat_id), ("text", text)])
            .send();
        match response {
            Ok(resp) => match resp.json::<Value>() {
                Ok(body) => results.push(json!({"chat_id": chat_id, "ok": true, "response": body})),
                Err(err) => {
                    results.push(json!({"chat_id": chat_id, "ok": false, "error": err.to_string()}))
                }
            },
            Err(_) => results
                .push(json!({"chat_id": chat_id, "ok": false, "error": "telegram request failed"})),
        }
    }
    results
}
