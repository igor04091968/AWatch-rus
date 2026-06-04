use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, CONNECTION, CONTENT_DISPOSITION, CONTENT_TYPE, HeaderValue};
use serde_json::json;
use tiny_http::{Header, Request, Response, Server, StatusCode};

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
        default_value = "http://127.0.0.1:8720",
        env = "DETMIR_DPD_UPSTREAM_BASE"
    )]
    upstream_base: String,

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
    let path = normalize_path_with_query(request.url());
    match path_no_query(&path) {
        "/_dpd/health" => respond_json_text(
            request,
            StatusCode(200),
            &json!({
                "ok": true,
                "portal": "detmir-dpd-portal",
                "mode": "parallel-full-mirror",
                "upstream": sanitize_upstream(&args.upstream_base),
            })
            .to_string(),
        ),
        "/preview" => respond_redirect(request, "preview/"),
        "/preview/" => respond_text(
            request,
            StatusCode(200),
            INDEX_HTML,
            "text/html; charset=utf-8",
        ),
        "/preview/app.css" => {
            respond_text(request, StatusCode(200), APP_CSS, "text/css; charset=utf-8")
        }
        "/preview/app.js" => respond_text(
            request,
            StatusCode(200),
            APP_JS,
            "application/javascript; charset=utf-8",
        ),
        _ => proxy_to_upstream(request, args, &path),
    }
}

fn proxy_to_upstream(mut request: Request, args: &Cli, path: &str) -> Result<()> {
    let method = reqwest::Method::from_bytes(request.method().as_str().as_bytes())
        .map_err(|err| anyhow!("unsupported method {}: {err}", request.method()))?;
    let content_type = request_header(&request, "Content-Type");
    let accept = request_header(&request, "Accept");
    let mut body = Vec::new();
    request
        .as_reader()
        .read_to_end(&mut body)
        .context("read request body")?;

    let client = Client::builder()
        .timeout(Duration::from_secs(args.timeout_seconds))
        .no_proxy()
        .build()
        .context("upstream HTTP client")?;

    let url = format!("{}{}", args.upstream_base.trim_end_matches('/'), path);
    let mut upstream = client
        .request(method, &url)
        .header(CONNECTION, HeaderValue::from_static("close"))
        .body(body);
    if let Some(value) = content_type {
        upstream = upstream.header(CONTENT_TYPE, value);
    }
    if let Some(value) = accept {
        upstream = upstream.header(ACCEPT, value);
    }

    let upstream_response = upstream
        .send()
        .with_context(|| format!("proxy upstream {url}"))?;
    let status = StatusCode(upstream_response.status().as_u16());
    let content_type = upstream_response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(ToString::to_string);
    let content_disposition = upstream_response
        .headers()
        .get(CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .map(ToString::to_string);
    let mut bytes = upstream_response
        .bytes()
        .context("upstream response body")?
        .to_vec();
    rewrite_mirrored_body(&mut bytes, content_type.as_deref());
    respond_bytes(request, status, bytes, content_type, content_disposition)
}

fn request_header(request: &Request, name: &'static str) -> Option<String> {
    request
        .headers()
        .iter()
        .find(|header| header.field.equiv(name))
        .map(|header| header.value.as_str().to_string())
}

fn normalize_path_with_query(url: &str) -> String {
    let path = url.trim();
    for prefix in ["/portal-dpd", "/dpd"] {
        if let Some(stripped) = path.strip_prefix(prefix) {
            return if stripped.is_empty() {
                "/".to_string()
            } else {
                stripped.to_string()
            };
        }
    }
    if path.is_empty() {
        "/".to_string()
    } else {
        path.to_string()
    }
}

fn path_no_query(path: &str) -> &str {
    path.split('?').next().unwrap_or(path)
}

fn sanitize_upstream(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.starts_with("http://127.0.0.1") || trimmed.starts_with("http://localhost") {
        "local detmir-portal".to_string()
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

fn respond_redirect(request: Request, location: &str) -> Result<()> {
    let response = Response::from_string("")
        .with_status_code(StatusCode(302))
        .with_header(
            Header::from_bytes("Location", location)
                .map_err(|_| anyhow!("invalid Location header"))?,
        )
        .with_header(
            Header::from_bytes("Cache-Control", "no-store")
                .map_err(|_| anyhow!("invalid Cache-Control header"))?,
        );
    request.respond(response)?;
    Ok(())
}

fn respond_bytes(
    request: Request,
    status: StatusCode,
    body: Vec<u8>,
    content_type: Option<String>,
    content_disposition: Option<String>,
) -> Result<()> {
    let mut response = Response::from_data(body)
        .with_status_code(status)
        .with_header(
            Header::from_bytes("Cache-Control", "no-store")
                .map_err(|_| anyhow!("invalid Cache-Control header"))?,
        );
    if let Some(value) = content_type {
        response = response.with_header(
            Header::from_bytes("Content-Type", value)
                .map_err(|_| anyhow!("invalid upstream Content-Type header"))?,
        );
    }
    if let Some(value) = content_disposition {
        response = response.with_header(
            Header::from_bytes("Content-Disposition", value)
                .map_err(|_| anyhow!("invalid upstream Content-Disposition header"))?,
        );
    }
    request.respond(response)?;
    Ok(())
}

fn rewrite_mirrored_body(body: &mut Vec<u8>, content_type: Option<&str>) {
    let Some(content_type) = content_type else {
        return;
    };
    if !(content_type.contains("text/html") || content_type.contains("javascript")) {
        return;
    }
    let Ok(text) = std::str::from_utf8(body) else {
        return;
    };
    if !text.contains("/portal") {
        return;
    }
    *body = text.replace("/portal", "/dpd").into_bytes();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_dpd_prefix_and_preserves_query() {
        assert_eq!(normalize_path_with_query("/dpd/"), "/");
        assert_eq!(
            normalize_path_with_query("/dpd/api/reports?anonymize=1"),
            "/api/reports?anonymize=1"
        );
        assert_eq!(
            normalize_path_with_query("/portal-dpd/api/cases/c1?format=markdown"),
            "/api/cases/c1?format=markdown"
        );
    }

    #[test]
    fn preview_paths_are_not_upstream_mirrored() {
        assert_eq!(path_no_query("/preview/app.js?v=1"), "/preview/app.js");
    }

    #[test]
    fn rewrites_portal_absolute_paths_for_dpd_mirror() {
        let mut body = br#"href="/portal/api/reports"; const x = `/portal/api/cases/c1`;"#.to_vec();
        rewrite_mirrored_body(&mut body, Some("application/javascript; charset=utf-8"));
        let text = String::from_utf8(body).unwrap();
        assert!(text.contains("/dpd/api/reports"));
        assert!(text.contains("/dpd/api/cases/c1"));
        assert!(!text.contains("/portal/api"));
    }
}
