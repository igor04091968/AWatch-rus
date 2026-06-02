use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow, bail};
use chrono::{SecondsFormat, Utc};
use clap::Parser;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};
use urlencoding::decode;

#[derive(Debug, Parser)]
#[command(about = "AWatch DLP Case Management API")]
struct Cli {
    #[arg(long, default_value = "127.0.0.1")]
    bind_host: String,

    #[arg(long, default_value_t = 5602)]
    port: u16,

    #[arg(
        long,
        default_value = "/opt/activitywatch/dlp-case-management/cases.db"
    )]
    db_path: PathBuf,
}

#[derive(Clone)]
struct AppState {
    db_path: PathBuf,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = apply_env(Cli::parse());
    let storage = CaseStorage::new(cli.db_path.clone())?;
    storage.init_schema()?;
    let state = AppState {
        db_path: cli.db_path,
    };
    let address = format!("{}:{}", cli.bind_host, cli.port);
    let server = Server::http(&address).map_err(|err| anyhow!("bind {address}: {err}"))?;
    eprintln!("aw-dlp-case-management 0.1.0 listening on {address}");
    for request in server.incoming_requests() {
        if let Err(err) = handle_request(&state, request) {
            eprintln!("request error: {err:#}");
        }
    }
    Ok(())
}

fn apply_env(mut cli: Cli) -> Cli {
    if !cli_arg_present("--bind-host") {
        if let Ok(value) = std::env::var("AW_DLP_CASE_BIND_HOST") {
            if !value.is_empty() {
                cli.bind_host = value;
            }
        }
    }
    if !cli_arg_present("--port") {
        if let Ok(value) = std::env::var("AW_DLP_CASE_PORT") {
            if let Ok(port) = value.parse() {
                cli.port = port;
            }
        }
    }
    if !cli_arg_present("--db-path") {
        if let Ok(value) = std::env::var("AW_DLP_CASE_DB_PATH") {
            if !value.is_empty() {
                cli.db_path = PathBuf::from(value);
            }
        }
    }
    cli
}

fn cli_arg_present(flag: &str) -> bool {
    std::env::args().any(|arg| arg == flag || arg.starts_with(&format!("{flag}=")))
}

