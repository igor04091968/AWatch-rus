#!/usr/bin/env python3
from __future__ import annotations

import argparse
import math
import os
from collections import defaultdict
from dataclasses import dataclass
from datetime import UTC, date, datetime, timedelta
from statistics import fmean, pstdev
from typing import Any

import clickhouse_connect


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Refresh company forecasts and health signals for analytics_1c")
    p.add_argument("--host", default=os.getenv("CLICKHOUSE_HOST", "localhost"))
    p.add_argument("--port", type=int, default=int(os.getenv("CLICKHOUSE_PORT", "8123")))
    p.add_argument("--user", default=os.getenv("CLICKHOUSE_USER", "default"))
    p.add_argument("--password", default=os.getenv("CLICKHOUSE_PASSWORD", ""))
    p.add_argument("--database", default=os.getenv("CLICKHOUSE_DB", "analytics_1c"))
    p.add_argument("--lookback-days", type=int, default=int(os.getenv("AW_1C_COMPANY_LOOKBACK_DAYS", "30")))
    p.add_argument("--min-days", type=int, default=int(os.getenv("AW_1C_COMPANY_MIN_DAYS", "1")))
    p.add_argument("--horizons", default=os.getenv("AW_1C_COMPANY_HORIZONS", "7,30"))
    return p.parse_args()


@dataclass
class DailyPoint:
    d: date
    docs_total: float
    amount_total: float


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


def fill_daily_series(points: list[DailyPoint]) -> list[DailyPoint]:
    if not points:
        return []
    by_day = {p.d: p for p in points}
    current = points[0].d
    end = points[-1].d
    filled: list[DailyPoint] = []
    while current <= end:
        filled.append(by_day.get(current, DailyPoint(current, 0.0, 0.0)))
        current += timedelta(days=1)
    return filled


def linear_slope(values: list[float]) -> float:
    n = len(values)
    if n < 2:
        return 0.0
    x_mean = (n - 1) / 2
    y_mean = fmean(values)
    num = sum((i - x_mean) * (v - y_mean) for i, v in enumerate(values))
    den = sum((i - x_mean) ** 2 for i in range(n))
    if den == 0:
        return 0.0
    return num / den


def normalize_company_key(value: str) -> str:
    import re

    text = (value or "").upper().replace("Ё", "Е")
    text = re.sub(r"(^|\s)20\d{2}($|\s)", " ", text)
    text = re.sub(r"[^0-9A-ZА-Я]+", " ", text)
    text = re.sub(r"\s+", " ", text)
    return text.strip()


def build_forecast(values: list[float], horizon: int, min_days: int, lookback_days: int) -> tuple[float, float, float, float, int, str]:
    if len(values) < min_days:
        raise ValueError("not enough data")
    window = values[-min(len(values), lookback_days):]
    baseline = fmean(window)
    slope = linear_slope(window)
    projected = [max(0.0, baseline + slope * step) for step in range(1, horizon + 1)]
    predicted_total = sum(projected)
    predicted_daily = projected[-1] if projected else baseline
    if len(window) > 1 and baseline > 0:
        volatility = pstdev(window) / baseline
    elif len(window) > 1:
        volatility = pstdev(window)
    else:
        volatility = 0.0
    coverage = min(1.0, len(window) / max(lookback_days, 1))
    stability = max(0.15, 1.0 - min(volatility, 1.0))
    confidence = max(0.1, min(0.95, coverage * stability))
    note_parts: list[str] = []
    if len(values) < lookback_days:
        note_parts.append("sparse_history")
    if abs(slope) < 0.01:
        note_parts.append("flat_trend")
    note = ",".join(note_parts) if note_parts else "ok"
    return baseline, slope, predicted_daily, predicted_total, len(window), note


def severity_score_to_label(score: int) -> str:
    if score >= 80:
        return "critical"
    if score >= 60:
        return "high"
    if score >= 35:
        return "medium"
    return "low"


