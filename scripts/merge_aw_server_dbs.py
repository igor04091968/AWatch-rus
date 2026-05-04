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
    if tmp_output.exists():
        tmp_output.unlink()
    shutil.copy2(base, tmp_output)

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

            for src_bucket in source_buckets:
                key = bucket_key(src_bucket)
                dest_rowid = dest_bucket_map.get(key)
                if dest_rowid is None:
                    cursor = dest.execute(
                        """
                        insert into buckets (name, type, client, hostname, created, data_deprecated, data)
                        values (?, ?, ?, ?, ?, ?, ?)
                        """,
                        (
                            src_bucket["name"],
                            src_bucket["type"],
                            src_bucket["client"],
                            src_bucket["hostname"],
                            src_bucket["created"],
                            src_bucket["data_deprecated"],
                            src_bucket["data"],
                        ),
                    )
                    dest_rowid = int(cursor.lastrowid)
                    dest_bucket_map[key] = dest_rowid
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