fn handle_request(state: &AppState, mut request: Request) -> Result<()> {
    let method = request.method().clone();
    let url = request.url().to_string();
    let (path, query) = split_url(&url);
    let segments = path_segments(&path);
    let storage = CaseStorage::new(state.db_path.clone())?;

    let response = match (method, segments.as_slice()) {
        (Method::Get, ["health"]) => json_response(
            StatusCode(200),
            json!({"ok": true, "db": state.db_path.to_string_lossy()}),
        ),
        (Method::Post, ["api", "0", "dlp", "cases"]) => {
            let payload = read_json_body(&mut request)?;
            if is_self_test_case(
                payload.get("incident_id").and_then(Value::as_str),
                payload.get("title").and_then(Value::as_str),
            ) {
                json_response(
                    StatusCode(422),
                    json!({"detail": "self_test cases are not allowed"}),
                )
            } else {
                match storage.create_case(&payload) {
                    Ok(item) => json_response(StatusCode(200), item),
                    Err(err) => json_response(StatusCode(422), json!({"detail": err.to_string()})),
                }
            }
        }
        (Method::Get, ["api", "0", "dlp", "cases"]) => {
            let status = query.get("status").cloned();
            let host = query.get("host").cloned();
            let limit = query_limit(&query, 200, 2000);
            json_response(
                StatusCode(200),
                Value::Array(storage.list_cases(status, host, limit)?),
            )
        }
        (Method::Get, ["api", "0", "dlp", "cases", case_id]) => {
            let case_id = parse_id(case_id)?;
            match storage.get_case_full(case_id)? {
                Some(item) => json_response(StatusCode(200), item),
                None => json_response(StatusCode(404), json!({"detail": "case not found"})),
            }
        }
        (Method::Patch, ["api", "0", "dlp", "cases", case_id]) => {
            let case_id = parse_id(case_id)?;
            let payload = read_json_body(&mut request)?;
            match storage.update_case(case_id, &payload) {
                Ok(Some(item)) => json_response(StatusCode(200), item),
                Ok(None) => json_response(StatusCode(404), json!({"detail": "case not found"})),
                Err(err) => json_response(StatusCode(422), json!({"detail": err.to_string()})),
            }
        }
        (Method::Post, ["api", "0", "dlp", "cases", case_id, "comments"]) => {
            let case_id = parse_id(case_id)?;
            let payload = read_json_body(&mut request)?;
            if storage.get_case(case_id)?.is_none() {
                json_response(StatusCode(404), json!({"detail": "case not found"}))
            } else {
                match storage.add_comment(case_id, &payload) {
                    Ok(item) => json_response(StatusCode(200), item),
                    Err(err) => json_response(StatusCode(422), json!({"detail": err.to_string()})),
                }
            }
        }
        (Method::Get, ["api", "0", "dlp", "cases", case_id, "comments"]) => {
            let case_id = parse_id(case_id)?;
            let limit = query_limit(&query, 200, 2000);
            json_response(
                StatusCode(200),
                Value::Array(storage.list_comments(case_id, limit)?),
            )
        }
        (Method::Post, ["api", "0", "dlp", "cases", case_id, "forensics", "hayabusa"]) => {
            let case_id = parse_id(case_id)?;
            let payload = read_json_body(&mut request)?;
            match storage.link_hayabusa(case_id, &payload) {
                Ok(Some(item)) => json_response(StatusCode(200), item),
                Ok(None) => json_response(StatusCode(404), json!({"detail": "case not found"})),
                Err(CaseError::HostMismatch(text)) => {
                    json_response(StatusCode(409), json!({"detail": text}))
                }
                Err(CaseError::Other(err)) => {
                    json_response(StatusCode(422), json!({"detail": err.to_string()}))
                }
            }
        }
        _ => json_response(StatusCode(404), json!({"detail": "not found"})),
    };

    request.respond(response).context("send HTTP response")
}

struct CaseStorage {
    db_path: PathBuf,
}

enum CaseError {
    HostMismatch(String),
    Other(anyhow::Error),
}

impl From<anyhow::Error> for CaseError {
    fn from(value: anyhow::Error) -> Self {
        Self::Other(value)
    }
}

impl From<rusqlite::Error> for CaseError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Other(value.into())
    }
}

impl From<serde_json::Error> for CaseError {
    fn from(value: serde_json::Error) -> Self {
        Self::Other(value.into())
    }
}

