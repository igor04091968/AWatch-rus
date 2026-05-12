#!/usr/bin/env python3
import argparse
import json
import os
import shutil
import sqlite3
from pathlib import Path


def connect(path: Path) -> sqlite3.Connection:
    connection = sqlite3.connect(str(path))
    connection.execute("PRAGMA journal_mode=WAL")
    connection.execute("PRAGMA synchronous=NORMAL")
    return connection


def bucket_key(row: sqlite3.Row) -> tuple[str, str, str, str]:
    return (
        str(row["name"]),
        str(row["type"]),
        str(row["client"]),
        str(row["hostname"]),
    )

def find_bucket_by_name(connection: sqlite3.Connection, name: str) -> int | None:
    row = connection.execute(
        "select rowid as bucketrow from buckets where name = ? order by rowid limit 1",
        (name,),
    ).fetchone()
    return int(row["bucketrow"]) if row else None


def find_bucket_by_key(connection: sqlite3.Connection, name: str, type_: str, client: str, hostname: str) -> int | None:
    row = connection.execute(
        "select rowid as bucketrow from buckets where name = ? and type = ? and client = ? and hostname = ? order by rowid limit 1",
        (name, type_, client, hostname),
    ).fetchone()
    return int(row["bucketrow"]) if row else None


def copy_sqlite_via_backup(src: Path, dst: Path) -> None:
    """Create a consistent copy of an sqlite DB using the sqlite backup API.

    This avoids corrupt/inconsistent files if the source DB is live.
    """
    # ensure parent exists
    dst.parent.mkdir(parents=True, exist_ok=True)
    # remove any existing tmp file
    if dst.exists():
        dst.unlink()
    src_conn = sqlite3.connect(str(src))
    dst_conn = sqlite3.connect(str(dst))
    try:
        # perform online backup
        src_conn.backup(dst_conn)
        dst_conn.commit()
    finally:
        try:
            src_conn.close()
        finally:
            dst_conn.close()


def ensure_parent(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)


def load_existing_events(connection: sqlite3.Connection, bucketrow: int) -> set[tuple[int, int, str]]:
    cursor = connection.execute(
        "select starttime, endtime, data from events where bucketrow = ?",
        (bucketrow,),
    )
    return {(int(start), int(end), str(data)) for start, end, data in cursor.fetchall()}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--overlay")
    args = parser.parse_args()

    base = Path(args.base)
    output = Path(args.output)
    overlay = Path(args.overlay) if args.overlay else None

    if not base.exists():
        raise SystemExit(f"Base DB not found: {base}")

    ensure_parent(output)
    tmp_output = output.with_suffix(output.suffix + ".tmp")
    # make a consistent copy of the base DB into tmp_output
    copy_sqlite_via_backup(base, tmp_output)

    dest = connect(tmp_output)
    dest.row_factory = sqlite3.Row

    inserted_buckets = 0
    inserted_events = 0

    if overlay and overlay.exists():
        source = connect(overlay)
        source.row_factory = sqlite3.Row
        try:
            source_buckets = source.execute(
                "select rowid as bucketrow, id, name, type, client, hostname, created, data_deprecated, data from buckets order by rowid"
            ).fetchall()

            dest_bucket_map = {
                bucket_key(row): row["bucketrow"]
                for row in dest.execute(
                    "select rowid as bucketrow, id, name, type, client, hostname, created, data_deprecated, data from buckets order by rowid"
                ).fetchall()
            }
            dest_id_map = {
                row["id"]: row["bucketrow"]
                for row in dest.execute(
                    "select rowid as bucketrow, id, name, type, client, hostname, created, data_deprecated, data from buckets order by rowid"
                ).fetchall()
                if row["id"]
            }

            for src_bucket in source_buckets:
                key = bucket_key(src_bucket)
                src_id = src_bucket["id"] if "id" in src_bucket.keys() else None
                dest_rowid = None
                # Prefer exact id match if available
                if src_id:
                    dest_rowid = dest_id_map.get(src_id)
                if dest_rowid is None:
                    dest_rowid = dest_bucket_map.get(key)
                if dest_rowid is None:
                    # Use UPSERT to handle UNIQUE(name) constraint gracefully
                    cursor = dest.execute(
                        """
                        INSERT INTO buckets (name, type, client, hostname, created, data_deprecated, data)
                        VALUES (?, ?, ?, ?, ?, ?, ?)
                        ON CONFLICT(name) DO UPDATE SET
                            type=excluded.type,
                            client=excluded.client,
                            hostname=excluded.hostname,
                            created=excluded.created,
                            data_deprecated=excluded.data_deprecated,
                            data=excluded.data
                        WHERE rowid = (SELECT rowid FROM buckets WHERE name = ? LIMIT 1)
                        """,
                        (
                            src_bucket["name"],
                            src_bucket["type"],
                            src_bucket["client"],
                            src_bucket["hostname"],
                            src_bucket["created"],
                            src_bucket["data_deprecated"],
                            src_bucket["data"],
                            str(src_bucket["name"]),
                        ),
                    )
                    # Get the rowid of the affected bucket (either inserted or updated)
                    cursor.execute("SELECT last_insert_rowid(), (SELECT rowid FROM buckets WHERE name = ? LIMIT 1)", 
                                   (str(src_bucket["name"]),))
                    result = cursor.fetchone()
                    dest_rowid = result[0] if result[0] != 0 else result[1]
                    
                    if dest_rowid:
                        dest_bucket_map[key] = dest_rowid
                        # Only count as inserted if it was a true insert (not update)
                        cursor.execute("SELECT changes() FROM buckets WHERE rowid = ?", (dest_rowid,))
                        if cursor.fetchone()[0] > 0:
                            inserted_buckets += 1

                existing_events = load_existing_events(dest, dest_rowid)
                for starttime, endtime, data in source.execute(
                    "select starttime, endtime, data from events where bucketrow = ? order by id",
                    (src_bucket["bucketrow"],),
                ).fetchall():
                    event_key = (int(starttime), int(endtime), str(data))
                    if event_key in existing_events:
                        continue
                    dest.execute(
                        "insert into events (bucketrow, starttime, endtime, data) values (?, ?, ?, ?)",
                        (dest_rowid, int(starttime), int(endtime), str(data)),
                    )
                    existing_events.add(event_key)
                    inserted_events += 1

            dest.commit()
        finally:
            source.close()

    dest.close()
    os.replace(tmp_output, output)
    print(
        json.dumps(
            {
                "base": str(base),
                "overlay": str(overlay) if overlay else None,
                "output": str(output),
                "inserted_buckets": inserted_buckets,
                "inserted_events": inserted_events,
            },
            ensure_ascii=False,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
