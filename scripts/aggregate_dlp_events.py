#!/usr/bin/env python3
import argparse
import json
import os
import sqlite3
import sys
import urllib.error
import urllib.parse
import urllib.request
from collections.abc import Iterable
from dataclasses import dataclass
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import Protocol, TypeAlias


JsonScalar: TypeAlias = str | int | float | bool | None
JsonValue: TypeAlias = JsonScalar | list["JsonValue"] | dict[str, "JsonValue"]


DEFAULT_BUCKET_PREFIXES = ("aw-file-operations_", "aw-dlp-incidents_")
DEFAULT_SQLITE_PATH = "data/dlp-events.sqlite3"
EVENT_COLUMNS = (
    "bucket_id",
    "event_id",
    "stream_type",
    "hostname",
    "username",
    "event_ts",
    "duration",
    "operation",
    "file_path",
    "old_file_path",
    "extension",
    "archive_hint",
    "rule_id",
    "action",
    "severity",
    "signal_type",
    "message",
    "source",
    "screenshot_path",
    "raw_json",
    "ingested_at",
)


@dataclass(frozen=True)
class Bucket:
    id: str
    type: str
    client: str
    hostname: str


@dataclass(frozen=True)
class AwEvent:
    bucket_id: str
    hostname: str
    stream_type: str
    event_id: str
    timestamp: str
    duration: float
    data: dict[str, JsonValue]


class PsycopgConnection(Protocol):
    def cursor(self):
        ...

    def commit(self) -> None:
        ...


def utc_now() -> datetime:
    return datetime.now(tz=UTC)


def parse_timestamp(value: str) -> datetime:
    normalized = value.replace("Z", "+00:00")
    parsed = datetime.fromisoformat(normalized)
    if parsed.tzinfo is None:
        return parsed.replace(tzinfo=UTC)
    return parsed.astimezone(UTC)


def format_aw_timestamp(value: datetime) -> str:
    return value.astimezone(UTC).isoformat().replace("+00:00", "Z")


def load_state(path: Path) -> dict[str, str]:
    if not path.exists():
        return {}
    return json.loads(path.read_text(encoding="utf-8"))