impl CaseStorage {
    fn new(db_path: PathBuf) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create db parent {}", parent.display()))?;
        }
        Ok(Self { db_path })
    }

    fn connect(&self) -> Result<Connection> {
        let conn = Connection::open(&self.db_path)
            .with_context(|| format!("open sqlite {}", self.db_path.display()))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        Ok(conn)
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.connect()?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS cases (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              incident_id TEXT NOT NULL,
              host TEXT,
              title TEXT NOT NULL,
              severity TEXT NOT NULL DEFAULT 'medium',
              assignee TEXT,
              status TEXT NOT NULL DEFAULT 'open',
              source_bucket TEXT,
              source_event_ts TEXT,
              evidence_json TEXT,
              forensics_json TEXT,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_cases_incident_id ON cases(incident_id);
            CREATE INDEX IF NOT EXISTS idx_cases_status ON cases(status);

            CREATE TABLE IF NOT EXISTS case_comments (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              case_id INTEGER NOT NULL,
              comment TEXT NOT NULL,
              author TEXT,
              created_at TEXT NOT NULL,
              FOREIGN KEY(case_id) REFERENCES cases(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS case_audit (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              case_id INTEGER NOT NULL,
              action TEXT NOT NULL,
              actor TEXT,
              details_json TEXT,
              created_at TEXT NOT NULL,
              FOREIGN KEY(case_id) REFERENCES cases(id) ON DELETE CASCADE
            );
            "#,
        )?;
        ensure_column(&conn, "cases", "forensics_json", "TEXT")?;
        Ok(())
    }

    fn create_case(&self, payload: &Value) -> Result<Value> {
        let incident_id = required_string(payload.get("incident_id"), "incident_id")?;
        validate_len("incident_id", &incident_id, 1, 256)?;
        let title = required_string(payload.get("title"), "title")?;
        validate_len("title", &title, 1, 512)?;
        let host = optional_string(payload.get("host"));
        validate_optional_len("host", host.as_deref(), 128)?;
        let severity =
            optional_string(payload.get("severity")).unwrap_or_else(|| "medium".to_string());
        validate_optional_len("severity", Some(&severity), 32)?;
        let assignee = optional_string(payload.get("assignee"));
        validate_optional_len("assignee", assignee.as_deref(), 128)?;
        let source_bucket = optional_string(payload.get("source_bucket"));
        validate_optional_len("source_bucket", source_bucket.as_deref(), 256)?;
        let source_event_ts = optional_string(payload.get("source_event_ts"));
        validate_optional_len("source_event_ts", source_event_ts.as_deref(), 64)?;

        let (normalized_evidence, evidence_digest) = match payload.get("evidence") {
            Some(Value::Null) | None => (None, None),
            Some(evidence) => {
                let normalized = normalize_evidence_chain(
                    evidence,
                    source_bucket.as_deref(),
                    source_event_ts.as_deref(),
                )?;
                let digest = normalized
                    .get("latest_sha256")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| evidence_sha256(evidence));
                (Some(normalized), Some(digest))
            }
        };
        let now = utc_now();
        let conn = self.connect()?;
        let tx = conn.unchecked_transaction()?;
        let existing = tx
            .query_row(
                r#"
                SELECT * FROM cases
                WHERE incident_id = ? AND COALESCE(host, '') = COALESCE(?, '')
                ORDER BY id DESC
                LIMIT 1
                "#,
                params![incident_id, host],
                case_from_row,
            )
            .optional()?;
        if let Some(existing) = existing {
            tx.commit()?;
            return Ok(existing);
        }
        tx.execute(
            r#"
            INSERT INTO cases (
              incident_id, host, title, severity, assignee, status,
              source_bucket, source_event_ts, evidence_json, forensics_json, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, 'open', ?, ?, ?, ?, ?, ?)
            "#,
            params![
                incident_id,
                host,
                title,
                severity,
                assignee,
                source_bucket,
                source_event_ts,
                normalized_evidence
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?,
                Option::<String>::None,
                now,
                now,
            ],
        )?;
        let case_id = tx.last_insert_rowid();
        let mut fields = payload.as_object().cloned().unwrap_or_default();
        fields.remove("evidence");
        insert_audit(
            &tx,
            case_id,
            "create",
            Some("api"),
            Some(json!({"fields": fields, "evidence_sha256": evidence_digest})),
        )?;
        tx.commit()?;
        self.get_case(case_id)?
            .ok_or_else(|| anyhow!("created case not found"))
    }

    fn list_cases(
        &self,
        status: Option<String>,
        host: Option<String>,
        limit: i64,
    ) -> Result<Vec<Value>> {
        let conn = self.connect()?;
        let mut query = "SELECT * FROM cases".to_string();
        let mut clauses = Vec::new();
        let mut values = Vec::new();
        if let Some(status) = status {
            clauses.push("status = ?");
            values.push(status);
        }
        if let Some(host) = host {
            clauses.push("host = ?");
            values.push(host);
        }
        if !clauses.is_empty() {
            query.push_str(" WHERE ");
            query.push_str(&clauses.join(" AND "));
        }
        query.push_str(" ORDER BY id DESC LIMIT ?");
        let mut stmt = conn.prepare(&query)?;
        let rows = match values.as_slice() {
            [] => stmt.query_map([limit.to_string()], case_from_row)?,
            [a] => stmt.query_map(params![a, limit], case_from_row)?,
            [a, b] => stmt.query_map(params![a, b, limit], case_from_row)?,
            _ => unreachable!(),
        };
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    fn get_case(&self, case_id: i64) -> Result<Option<Value>> {
        let conn = self.connect()?;
        conn.query_row("SELECT * FROM cases WHERE id = ?", [case_id], case_from_row)
            .optional()
            .map_err(Into::into)
    }

    fn get_case_full(&self, case_id: i64) -> Result<Option<Value>> {
        let Some(mut item) = self.get_case(case_id)? else {
            return Ok(None);
        };
        let comments = self.list_comments(case_id, 200)?;
        let audit = self.list_audit(case_id, 200)?;
        let object = item
            .as_object_mut()
            .ok_or_else(|| anyhow!("case row is not object"))?;
        object.insert("comments".to_string(), Value::Array(comments));
        object.insert("audit".to_string(), Value::Array(audit));
        Ok(Some(item))
    }

    fn update_case(&self, case_id: i64, patch: &Value) -> Result<Option<Value>> {
        if self.get_case(case_id)?.is_none() {
            return Ok(None);
        }
        let mut fields = Vec::new();
        let mut args = Vec::new();
        for key in ["status", "assignee", "title", "severity"] {
            if let Some(value) = patch.get(key).filter(|value| !value.is_null()) {
                if key == "status" {
                    let status = required_string(Some(value), "status")?;
                    if !matches!(
                        status.as_str(),
                        "open" | "investigating" | "resolved" | "closed"
                    ) {
                        bail!("invalid status");
                    }
                    args.push(status);
                } else {
                    args.push(required_string(Some(value), key)?);
                }
                fields.push(key);
            }
        }
        if fields.is_empty() {
            return self.get_case(case_id);
        }
        let now = utc_now();
        let conn = self.connect()?;
        let tx = conn.unchecked_transaction()?;
        let assignments = fields
            .iter()
            .map(|field| format!("{field} = ?"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("UPDATE cases SET {assignments}, updated_at = ? WHERE id = ?");
        let mut params_values: Vec<&dyn rusqlite::ToSql> = args
            .iter()
            .map(|value| value as &dyn rusqlite::ToSql)
            .collect();
        params_values.push(&now);
        params_values.push(&case_id);
        tx.execute(&sql, params_values.as_slice())?;
        insert_audit(&tx, case_id, "update", Some("api"), Some(patch.clone()))?;
        tx.commit()?;
        self.get_case(case_id)
    }

    fn add_comment(&self, case_id: i64, payload: &Value) -> Result<Value> {
        let comment = required_string(payload.get("comment"), "comment")?;
        validate_len("comment", &comment, 1, 2000)?;
        let author = optional_string(payload.get("author"));
        validate_optional_len("author", author.as_deref(), 128)?;
        let now = utc_now();
        let conn = self.connect()?;
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO case_comments (case_id, comment, author, created_at) VALUES (?, ?, ?, ?)",
            params![case_id, comment, author, now],
        )?;
        let comment_id = tx.last_insert_rowid();
        insert_audit(
            &tx,
            case_id,
            "comment",
            author.as_deref(),
            Some(json!({"comment_id": comment_id})),
        )?;
        let row = tx.query_row(
            "SELECT id, case_id, comment, author, created_at FROM case_comments WHERE id = ?",
            [comment_id],
            comment_from_row,
        )?;
        tx.commit()?;
        Ok(row)
    }

    fn list_comments(&self, case_id: i64, limit: i64) -> Result<Vec<Value>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT id, case_id, comment, author, created_at FROM case_comments WHERE case_id = ? ORDER BY id DESC LIMIT ?",
        )?;
        let rows = stmt.query_map(params![case_id, limit], comment_from_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    fn list_audit(&self, case_id: i64, limit: i64) -> Result<Vec<Value>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT id, case_id, action, actor, details_json, created_at FROM case_audit WHERE case_id = ? ORDER BY id DESC LIMIT ?",
        )?;
        let rows = stmt.query_map(params![case_id, limit], audit_from_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    fn link_hayabusa(
        &self,
        case_id: i64,
        payload: &Value,
    ) -> std::result::Result<Option<Value>, CaseError> {
        let existing = match self.get_case(case_id)? {
            Some(item) => item,
            None => return Ok(None),
        };
        let case_host = normalize_host(existing.get("host").and_then(Value::as_str));
        let forensic_host = normalize_host(payload.get("host").and_then(Value::as_str));
        if !case_host.is_empty() && !forensic_host.is_empty() && case_host != forensic_host {
            return Err(CaseError::HostMismatch(format!(
                "hayabusa host mismatch: case host={} payload host={}",
                existing.get("host").and_then(Value::as_str).unwrap_or(""),
                payload.get("host").and_then(Value::as_str).unwrap_or("")
            )));
        }
        let host = required_string(payload.get("host"), "host")?;
        validate_len("host", &host, 1, 128)?;
        let mode = required_string(payload.get("mode"), "mode")?;
        validate_len("mode", &mode, 1, 32)?;
        let status = required_string(payload.get("status"), "status")?;
        validate_len("status", &status, 1, 64)?;
        let now = utc_now();
        let mut forensics = existing
            .get("forensics")
            .cloned()
            .filter(|value| value.is_object())
            .unwrap_or_else(|| json!({}));
        forensics.as_object_mut().expect("object").insert(
            "hayabusa".to_string(),
            json!({
                "tool": "hayabusa",
                "host": host,
                "mode": mode,
                "status": status,
                "intake_id": optional_json_string(payload.get("intake_id")),
                "package_path": optional_json_string(payload.get("package_path")),
                "sha256": optional_json_string(payload.get("sha256")),
                "report_dir": optional_json_string(payload.get("report_dir")),
                "summary_html": optional_json_string(payload.get("summary_html")),
                "timeline_path": optional_json_string(payload.get("timeline_path")),
                "manifest_path": optional_json_string(payload.get("manifest_path")),
                "linked_at": optional_string(payload.get("linked_at")).unwrap_or_else(|| now.clone()),
                "link_source": optional_string(payload.get("link_source")).unwrap_or_else(|| "api".to_string()),
            }),
        );
        let conn = self.connect()?;
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE cases SET forensics_json = ?, updated_at = ? WHERE id = ?",
            params![serde_json::to_string(&forensics)?, now, case_id],
        )?;
        insert_audit(
            &tx,
            case_id,
            "link_hayabusa",
            Some("api"),
            Some(json!({
                "host": payload.get("host").cloned().unwrap_or(Value::Null),
                "mode": payload.get("mode").cloned().unwrap_or(Value::Null),
                "status": payload.get("status").cloned().unwrap_or(Value::Null),
                "intake_id": payload.get("intake_id").cloned().unwrap_or(Value::Null),
                "report_dir": payload.get("report_dir").cloned().unwrap_or(Value::Null),
            })),
        )?;
        tx.commit()?;
        Ok(self.get_case(case_id)?)
    }
}

fn ensure_column(conn: &Connection, table: &str, column: &str, definition: &str) -> Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if !columns.iter().any(|item| item == column) {
        conn.execute(
            &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
            [],
        )?;
    }
    Ok(())
}

