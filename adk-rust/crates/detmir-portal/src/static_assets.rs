//! Static portal assets and generated API contract text.
//!
//! CONTRACT: this module only exposes embedded static files. Do not change
//! file contents, MIME handling, routes or API contracts from here.

pub(crate) const INDEX_HTML: &str = include_str!("static/index.html");
pub(crate) const ARCHITECTURE_HTML: &str = include_str!("static/architecture.html");
pub(crate) const APP_CSS: &str = include_str!("static/app.css");
pub(crate) const APP_JS: &str = include_str!("static/app.js");
pub(crate) const API_CONTRACT_OPENAPI: &str = include_str!("contracts/openapi.json");
pub(crate) const API_CONTRACT_TYPESCRIPT: &str = include_str!("contracts/typescript.d.ts");
