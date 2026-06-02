use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow, bail};
use chrono::{SecondsFormat, Utc};
use clap::Parser;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};
use urlencoding::decode;

const APP_NAME: &str = "aw-dlp-policy-engine";
const APP_VERSION: &str = "0.1.0";

#[derive(Debug, Parser)]
#[command(about = "AW DLP Policy Engine")]
struct Cli {
    #[arg(long, default_value = "127.0.0.1")]
    bind_host: String,

    #[arg(long, default_value_t = 5601)]
    port: u16,

    #[arg(
        long,
        default_value = "/var/lib/activitywatch/dlp-policy-engine.sqlite"
    )]
    db_path: PathBuf,
}

#[derive(Clone)]
struct AppState {
    db_path: PathBuf,
    agents: Arc<Mutex<HashMap<String, Value>>>,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = apply_env(Cli::parse());
    let storage = PolicyStorage::new(cli.db_path.clone())?;
    storage.init_schema()?;
    let state = AppState {
        db_path: cli.db_path,
        agents: Arc::new(Mutex::new(HashMap::new())),
    };
    let address = format!("{}:{}", cli.bind_host, cli.port);
    let server = Server::http(&address).map_err(|err| anyhow!("bind {address}: {err}"))?;
    eprintln!("{APP_NAME} {APP_VERSION} listening on {address}");
    for request in server.incoming_requests() {
        if let Err(err) = handle_request(&state, request) {
            eprintln!("request error: {err:#}");
        }
    }
    Ok(())
}