fn case_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let evidence = json_field(row.get::<_, Option<String>>("evidence_json")?);
    let forensics = json_field(row.get::<_, Option<String>>("forensics_json")?);
    Ok(json!({
        "id": row.get::<_, i64>("id")?,
        "incident_id": row.get::<_, String>("incident_id")?,
        "host": row.get::<_, Option<String>>("host")?,
        "title": row.get::<_, String>("title")?,
        "severity": row.get::<_, String>("severity")?,
        "assignee": row.get::<_, Option<String>>("assignee")?,
        "status": row.get::<_, String>("status")?,
        "source_bucket": row.get::<_, Option<String>>("source_bucket")?,
        "source_event_ts": row.get::<_, Option<String>>("source_event_ts")?,
        "evidence": evidence,
        "forensics": forensics,
        "created_at": row.get::<_, String>("created_at")?,
        "updated_at": row.get::<_, String>("updated_at")?,
    }))
}

fn comment_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    Ok(json!({
        "id": row.get::<_, i64>("id")?,
        "case_id": row.get::<_, i64>("case_id")?,
        "comment": row.get::<_, String>("comment")?,
        "author": row.get::<_, Option<String>>("author")?,
        "created_at": row.get::<_, String>("created_at")?,
    }))
}

fn audit_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    Ok(json!({
        "id": row.get::<_, i64>("id")?,
        "case_id": row.get::<_, i64>("case_id")?,
        "action": row.get::<_, String>("action")?,
        "actor": row.get::<_, Option<String>>("actor")?,
        "details": json_field(row.get::<_, Option<String>>("details_json")?),
        "created_at": row.get::<_, String>("created_at")?,
    }))
}

