#!/usr/bin/env python3
from __future__ import annotations

import argparse
import os
from datetime import UTC, datetime
from typing import Any

import clickhouse_connect


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Refresh technical company->registry bindings for analytics_1c")
    p.add_argument("--host", default=os.getenv("CLICKHOUSE_HOST", "localhost"))
    p.add_argument("--port", type=int, default=int(os.getenv("CLICKHOUSE_PORT", "8123")))
    p.add_argument("--user", default=os.getenv("CLICKHOUSE_USER", "default"))
    p.add_argument("--password", default=os.getenv("CLICKHOUSE_PASSWORD", ""))
    p.add_argument("--database", default=os.getenv("CLICKHOUSE_DB", "analytics_1c"))
    return p.parse_args()


def ch_client(args: argparse.Namespace):
    return clickhouse_connect.get_client(
        host=args.host,
        port=args.port,
        username=args.user,
        password=args.password,
        database=args.database,
    )


def query_rows(client, sql: str) -> list[dict[str, Any]]:
    result = client.query(sql)
    return [dict(zip(result.column_names, row)) for row in result.result_rows]


def main() -> int:
    args = parse_args()
    client = ch_client(args)
    generated_at = datetime.now(UTC).replace(tzinfo=None, microsecond=0)

    current_bindings = {
        str(row["company_entity_key"]): row
        for row in query_rows(
            client,
            """
            SELECT
                company_entity_key,
                registry_company_key,
                registry_company_name,
                binding_source
            FROM analytics_1c.v_company_registry_bindings_current
            """,
        )
    }

    candidates = query_rows(
        client,
        """
        SELECT
            company_entity_key,
            infobase,
            base_id,
            base_path,
            ifNull(base_path_key, '') AS base_path_key,
            registry_company_key,
            company_name,
            registry_match_mode
        FROM analytics_1c.v_company_portfolio_overview
        WHERE registry_match_mode IN ('direct', 'alias', 'manual')
          AND registry_company_key != ''
          AND company_entity_key != ''
        """
    )

    inserts: list[list[Any]] = []
    for row in candidates:
        entity_key = str(row["company_entity_key"])
        registry_key = str(row["registry_company_key"])
        current = current_bindings.get(entity_key)
        if current and str(current.get("registry_company_key") or "") == registry_key:
            continue
        inserts.append(
            [
                generated_at,
                str(row["infobase"] or ""),
                entity_key,
                str(row["base_id"] or ""),
                str(row["base_path"] or ""),
                str(row["base_path_key"] or ""),
                registry_key,
                str(row["company_name"] or ""),
                f"bootstrap_{row['registry_match_mode']}",
                "autobound_from_portfolio",
            ]
        )

    if inserts:
        client.insert(
            "analytics_1c.company_registry_bindings",
            inserts,
            column_names=[
                "ts",
                "infobase",
                "company_entity_key",
                "base_id",
                "base_path",
                "base_path_key",
                "registry_company_key",
                "registry_company_name",
                "binding_source",
                "note",
            ],
        )

    print(f"company registry bindings refreshed: inserted={len(inserts)} generated_at={generated_at.isoformat()}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