def save_state(path: Path, state: dict[str, str]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(state, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def normalize_base_url(base_url: str) -> str:
    return base_url.rstrip("/")


def aw_get_json(base_url: str, path: str, timeout: int) -> JsonValue:
    url = normalize_base_url(base_url) + path
    request = urllib.request.Request(url, headers={"Accept": "application/json"})
    with urllib.request.urlopen(request, timeout=timeout) as response:
        return json.loads(response.read().decode("utf-8"))


def list_buckets(base_url: str, timeout: int) -> list[Bucket]:
    payload = aw_get_json(base_url, "/buckets", timeout)
    if not isinstance(payload, dict):
        raise ValueError("ActivityWatch /buckets response must be a JSON object")
    buckets: list[Bucket] = []
    for bucket_id, bucket_data in payload.items():
        if not isinstance(bucket_data, dict):
            continue
        buckets.append(
            Bucket(
                id=str(bucket_id),
                type=str(bucket_data.get("type", "")),
                client=str(bucket_data.get("client", "")),
                hostname=str(bucket_data.get("hostname", "")),
            )
        )
    return buckets


def bucket_stream_type(bucket: Bucket) -> str | None:
    if bucket.id.startswith("aw-file-operations_") or bucket.type == "aw.file.operation":
        return "file_operation"
    if bucket.id.startswith("aw-dlp-incidents_") or bucket.type == "aw.dlp.incident":
        return "dlp_incident"
    return None


def select_buckets(buckets: Iterable[Bucket], prefixes: tuple[str, ...]) -> list[tuple[Bucket, str]]:
    selected: list[tuple[Bucket, str]] = []
    for bucket in buckets:
        stream_type = bucket_stream_type(bucket)
        if stream_type and any(bucket.id.startswith(prefix) for prefix in prefixes):
            selected.append((bucket, stream_type))
    return selected


def build_events_path(bucket_id: str, start: datetime, end: datetime, limit: int) -> str:
    query = urllib.parse.urlencode(
        {
            "start": format_aw_timestamp(start),
            "end": format_aw_timestamp(end),
            "limit": str(limit),
        }
    )
    return f"/buckets/{urllib.parse.quote(bucket_id, safe='')}/events?{query}"


def event_key(bucket_id: str, timestamp: str, duration: float, data: dict[str, JsonValue]) -> str:
    payload = json.dumps(data, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    return f"{bucket_id}|{timestamp}|{duration}|{payload}"


def fetch_bucket_events(
    base_url: str,
    bucket: Bucket,
    stream_type: str,
    start: datetime,
    end: datetime,
    limit: int,
    timeout: int,
) -> list[AwEvent]:
    payload = aw_get_json(base_url, build_events_path(bucket.id, start, end, limit), timeout)
    if not isinstance(payload, list):
        raise ValueError(f"ActivityWatch events response for {bucket.id} must be a JSON array")
    events: list[AwEvent] = []
    for item in payload:
        if not isinstance(item, dict):
            continue
        timestamp = str(item["timestamp"])
        duration = float(item.get("duration", 0) or 0)
        data = item.get("data") or {}
        if not isinstance(data, dict):
            data = {"raw": data}
        item_id = str(item.get("id") or event_key(bucket.id, timestamp, duration, data))
        events.append(
            AwEvent(
                bucket_id=bucket.id,
                hostname=bucket.hostname or str(data.get("hostname") or ""),
                stream_type=stream_type,
                event_id=item_id,
                timestamp=timestamp,
                duration=duration,
                data=data,
            )
        )
    return events


def connect_sqlite(path: Path) -> sqlite3.Connection:
    path.parent.mkdir(parents=True, exist_ok=True)
    connection = sqlite3.connect(str(path))
    connection.execute("PRAGMA journal_mode=WAL")
    connection.execute("PRAGMA synchronous=NORMAL")
    connection.execute("PRAGMA foreign_keys=ON")
    return connection


def ensure_schema(connection: sqlite3.Connection) -> None:
    connection.executescript(
        """
        create table if not exists dlp_events (
            id integer primary key autoincrement,
            bucket_id text not null,
            event_id text not null,
            stream_type text not null,
            hostname text not null,
            username text,
            event_ts text not null,
            duration real not null default 0,
            operation text,
            file_path text,
            old_file_path text,
            extension text,
            archive_hint integer not null default 0,
            rule_id text,
            action text,
            severity text,
            signal_type text,
            message text,
            source text,
            screenshot_path text,
            raw_json text not null,
            ingested_at text not null,
            unique (bucket_id, event_id)
        );
        create index if not exists idx_dlp_events_event_ts on dlp_events(event_ts);
        create index if not exists idx_dlp_events_host_ts on dlp_events(hostname, event_ts);
        create index if not exists idx_dlp_events_stream_ts on dlp_events(stream_type, event_ts);
        create index if not exists idx_dlp_events_archive on dlp_events(archive_hint, event_ts);
        create index if not exists idx_dlp_events_rule on dlp_events(rule_id, event_ts);

        create view if not exists dlp_file_operations as
        select *
        from dlp_events
        where stream_type = 'file_operation';

        create view if not exists dlp_incidents as
        select *
        from dlp_events
        where stream_type = 'dlp_incident';
        """
    )
    connection.commit()


def ensure_postgres_schema(connection: PsycopgConnection) -> None:
    with connection.cursor() as cursor:
        cursor.execute(
            """
            create table if not exists dlp_events (
                id bigserial primary key,
                bucket_id text not null,
                event_id text not null,
                stream_type text not null,
                hostname text not null,
                username text,
                event_ts timestamptz not null,
                duration double precision not null default 0,
                operation text,
                file_path text,
                old_file_path text,
                extension text,
                archive_hint boolean not null default false,
                rule_id text,
                action text,
                severity text,
                signal_type text,
                message text,
                source text,
                screenshot_path text,
                raw_json jsonb not null,
                ingested_at timestamptz not null,
                unique (bucket_id, event_id)
            );
            create index if not exists idx_dlp_events_event_ts on dlp_events(event_ts);
            create index if not exists idx_dlp_events_host_ts on dlp_events(hostname, event_ts);
            create index if not exists idx_dlp_events_stream_ts on dlp_events(stream_type, event_ts);
            create index if not exists idx_dlp_events_archive on dlp_events(archive_hint, event_ts);
            create index if not exists idx_dlp_events_rule on dlp_events(rule_id, event_ts);

            create or replace view dlp_file_operations as
            select *
            from dlp_events
            where stream_type = 'file_operation';

            create or replace view dlp_incidents as
            select *
            from dlp_events
            where stream_type = 'dlp_incident';
            """
        )
    connection.commit()


def first_string(data: dict[str, JsonValue], keys: tuple[str, ...]) -> str | None:
    for key in keys:
        value = data.get(key)
        if value is not None and str(value) != "":
            return str(value)
    return None


def bool_as_int(value: JsonValue) -> int:
    if isinstance(value, bool):
        return int(value)
    if isinstance(value, str):
        return int(value.lower() in {"1", "true", "yes", "y"})
    return int(bool(value))


def event_row(event: AwEvent, ingested_at: str) -> tuple[JsonValue, ...]:
    data = event.data
    event_id = event.event_id or event_key(event.bucket_id, event.timestamp, event.duration, data)
    return (
        event.bucket_id,
        event_id,
        event.stream_type,
        event.hostname,
        first_string(data, ("username", "user")),
        event.timestamp,
        event.duration,
        first_string(data, ("operation",)),
        first_string(data, ("path", "filePath")),
        first_string(data, ("oldPath", "oldFilePath")),
        first_string(data, ("extension",)),
        bool_as_int(data.get("archiveHint")),
        first_string(data, ("ruleId", "rule")),
        first_string(data, ("action",)),
        first_string(data, ("severity",)),
        first_string(data, ("signalType",)),
        first_string(data, ("message",)),
        first_string(data, ("source",)),
        first_string(data, ("screenshotPath", "capturePath", "artifactPath")),
        json.dumps(data, ensure_ascii=False, sort_keys=True),
        ingested_at,
    )


def insert_events(connection: sqlite3.Connection, events: Iterable[AwEvent]) -> int:
    inserted = 0
    now = format_aw_timestamp(utc_now())
    for event in events:
        cursor = connection.execute(
            """
            insert or ignore into dlp_events (
                bucket_id,
                event_id,
                stream_type,
                hostname,
                username,
                event_ts,
                duration,
                operation,
                file_path,
                old_file_path,
                extension,
                archive_hint,
                rule_id,
                action,
                severity,
                signal_type,
                message,
                source,
                screenshot_path,
                raw_json,
                ingested_at
            )
            values (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            event_row(event, now),
        )
        inserted += int(cursor.rowcount > 0)
    connection.commit()
    return inserted


def insert_postgres_events(dsn: str, events: Iterable[AwEvent]) -> int:
    try:
        import psycopg
    except ImportError as exc:
        raise SystemExit("PostgreSQL mode requires psycopg: python3 -m pip install 'psycopg[binary]'") from exc

    inserted = 0
    now = format_aw_timestamp(utc_now())
    columns = ", ".join(EVENT_COLUMNS)
    placeholders = ", ".join(["%s"] * len(EVENT_COLUMNS))
    sql = f"""
        insert into dlp_events ({columns})
        values ({placeholders})
        on conflict (bucket_id, event_id) do nothing
    """
    with psycopg.connect(dsn) as connection:
        ensure_postgres_schema(connection)
        with connection.cursor() as cursor:
            for event in events:
                row = list(event_row(event, now))
                row[EVENT_COLUMNS.index("archive_hint")] = bool(row[EVENT_COLUMNS.index("archive_hint")])
                cursor.execute(sql, row)
                inserted += int(cursor.rowcount > 0)
        connection.commit()
    return inserted


def get_start_time(args: argparse.Namespace, state: dict[str, str]) -> datetime:
    if args.since:
        return parse_timestamp(args.since)
    if state.get("last_end"):
        return parse_timestamp(state["last_end"]) - timedelta(seconds=args.overlap_seconds)
    return utc_now() - timedelta(hours=args.lookback_hours)


def parse_prefixes(value: str) -> tuple[str, ...]:
    prefixes = tuple(item.strip() for item in value.split(",") if item.strip())
    if not prefixes:
        raise argparse.ArgumentTypeError("at least one bucket prefix is required")
    return prefixes


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Aggregate AWatch-rus DLP buckets into a local warehouse database.")
    parser.add_argument("--aw-url", default=os.environ.get("AW_URL", "http://127.0.0.1:5600/api/0"))
    parser.add_argument("--postgres-dsn", default=os.environ.get("DLP_AGGREGATOR_POSTGRES_DSN"))
    parser.add_argument("--sqlite-path", default=os.environ.get("DLP_AGGREGATOR_SQLITE_PATH", DEFAULT_SQLITE_PATH))
    parser.add_argument("--state-path", default=os.environ.get("DLP_AGGREGATOR_STATE_PATH", "data/dlp-aggregator-state.json"))
    parser.add_argument("--bucket-prefixes", type=parse_prefixes, default=DEFAULT_BUCKET_PREFIXES)
    parser.add_argument("--since", help="UTC ISO timestamp. Overrides saved state, for example 2026-05-02T00:00:00Z.")
    parser.add_argument("--lookback-hours", type=int, default=24)
    parser.add_argument("--overlap-seconds", type=int, default=60)
    parser.add_argument("--limit", type=int, default=10000)
    parser.add_argument("--timeout", type=int, default=15)
    parser.add_argument("--dry-run", action="store_true")
    return parser


def main() -> int:
    args = build_parser().parse_args()
    state_path = Path(args.state_path)
    state = load_state(state_path)
    start = get_start_time(args, state)
    end = utc_now()

    buckets = select_buckets(list_buckets(args.aw_url, args.timeout), args.bucket_prefixes)
    all_events: list[AwEvent] = []
    for bucket, stream_type in buckets:
        all_events.extend(fetch_bucket_events(args.aw_url, bucket, stream_type, start, end, args.limit, args.timeout))

    if args.dry_run:
        print(
            json.dumps(
                {
                    "aw_url": args.aw_url,
                    "start": format_aw_timestamp(start),
                    "end": format_aw_timestamp(end),
                    "selected_buckets": [bucket.id for bucket, _stream_type in buckets],
                    "fetched_events": len(all_events),
                },
                ensure_ascii=False,
                indent=2,
            )
        )
        return 0

    if args.postgres_dsn:
        target = "postgres"
        target_path = args.postgres_dsn.split("@")[-1]
        inserted = insert_postgres_events(args.postgres_dsn, all_events)
    else:
        target = "sqlite"
        sqlite_path = Path(args.sqlite_path)
        target_path = str(sqlite_path)
        connection = connect_sqlite(sqlite_path)
        try:
            ensure_schema(connection)
            inserted = insert_events(connection, all_events)
        finally:
            connection.close()

    state["last_end"] = format_aw_timestamp(end)
    save_state(state_path, state)
    print(
        json.dumps(
            {
                "aw_url": args.aw_url,
                "target": target,
                "target_path": target_path,
                "state_path": str(state_path),
                "start": format_aw_timestamp(start),
                "end": format_aw_timestamp(end),
                "selected_buckets": len(buckets),
                "fetched_events": len(all_events),
                "inserted_events": inserted,
            },
            ensure_ascii=False,
            indent=2,
        )
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except urllib.error.URLError as exc:
        print(f"ActivityWatch API request failed: {exc}", file=sys.stderr)
        raise SystemExit(2)
