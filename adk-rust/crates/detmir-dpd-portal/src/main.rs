use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use reqwest::blocking::Client;
use reqwest::header::{
    CONNECTION, CONTENT_DISPOSITION, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue, LOCATION,
};
use serde_json::json;
use tiny_http::{Header, Request, Response, Server, StatusCode};

const INDEX_HTML: &str = include_str!("static/index.html");
const APP_CSS: &str = include_str!("static/app.css");
const APP_JS: &str = include_str!("static/app.js");

#[derive(Clone, Debug, Parser)]
#[command(about = "DetMir DPD parallel portal gateway")]
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
    let forwarded_headers = forwarded_request_headers(&request)?;
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
    for (name, value) in forwarded_headers {
        upstream = upstream.header(name, value);
    }

    let upstream_response = upstream
        .send()
        .with_context(|| format!("proxy upstream {url}"))?;
    let status = StatusCode(upstream_response.status().as_u16());
    let response_headers = mirrored_response_headers(upstream_response.headers())?;
    let mut bytes = upstream_response
        .bytes()
        .context("upstream response body")?
        .to_vec();
    let content_type = response_headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("Content-Type"))
        .map(|(_, value)| value.as_str());
    rewrite_mirrored_body(&mut bytes, content_type);
    respond_bytes(request, status, bytes, response_headers)
}

fn forwarded_request_headers(request: &Request) -> Result<Vec<(HeaderName, String)>> {
    let mut headers = Vec::new();
    for header in request.headers() {
        let name = header.field.as_str();
        let lower = name.to_string().to_ascii_lowercase();
        if is_hop_by_hop_header(&lower) {
            continue;
        }
        if should_forward_header(&lower) {
            let name = HeaderName::from_bytes(name.as_bytes())
                .with_context(|| format!("invalid request header name {name}"))?;
            headers.push((name, header.value.as_str().to_string()));
        }
    }
    Ok(headers)
}

fn should_forward_header(lower_name: &str) -> bool {
    matches!(
        lower_name,
        "accept"
            | "accept-language"
            | "authorization"
            | "content-type"
            | "cookie"
            | "origin"
            | "referer"
            | "user-agent"
            | "x-forwarded-for"
            | "x-forwarded-host"
            | "x-forwarded-proto"
            | "x-gateway-user"
            | "x-real-ip"
            | "x-remote-user"
    )
}

fn is_hop_by_hop_header(lower_name: &str) -> bool {
    matches!(
        lower_name,
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}

fn mirrored_response_headers(headers: &HeaderMap) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    for name in [CONTENT_TYPE, CONTENT_DISPOSITION, LOCATION] {
        if let Some(value) = headers.get(&name) {
            let mut value = value
                .to_str()
                .with_context(|| format!("invalid upstream {name} header"))?
                .to_string();
            if name == LOCATION {
                value = rewrite_mirrored_text(&value);
            }
            out.push((name.as_str().to_string(), value));
        }
    }
    Ok(out)
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
    response_headers: Vec<(String, String)>,
) -> Result<()> {
    let mut response = Response::from_data(body)
        .with_status_code(status)
        .with_header(
            Header::from_bytes("Cache-Control", "no-store")
                .map_err(|_| anyhow!("invalid Cache-Control header"))?,
        );
    for (name, value) in response_headers {
        response = response.with_header(
            Header::from_bytes(name, value)
                .map_err(|_| anyhow!("invalid mirrored response header"))?,
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
    *body = rewrite_mirrored_text(text).into_bytes();
}

fn rewrite_mirrored_text(text: &str) -> String {
    text.replace("/portal", "/dpd")
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

    #[test]
    fn forwards_operator_identity_headers_but_not_hop_by_hop_headers() {
        assert!(should_forward_header("x-remote-user"));
        assert!(should_forward_header("x-gateway-user"));
        assert!(should_forward_header("authorization"));
        assert!(should_forward_header("cookie"));
        assert!(!should_forward_header("x-debug-private"));
        assert!(is_hop_by_hop_header("connection"));
        assert!(is_hop_by_hop_header("transfer-encoding"));
        assert!(!is_hop_by_hop_header("x-remote-user"));
    }

    #[test]
    fn rewrites_portal_locations_for_dpd_mirror() {
        assert_eq!(
            rewrite_mirrored_text("/portal/reports?format=markdown"),
            "/dpd/reports?format=markdown"
        );
    }
}
