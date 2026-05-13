#!/usr/bin/env python3
from __future__ import annotations

import json
import sqlite3
from contextlib import contextmanager
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterator


class CaseStorage:
    def __init__(self, db_path: Path) -> None:
        self.db_path = db_path
        self.db_path.parent.mkdir(parents=True, exist_ok=True)
        self._init_db()

    @contextmanager
    def conn(self) -> Iterator[sqlite3.Connection]:
        c = sqlite3.connect(self.db_path)
        c.row_factory = sqlite3.Row
        c.execute("PRAGMA journal_mode=WAL")
        c.execute("PRAGMA foreign_keys=ON")
        try:
            yield c
        finally:
            c.close()

    def _init_db(self) -> None:
        with self.conn() as c:
            c.executescript(
                """
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
                """
            )
            c.commit()

    @staticmethod
    def _now() -> str:
        return datetime.now(timezone.utc).isoformat()

    @staticmethod
    def _to_case_dict(row: sqlite3.Row) -> dict[str, Any]:
        evidence = None
        if row["evidence_json"]:
            try:
                evidence = json.loads(row["evidence_json"])
            except Exception:
                evidence = None
        return {
            "id": int(row["id"]),
            "incident_id": row["incident_id"],
            "host": row["host"],
            "title": row["title"],
            "severity": row["severity"],
            "assignee": row["assignee"],
            "status": row["status"],
            "source_bucket": row["source_bucket"],
            "source_event_ts": row["source_event_ts"],
            "evidence": evidence,
            "created_at": row["created_at"],
            "updated_at": row["updated_at"],
        }

    def create_case(self, payload: dict[str, Any], actor: str | None = None) -> dict[str, Any]:
        now = self._now()
        with self.conn() as c:
            cur = c.execute(
                """
                INSERT INTO cases (
                  incident_id, host, title, severity, assignee, status,
                  source_bucket, source_event_ts, evidence_json, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, 'open', ?, ?, ?, ?, ?)
                """,
                (
                    payload["incident_id"],
                    payload.get("host"),
                    payload["title"],
                    payload.get("severity", "medium"),
                    payload.get("assignee"),
                    payload.get("source_bucket"),
                    payload.get("source_event_ts"),
                    json.dumps(payload.get("evidence"), ensure_ascii=False) if payload.get("evidence") is not None else None,
                    now,
                    now,
                ),
            )
            case_id = int(cur.lastrowid)
            self._insert_audit(
                c,
                case_id=case_id,
                action="create",
                actor=actor,
                details={"fields": {k: v for k, v in payload.items() if k != "evidence"}},
            )
            c.commit()
            return self.get_case(case_id, c)

    def list_cases(self, status: str | None = None, host: str | None = None, limit: int = 200) -> list[dict[str, Any]]:
        q = "SELECT * FROM cases"
        clauses = []
        args: list[Any] = []
        if status:
            clauses.append("status = ?")
            args.append(status)
        if host:
            clauses.append("host = ?")
            args.append(host)
        if clauses:
            q += " WHERE " + " AND ".join(clauses)
        q += " ORDER BY id DESC LIMIT ?"
        args.append(int(limit))
        with self.conn() as c:
            rows = c.execute(q, args).fetchall()
            return [self._to_case_dict(r) for r in rows]

    def get_case(self, case_id: int, c: sqlite3.Connection | None = None) -> dict[str, Any]:
        own = False
        if c is None:
            own = True
            c = sqlite3.connect(self.db_path)
            c.row_factory = sqlite3.Row
        try:
            row = c.execute("SELECT * FROM cases WHERE id = ?", (int(case_id),)).fetchone()
            if not row:
                raise KeyError(case_id)
            return self._to_case_dict(row)
        finally:
            if own:
                c.close()

    def update_case(self, case_id: int, patch: dict[str, Any], actor: str | None = None) -> dict[str, Any]:
        fields = []
        args: list[Any] = []
        for key in ("status", "assignee", "title", "severity"):
            if key in patch and patch[key] is not None:
                fields.append(f"{key} = ?")
                args.append(patch[key])
        if not fields:
            return self.get_case(case_id)
        fields.append("updated_at = ?")
        args.append(self._now())
        args.append(int(case_id))
        with self.conn() as c:
            c.execute(f"UPDATE cases SET {', '.join(fields)} WHERE id = ?", args)
            self._insert_audit(c, case_id=case_id, action="update", actor=actor, details=patch)
            c.commit()
            return self.get_case(case_id, c)

    def add_comment(self, case_id: int, comment: str, author: str | None = None) -> dict[str, Any]:
        now = self._now()
        with self.conn() as c:
            cur = c.execute(
                "INSERT INTO case_comments (case_id, comment, author, created_at) VALUES (?, ?, ?, ?)",
                (int(case_id), comment, author, now),
            )
            cid = int(cur.lastrowid)
            self._insert_audit(
                c,
                case_id=case_id,
                action="comment",
                actor=author,
                details={"comment_id": cid},
            )
            c.commit()
            row = c.execute("SELECT id, case_id, comment, author, created_at FROM case_comments WHERE id = ?", (cid,)).fetchone()
            return dict(row)

    def list_comments(self, case_id: int, limit: int = 200) -> list[dict[str, Any]]:
        with self.conn() as c:
            rows = c.execute(
                "SELECT id, case_id, comment, author, created_at FROM case_comments WHERE case_id = ? ORDER BY id DESC LIMIT ?",
                (int(case_id), int(limit)),
            ).fetchall()
            return [dict(r) for r in rows]

    def list_audit(self, case_id: int, limit: int = 200) -> list[dict[str, Any]]:
        with self.conn() as c:
            rows = c.execute(
                "SELECT id, case_id, action, actor, details_json, created_at FROM case_audit WHERE case_id = ? ORDER BY id DESC LIMIT ?",
                (int(case_id), int(limit)),
            ).fetchall()
            out: list[dict[str, Any]] = []
            for r in rows:
                details = None
                if r["details_json"]:
                    try:
                        details = json.loads(r["details_json"])
                    except Exception:
                        details = None
                out.append(
                    {
                        "id": int(r["id"]),
                        "case_id": int(r["case_id"]),
                        "action": r["action"],
                        "actor": r["actor"],
                        "details": details,
                        "created_at": r["created_at"],
                    }
                )
            return out

    def _insert_audit(
        self,
        c: sqlite3.Connection,
        case_id: int,
        action: str,
        actor: str | None,
        details: dict[str, Any] | None = None,
    ) -> None:
        c.execute(
            "INSERT INTO case_audit (case_id, action, actor, details_json, created_at) VALUES (?, ?, ?, ?, ?)",
            (
                int(case_id),
                action,
                actor,
                json.dumps(details, ensure_ascii=False) if details is not None else None,
                self._now(),
            ),
        )

