#!/usr/bin/env python3
from __future__ import annotations

import argparse
import os
from datetime import UTC, datetime
from decimal import Decimal
from typing import Any

import clickhouse_connect
import uvicorn
from fastapi import FastAPI, HTTPException, Query


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Read-only company intelligence API for analytics_1c")
    p.add_argument("--host", default=os.getenv("AW_1C_COMPANY_API_HOST", "127.0.0.1"))
    p.add_argument("--port", type=int, default=int(os.getenv("AW_1C_COMPANY_API_PORT", "8710")))
    return p.parse_args()


def q(value: str) -> str:
    return "'" + value.replace("'", "''") + "'"


def to_plain(value: Any) -> Any:
    if isinstance(value, Decimal):
        return float(value)
    if isinstance(value, datetime):
        return value.isoformat()
    return value


def rows_to_dict(result) -> list[dict[str, Any]]:
    return [
        {name: to_plain(value) for name, value in zip(result.column_names, row)}
        for row in result.result_rows
    ]


def ch_client():
    return clickhouse_connect.get_client(
        host=os.getenv("CLICKHOUSE_HOST", "localhost"),
        port=int(os.getenv("CLICKHOUSE_PORT", "8123")),
        username=os.getenv("CLICKHOUSE_USER", "default"),
        password=os.getenv("CLICKHOUSE_PASSWORD", ""),
        database=os.getenv("CLICKHOUSE_DB", "analytics_1c"),
    )


app = FastAPI(title="AW-rus 1C Company Intelligence API", version="1.0.0")


@app.get("/health")
def health() -> dict[str, Any]:
    client = ch_client()
    summary = rows_to_dict(
        client.query(
            """
            SELECT
                countIf(counterparty != '') AS documents_with_counterparty,
                (SELECT count() FROM analytics_1c.companies) AS companies_total,
                (SELECT count() FROM analytics_1c.company_registry) AS registry_rows_total,
                (SELECT count() FROM analytics_1c.company_forecasts) AS forecasts_total,
                (SELECT count() FROM analytics_1c.company_health_signals) AS health_signals_total
            FROM analytics_1c.documents
            """
        )
    )[0]
    return {"status": "ok", "generated_at": datetime.now(UTC).isoformat(), **summary}


@app.get("/api/1/analytics-1c/companies/overview")
def companies_overview(
    infobase: str | None = None,
    min_signal_score: int = Query(default=0, ge=0, le=100),
    limit: int = Query(default=50, ge=1, le=500),
) -> dict[str, Any]:
    client = ch_client()
    where = [f"signal_score >= {int(min_signal_score)}"]
    if infobase:
        where.append(f"infobase = {q(infobase)}")
    sql = f"""
    SELECT
        infobase,
        organization,
        counterparty,
        company_name,
        normalized_counterparty,
        registry_match_mode,
        registry_assignee_name,
        registry_status,
        registry_share_text,
        registry_key_contour,
        registry_inn,
        registry_kpp,
        owner_user,
        base_path,
        current_status,
        db_size_bytes,
        reglog_size_bytes,
        active_locks,
        current_activity_score,
        last_seen_at,
        days_since_last_activity,
        docs_7d,
        amount_7d,
        docs_30d,
        amount_30d,
        amount_forecast_30d,
        docs_forecast_30d,
        signal_severity,
        signal_score,
        top_signal
    FROM analytics_1c.v_company_portfolio_overview
    WHERE {' AND '.join(where)}
    ORDER BY signal_score DESC, amount_30d DESC, counterparty
    LIMIT {int(limit)}
    """
    rows = rows_to_dict(client.query(sql))
    return {"items": rows, "count": len(rows)}