fn apply_env(mut cli: Cli) -> Cli {
    if !cli_arg_present("--bind-host") {
        if let Ok(value) = std::env::var("AW_DLP_POLICY_ENGINE_BIND_HOST") {
            if !value.is_empty() {
                cli.bind_host = value;
            }
        }
    }
    if !cli_arg_present("--port") {
        if let Ok(value) = std::env::var("AW_DLP_POLICY_ENGINE_PORT") {
            if let Ok(port) = value.parse() {
                cli.port = port;
            }
        }
    }
    if !cli_arg_present("--db-path") {
        if let Ok(value) = std::env::var("AW_DLP_POLICY_ENGINE_DB_PATH") {
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
    let storage = PolicyStorage::new(state.db_path.clone())?;

    let response = match (method, segments.as_slice()) {
        (Method::Get, ["healthz"]) => json_response(
            StatusCode(200),
            json!({
                "status": "ok",
                "service": APP_NAME,
                "db_path": state.db_path.to_string_lossy(),
                "db_exists": state.db_path.exists().to_string(),
            }),
        ),
        (Method::Get, ["api", "0", "dlp", "policies"]) => {
            json_response(StatusCode(200), json!({"items": storage.list_policies()?}))
        }
        (Method::Post, ["api", "0", "dlp", "policies"]) => {
            let payload = read_json_body(&mut request)?;
            match storage.create_policy(&payload) {
                Ok(item) => json_response(StatusCode(201), json!({"item": item})),
                Err(err) => json_response(StatusCode(400), json!({"detail": err.to_string()})),
            }
        }
        (Method::Get, ["api", "0", "dlp", "policies", "active"]) => {
            match storage.get_active_policy()? {
                Some(item) => json_response(StatusCode(200), build_policy_bundle(&item)),
                None => json_response(
                    StatusCode(404),
                    json!({"detail": "no active policy configured"}),
                ),
            }
        }
        (Method::Get, ["api", "0", "dlp", "policies", "active", "version"]) => {
            match storage.get_active_policy()? {
                Some(item) => json_response(
                    StatusCode(200),
                    json!({
                        "active": true,
                        "policyId": item.get("id").cloned().unwrap_or(Value::Null),
                        "version": item.get("current_version").cloned().unwrap_or(Value::Null),
                        "checksum": item.get("checksum").cloned().unwrap_or(Value::Null),
                        "updatedAtUtc": item.get("updated_at").cloned().unwrap_or(Value::Null),
                    }),
                ),
                None => json_response(
                    StatusCode(404),
                    json!({"detail": "no active policy configured"}),
                ),
            }
        }
        (
            Method::Post,
            [
                "api",
                "0",
                "dlp",
                "policies",
                "agents",
                agent_id,
                "heartbeat",
            ],
        ) => {
            let payload = read_json_body(&mut request)?;
            let agent = agent_heartbeat(agent_id, &payload);
            state
                .agents
                .lock()
                .map_err(|_| anyhow!("agent state lock poisoned"))?
                .insert(agent_id.to_string(), agent.clone());
            json_response(StatusCode(200), json!({"ok": true, "agent": agent}))
        }
        (Method::Get, ["api", "0", "dlp", "policies", "agents", agent_id, "desired"]) => {
            match storage.get_active_policy()? {
                Some(item) => {
                    let current = state
                        .agents
                        .lock()
                        .map_err(|_| anyhow!("agent state lock poisoned"))?
                        .get(*agent_id)
                        .cloned()
                        .unwrap_or_else(|| json!({}));
                    json_response(StatusCode(200), desired_policy(agent_id, &current, &item))
                }
                None => json_response(
                    StatusCode(404),
                    json!({"detail": "no active policy configured"}),
                ),
            }
        }
        (Method::Post, ["api", "0", "dlp", "policies", "rollback"]) => {
            let payload = read_json_body(&mut request)?;
            match storage.rollback_active_policy(actor_from(&payload))? {
                Some(item) => json_response(StatusCode(200), json!({"item": item})),
                None => json_response(
                    StatusCode(404),
                    json!({"detail": "no active policy configured"}),
                ),
            }
        }
        (Method::Get, ["api", "0", "dlp", "policies", "audit"]) => {
            let limit = query_limit(&query);
            json_response(
                StatusCode(200),
                json!({"items": storage.list_audit(None, limit)?}),
            )
        }
        (Method::Get, ["api", "0", "dlp", "policies", policy_id]) => {
            match parse_id(policy_id).and_then(|id| storage.get_policy(id))? {
                Some(item) => json_response(StatusCode(200), json!({"item": item})),
                None => json_response(StatusCode(404), json!({"detail": "policy not found"})),
            }
        }
        (Method::Put, ["api", "0", "dlp", "policies", policy_id]) => {
            let payload = read_json_body(&mut request)?;
            let policy_id = parse_id(policy_id)?;
            match storage.update_policy(policy_id, &payload) {
                Ok(Some(item)) => json_response(StatusCode(200), json!({"item": item})),
                Ok(None) => json_response(StatusCode(404), json!({"detail": "policy not found"})),
                Err(err) => json_response(StatusCode(400), json!({"detail": err.to_string()})),
            }
        }
        (Method::Post, ["api", "0", "dlp", "policies", policy_id, "activate"]) => {
            let payload = read_json_body(&mut request)?;
            let policy_id = parse_id(policy_id)?;
            match storage.activate_policy(policy_id, actor_from(&payload)) {
                Ok(Some(item)) => json_response(StatusCode(200), json!({"item": item})),
                Ok(None) => json_response(StatusCode(404), json!({"detail": "policy not found"})),
                Err(err) => json_response(StatusCode(400), json!({"detail": err.to_string()})),
            }
        }
        (Method::Post, ["api", "0", "dlp", "policies", policy_id, action])
            if matches!(*action, "submit" | "approve" | "draft") =>
        {
            let payload = read_json_body(&mut request)?;
            let policy_id = parse_id(policy_id)?;
            let status = match *action {
                "submit" => "pending_approval",
                "approve" => "approved",
                "draft" => "draft",
                _ => unreachable!(),
            };
            match storage.set_policy_status(
                policy_id,
                status,
                actor_from(&payload),
                optional_string(payload.get("comment")),
            ) {
                Ok(Some(item)) => json_response(StatusCode(200), json!({"item": item})),
                Ok(None) => json_response(StatusCode(404), json!({"detail": "policy not found"})),
                Err(err) => json_response(StatusCode(400), json!({"detail": err.to_string()})),
            }
        }
        (Method::Delete, ["api", "0", "dlp", "policies", policy_id]) => {
            let policy_id = parse_id(policy_id)?;
            match storage.delete_policy(policy_id) {
                Ok(true) => json_response(StatusCode(200), json!({"deleted": true})),
                Ok(false) => json_response(StatusCode(404), json!({"detail": "policy not found"})),
                Err(err) => json_response(StatusCode(409), json!({"detail": err.to_string()})),
            }
        }
        (Method::Get, ["api", "0", "dlp", "policies", policy_id, "audit"]) => {
            let policy_id = parse_id(policy_id)?;
            if storage.get_policy(policy_id)?.is_none() {
                json_response(StatusCode(404), json!({"detail": "policy not found"}))
            } else {
                let limit = query_limit(&query);
                json_response(
                    StatusCode(200),
                    json!({"items": storage.list_audit(Some(policy_id), limit)?}),
                )
            }
        }
        _ => json_response(StatusCode(404), json!({"detail": "not found"})),
    };
    request.respond(response).context("send HTTP response")
}

struct PolicyStorage {
    db_path: PathBuf,
}

impl PolicyStorage {
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
        Ok(conn)
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.connect()?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS policies (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                description TEXT,
                status TEXT NOT NULL DEFAULT 'draft',
                is_active INTEGER NOT NULL DEFAULT 0,
                current_version INTEGER NOT NULL DEFAULT 1,
                checksum TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS policy_versions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                policy_id INTEGER NOT NULL,
                version INTEGER NOT NULL,
                policy_json TEXT NOT NULL,
                checksum TEXT NOT NULL,
                created_at TEXT NOT NULL,
                created_by TEXT,
                rollback_of_version INTEGER,
                FOREIGN KEY(policy_id) REFERENCES policies(id),
                UNIQUE(policy_id, version)
            );

            CREATE TABLE IF NOT EXISTS policy_audit (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                policy_id INTEGER,
                action TEXT NOT NULL,
                actor TEXT,
                comment TEXT,
                details_json TEXT,
                created_at TEXT NOT NULL,
                FOREIGN KEY(policy_id) REFERENCES policies(id)
            );

            CREATE INDEX IF NOT EXISTS idx_policies_active ON policies(is_active);
            CREATE INDEX IF NOT EXISTS idx_policy_versions_policy ON policy_versions(policy_id, version DESC);
            CREATE INDEX IF NOT EXISTS idx_policy_audit_policy ON policy_audit(policy_id, id DESC);
            "#,
        )?;
        let has_status = {
            let mut stmt = conn.prepare("PRAGMA table_info(policies)")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
            let mut found = false;
            for row in rows {
                if row? == "status" {
                    found = true;
                    break;
                }
            }
            found
        };
        if !has_status {
            conn.execute(
                "ALTER TABLE policies ADD COLUMN status TEXT NOT NULL DEFAULT 'draft'",
                [],
            )?;
        }
        Ok(())
    }

    fn list_policies(&self) -> Result<Vec<Value>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT id, name, description, status, is_active, current_version, checksum, created_at, updated_at
            FROM policies
            ORDER BY is_active DESC, updated_at DESC, id DESC
            "#,
        )?;
        let rows = stmt.query_map([], policy_summary_from_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    fn get_policy(&self, policy_id: i64) -> Result<Option<Value>> {
        let conn = self.connect()?;
        let policy = conn
            .query_row(
                r#"
                SELECT id, name, description, status, is_active, current_version, checksum, created_at, updated_at
                FROM policies
                WHERE id = ?
                "#,
                [policy_id],
                policy_summary_from_row,
            )
            .optional()?;
        let Some(mut policy) = policy else {
            return Ok(None);
        };
        let current_version = policy
            .get("current_version")
            .and_then(Value::as_i64)
            .ok_or_else(|| anyhow!("policy current_version missing"))?;
        let version = conn
            .query_row(
                r#"
                SELECT version, policy_json, checksum, created_at, created_by
                FROM policy_versions
                WHERE policy_id = ? AND version = ?
                "#,
                params![policy_id, current_version],
                |row| {
                    Ok((
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                    ))
                },
            )
            .optional()?;
        let Some((policy_json, version_created_at, version_created_by)) = version else {
            return Ok(None);
        };
        let policy_doc: Value = serde_json::from_str(&policy_json).context("decode policy_json")?;
        let obj = policy
            .as_object_mut()
            .ok_or_else(|| anyhow!("policy row is not object"))?;
        obj.insert("policy".to_string(), policy_doc);
        obj.insert(
            "version_created_at".to_string(),
            Value::String(version_created_at),
        );
        obj.insert(
            "version_created_by".to_string(),
            version_created_by.map(Value::String).unwrap_or(Value::Null),
        );
        Ok(Some(policy))
    }

    fn get_active_policy(&self) -> Result<Option<Value>> {
        let conn = self.connect()?;
        let id = conn
            .query_row(
                "SELECT id FROM policies WHERE is_active = 1 ORDER BY updated_at DESC LIMIT 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        match id {
            Some(id) => self.get_policy(id),
            None => Ok(None),
        }
    }

    fn create_policy(&self, payload: &Value) -> Result<Value> {
        let name = required_string(payload.get("name"), "name")?;
        validate_name(&name)?;
        let description = optional_string(payload.get("description"));
        validate_description(description.as_deref())?;
        let policy = payload
            .get("policy")
            .cloned()
            .ok_or_else(|| anyhow!("policy is required"))?;
        let activate = payload
            .get("activate")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let actor = actor_from(payload);
        let checksum = checksum_policy(&policy);
        let policy_json = canonical_json(&policy)?;
        let now = utc_now();
        let status = if activate { "deployed" } else { "draft" };
        let conn = self.connect()?;
        let tx = conn.unchecked_transaction()?;
        if activate {
            tx.execute("UPDATE policies SET is_active = 0", [])?;
        }
        tx.execute(
            r#"
            INSERT INTO policies(name, description, status, is_active, current_version, checksum, created_at, updated_at)
            VALUES(?, ?, ?, ?, 1, ?, ?, ?)
            "#,
            params![
                name,
                description,
                status,
                if activate { 1 } else { 0 },
                checksum,
                now,
                now
            ],
        )?;
        let policy_id = tx.last_insert_rowid();
        tx.execute(
            r#"
            INSERT INTO policy_versions(policy_id, version, policy_json, checksum, created_at, created_by, rollback_of_version)
            VALUES(?, 1, ?, ?, ?, ?, NULL)
            "#,
            params![policy_id, policy_json, checksum, now, actor],
        )?;
        audit(
            &tx,
            Some(policy_id),
            "create",
            actor.as_deref(),
            None,
            Some(json!({"activate": activate, "status": status})),
        )?;
        tx.commit()?;
        self.get_policy(policy_id)?
            .ok_or_else(|| anyhow!("created policy not found"))
    }

    fn update_policy(&self, policy_id: i64, payload: &Value) -> Result<Option<Value>> {
        let current = match self.get_policy(policy_id)? {
            Some(item) => item,
            None => return Ok(None),
        };
        let name = match payload.get("name") {
            Some(Value::Null) | None => current
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            Some(value) => {
                let name = required_string(Some(value), "name")?;
                validate_name(&name)?;
                name
            }
        };
        let description = match payload.get("description") {
            Some(value) => optional_string(Some(value)),
            None => optional_string(current.get("description")),
        };
        validate_description(description.as_deref())?;
        let activate = payload
            .get("activate")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let actor = actor_from(payload);
        let mut new_version = current
            .get("current_version")
            .and_then(Value::as_i64)
            .ok_or_else(|| anyhow!("current_version missing"))?;
        let mut new_checksum = required_string(current.get("checksum"), "checksum")?;
        let mut new_status = current
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("draft")
            .to_string();
        let conn = self.connect()?;
        let tx = conn.unchecked_transaction()?;
        if let Some(policy) = payload.get("policy").filter(|value| !value.is_null()) {
            new_version += 1;
            new_checksum = checksum_policy(policy);
            new_status = "draft".to_string();
            tx.execute(
                r#"
                INSERT INTO policy_versions(policy_id, version, policy_json, checksum, created_at, created_by, rollback_of_version)
                VALUES(?, ?, ?, ?, ?, ?, NULL)
                "#,
                params![
                    policy_id,
                    new_version,
                    canonical_json(policy)?,
                    new_checksum,
                    utc_now(),
                    actor
                ],
            )?;
        }
        if activate {
            tx.execute("UPDATE policies SET is_active = 0", [])?;
            new_status = "deployed".to_string();
        }
        tx.execute(
            r#"
            UPDATE policies
            SET name = ?, description = ?, status = ?, is_active = ?, current_version = ?, checksum = ?, updated_at = ?
            WHERE id = ?
            "#,
            params![
                name,
                description,
                new_status,
                if activate {
                    1
                } else {
                    current
                        .get("is_active")
                        .and_then(Value::as_i64)
                        .unwrap_or(0)
                },
                new_version,
                new_checksum,
                utc_now(),
                policy_id,
            ],
        )?;
        audit(
            &tx,
            Some(policy_id),
            "update",
            actor.as_deref(),
            None,
            Some(json!({"activate": activate, "status": new_status})),
        )?;
        tx.commit()?;
        self.get_policy(policy_id)
    }

    fn activate_policy(&self, policy_id: i64, actor: Option<String>) -> Result<Option<Value>> {
        let current = match self.get_policy(policy_id)? {
            Some(item) => item,
            None => return Ok(None),
        };
        if current.get("status").and_then(Value::as_str) != Some("approved") {
            bail!("policy must be approved before deploy");
        }
        let conn = self.connect()?;
        let tx = conn.unchecked_transaction()?;
        tx.execute("UPDATE policies SET is_active = 0", [])?;
        tx.execute(
            "UPDATE policies SET status = 'deployed', is_active = 1, updated_at = ? WHERE id = ?",
            params![utc_now(), policy_id],
        )?;
        audit(&tx, Some(policy_id), "deploy", actor.as_deref(), None, None)?;
        tx.commit()?;
        self.get_policy(policy_id)
    }

    fn rollback_active_policy(&self, actor: Option<String>) -> Result<Option<Value>> {
        let active = match self.get_active_policy()? {
            Some(item) => item,
            None => return Ok(None),
        };
        let policy_id = active
            .get("id")
            .and_then(Value::as_i64)
            .ok_or_else(|| anyhow!("active policy id missing"))?;
        let current_version = active
            .get("current_version")
            .and_then(Value::as_i64)
            .ok_or_else(|| anyhow!("active current_version missing"))?;
        let conn = self.connect()?;
        let rows = {
            let mut stmt = conn.prepare(
                r#"
                SELECT version, policy_json
                FROM policy_versions
                WHERE policy_id = ?
                ORDER BY version DESC
                LIMIT 2
                "#,
            )?;
            stmt.query_map([policy_id], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?
        };
        if rows.len() < 2 {
            return Ok(Some(active));
        }
        let previous_version = rows[1].0;
        let previous_policy: Value = serde_json::from_str(&rows[1].1)?;
        let rollback_version = current_version + 1;
        let rollback_checksum = checksum_policy(&previous_policy);
        let now = utc_now();
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            r#"
            INSERT INTO policy_versions(policy_id, version, policy_json, checksum, created_at, created_by, rollback_of_version)
            VALUES(?, ?, ?, ?, ?, ?, ?)
            "#,
            params![
                policy_id,
                rollback_version,
                canonical_json(&previous_policy)?,
                rollback_checksum,
                now,
                actor,
                previous_version
            ],
        )?;
        tx.execute(
            r#"
            UPDATE policies
            SET status = 'draft', current_version = ?, checksum = ?, updated_at = ?
            WHERE id = ?
            "#,
            params![rollback_version, rollback_checksum, now, policy_id],
        )?;
        audit(
            &tx,
            Some(policy_id),
            "rollback",
            actor.as_deref(),
            None,
            Some(json!({"rollback_to": previous_version})),
        )?;
        tx.commit()?;
        self.get_policy(policy_id)
    }

    fn delete_policy(&self, policy_id: i64) -> Result<bool> {
        let current = match self.get_policy(policy_id)? {
            Some(item) => item,
            None => return Ok(false),
        };
        if current
            .get("is_active")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            != 0
        {
            bail!("cannot delete active policy");
        }
        let conn = self.connect()?;
        let tx = conn.unchecked_transaction()?;
        audit(&tx, Some(policy_id), "delete", None, None, None)?;
        tx.execute(
            "DELETE FROM policy_versions WHERE policy_id = ?",
            [policy_id],
        )?;
        tx.execute("DELETE FROM policies WHERE id = ?", [policy_id])?;
        tx.commit()?;
        Ok(true)
    }

    fn set_policy_status(
        &self,
        policy_id: i64,
        status: &str,
        actor: Option<String>,
        comment: Option<String>,
    ) -> Result<Option<Value>> {
        if !matches!(
            status,
            "draft" | "pending_approval" | "approved" | "deployed"
        ) {
            bail!("unsupported status: {status}");
        }
        if self.get_policy(policy_id)?.is_none() {
            return Ok(None);
        }
        validate_description(comment.as_deref())?;
        let conn = self.connect()?;
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE policies SET status = ?, updated_at = ? WHERE id = ?",
            params![status, utc_now(), policy_id],
        )?;
        audit(
            &tx,
            Some(policy_id),
            "status_change",
            actor.as_deref(),
            comment.as_deref(),
            Some(json!({"status": status})),
        )?;
        tx.commit()?;
        self.get_policy(policy_id)
    }

    fn list_audit(&self, policy_id: Option<i64>, limit: i64) -> Result<Vec<Value>> {
        let conn = self.connect()?;
        let limit = limit.clamp(1, 1000);
        let mut items = Vec::new();
        if let Some(policy_id) = policy_id {
            let mut stmt = conn.prepare(
                r#"
                SELECT id, policy_id, action, actor, comment, details_json, created_at
                FROM policy_audit
                WHERE policy_id = ?
                ORDER BY id DESC
                LIMIT ?
                "#,
            )?;
            let rows = stmt.query_map(params![policy_id, limit], audit_from_row)?;
            for row in rows {
                items.push(row?);
            }
        } else {
            let mut stmt = conn.prepare(
                r#"
                SELECT id, policy_id, action, actor, comment, details_json, created_at
                FROM policy_audit
                ORDER BY id DESC
                LIMIT ?
                "#,
            )?;
            let rows = stmt.query_map([limit], audit_from_row)?;
            for row in rows {
                items.push(row?);
            }
        }
        Ok(items)
    }
}

