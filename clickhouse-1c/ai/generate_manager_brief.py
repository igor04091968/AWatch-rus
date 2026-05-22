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
    }


def render_deterministic_payload(context: dict[str, Any]) -> dict[str, Any]:
    summary = context["portfolio_summary"]
    freshness = context["freshness"]
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

    client = ch_client(args)
    context = build_context(client, top_limit=args.top_limit, freshness_hours=args.freshness_hours)
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