fn insert_audit(
    conn: &Connection,
    case_id: i64,
    action: &str,
    actor: Option<&str>,
    details: Option<Value>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO case_audit (case_id, action, actor, details_json, created_at) VALUES (?, ?, ?, ?, ?)",
        params![
            case_id,
            action,
            actor,
            details.as_ref().map(serde_json::to_string).transpose()?,
            utc_now(),
        ],
    )?;
    Ok(())
}

fn normalize_evidence_chain(
    payload: &Value,
    source_bucket: Option<&str>,
    source_event_ts: Option<&str>,
) -> Result<Value> {
    if payload.get("items").and_then(Value::as_array).is_some() {
        return Ok(payload.clone());
    }
    let digest = evidence_sha256(payload);
    let record = json!({
        "recorded_at": utc_now(),
        "source_bucket": source_bucket,
        "source_event_ts": source_event_ts,
        "sha256": digest,
        "payload": payload,
    });
    Ok(json!({
        "items": [record],
        "latest_sha256": digest,
        "chain_length": 1,
    }))
}

fn evidence_sha256(payload: &Value) -> String {
    let mut hasher = Sha256::new();
    hasher.update(
        canonical_json(payload)
            .expect("evidence JSON serializable")
            .as_bytes(),
    );
    format!("{:x}", hasher.finalize())
}

