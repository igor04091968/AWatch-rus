//! URL path and query parsing helpers for the portal.
//!
//! CONTRACT: these helpers are routing glue. Keep accepted URL shapes stable
//! because API handlers and the HTML portal depend on them.

pub(crate) fn normalize_path(url: &str) -> String {
    let path = url.split('?').next().unwrap_or("/");
    let path = path.strip_prefix("/portal").unwrap_or(path);
    if path.is_empty() {
        "/".to_string()
    } else {
        path.to_string()
    }
}

pub(crate) fn query_flag(url: &str, key: &str) -> bool {
    let Some(query) = url.split_once('?').map(|(_, query)| query) else {
        return false;
    };
    query.split('&').any(|pair| {
        let (name, value) = pair.split_once('=').unwrap_or((pair, "1"));
        name == key && matches!(value, "1" | "true" | "yes" | "on")
    })
}

pub(crate) fn query_param(url: &str, key: &str) -> Option<String> {
    let query = url.split_once('?').map(|(_, query)| query)?;
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        (name == key && !value.is_empty()).then(|| value.to_string())
    })
}

pub(crate) fn parse_investigation_pack_path(path: &str) -> Option<String> {
    path.strip_prefix("/api/investigation-pack/")
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.contains('/'))
        .map(ToString::to_string)
}

pub(crate) fn parse_case_path(path: &str) -> Option<String> {
    path.strip_prefix("/api/cases/")
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.contains('/'))
        .map(ToString::to_string)
}

pub(crate) fn parse_case_status_path(path: &str) -> Option<String> {
    path.strip_prefix("/api/cases/")
        .and_then(|value| value.strip_suffix("/status"))
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.contains('/'))
        .map(ToString::to_string)
}
