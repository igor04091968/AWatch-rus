use serde_json::{Value, json};

use crate::now;

pub(crate) fn build_healthz() -> Value {
    json!({
        "status": "ok",
        "generated_at_utc": now(),
    })
}