fn audit(
    conn: &Connection,
    policy_id: Option<i64>,
    action: &str,
    actor: Option<&str>,
    comment: Option<&str>,
    details: Option<Value>,
) -> Result<()> {
    let details_json = details
        .as_ref()
        .map(canonical_json)
        .transpose()
        .context("serialize audit details")?;
    conn.execute(
        r#"
        INSERT INTO policy_audit(policy_id, action, actor, comment, details_json, created_at)
        VALUES(?, ?, ?, ?, ?, ?)
        "#,
        params![policy_id, action, actor, comment, details_json, utc_now()],
    )?;
    Ok(())
}

fn policy_summary_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    Ok(json!({
        "id": row.get::<_, i64>(0)?,
        "name": row.get::<_, String>(1)?,
        "description": row.get::<_, Option<String>>(2)?,
        "status": row.get::<_, String>(3)?,
        "is_active": row.get::<_, i64>(4)?,
        "current_version": row.get::<_, i64>(5)?,
        "checksum": row.get::<_, String>(6)?,
        "created_at": row.get::<_, String>(7)?,
        "updated_at": row.get::<_, String>(8)?,
    }))
}

fn audit_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Value> {
    let details_json: Option<String> = row.get(5)?;
    let details = details_json
        .as_deref()
        .and_then(|text| serde_json::from_str(text).ok())
        .unwrap_or(Value::Null);
    Ok(json!({
        "id": row.get::<_, i64>(0)?,
        "policy_id": row.get::<_, Option<i64>>(1)?,
        "action": row.get::<_, String>(2)?,
        "actor": row.get::<_, Option<String>>(3)?,
        "comment": row.get::<_, Option<String>>(4)?,
        "created_at": row.get::<_, String>(6)?,
        "details": details,
    }))
}