fn canonical_json(value: &Value) -> Result<String> {
    match value {
        Value::Null => Ok("null".to_string()),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Number(value) => Ok(value.to_string()),
        Value::String(value) => serde_json::to_string(value).map_err(Into::into),
        Value::Array(items) => {
            let mut out = String::from("[");
            for (idx, item) in items.iter().enumerate() {
                if idx > 0 {
                    out.push(',');
                }
                out.push_str(&canonical_json(item)?);
            }
            out.push(']');
            Ok(out)
        }
        Value::Object(map) => {
            let mut out = String::from("{");
            let mut sorted = BTreeMap::new();
            for (key, value) in map {
                sorted.insert(key, value);
            }
            for (idx, (key, value)) in sorted.into_iter().enumerate() {
                if idx > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key)?);
                out.push(':');
                out.push_str(&canonical_json(value)?);
            }
            out.push('}');
            Ok(out)
        }
    }
}

fn is_self_test_case(incident_id: Option<&str>, title: Option<&str>) -> bool {
    incident_id
        .unwrap_or("")
        .to_lowercase()
        .contains("|self_test|")
        || title
            .unwrap_or("")
            .to_lowercase()
            .starts_with("dlp self_test")
}

fn split_url(url: &str) -> (String, BTreeMap<String, String>) {
    let (path, query) = url.split_once('?').unwrap_or((url, ""));
    let mut params = BTreeMap::new();
    for pair in query.split('&').filter(|item| !item.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        params.insert(url_decode(key), url_decode(value));
    }
    (path.to_string(), params)
}

