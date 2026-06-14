//! Readiness API payload helpers for the portal.
//!
//! CONTRACT: these helpers expose existing readiness bundle/status/verify
//! payloads. Keep file names, JSON fields and verification commands stable
//! unless the customer readiness contract is updated in the same PR.

use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use serde_json::{Value, json};

use crate::command_runner::run_in_dir;
use crate::{Cli, now};

pub(crate) fn readiness_latest(args: &Cli) -> Value {
    read_json_file(
        &args
            .readiness_bundle_dir
            .join("detmir-readiness-latest.json"),
    )
    .unwrap_or_else(|err| {
        json!({
            "ok": false,
            "generated_at_utc": now(),
            "error": err.to_string(),
        })
    })
}

pub(crate) fn readiness_bundle(args: &Cli) -> Value {
    let dir = &args.readiness_bundle_dir;
    let status = read_json_file(&dir.join("detmir-readiness-status.json")).unwrap_or_else(|err| {
        json!({
            "ok": false,
            "error": err.to_string(),
        })
    });
    let latest_dir = fs::read_to_string(dir.join("latest-dir.txt"))
        .unwrap_or_default()
        .trim()
        .to_string();
    let artifacts = [
        "detmir-readiness-latest.json",
        "detmir-readiness-act.md",
        "detmir-readiness-act.html",
        "sha256sums.txt",
        "sha256sums.txt.sig",
        "public-key.pem",
        "detmir-readiness-status.json",
        "detmir-readiness.prom",
    ]
    .into_iter()
    .filter_map(|name| {
        let path = dir.join(name);
        path.metadata().ok().map(|meta| {
            json!({
                "name": name,
                "bytes": meta.len(),
                "available": true,
            })
        })
    })
    .collect::<Vec<_>>();
    json!({
        "ok": status.get("ok").and_then(Value::as_bool).unwrap_or(false),
        "generated_at_utc": now(),
        "bundle_dir": dir.display().to_string(),
        "latest_archive_dir": latest_dir,
        "status": status,
        "artifacts": artifacts,
    })
}

pub(crate) fn readiness_verify(args: &Cli) -> Value {
    let dir = &args.readiness_bundle_dir;
    let checksum = run_in_dir(
        dir,
        Command::new("sha256sum").arg("-c").arg("sha256sums.txt"),
    );
    let sig_path = dir.join("sha256sums.txt.sig");
    let pub_path = dir.join("public-key.pem");
    let signature = if sig_path.is_file() && pub_path.is_file() {
        run_in_dir(
            dir,
            Command::new("openssl")
                .arg("dgst")
                .arg("-sha256")
                .arg("-verify")
                .arg("public-key.pem")
                .arg("-signature")
                .arg("sha256sums.txt.sig")
                .arg("sha256sums.txt"),
        )
    } else {
        Err("signature files are not available".to_string())
    };
    json!({
        "ok": checksum.is_ok() && signature.is_ok(),
        "generated_at_utc": now(),
        "checksum_verified": checksum.is_ok(),
        "signature_verified": signature.is_ok(),
        "checksum_error": checksum.err(),
        "signature_error": signature.err(),
    })
}

fn read_json_file(path: &Path) -> Result<Value> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))
}
