#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import shlex
import subprocess
import tempfile
from datetime import UTC, datetime
from decimal import Decimal
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
PROMPT_PATH = ROOT / "ai" / "manager_brief_prompt.md"
SCHEMA_PATH = ROOT / "ai" / "manager_brief_schema.json"


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Generate executive manager brief for analytics_1c")
    p.add_argument("--host", default=os.getenv("CLICKHOUSE_HOST", "localhost"))
    p.add_argument("--port", type=int, default=int(os.getenv("CLICKHOUSE_PORT", "8123")))
    p.add_argument("--user", default=os.getenv("CLICKHOUSE_USER", "default"))
    p.add_argument("--password", default=os.getenv("CLICKHOUSE_PASSWORD", ""))
    p.add_argument("--database", default=os.getenv("CLICKHOUSE_DB", "analytics_1c"))
    p.add_argument(
        "--state-dir",
        default=os.getenv("AW_1C_MANAGER_BRIEF_STATE_DIR", str(ROOT / "state" / "manager-brief")),
    )
    p.add_argument(
        "--codex-user",
        default=os.getenv("AW_1C_MANAGER_BRIEF_CODEX_USER", "codex"),
    )
    p.add_argument(
        "--codex-bin",
        default=os.getenv("AW_1C_MANAGER_BRIEF_CODEX_BIN", "codex"),
    )
    p.add_argument(
        "--workdir",
        default=os.getenv("AW_1C_MANAGER_BRIEF_WORKDIR", "/home/codex/infra-admin"),
    )
    p.add_argument(
        "--model",
        default=os.getenv("AW_1C_MANAGER_BRIEF_MODEL", "gpt-5.3-codex"),
    )
    p.add_argument(
        "--top-limit",
        type=int,
        default=int(os.getenv("AW_1C_MANAGER_BRIEF_TOP_LIMIT", "5")),
    )
    p.add_argument(
        "--freshness-hours",
        type=int,
        default=int(os.getenv("AW_1C_MANAGER_BRIEF_FRESHNESS_HOURS", "8")),
    )
    p.add_argument(
        "--timeout-sec",
        type=int,
        default=int(os.getenv("AW_1C_MANAGER_BRIEF_TIMEOUT_SEC", "300")),
    )
    return p.parse_args()


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


def ch_client(args: argparse.Namespace):
    import clickhouse_connect

    return clickhouse_connect.get_client(
        host=args.host,
        port=args.port,
        username=args.user,
        password=args.password,
        database=args.database,
    )


