//! Liveness probe payload.
//!
//! CONTRACT: `/healthz` is intentionally shallow. It proves that the portal
//! process can answer HTTP, while dependency checks belong to `/readyz`.

use serde_json::{Value, json};

use crate::now;

pub(crate) fn build_healthz() -> Value {
    json!({
        "status": "ok",
        "generated_at_utc": now(),
    })
}