fn path_segments(path: &str) -> Vec<&str> {
    path.trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn query_limit(query: &BTreeMap<String, String>, default: i64, max: i64) -> i64 {
    query
        .get("limit")
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
        .clamp(1, max)
}

fn read_json_body(request: &mut Request) -> Result<Value> {
    let mut body = String::new();
    request
        .as_reader()
        .read_to_string(&mut body)
        .context("read request body")?;
    if body.trim().is_empty() {
        Ok(json!({}))
    } else {
        serde_json::from_str(&body).context("decode JSON body")
    }
}

fn json_response(status: StatusCode, payload: Value) -> Response<std::io::Cursor<Vec<u8>>> {
    let body =
        serde_json::to_vec_pretty(&payload).unwrap_or_else(|_| b"{\"detail\":\"json\"}".to_vec());
    let mut response = Response::from_data(body).with_status_code(status);
    for (key, value) in [
        ("Content-Type", "application/json; charset=utf-8"),
        ("Access-Control-Allow-Origin", "*"),
        ("Access-Control-Allow-Methods", "*"),
        ("Access-Control-Allow-Headers", "*"),
    ] {
        response.add_header(
            Header::from_bytes(key.as_bytes(), value.as_bytes()).expect("valid header"),
        );
    }
    response
}

fn parse_id(value: &str) -> Result<i64> {
    value
        .parse::<i64>()
        .with_context(|| format!("invalid case id: {value}"))
}

fn required_string(value: Option<&Value>, field: &str) -> Result<String> {
    value
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("{field} is required"))
}

fn optional_string(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(text)) => Some(text.clone()),
        _ => None,
    }
}

fn optional_json_string(value: Option<&Value>) -> Value {
    optional_string(value)
        .map(Value::String)
        .unwrap_or(Value::Null)
}

fn validate_len(field: &str, value: &str, min: usize, max: usize) -> Result<()> {
    let len = value.chars().count();
    if len < min || len > max {
        bail!("{field} length must be {min}..{max}");
    }
    Ok(())
}

fn validate_optional_len(field: &str, value: Option<&str>, max: usize) -> Result<()> {
    if let Some(value) = value {
        if value.chars().count() > max {
            bail!("{field} length must be <= {max}");
        }
    }
    Ok(())
}

fn json_field(raw: Option<String>) -> Value {
    raw.as_deref()
        .and_then(|text| serde_json::from_str(text).ok())
        .unwrap_or(Value::Null)
}

fn utc_now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Micros, false)
}

fn normalize_host(value: Option<&str>) -> String {
    value.unwrap_or("").trim().to_lowercase()
}

fn url_decode(value: &str) -> String {
    decode(value)
        .map(|value| value.into_owned())
        .unwrap_or_else(|_| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_test_detection_matches_python() {
        assert!(is_self_test_case(Some("x|self_test|y"), Some("normal")));
        assert!(is_self_test_case(None, Some("DLP self_test probe")));
        assert!(!is_self_test_case(Some("incident"), Some("DLP incident")));
    }

    #[test]
    fn evidence_checksum_matches_python_fixture() {
        let value = json!({"b": 2, "a": "тест"});
        assert_eq!(
            evidence_sha256(&value),
            "350682d4ca349bdb90a337bf891720f43e85fad66e7811637a72f96b3f29984a"
        );
    }
}
