use std::net::ToSocketAddrs;
use std::time::Duration;

use anyhow::{Context, Result};
use tiny_http::{Header, Response, Server, StatusCode};

use crate::envelope::AGENT_VERSION;
use crate::metrics::AgentMetrics;

pub fn serve_health(bind: &str, metrics: AgentMetrics, max_requests: Option<usize>) -> Result<()> {
    bind.to_socket_addrs()
        .with_context(|| format!("parse health bind address {bind}"))?;
    let server =
        Server::http(bind).map_err(|err| anyhow::anyhow!("bind health endpoint: {err}"))?;
    let mut served = 0_usize;
    loop {
        if max_requests.is_some_and(|limit| served >= limit) {
            return Ok(());
        }
        let Some(request) = server
            .recv_timeout(Duration::from_millis(250))
            .map_err(|err| anyhow::anyhow!("receive health request: {err}"))?
        else {
            continue;
        };
        served += 1;
        let response = match (request.method().as_str(), request.url()) {
            ("GET", "/healthz") => json_response(serde_json::json!({
                "ok": true,
                "status": "online",
                "agent_version": AGENT_VERSION,
            })),
            ("GET", "/metrics") => text_response(metrics.render_prometheus()),
            _ => Response::from_string("not found").with_status_code(StatusCode(404)),
        };
        request
            .respond(response)
            .map_err(|err| anyhow::anyhow!("send health response: {err}"))?;
    }
}

fn json_response(value: serde_json::Value) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut response = Response::from_data(serde_json::to_vec(&value).unwrap_or_default());
    if let Ok(header) = Header::from_bytes("Content-Type", "application/json") {
        response.add_header(header);
    }
    response
}

fn text_response(value: String) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut response = Response::from_string(value);
    if let Ok(header) = Header::from_bytes("Content-Type", "text/plain; version=0.0.4") {
        response.add_header(header);
    }
    response
}