@app.get("/api/1/analytics-1c/companies/{counterparty}/summary")
def company_summary(counterparty: str, infobase: str | None = None) -> dict[str, Any]:
    client = ch_client()
    filters = [f"counterparty = {q(counterparty)}"]
    if infobase:
        filters.append(f"infobase = {q(infobase)}")
    sql = f"""
    SELECT *
    FROM analytics_1c.v_company_portfolio_overview
    WHERE {' AND '.join(filters)}
    ORDER BY last_company_snapshot_at DESC, amount_30d DESC
    LIMIT 1
    """
    rows = rows_to_dict(client.query(sql))
    if not rows:
        raise HTTPException(status_code=404, detail="counterparty not found in analytics_1c.v_company_portfolio_overview")
    card = rows[0]
    forecast_sql = f"""
    SELECT metric, horizon_days, baseline_daily, trend_slope, predicted_daily, predicted_total, confidence, note
    FROM analytics_1c.v_company_forecasts_current
    WHERE counterparty = {q(counterparty)}
    {"AND infobase = " + q(infobase) if infobase else ""}
    ORDER BY metric, horizon_days
    """
    signals_sql = f"""
    SELECT generated_at, severity, score, signal_type, summary
    FROM analytics_1c.v_company_health_current
    WHERE counterparty = {q(counterparty)}
    {"AND infobase = " + q(infobase) if infobase else ""}
    ORDER BY score DESC, generated_at DESC
    """
    timeline_sql = f"""
    SELECT last_company_snapshot_at AS ts, infobase, company_name, owner_user, current_status, db_size_bytes, reglog_size_bytes, active_locks, current_activity_score
    FROM analytics_1c.v_company_portfolio_overview
    WHERE counterparty = {q(counterparty)}
    {"AND infobase = " + q(infobase) if infobase else ""}
    ORDER BY ts DESC
    LIMIT 1
    """
    forecasts = rows_to_dict(client.query(forecast_sql))
    signals = rows_to_dict(client.query(signals_sql))
    company_state = rows_to_dict(client.query(timeline_sql))
    timeline_sql = f"""
    SELECT ts, infobase, doc_type, operation_type, amount, status, author
    FROM analytics_1c.documents
    WHERE counterparty = {q(counterparty)}
    {"AND infobase = " + q(infobase) if infobase else ""}
    ORDER BY ts DESC
    LIMIT 20
    """
    timeline = rows_to_dict(client.query(timeline_sql))
    essence = (
        f"Компания {counterparty}: за 30 дней событий {card['docs_30d']}, суммарная активность {card['amount_30d']}, "
        f"прогноз активности на 30 дней {card['amount_forecast_30d']}, риск {card['signal_severity']}."
    )
    return {
        "essence": essence,
        "card": card,
        "company_state": company_state[0] if company_state else None,
        "forecasts": forecasts,
        "signals": signals,
        "recent_documents": timeline,
    }


@app.get("/api/1/analytics-1c/companies/{counterparty}/forecast")
def company_forecast(
    counterparty: str,
    infobase: str | None = None,
    horizon_days: int | None = Query(default=None, ge=1, le=365),
) -> dict[str, Any]:
    client = ch_client()
    filters = [f"counterparty = {q(counterparty)}"]
    if infobase:
        filters.append(f"infobase = {q(infobase)}")
    if horizon_days is not None:
        filters.append(f"horizon_days = {int(horizon_days)}")
    sql = f"""
    SELECT generated_at, infobase, counterparty, metric, horizon_days, baseline_daily, trend_slope, predicted_daily, predicted_total, confidence, note
    FROM analytics_1c.v_company_forecasts_current
    WHERE {' AND '.join(filters)}
    ORDER BY metric, horizon_days
    """
    rows = rows_to_dict(client.query(sql))
    return {"items": rows, "count": len(rows)}


@app.get("/api/1/analytics-1c/companies/{counterparty}/timeline")
def company_timeline(
    counterparty: str,
    infobase: str | None = None,
    limit: int = Query(default=100, ge=1, le=500),
) -> dict[str, Any]:
    client = ch_client()
    filters = [f"counterparty = {q(counterparty)}"]
    if infobase:
        filters.append(f"infobase = {q(infobase)}")
    sql = f"""
    SELECT ts, infobase, organization, doc_type, doc_number, author, operation_type, amount, status, posted
    FROM analytics_1c.documents
    WHERE {' AND '.join(filters)}
    ORDER BY ts DESC
    LIMIT {int(limit)}
    """
    rows = rows_to_dict(client.query(sql))
    return {"items": rows, "count": len(rows)}


if __name__ == "__main__":
    args = parse_args()
    uvicorn.run(app, host=args.host, port=args.port)