fn build_policy_bundle(record: &Value) -> Value {
    json!({
        "active": true,
        "policyId": record.get("id").cloned().unwrap_or(Value::Null),
        "name": record.get("name").cloned().unwrap_or(Value::Null),
        "version": record.get("current_version").cloned().unwrap_or(Value::Null),
        "checksum": record.get("checksum").cloned().unwrap_or(Value::Null),
        "updatedAtUtc": record.get("updated_at").cloned().unwrap_or(Value::Null),
        "policy": record.get("policy").cloned().unwrap_or(Value::Null),
    })
}

fn desired_policy(agent_id: &str, current: &Value, active: &Value) -> Value {
    let current_version = current.get("version").cloned().unwrap_or(Value::Null);
    let current_checksum = current.get("checksum").cloned().unwrap_or(Value::Null);
    let desired_version = active
        .get("current_version")
        .cloned()
        .unwrap_or(Value::Null);
    let desired_checksum = active.get("checksum").cloned().unwrap_or(Value::Null);
    let refresh_now = value_to_string(&current_version) != value_to_string(&desired_version)
        || value_to_string(&current_checksum) != value_to_string(&desired_checksum);
    json!({
        "agentId": agent_id,
        "refreshNow": refresh_now,
        "reason": if refresh_now { "mismatch" } else { "up-to-date" },
        "current": {
            "version": current_version,
            "checksum": current_checksum,
        },
        "desired": {
            "policyId": active.get("id").cloned().unwrap_or(Value::Null),
            "version": desired_version,
            "checksum": desired_checksum,
            "updatedAtUtc": active.get("updated_at").cloned().unwrap_or(Value::Null),
        },
    })
}