def load_text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def write_text(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def write_json(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def q(value: str) -> str:
    return "'" + value.replace("'", "''") + "'"


def severity_rank(value: str | None) -> int:
    return {
        "none": 0,
        "low": 1,
        "medium": 2,
        "high": 3,
        "critical": 4,
    }.get((value or "none").lower(), 0)


def load_previous_artifact(state_dir: Path) -> dict[str, Any] | None:
    latest_path = state_dir / "latest.json"
    if not latest_path.exists():
        return None
    try:
        return json.loads(latest_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return None


def snapshot_from_context(context: dict[str, Any]) -> dict[tuple[Any, Any], dict[str, Any]]:
    snapshot_items = context.get("portfolio_snapshot") or []
    if snapshot_items:
        return {
            (item.get("infobase"), item.get("counterparty")): item
            for item in snapshot_items
        }

    merged: dict[tuple[Any, Any], dict[str, Any]] = {}
    for source_name in ("top_risks", "top_forecasts", "watchlist", "busy_bases"):
        for item in context.get(source_name, []):
            key = (item.get("infobase"), item.get("counterparty"))
            if key not in merged:
                merged[key] = dict(item)
            else:
                merged[key].update({k: v for k, v in item.items() if v not in (None, "")})
    return merged


def build_context(client, top_limit: int, freshness_hours: int) -> dict[str, Any]:
    now = datetime.now(UTC)

    portfolio_summary = rows_to_dict(
        client.query(
            """
            SELECT
                count() AS companies_total,
                countIf(signal_severity = 'critical') AS critical_total,
                countIf(signal_severity = 'high') AS high_total,
                countIf(signal_severity = 'medium') AS medium_total,
                countIf(signal_severity = 'low') AS low_total,
                countIf(signal_severity = 'none') AS none_total,
                countIf(registry_match_mode = 'direct') AS direct_total,
                countIf(registry_match_mode = 'alias') AS alias_total,
                countIf(registry_match_mode = 'manual') AS manual_total,
                countIf(registry_match_mode = 'none') AS unmatched_total,
                countIf(days_since_last_activity >= 7) AS stale_7d_total,
                countIf(days_since_last_activity >= 14) AS stale_14d_total,
                countIf(current_status = 'busy' OR active_locks > 0 OR temp_db_present > 0) AS busy_total,
                round(sum(amount_30d), 2) AS activity_30d_total,
                round(sum(amount_forecast_30d), 2) AS activity_forecast_30d_total,
                sum(open_cases_total) AS open_cases_total,
                sum(detections_total) AS detections_total
            FROM analytics_1c.v_company_portfolio_overview
            """
        )
    )[0]

    freshness = rows_to_dict(
        client.query(
            """
            SELECT
                (SELECT max(ts) FROM analytics_1c.documents) AS documents_ts,
                (SELECT max(ts) FROM analytics_1c.companies) AS companies_ts,
                (SELECT max(ts) FROM analytics_1c.reglog_events) AS reglog_ts,
                (SELECT max(ts) FROM analytics_1c.audit_events) AS audit_ts,
                (SELECT max(ts) FROM analytics_1c.host_events) AS host_ts,
                (SELECT max(generated_at) FROM analytics_1c.company_forecasts) AS forecasts_ts,
                (SELECT max(generated_at) FROM analytics_1c.company_health_signals) AS signals_ts
            """
        )
    )[0]
    freshness_items: list[dict[str, Any]] = []
    for source, ts in freshness.items():
        lag_hours = None
        stale = True
        if isinstance(ts, str):
            parsed = datetime.fromisoformat(ts)
            if parsed.tzinfo is None:
                parsed = parsed.replace(tzinfo=UTC)
            lag_hours = round((now - parsed).total_seconds() / 3600, 2)
            stale = lag_hours > freshness_hours
            ts = parsed.isoformat()
        freshness_items.append(
            {
                "source": source,
                "latest_ts": ts,
                "lag_hours": lag_hours,
                "stale": stale,
            }
        )

    top_risks = rows_to_dict(
        client.query(
            f"""
            SELECT
                infobase,
                counterparty,
                normalized_counterparty,
                registry_match_mode,
                registry_assignee_name,
                signal_severity,
                signal_score,
                top_signal,
                current_status,
                active_locks,
                open_cases_total,
                detections_total,
                days_since_last_activity,
                round(amount_30d, 2) AS amount_30d,
                round(amount_forecast_30d, 2) AS amount_forecast_30d
            FROM analytics_1c.v_company_portfolio_overview
            ORDER BY signal_score DESC, open_cases_total DESC, detections_total DESC, amount_30d DESC, counterparty
            LIMIT {int(top_limit)}
            """
        )
    )

    top_forecasts = rows_to_dict(
        client.query(
            f"""
            SELECT
                infobase,
                counterparty,
                normalized_counterparty,
                registry_match_mode,
                signal_severity,
                signal_score,
                round(amount_30d, 2) AS amount_30d,
                round(amount_forecast_30d, 2) AS amount_forecast_30d,
                round(docs_forecast_30d, 2) AS docs_forecast_30d,
                amount_forecast_confidence,
                top_signal
            FROM analytics_1c.v_company_portfolio_overview
            ORDER BY amount_forecast_30d DESC, signal_score DESC, amount_30d DESC, counterparty
            LIMIT {int(top_limit)}
            """
        )
    )

    watchlist = rows_to_dict(
        client.query(
            f"""
            SELECT
                s.infobase,
                s.counterparty,
                p.normalized_counterparty,
                p.registry_match_mode,
                s.signal_type,
                s.severity,
                s.score,
                s.summary,
                p.days_since_last_activity,
                round(p.amount_30d, 2) AS amount_30d,
                round(p.amount_forecast_30d, 2) AS amount_forecast_30d
            FROM analytics_1c.v_company_health_current AS s
            LEFT JOIN analytics_1c.v_company_portfolio_overview AS p
              ON p.infobase = s.infobase AND p.counterparty = s.counterparty
            WHERE s.signal_type IN ('inactive_company', 'amount_drop', 'docs_stopped')
            ORDER BY s.score DESC, p.days_since_last_activity DESC, p.amount_30d DESC
            LIMIT {int(top_limit)}
            """
        )
    )

    busy_bases = rows_to_dict(
        client.query(
            f"""
            SELECT
                infobase,
                counterparty,
                normalized_counterparty,
                current_status,
                active_locks,
                temp_db_present,
                scheduler_touched,
                current_activity_score,
                signal_severity,
                signal_score
            FROM analytics_1c.v_company_portfolio_overview
            WHERE current_status = 'busy' OR active_locks > 0 OR temp_db_present > 0
            ORDER BY active_locks DESC, temp_db_present DESC, current_activity_score DESC, counterparty
            LIMIT {int(top_limit)}
            """
        )
    )

    recent_cases = rows_to_dict(
        client.query(
            f"""
            SELECT
                c.opened_at,
                c.infobase,
                c.entity_id AS counterparty,
                c.title,
                c.severity,
                c.status
            FROM analytics_1c.cases AS c
            WHERE c.entity_type = 'counterparty' AND c.status != 'closed'
            ORDER BY c.opened_at DESC
            LIMIT {int(top_limit)}
            """
        )
    )

    portfolio_snapshot = rows_to_dict(
        client.query(
            """
            SELECT
                infobase,
                counterparty,
                normalized_counterparty,
                registry_match_mode,
                signal_severity,
                signal_score,
                current_status,
                active_locks,
                days_since_last_activity,
                round(amount_30d, 2) AS amount_30d,
                round(amount_forecast_30d, 2) AS amount_forecast_30d,
                open_cases_total,
                detections_total
            FROM analytics_1c.v_company_portfolio_overview
            ORDER BY counterparty, infobase
            """
        )
    )

    return {
        "generated_at": now.isoformat(),
        "freshness_threshold_hours": freshness_hours,
        "portfolio_summary": portfolio_summary,
        "freshness": freshness_items,
        "top_risks": top_risks,
        "top_forecasts": top_forecasts,
        "watchlist": watchlist,
        "busy_bases": busy_bases,
        "recent_cases": recent_cases,
        "portfolio_snapshot": portfolio_snapshot,
    }


def compute_delta_context(current: dict[str, Any], previous_artifact: dict[str, Any] | None) -> dict[str, Any]:
    if not previous_artifact:
        return {
            "available": False,
            "reason": "no previous brief artifact",
            "current_generated_at": current.get("generated_at"),
        }

    previous = previous_artifact.get("context", {})
    current_summary = current.get("portfolio_summary", {})
    previous_summary = previous.get("portfolio_summary", {})
    current_watchlist = {(item.get("infobase"), item.get("counterparty")) for item in current.get("watchlist", [])}
    previous_watchlist = {(item.get("infobase"), item.get("counterparty")) for item in previous.get("watchlist", [])}
    current_snapshot = snapshot_from_context(current)
    previous_snapshot = snapshot_from_context(previous)

    delta_summary = {
        "companies_total_delta": current_summary.get("companies_total", 0) - previous_summary.get("companies_total", 0),
        "critical_total_delta": current_summary.get("critical_total", 0) - previous_summary.get("critical_total", 0),
        "high_total_delta": current_summary.get("high_total", 0) - previous_summary.get("high_total", 0),
        "busy_total_delta": current_summary.get("busy_total", 0) - previous_summary.get("busy_total", 0),
        "open_cases_total_delta": current_summary.get("open_cases_total", 0) - previous_summary.get("open_cases_total", 0),
        "detections_total_delta": current_summary.get("detections_total", 0) - previous_summary.get("detections_total", 0),
        "activity_30d_total_delta": round(
            float(current_summary.get("activity_30d_total", 0) or 0)
            - float(previous_summary.get("activity_30d_total", 0) or 0),
            2,
        ),
        "activity_forecast_30d_total_delta": round(
            float(current_summary.get("activity_forecast_30d_total", 0) or 0)
            - float(previous_summary.get("activity_forecast_30d_total", 0) or 0),
            2,
        ),
    }

    new_critical: list[str] = []
    resolved_critical: list[str] = []
    top_changes: list[dict[str, Any]] = []

    for key, current_item in current_snapshot.items():
        previous_item = previous_snapshot.get(key)
        if not previous_item:
            continue

        current_severity = str(current_item.get("signal_severity") or "none")
        previous_severity = str(previous_item.get("signal_severity") or "none")
        current_rank = severity_rank(current_severity)
        previous_rank = severity_rank(previous_severity)
        score_before = int(previous_item.get("signal_score") or 0)
        score_after = int(current_item.get("signal_score") or 0)
        score_delta = score_after - score_before
        cases_before = int(previous_item.get("open_cases_total") or 0)
        cases_after = int(current_item.get("open_cases_total") or 0)
        cases_delta = cases_after - cases_before
        detections_before = int(previous_item.get("detections_total") or 0)
        detections_after = int(current_item.get("detections_total") or 0)
        detections_delta = detections_after - detections_before
        locks_before = int(previous_item.get("active_locks") or 0)
        locks_after = int(current_item.get("active_locks") or 0)
        locks_delta = locks_after - locks_before
        forecast_before = float(previous_item.get("amount_forecast_30d") or 0)
        forecast_after = float(current_item.get("amount_forecast_30d") or 0)
        forecast_delta = round(forecast_after - forecast_before, 2)

        if current_severity == "critical" and previous_severity != "critical":
            new_critical.append(str(current_item.get("counterparty") or "-"))
        if previous_severity == "critical" and current_severity != "critical":
            resolved_critical.append(str(current_item.get("counterparty") or "-"))

        change_type = None
        summary = None
        significance = 0.0

        if current_rank > previous_rank:
            change_type = "severity_up"
            summary = f"Severity {previous_severity} -> {current_severity}, score {score_before} -> {score_after}."
            significance = max(significance, (current_rank - previous_rank) * 50 + max(score_delta, 0))
        elif current_rank < previous_rank:
            change_type = "severity_down"
            summary = f"Severity {previous_severity} -> {current_severity}, напряжение по компании снизилось."
            significance = max(significance, (previous_rank - current_rank) * 40 + abs(score_delta))

        if cases_delta > 0 and cases_delta * 6 > significance:
            change_type = "cases_up"
            summary = f"Открытых кейсов стало больше: {cases_before} -> {cases_after}."
            significance = cases_delta * 6 + max(score_delta, 0)

        if locks_delta > 0 and locks_delta * 8 > significance:
            change_type = "locks_up"
            summary = f"Активные блокировки выросли: {locks_before} -> {locks_after}."
            significance = locks_delta * 8 + max(score_delta, 0)

        if forecast_delta < 0:
            forecast_drop_pct = abs(forecast_delta) / max(abs(forecast_before), 1.0) * 100.0
            if forecast_drop_pct > significance:
                change_type = "forecast_drop"
                summary = f"Прогноз активности 30д снизился: {round(forecast_before, 2)} -> {round(forecast_after, 2)}."
                significance = forecast_drop_pct
        elif forecast_delta > 0:
            forecast_growth_pct = abs(forecast_delta) / max(abs(forecast_before), 1.0) * 100.0
            if forecast_growth_pct > significance and not change_type:
                change_type = "forecast_growth"
                summary = f"Прогноз активности 30д вырос: {round(forecast_before, 2)} -> {round(forecast_after, 2)}."
                significance = forecast_growth_pct

        if detections_delta > 0 and detections_delta * 4 > significance:
            change_type = "detections_up"
            summary = f"Число detections выросло: {detections_before} -> {detections_after}."
            significance = detections_delta * 4

        if not change_type:
            continue

        top_changes.append(
            {
                "infobase": current_item.get("infobase"),
                "company": current_item.get("counterparty"),
                "normalized_counterparty": current_item.get("normalized_counterparty"),
                "registry_match_mode": current_item.get("registry_match_mode"),
                "change_type": change_type,
                "summary": summary,
                "severity_before": previous_severity,
                "severity_after": current_severity,
                "score_before": score_before,
                "score_after": score_after,
                "score_delta": score_delta,
                "open_cases_before": cases_before,
                "open_cases_after": cases_after,
                "open_cases_delta": cases_delta,
                "detections_before": detections_before,
                "detections_after": detections_after,
                "detections_delta": detections_delta,
                "active_locks_before": locks_before,
                "active_locks_after": locks_after,
                "active_locks_delta": locks_delta,
                "forecast_before": round(forecast_before, 2),
                "forecast_after": round(forecast_after, 2),
                "forecast_delta": forecast_delta,
                "significance": round(significance, 2),
            }
        )

    entered_watchlist = sorted(
        key[1] for key in current_watchlist - previous_watchlist if key[1]
    )
    left_watchlist = sorted(
        key[1] for key in previous_watchlist - current_watchlist if key[1]
    )

    top_changes.sort(
        key=lambda item: (
            float(item.get("significance") or 0),
            int(item.get("score_after") or 0),
            int(item.get("open_cases_after") or 0),
        ),
        reverse=True,
    )

    delta_summary.update(
        {
            "new_critical_total": len(new_critical),
            "resolved_critical_total": len(resolved_critical),
            "entered_watchlist_total": len(entered_watchlist),
            "left_watchlist_total": len(left_watchlist),
        }
    )

    return {
        "available": True,
        "previous_generated_at": previous.get("generated_at") or previous_artifact.get("generated_at"),
        "current_generated_at": current.get("generated_at"),
        "summary": delta_summary,
        "new_critical": new_critical[:10],
        "resolved_critical": resolved_critical[:10],
        "entered_watchlist": entered_watchlist[:10],
        "left_watchlist": left_watchlist[:10],
        "top_changes": top_changes[:15],
    }


def render_deterministic_payload(context: dict[str, Any]) -> dict[str, Any]:
    summary = context["portfolio_summary"]
    freshness = context["freshness"]
    delta = context.get("delta", {})
    stale_sources = [item["source"] for item in freshness if item["stale"]]
    top_risks = context["top_risks"][:5]
    top_forecasts = context["top_forecasts"][:5]
    watchlist = context["watchlist"][:5]

    headline = (
        f"Портфель {summary['companies_total']} компаний: критичных {summary['critical_total']}, "
        f"high {summary['high_total']}, stale 14д {summary['stale_14d_total']}."
    )
    summary_lines = [
        f"Покрытие реестра полное: direct {summary['direct_total']}, alias {summary['alias_total']}, manual {summary['manual_total']}, unmatched {summary['unmatched_total']}.",
        f"Суммарная активность за 30 дней {summary['activity_30d_total']}, прогнозная активность на 30 дней {summary['activity_forecast_30d_total']}.",
        f"Открытых кейсов по компаниям {summary['open_cases_total']}, активных detections {summary['detections_total']}.",
    ]
    if delta.get("available"):
        delta_summary = delta.get("summary", {})
        summary_lines.append(
            "С прошлого запуска: "
            f"critical {delta_summary.get('critical_total_delta', 0):+d}, "
            f"busy {delta_summary.get('busy_total_delta', 0):+d}, "
            f"кейсы {delta_summary.get('open_cases_total_delta', 0):+d}, "
            f"detections {delta_summary.get('detections_total_delta', 0):+d}."
        )
        if delta.get("top_changes"):
            leaders = ", ".join(item["company"] for item in delta["top_changes"][:3] if item.get("company"))
            if leaders:
                summary_lines.append(f"Главные изменения с прошлого запуска: {leaders}.")
    if stale_sources:
        summary_lines.append(f"Есть просрочка по источникам: {', '.join(stale_sources)}.")
    else:
        summary_lines.append("Свежесть источников укладывается в заданный порог.")

    risk_items = [
        {
            "company": item["counterparty"],
            "severity": item["signal_severity"],
            "reason": item["top_signal"] or "Повышенный signal score без детализации top_signal.",
            "recommended_action": (
                "Проверить открытые кейсы, detections и фактическую занятость файловой базы."
                if item["open_cases_total"] or item["detections_total"] or item["active_locks"]
                else "Проверить последние события по компании и причину роста operational severity."
            ),
        }
        for item in top_risks
    ]

    forecast_items = [
        {
            "company": item["counterparty"],
            "forecast_30d": str(item["amount_forecast_30d"]),
            "interpretation": (
                f"Текущая активность 30д {item['amount_30d']}, match {item['registry_match_mode']}, severity {item['signal_severity']}."
            ),
        }
        for item in top_forecasts
    ]

    actions = [
        "Разобрать компании с открытыми кейсами и максимальным signal score в первую очередь.",
        "Проверить watchlist по inactivity/amount_drop/docs_stopped и подтвердить, это бизнес-пауза или operational сбой.",
        "Отдельно пройти по manual-match компаниям перед управленческими выводами из реестра.",
    ]
    if delta.get("available") and delta.get("summary", {}).get("new_critical_total", 0) > 0:
        actions.insert(0, f"Сначала разобрать новые critical-компании: {', '.join(delta.get('new_critical', [])[:3])}.")
    if watchlist:
        actions[1] = (
            f"Проверить watchlist: {', '.join(item['counterparty'] for item in watchlist[:3])}."
        )

    caveats = [
        "Показатель amount здесь трактуется как activity score, а не как деньги или выручка.",
        "Severity operational-driven: high/critical отражают кейсы, detections и занятость базы, а не автоматически финансовый риск.",
    ]
    if summary["manual_total"] > 0:
        caveats.append("Компании с registry_match_mode=manual требуют осторожности при юридической интерпретации реестра.")

    return {
        "headline": headline,
        "summary": summary_lines[:6],
        "top_risks": risk_items[:5],
        "top_forecasts": forecast_items[:5],
        "actions": actions[:5],
        "caveats": caveats[:4],
    }


def render_markdown(payload: dict[str, Any], generated_at: str) -> str:
    lines = [
        f"# Executive Brief 1C",
        "",
        f"_Сформировано: {generated_at}_",
        "",
        f"## Заголовок",
        payload["headline"],
        "",
        "## Кратко",
    ]
    for item in payload["summary"]:
        lines.append(f"- {item}")
    lines.extend(["", "## Компании риска"])
    for idx, item in enumerate(payload["top_risks"], start=1):
        lines.append(
            f"{idx}. {item['company']} [{item['severity']}] — {item['reason']} Действие: {item['recommended_action']}"
        )
    lines.extend(["", "## Прогноз по активности 30д"])
    for idx, item in enumerate(payload["top_forecasts"], start=1):
        lines.append(
            f"{idx}. {item['company']} — прогноз {item['forecast_30d']}. {item['interpretation']}"
        )
    lines.extend(["", "## Рекомендуемые действия"])
    for item in payload["actions"]:
        lines.append(f"- {item}")
    lines.extend(["", "## Ограничения"])
    for item in payload["caveats"]:
        lines.append(f"- {item}")
    lines.append("")
    return "\n".join(lines)


def run_codex(prompt: str, args: argparse.Namespace) -> tuple[int, str, str]:
    output_file = Path(tempfile.mkstemp(prefix="aw-1c-manager-brief-", suffix=".json")[1])
    os.chmod(output_file, 0o666)
    cmd_inner = (
        f"cd {shlex.quote(args.workdir)} && "
        f"{shlex.quote(args.codex_bin)} exec --ephemeral --skip-git-repo-check "
        f"--model {shlex.quote(args.model)} "
        f"-C {shlex.quote(args.workdir)} "
        f"-s read-only "
        f"--color never "
        f"--output-schema {shlex.quote(str(SCHEMA_PATH))} "
        f"-o {shlex.quote(str(output_file))} -"
    )
    if os.geteuid() == 0 and args.codex_user:
        cmd = ["sudo", "-u", args.codex_user, "-H", "bash", "-lc", cmd_inner]
    else:
        cmd = ["bash", "-lc", cmd_inner]

    try:
        result = subprocess.run(
            cmd,
            input=prompt,
            text=True,
            capture_output=True,
            timeout=args.timeout_sec,
            check=False,
        )
        reply = output_file.read_text(encoding="utf-8").strip() if output_file.exists() else ""
        stdout_stderr = (result.stdout or "") + ("\n" + result.stderr if result.stderr else "")
        return result.returncode, stdout_stderr.strip(), reply
    finally:
        try:
            output_file.unlink()
        except FileNotFoundError:
            pass


def build_prompt(context: dict[str, Any]) -> str:
    template = load_text(PROMPT_PATH)
    return template.replace(
        "{{CONTEXT_JSON}}",
        json.dumps(context, ensure_ascii=False, indent=2),
    )


def save_artifacts(
    state_dir: Path,
    artifact: dict[str, Any],
    markdown: str,
) -> None:
    timestamp = datetime.fromisoformat(artifact["generated_at"]).strftime("%Y%m%dT%H%M%SZ")
    history_dir = state_dir / "history"
    history_dir.mkdir(parents=True, exist_ok=True)
    latest_json = state_dir / "latest.json"
    latest_md = state_dir / "latest.md"
    history_json = history_dir / f"{timestamp}.json"
    history_md = history_dir / f"{timestamp}.md"
    write_json(latest_json, artifact)
    write_text(latest_md, markdown)
    write_json(history_json, artifact)
    write_text(history_md, markdown)


def main() -> int:
    args = parse_args()
    state_dir = Path(args.state_dir)
    state_dir.mkdir(parents=True, exist_ok=True)

    previous_artifact = load_previous_artifact(state_dir)
    client = ch_client(args)
    context = build_context(client, top_limit=args.top_limit, freshness_hours=args.freshness_hours)
    context["delta"] = compute_delta_context(context, previous_artifact)
    prompt = build_prompt(context)

    codex_rc = None
    codex_output = ""
    render_mode = "deterministic"
    payload: dict[str, Any]

    try:
        codex_rc, codex_output, codex_reply = run_codex(prompt, args)
        payload = json.loads(codex_reply) if codex_reply else {}
        if not payload:
            raise ValueError("empty codex payload")
        render_mode = "codex"
    except Exception as exc:  # noqa: BLE001
        payload = render_deterministic_payload(context)
        codex_output = f"{codex_output}\nFALLBACK: {exc}".strip()
        render_mode = "deterministic"

    generated_at = datetime.now(UTC).replace(microsecond=0).isoformat()
    markdown = render_markdown(payload, generated_at)
    artifact = {
        "generated_at": generated_at,
        "render_mode": render_mode,
        "model": args.model,
        "codex_rc": codex_rc,
        "context": context,
        "brief": payload,
        "markdown": markdown,
        "codex_output_excerpt": codex_output[-4000:] if codex_output else "",
    }
    save_artifacts(state_dir, artifact, markdown)
    print(json.dumps({"status": "ok", "render_mode": render_mode, "state_dir": str(state_dir)}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