def main() -> int:
    args = parse_args()
    client = ch_client(args)
    horizons = [int(x.strip()) for x in args.horizons.split(",") if x.strip()]
    generated_at = datetime.now(UTC).replace(tzinfo=None, microsecond=0)

    daily_rows = query_rows(
        client,
        """
        SELECT infobase, organization, company_entity_key, source_counterparty, d, docs_total, amount_total
        FROM analytics_1c.v_company_activity_daily
        ORDER BY infobase, company_entity_key, d
        """,
    )
    if not daily_rows:
        print("no company activity rows in analytics_1c.v_company_activity_daily; nothing to refresh")
        return 0

    grouped: dict[tuple[str, str, str, str], list[DailyPoint]] = defaultdict(list)
    for row in daily_rows:
        key = (row["infobase"], row["organization"], row["company_entity_key"], row.get("source_counterparty") or row["company_entity_key"])
        grouped[key].append(
            DailyPoint(
                d=row["d"],
                docs_total=float(row["docs_total"] or 0),
                amount_total=float(row["amount_total"] or 0),
            )
        )

    cases_map = {
        (row["infobase"], row["company_entity_key"]): int(row["open_cases_total"] or 0)
        for row in query_rows(
            client,
            """
            SELECT infobase, entity_id AS company_entity_key, countIf(status != 'closed') AS open_cases_total
            FROM analytics_1c.cases
            WHERE entity_type = 'counterparty'
            GROUP BY infobase, company_entity_key
            """,
        )
    }
    detections_map = {
        (row["infobase"], row["company_entity_key"]): int(row["detections_total"] or 0)
        for row in query_rows(
            client,
            """
            SELECT infobase, entity_id AS company_entity_key, count() AS detections_total
            FROM analytics_1c.detections
            WHERE entity_type = 'counterparty' AND status != 'closed'
            GROUP BY infobase, company_entity_key
            """,
        )
    }
    company_state_map = {
        row["infobase"]: row
        for row in query_rows(
            client,
            """
            SELECT
                infobase,
                current_status,
                active_locks,
                temp_db_present,
                scheduler_touched,
                current_activity_score
            FROM analytics_1c.v_companies_current
            """,
        )
    }
    excluded_company_keys = {
        str(row["source_company_key"])
        for row in query_rows(
            client,
            """
            SELECT source_company_key
            FROM analytics_1c.v_company_registry_alias_map
            WHERE exclude_from_portfolio = 1
            """,
        )
    }

    forecast_rows: list[list[Any]] = []
    signal_rows: list[list[Any]] = []

    for (infobase, _organization, company_entity_key, source_counterparty), points in grouped.items():
        if normalize_company_key(source_counterparty) in excluded_company_keys:
            continue
        points.sort(key=lambda p: p.d)
        filled = fill_daily_series(points)
        docs_series = [p.docs_total for p in filled]
        amount_series = [p.amount_total for p in filled]
        if len(filled) < args.min_days:
            continue

        latest_day = filled[-1].d
        last_7 = filled[-7:]
        prev_7 = filled[-14:-7]
        docs_7d = int(sum(p.docs_total for p in last_7))
        docs_prev_7d = int(sum(p.docs_total for p in prev_7))
        amount_7d = float(sum(p.amount_total for p in last_7))
        amount_prev_7d = float(sum(p.amount_total for p in prev_7))
        days_since_last_activity = (date.today() - latest_day).days
        open_cases_total = cases_map.get((infobase, company_entity_key), 0)
        detections_total = detections_map.get((infobase, company_entity_key), 0)
        company_state = company_state_map.get(infobase, {})
        current_status = str(company_state.get("current_status") or "")
        active_locks = int(company_state.get("active_locks") or 0)
        temp_db_present = int(company_state.get("temp_db_present") or 0)
        scheduler_touched = int(company_state.get("scheduler_touched") or 0)
        current_activity_score = float(company_state.get("current_activity_score") or 0)

        for metric, values in (("docs_total", docs_series), ("amount_total", amount_series)):
            for horizon in horizons:
                baseline, slope, predicted_daily, predicted_total, source_days, note = build_forecast(
                    values=values,
                    horizon=horizon,
                    min_days=args.min_days,
                    lookback_days=args.lookback_days,
                )
                forecast_rows.append(
                    [
                        generated_at,
                        latest_day,
                        infobase,
                        company_entity_key,
                        int(horizon),
                        metric,
                        float(baseline),
                        float(slope),
                        float(predicted_daily),
                        float(predicted_total),
                        round(float(max(0.1, min(0.95, 1.0 - abs(slope) / (abs(baseline) + 1.0)))), 4),
                        "linear_baseline",
                        int(source_days),
                        note,
                    ]
                )

        signals: list[tuple[str, int, str, str]] = []
        if days_since_last_activity >= 14 and (docs_prev_7d > 0 or amount_prev_7d > 0):
            signals.append(("inactive_company", 85, "high", f"Нет активности по компании {source_counterparty} уже {days_since_last_activity} дн."))
        if amount_prev_7d > 0 and amount_7d < amount_prev_7d * 0.5:
            signals.append(("amount_drop", 70, "high", f"Активность по компании {source_counterparty} упала более чем на 50% неделя к неделе."))
        if docs_prev_7d > 0 and docs_7d == 0:
            signals.append(("docs_stopped", 55, "medium", f"По компании {source_counterparty} прекратился поток документов за последние 7 дней."))
        if current_status == "busy" or active_locks > 0 or temp_db_present > 0:
            score = min(85, 45 + active_locks * 5 + temp_db_present * 10)
            signals.append(("base_busy", score, severity_score_to_label(score), f"Файловая база компании {source_counterparty} занята: status={current_status}, locks={active_locks}, tempDb={temp_db_present}."))
        if scheduler_touched > 0 and current_activity_score >= 15:
            signals.append(("scheduler_activity", 35, "medium", f"По компании {source_counterparty} есть активность scheduler и повышенный activity score {current_activity_score}."))
        if open_cases_total > 0:
            signals.append(("open_cases", min(95, 40 + open_cases_total * 10), severity_score_to_label(min(95, 40 + open_cases_total * 10)), f"По компании {source_counterparty} есть открытые кейсы: {open_cases_total}."))
        if detections_total > 0:
            signals.append(("open_detections", min(90, 35 + detections_total * 5), severity_score_to_label(min(90, 35 + detections_total * 5)), f"По компании {source_counterparty} есть активные detections: {detections_total}."))

        for signal_type, score, severity, summary in signals:
            signal_rows.append(
                    [
                        generated_at,
                        infobase,
                        company_entity_key,
                        f"{signal_type}:{infobase}:{company_entity_key}",
                        severity,
                    int(score),
                    signal_type,
                    summary,
                    float(amount_7d),
                    float(amount_prev_7d),
                    int(docs_7d),
                    int(docs_prev_7d),
                    int(max(days_since_last_activity, 0)),
                    int(open_cases_total),
                    int(detections_total),
                ]
            )

    if forecast_rows:
        client.insert(
            "analytics_1c.company_forecasts",
            forecast_rows,
            column_names=[
                "generated_at",
                "as_of_date",
                "infobase",
                "counterparty",
                "horizon_days",
                "metric",
                "baseline_daily",
                "trend_slope",
                "predicted_daily",
                "predicted_total",
                "confidence",
                "model",
                "source_days",
                "note",
            ],
        )
    if signal_rows:
        client.insert(
            "analytics_1c.company_health_signals",
            signal_rows,
            column_names=[
                "generated_at",
                "infobase",
                "counterparty",
                "signal_id",
                "severity",
                "score",
                "signal_type",
                "summary",
                "amount_7d",
                "amount_prev_7d",
                "docs_7d",
                "docs_prev_7d",
                "days_since_last_activity",
                "open_cases_total",
                "detections_total",
            ],
        )

    print(
        f"company intelligence refreshed: forecasts={len(forecast_rows)} signals={len(signal_rows)} generated_at={generated_at.isoformat()}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
