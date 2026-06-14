//! Build/version probe payload.
//!
//! CONTRACT: `/version` is used by smoke tests, runbooks and release evidence.
//! Keep field names stable and add new fields only in a backward-compatible way.

use serde_json::{Value, json};

use crate::{Cli, PORTAL_SCHEMA_VERSION};

pub(crate) fn build_version(args: &Cli) -> Value {
    json!({
        "app_version": env!("CARGO_PKG_VERSION"),
        "git_commit": option_env!("GIT_COMMIT").unwrap_or("unknown"),
        "build_time": option_env!("BUILD_TIME").unwrap_or("unknown"),
        "schema_version": PORTAL_SCHEMA_VERSION,
        "environment": args.environment,
    })
}