fn agent_heartbeat(agent_id: &str, payload: &Value) -> Value {
    json!({
        "agentId": agent_id,
        "hostname": payload.get("hostname").and_then(Value::as_str).unwrap_or(agent_id),
        "version": payload.get("version").cloned().unwrap_or(Value::Null),
        "checksum": payload.get("checksum").cloned().unwrap_or(Value::Null),
        "updatedAtUtc": payload.get("updatedAtUtc").cloned().unwrap_or(Value::Null),
    })
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

fn query_limit(query: &BTreeMap<String, String>) -> i64 {
    query
        .get("limit")
        .and_then(|value| value.parse().ok())
        .unwrap_or(200)
        .clamp(1, 1000)
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
    response.add_header(
        Header::from_bytes(
            &b"Content-Type"[..],
            &b"application/json; charset=utf-8"[..],
        )
        .expect("valid header"),
    );
    response
}

fn parse_id(value: &str) -> Result<i64> {
    value
        .parse::<i64>()
        .with_context(|| format!("invalid policy id: {value}"))
}

fn actor_from(payload: &Value) -> Option<String> {
    match payload.get("actor") {
        None => Some("api".to_string()),
        Some(Value::Null) => None,
        Some(value) => value.as_str().map(ToOwned::to_owned),
    }
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

fn validate_name(value: &str) -> Result<()> {
    if value.is_empty() || value.chars().count() > 128 {
        bail!("name length must be 1..128");
    }
    Ok(())
}

fn validate_description(value: Option<&str>) -> Result<()> {
    if value.is_some_and(|text| text.chars().count() > 2048) {
        bail!("description/comment length must be <= 2048");
    }
    Ok(())
}

fn utc_now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn checksum_policy(policy: &Value) -> String {
    let mut hasher = Sha256::new();
    hasher.update(
        canonical_json(policy)
            .expect("policy JSON serializable")
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
            let mut first = true;
            let mut sorted = BTreeMap::new();
            for (key, value) in map {
                sorted.insert(key, value);
            }
            for (key, value) in sorted {
                if !first {
                    out.push(',');
                }
                first = false;
                out.push_str(&serde_json::to_string(key)?);
                out.push(':');
                out.push_str(&canonical_json(value)?);
            }
            out.push('}');
            Ok(out)
        }
    }
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::Null => "None".to_string(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
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
    fn canonical_json_sorts_keys_without_spaces() {
        let value = json!({"b": 2, "a": {"z": true, "m": "тест"}});
        assert_eq!(
            canonical_json(&value).unwrap(),
            r#"{"a":{"m":"тест","z":true},"b":2}"#
        );
    }

    #[test]
    fn checksum_matches_python_contract_fixture() {
        let value = json!({
            "version": 1,
            "defaults": {"enabled": true, "action": "alert"},
            "endpoint": {"clipboard": []}
        });
        assert_eq!(
            checksum_policy(&value),
            "88d9966b1517b45b009dbf8aca7260cf67b64201ea91bc85f743ab8ba88da0cf"
        );
    }

    #[test]
    fn desired_policy_detects_string_equivalent_version() {
        let current = json!({"version": "6", "checksum": "abc"});
        let active = json!({"id": 1, "current_version": 6, "checksum": "abc", "updated_at": "now"});
        assert_eq!(desired_policy("a", &current, &active)["refreshNow"], false);
    }
}
