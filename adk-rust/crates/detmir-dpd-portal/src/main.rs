use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use reqwest::blocking::Client;
use reqwest::header::{CONNECTION, HeaderValue};
use serde_json::json;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

const INDEX_HTML: &str = include_str!("static/index.html");
const APP_CSS: &str = include_str!("static/app.css");
const APP_JS: &str = include_str!("static/app.js");

#[derive(Clone, Debug, Parser)]
#[command(about = "DetMir DPD/Dioxus pilot portal")]
struct Cli {
    #[arg(long, default_value = "127.0.0.1:8722", env = "DETMIR_DPD_BIND")]
    bind: String,

    #[arg(
        long,
        default_value = "http://127.0.0.1:8720/portal/api",
        env = "DETMIR_DPD_UPSTREAM_API"
    )]
    upstream_api: String,

    #[arg(long, default_value_t = 60, env = "DETMIR_DPD_TIMEOUT_SECONDS")]
    timeout_seconds: u64,
}

fn main() {
    let code = match run() {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("{err:#}");
            1
        }
    };
    std::process::exit(code);
}

fn run() -> Result<()> {
    let args = Cli::parse();
    let server = Server::http(&args.bind).map_err(|err| anyhow!("bind {}: {err}", args.bind))?;
    eprintln!("detmir-dpd-portal listening on http://{}", args.bind);
    for request in server.incoming_requests() {
        let args = args.clone();
        std::thread::spawn(move || {
            if let Err(err) = handle_request(request, &args) {
                eprintln!("detmir-dpd-portal request failed: {err:#}");
            }
        });
    }
    Ok(())
}

fn handle_request(request: Request, args: &Cli) -> Result<()> {
    let method = request.method().clone();
    let path = normalize_path(request.url());
    if method != Method::Get {
        return respond_text(request, StatusCode(405), "Method Not Allowed", "text/plain");
    }
    match path.as_str() {
        "/" | "/dpd" | "/dpd/" => respond_text(
            request,
            StatusCode(200),
            INDEX_HTML,
            "text/html; charset=utf-8",
        ),
        "/app.css" | "/dpd/app.css" => {
            respond_text(request, StatusCode(200), APP_CSS, "text/css; charset=utf-8")
        }
        "/app.js" | "/dpd/app.js" => respond_text(
            request,
            StatusCode(200),
            APP_JS,
            "application/javascript; charset=utf-8",
        ),
        "/api/reports" | "/dpd/api/reports" => {
            respond_json_text(request, StatusCode(200), &upstream_get(args, "reports")?)
        }
        "/api/health" | "/dpd/api/health" => respond_json_text(
            request,
            StatusCode(200),
            &json!({
                "ok": true,
                "portal": "detmir-dpd-portal",
                "mode": "parallel-preview",
                "upstream_api": sanitize_upstream(&args.upstream_api),
            })
            .to_string(),
        ),
        _ => respond_text(request, StatusCode(404), "Not Found", "text/plain"),
    }
}

fn upstream_get(args: &Cli, endpoint: &str) -> Result<String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(args.timeout_seconds))
        .no_proxy()
        .build()
        .context("upstream HTTP client")?;
    let url = format!(
        "{}/{}",
        args.upstream_api.trim_end_matches('/'),
        endpoint.trim_start_matches('/')
    );
    client
        .get(&url)
        .header(CONNECTION, HeaderValue::from_static("close"))
        .send()
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("upstream status for {url}"))?
        .text()
        .context("upstream response body")
}

fn normalize_path(url: &str) -> String {
    let raw = url.split('?').next().unwrap_or(url);
    let path = raw.trim();
    if let Some(stripped) = path.strip_prefix("/portal-dpd") {
        if stripped.is_empty() {
            "/".to_string()
        } else {
            stripped.to_string()
        }
    } else {
        path.to_string()
    }
}

fn sanitize_upstream(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.starts_with("http://127.0.0.1") || trimmed.starts_with("http://localhost") {
        "local detmir-portal api".to_string()
    } else {
        "configured upstream".to_string()
    }
}

fn respond_text(
    request: Request,
    status: StatusCode,
    text: &str,
    content_type: &str,
) -> Result<()> {
    let response = Response::from_string(text.to_string())
        .with_status_code(status)
        .with_header(
            Header::from_bytes("Content-Type", content_type)
                .map_err(|_| anyhow!("invalid Content-Type header"))?,
        )
        .with_header(
            Header::from_bytes("Cache-Control", "no-store")
                .map_err(|_| anyhow!("invalid Cache-Control header"))?,
        );
    request.respond(response)?;
    Ok(())
}

fn respond_json_text(request: Request, status: StatusCode, text: &str) -> Result<()> {
    respond_text(request, status, text, "application/json; charset=utf-8")
}
