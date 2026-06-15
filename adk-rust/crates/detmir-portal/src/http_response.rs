//! HTTP response helpers for the portal.
//!
//! CONTRACT: this module owns response serialization, headers, request-id /
//! correlation-id propagation and response metrics logging. It must not change
//! routes, payload schemas, MIME types or UI contents.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use serde::Serialize;
use tiny_http::{Header, Request, Response, StatusCode};

use crate::production::{http_request_metadata, log_http_request, record_http_metric};
use crate::screenshot_basename;

pub(crate) fn respond_json<T: Serialize>(request: Request, value: &T) -> Result<()> {
    let body = serde_json::to_string_pretty(value)?;
    respond_text(
        request,
        StatusCode(200),
        &body,
        "application/json; charset=utf-8",
    )
}

pub(crate) fn respond_json_status<T: Serialize>(
    request: Request,
    status: StatusCode,
    value: &T,
) -> Result<()> {
    let body = serde_json::to_string_pretty(value)?;
    respond_text(request, status, &body, "application/json; charset=utf-8")
}

pub(crate) fn respond_text(
    request: Request,
    status: StatusCode,
    body: &str,
    content_type: &str,
) -> Result<()> {
    let metadata = http_request_metadata(&request);
    record_http_metric(&metadata, status);
    log_http_request(&metadata, status, body.len());
    let response = Response::from_string(body.to_string())
        .with_status_code(status)
        .with_header(header("Content-Type", content_type)?)
        .with_header(header("Cache-Control", "no-store")?)
        .with_header(header("X-Request-Id", &metadata.request_id)?)
        .with_header(header("X-Correlation-Id", &metadata.correlation_id)?);
    request.respond(response).map_err(|err| anyhow!("{err}"))
}

pub(crate) fn respond_text_download(
    request: Request,
    status: StatusCode,
    body: &str,
    content_type: &str,
    download_name: &str,
) -> Result<()> {
    let metadata = http_request_metadata(&request);
    record_http_metric(&metadata, status);
    log_http_request(&metadata, status, body.len());
    let response = Response::from_string(body.to_string())
        .with_status_code(status)
        .with_header(header("Content-Type", content_type)?)
        .with_header(header("Cache-Control", "no-store")?)
        .with_header(header("X-Request-Id", &metadata.request_id)?)
        .with_header(header("X-Correlation-Id", &metadata.correlation_id)?)
        .with_header(header(
            "Content-Disposition",
            &format!(
                "attachment; filename=\"{}\"",
                download_name.replace('"', "")
            ),
        )?);
    request.respond(response).map_err(|err| anyhow!("{err}"))
}

pub(crate) fn respond_file(
    request: Request,
    path: &Path,
    content_type: &str,
    download_name: Option<&str>,
) -> Result<()> {
    let data = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let metadata = http_request_metadata(&request);
    record_http_metric(&metadata, StatusCode(200));
    log_http_request(&metadata, StatusCode(200), data.len());
    let mut response = Response::from_data(data)
        .with_status_code(StatusCode(200))
        .with_header(header("Content-Type", content_type)?)
        .with_header(header("Cache-Control", "no-store")?)
        .with_header(header("X-Request-Id", &metadata.request_id)?)
        .with_header(header("X-Correlation-Id", &metadata.correlation_id)?);
    if let Some(name) = download_name.and_then(screenshot_basename) {
        response = response.with_header(header(
            "Content-Disposition",
            &format!("attachment; filename=\"{}\"", name.replace('"', "")),
        )?);
    }
    request.respond(response).map_err(|err| anyhow!("{err}"))
}

pub(crate) fn safe_download_stem(value: &str) -> String {
    let stem = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .take(96)
        .collect::<String>();
    if stem.is_empty() {
        "candidate".to_string()
    } else {
        stem
    }
}

fn header(name: &str, value: &str) -> Result<Header> {
    Header::from_bytes(name.as_bytes(), value.as_bytes())
        .map_err(|_| anyhow!("invalid header {name}: {value}"))
}
