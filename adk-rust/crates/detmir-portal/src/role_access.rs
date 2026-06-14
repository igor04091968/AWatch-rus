//! Portal role extraction and access-denial helpers.
//!
//! CONTRACT: role aliases, role envelope fields and forbidden response shape
//! are part of the portal security boundary. Keep changes explicit and covered
//! by role-gate tests.

use anyhow::Result;
use serde_json::{Value, json};
use tiny_http::{Request, StatusCode};

use crate::path_query::query_param;
use crate::portal_roles::PortalRole;
use crate::respond_json_status;

pub(crate) fn portal_role_from_request(request: &Request, url: &str) -> PortalRole {
    query_param(url, "role")
        .as_deref()
        .and_then(PortalRole::parse)
        .or_else(|| {
            request
                .headers()
                .iter()
                .find(|header| header.field.equiv("X-AWatch-Role"))
                .and_then(|header| PortalRole::parse(header.value.as_str()))
        })
        .unwrap_or(PortalRole::Executive)
}

pub(crate) fn role_envelope(role: PortalRole, scope: &str) -> Value {
    json!({
        "role": role.as_str(),
        "role_label": role.label_ru(),
        "scope": scope,
        "allowed_scopes": role.allowed_scopes(),
        "server_enforced": true,
    })
}

pub(crate) fn respond_forbidden(request: Request, role: PortalRole, scope: &str) -> Result<()> {
    respond_json_status(
        request,
        StatusCode(403),
        &json!({
            "ok": false,
            "error": "forbidden",
            "message": format!("Роль {} не имеет доступа к контуру {scope}", role.label_ru()),
            "role": role.as_str(),
            "scope": scope,
            "server_enforced": true,
        }),
    )
}
