from __future__ import annotations

import hashlib
import json
import sqlite3
from contextlib import contextmanager
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterator


def utc_now() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def canonical_policy_json(policy: dict[str, Any]) -> str:
    return json.dumps(policy, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def checksum_policy(policy: dict[str, Any]) -> str:
    return hashlib.sha256(canonical_policy_json(policy).encode("utf-8")).hexdigest()


class PolicyStorage:
    def __init__(self, db_path: str) -> None:
        self.db_path = Path(db_path)
        self.db_path.parent.mkdir(parents=True, exist_ok=True)
        self._init_schema()

    @contextmanager
    def connect(self) -> Iterator[sqlite3.Connection]:
        conn = sqlite3.connect(self.db_path)
        conn.row_factory = sqlite3.Row
        try:
            yield conn
            conn.commit()
        finally:
            conn.close()

    def _init_schema(self) -> None:
        with self.connect() as conn:
            conn.executescript(
                """
                PRAGMA journal_mode=WAL;

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
                """
            )
            cols = [r["name"] for r in conn.execute("PRAGMA table_info(policies)").fetchall()]
            if "status" not in cols:
                conn.execute("ALTER TABLE policies ADD COLUMN status TEXT NOT NULL DEFAULT 'draft'")

    def _audit(
        self,
        conn: sqlite3.Connection,
        policy_id: int | None,
        action: str,
        actor: str | None,
        comment: str | None = None,
        details: dict[str, Any] | None = None,
    ) -> None:
        conn.execute(
            """
            INSERT INTO policy_audit(policy_id, action, actor, comment, details_json, created_at)
            VALUES(?, ?, ?, ?, ?, ?)
            """,
            (
                policy_id,
                action,
                actor,
                comment,
                canonical_policy_json(details) if details is not None else None,
                utc_now(),
            ),
        )

    def list_policies(self) -> list[dict[str, Any]]:
        with self.connect() as conn:
            rows = conn.execute(
                """
                SELECT id, name, description, status, is_active, current_version, checksum, created_at, updated_at
                FROM policies
                ORDER BY is_active DESC, updated_at DESC, id DESC
                """
            ).fetchall()
        return [dict(row) for row in rows]

    def get_policy(self, policy_id: int) -> dict[str, Any] | None:
        with self.connect() as conn:
            policy_row = conn.execute(
                """
                SELECT id, name, description, status, is_active, current_version, checksum, created_at, updated_at
                FROM policies
                WHERE id = ?
                """,
                (policy_id,),
            ).fetchone()
            if not policy_row:
                return None

            version_row = conn.execute(
                """
                SELECT version, policy_json, checksum, created_at, created_by
                FROM policy_versions
                WHERE policy_id = ? AND version = ?
                """,
                (policy_id, policy_row["current_version"]),
            ).fetchone()
            if not version_row:
                return None

        result = dict(policy_row)
        result["policy"] = json.loads(version_row["policy_json"])
        result["version_created_at"] = version_row["created_at"]
        result["version_created_by"] = version_row["created_by"]
        return result

    def get_active_policy(self) -> dict[str, Any] | None:
        with self.connect() as conn:
            row = conn.execute("SELECT id FROM policies WHERE is_active = 1 ORDER BY updated_at DESC LIMIT 1").fetchone()
            if not row:
                return None
        return self.get_policy(int(row["id"]))

    def create_policy(self, name: str, description: str | None, policy: dict[str, Any], activate: bool, actor: str | None) -> dict[str, Any]:
        checksum = checksum_policy(policy)
        now = utc_now()
        status = "deployed" if activate else "draft"
        policy_json = canonical_policy_json(policy)
        with self.connect() as conn:
            if activate:
                conn.execute("UPDATE policies SET is_active = 0")
            cursor = conn.execute(
                """
                INSERT INTO policies(name, description, status, is_active, current_version, checksum, created_at, updated_at)
                VALUES(?, ?, ?, ?, 1, ?, ?, ?)
                """,
                (name, description, status, 1 if activate else 0, checksum, now, now),
            )
            policy_id = int(cursor.lastrowid)
            conn.execute(
                """
                INSERT INTO policy_versions(policy_id, version, policy_json, checksum, created_at, created_by, rollback_of_version)
                VALUES(?, 1, ?, ?, ?, ?, NULL)
                """,
                (policy_id, policy_json, checksum, now, actor),
            )
            self._audit(conn, policy_id, "create", actor, details={"activate": activate, "status": status})
        return self.get_policy(policy_id)  # type: ignore[return-value]

    def update_policy(
        self,
        policy_id: int,
        name: str | None,
        description: str | None,
        policy: dict[str, Any] | None,
        activate: bool,
        actor: str | None,
    ) -> dict[str, Any] | None:
        current = self.get_policy(policy_id)
        if not current:
            return None

        with self.connect() as conn:
            new_name = name if name is not None else current["name"]
            new_description = description if description is not None else current["description"]
            new_version = int(current["current_version"])
            new_checksum = current["checksum"]
            new_status = current.get("status", "draft")

            if policy is not None:
                new_version += 1
                new_checksum = checksum_policy(policy)
                new_status = "draft"
                policy_json = canonical_policy_json(policy)
                conn.execute(
                    """
                    INSERT INTO policy_versions(policy_id, version, policy_json, checksum, created_at, created_by, rollback_of_version)
                    VALUES(?, ?, ?, ?, ?, ?, NULL)
                    """,
                    (policy_id, new_version, policy_json, new_checksum, utc_now(), actor),
                )

            if activate:
                conn.execute("UPDATE policies SET is_active = 0")
                new_status = "deployed"

            conn.execute(
                """
                UPDATE policies
                SET name = ?, description = ?, status = ?, is_active = ?, current_version = ?, checksum = ?, updated_at = ?
                WHERE id = ?
                """,
                (
                    new_name,
                    new_description,
                    new_status,
                    1 if activate else current["is_active"],
                    new_version,
                    new_checksum,
                    utc_now(),
                    policy_id,
                ),
            )
            self._audit(conn, policy_id, "update", actor, details={"activate": activate, "status": new_status})
        return self.get_policy(policy_id)

    def activate_policy(self, policy_id: int, actor: str | None) -> dict[str, Any] | None:
        current = self.get_policy(policy_id)
        if not current:
            return None
        if current.get("status") != "approved":
            raise ValueError("policy must be approved before deploy")

        with self.connect() as conn:
            conn.execute("UPDATE policies SET is_active = 0")
            conn.execute(
                "UPDATE policies SET status = 'deployed', is_active = 1, updated_at = ? WHERE id = ?",
                (utc_now(), policy_id),
            )
            self._audit(conn, policy_id, "deploy", actor)
        return self.get_policy(policy_id)

    def rollback_active_policy(self, actor: str | None) -> dict[str, Any] | None:
        active = self.get_active_policy()
        if not active:
            return None

        with self.connect() as conn:
            rows = conn.execute(
                """
                SELECT version, policy_json
                FROM policy_versions
                WHERE policy_id = ?
                ORDER BY version DESC
                LIMIT 2
                """,
                (active["id"],),
            ).fetchall()
            if len(rows) < 2:
                return active

            previous_version = int(rows[1]["version"])
            previous_policy = json.loads(rows[1]["policy_json"])
            rollback_version = int(active["current_version"]) + 1
            rollback_checksum = checksum_policy(previous_policy)
            now = utc_now()

            conn.execute(
                """
                INSERT INTO policy_versions(policy_id, version, policy_json, checksum, created_at, created_by, rollback_of_version)
                VALUES(?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    active["id"],
                    rollback_version,
                    canonical_policy_json(previous_policy),
                    rollback_checksum,
                    now,
                    actor,
                    previous_version,
                ),
            )
            conn.execute(
                """
                UPDATE policies
                SET status = 'draft', current_version = ?, checksum = ?, updated_at = ?
                WHERE id = ?
                """,
                (rollback_version, rollback_checksum, now, active["id"]),
            )
            self._audit(conn, int(active["id"]), "rollback", actor, details={"rollback_to": previous_version})
        return self.get_policy(int(active["id"]))

    def delete_policy(self, policy_id: int) -> bool:
        current = self.get_policy(policy_id)
        if not current:
            return False
        if current["is_active"]:
            raise ValueError("cannot delete active policy")

        with self.connect() as conn:
            self._audit(conn, policy_id, "delete", None)
            conn.execute("DELETE FROM policy_versions WHERE policy_id = ?", (policy_id,))
            conn.execute("DELETE FROM policies WHERE id = ?", (policy_id,))
        return True

    def set_policy_status(self, policy_id: int, status: str, actor: str | None, comment: str | None = None) -> dict[str, Any] | None:
        current = self.get_policy(policy_id)
        if not current:
            return None
        allowed = {"draft", "pending_approval", "approved", "deployed"}
        if status not in allowed:
            raise ValueError(f"unsupported status: {status}")
        with self.connect() as conn:
            conn.execute(
                "UPDATE policies SET status = ?, updated_at = ? WHERE id = ?",
                (status, utc_now(), policy_id),
            )
            self._audit(conn, policy_id, "status_change", actor, comment=comment, details={"status": status})
        return self.get_policy(policy_id)

    def list_audit(self, policy_id: int | None = None, limit: int = 200) -> list[dict[str, Any]]:
        query = """
            SELECT id, policy_id, action, actor, comment, details_json, created_at
            FROM policy_audit
        """
        params: tuple[Any, ...]
        if policy_id is None:
            query += " ORDER BY id DESC LIMIT ?"
            params = (limit,)
        else:
            query += " WHERE policy_id = ? ORDER BY id DESC LIMIT ?"
            params = (policy_id, limit)
        with self.connect() as conn:
            rows = conn.execute(query, params).fetchall()
        items: list[dict[str, Any]] = []
        for row in rows:
            d = dict(row)
            if d.get("details_json"):
                try:
                    d["details"] = json.loads(d["details_json"])
                except Exception:
                    d["details"] = None
            else:
                d["details"] = None
            d.pop("details_json", None)
            items.append(d)
        return items
