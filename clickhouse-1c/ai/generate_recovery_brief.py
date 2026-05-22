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

import clickhouse_connect

ROOT = Path(__file__).resolve().parents[1]
PROMPT_PATH = ROOT / "ai" / "recovery_brief_prompt.md"
SCHEMA_PATH = ROOT / "ai" / "recovery_brief_schema.json"


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Generate recovery brief for analytics_1c")
    p.add_argument("--host", default=os.getenv("CLICKHOUSE_HOST", "localhost"))
    p.add_argument("--port", type=int, default=int(os.getenv("CLICKHOUSE_PORT", "8123")))
    p.add_argument("--user", default=os.getenv("CLICKHOUSE_USER", "default"))
    p.add_argument("--password", default=os.getenv("CLICKHOUSE_PASSWORD", ""))
    p.add_argument("--database", default=os.getenv("CLICKHOUSE_DB", "analytics_1c"))
    p.add_argument(
        "--brief-state-dir",
        default=os.getenv("AW_1C_MANAGER_BRIEF_STATE_DIR", str(ROOT / "state" / "manager-brief")),
    )
    p.add_argument(
        "--weekly-digest-state-dir",
        default=os.getenv("AW_1C_WEEKLY_DIGEST_STATE_DIR", str(ROOT / "state" / "weekly-digest")),
    )
    p.add_argument(
        "--state-dir",
        default=os.getenv("AW_1C_RECOVERY_BRIEF_STATE_DIR", str(ROOT / "state" / "recovery-brief")),
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
        "--timeout-sec",
        type=int,
        default=int(os.getenv("AW_1C_MANAGER_BRIEF_TIMEOUT_SEC", "300")),
    )
    p.add_argument(
        "--top-limit",
        type=int,
        default=int(os.getenv("AW_1C_RECOVERY_BRIEF_TOP_LIMIT", "6")),
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


def q(value: str) -> str:
    return "'" + value.replace("'", "''") + "'"


def ch_client(args: argparse.Namespace):
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


def load_json_if_exists(path: Path) -> dict[str, Any] | None:
    if not path.exists():
        return None
    return json.loads(path.read_text(encoding="utf-8"))


def load_latest_json(state_dir: Path) -> dict[str, Any] | None:
    return load_json_if_exists(state_dir / "latest.json")


def build_context(client, args: argparse.Namespace) -> dict[str, Any]:
    latest_brief = load_latest_json(Path(args.brief_state_dir)) or {}
    latest_weekly_digest = load_latest_json(Path(args.weekly_digest_state_dir)) or {}

    problematic = rows_to_dict(
        client.query(
            f"""
            WITH recent AS (
                SELECT
                    infobase,
                    counterparty,
                    max(generated_at) AS latest_signal_at,
                    max(score) AS max_score,
                    sum(score) AS total_score,
                    count() AS signals_total,
                    countIf(severity = 'critical') AS critical_total,
                    countIf(severity = 'high') AS high_total,
                    argMax(severity, tuple(score, generated_at)) AS top_severity,
                    argMax(signal_type, tuple(score, generated_at)) AS top_signal_type,
                    argMax(summary, tuple(score, generated_at)) AS top_summary
                FROM analytics_1c.company_health_signals
                WHERE generated_at >= now() - INTERVAL 7 DAY
                GROUP BY infobase, counterparty
            )
            SELECT
                p.infobase AS infobase,
                p.counterparty AS counterparty,
                p.company_name,
                p.normalized_counterparty,
                p.registry_match_mode,
                p.registry_assignee_name,
                p.registry_status,
                p.signal_severity,
                p.signal_score,
                round(p.amount_30d, 2) AS amount_30d,
                round(p.amount_forecast_30d, 2) AS amount_forecast_30d,
                p.current_status,
                p.active_locks,
                p.open_cases_total,
                p.detections_total,
                r.latest_signal_at,
                r.max_score,
                r.total_score,
                r.signals_total,
                r.critical_total,
                r.high_total,
                r.top_severity,
                r.top_signal_type,
                r.top_summary
            FROM recent AS r
            INNER JOIN analytics_1c.v_company_portfolio_overview AS p
                ON p.infobase = r.infobase
               AND p.counterparty = r.counterparty
            ORDER BY r.max_score DESC, r.signals_total DESC, p.open_cases_total DESC, p.amount_30d DESC, p.counterparty
            LIMIT {int(args.top_limit)}
            """
        )
    )

    return {
        "generated_at": datetime.now(UTC).isoformat(),
        "latest_brief": {
            "generated_at": latest_brief.get("generated_at"),
            "headline": latest_brief.get("brief", {}).get("headline"),
            "summary": latest_brief.get("brief", {}).get("summary", []),
            "delta": latest_brief.get("context", {}).get("delta", {}),
            "portfolio_summary": latest_brief.get("context", {}).get("portfolio_summary", {}),
        },
        "latest_weekly_digest": {
            "generated_at": latest_weekly_digest.get("generated_at"),
            "headline": latest_weekly_digest.get("digest", {}).get("headline"),
            "summary": latest_weekly_digest.get("digest", {}).get("summary", []),
            "top_priorities": latest_weekly_digest.get("digest", {}).get("top_priorities", []),
        },
        "problematic_companies": problematic,
    }


def render_deterministic_recovery(context: dict[str, Any]) -> dict[str, Any]:
    latest_brief = context.get("latest_brief", {})
    portfolio = latest_brief.get("portfolio_summary", {})
    delta = latest_brief.get("delta", {})
    problematic = context.get("problematic_companies", [])

    headline = (
        f"Recovery-контур: хвост кейсов {portfolio.get('open_cases_total', 0)}, "
        f"critical {portfolio.get('critical_total', 0)}/{portfolio.get('companies_total', 0)}."
    )
    situation = [
        f"Открытых кейсов {portfolio.get('open_cases_total', 0)}, detections {portfolio.get('detections_total', 0)}.",
        f"С последнего запуска: open cases {int(delta.get('summary', {}).get('open_cases_total_delta', 0) or 0):+d}, detections {int(delta.get('summary', {}).get('detections_total_delta', 0) or 0):+d}.",
        "Проблема в накоплении operational-хвоста, а не в резком обвале активности портфеля.",
    ]
    portfolio_actions = [
        "Сжать фокус до 5–6 компаний первой очереди и перестать размазывать контроль по всему портфелю.",
        "По каждой компании первой очереди фиксировать только: причина, владелец, срок, факт снижения кейсов.",
        "Сначала убирать рост открытых кейсов и блокировок, а не обсуждать общий красный фон.",
    ]
    top_incidents = []
    for item in problematic[:6]:
        company = str(item.get("counterparty") or "-")
        actions = [
            "Проверить владельца и состав открытых кейсов по компании.",
            "Подтвердить, что по компании есть план снижения хвоста в ближайшие 24 часа.",
        ]
        if int(item.get("active_locks") or 0) > 0:
            actions.append("Проверить busy/lock-контур базы и не держать блокировки без владельца.")
        if item.get("registry_match_mode") == "manual":
            actions.append("Сначала подтвердить корректность manual-сопоставления.")
        top_incidents.append(
            {
                "company": company,
                "severity": str(item.get("signal_severity") or item.get("top_severity") or "critical"),
                "diagnosis": (
                    f"Открытые кейсы {item.get('open_cases_total')}, detections {item.get('detections_total')}, "
                    f"блокировки {item.get('active_locks')}, top signal: {item.get('top_signal_type') or '-'}."
                ),
                "actions": actions[:4],
                "stop_doing": (
                    "Не разбирать компанию общими совещаниями без владельца и без числовой цели на день."
                ),
                "target_state_24h": (
                    f"Снижение открытых кейсов ниже {max(int(item.get('open_cases_total') or 0) - 3, 0)} и отсутствие нового прироста по следующему запуску."
                ),
            }
        )
    caveats = [
        "Operational severity не равна финансовому кризису портфеля.",
        "Manual/alias сопоставления нельзя трактовать как окончательное юридическое соответствие без проверки.",
    ]
    return {
        "headline": headline,
        "situation": situation[:6],
        "portfolio_actions": portfolio_actions[:6],
        "top_incidents": top_incidents,
        "caveats": caveats[:5],
    }


def render_markdown(payload: dict[str, Any], generated_at: str) -> str:
    lines = [
        "# Recovery Brief 1C",
        "",
        f"_Сформировано: {generated_at}_",
        "",
        "## Заголовок",
        payload["headline"],
        "",
        "## Ситуация",
    ]
    for item in payload["situation"]:
        lines.append(f"- {item}")
    lines.extend(["", "## Действия по портфелю"])
    for item in payload["portfolio_actions"]:
        lines.append(f"- {item}")
    lines.extend(["", "## Компании первой очереди"])
    for idx, item in enumerate(payload["top_incidents"], start=1):
        lines.append(f"{idx}. {item['company']} [{item['severity']}] — {item['diagnosis']}")
        for action in item["actions"]:
            lines.append(f"   - {action}")
        lines.append(f"   - Стоп: {item['stop_doing']}")
        lines.append(f"   - Цель 24ч: {item['target_state_24h']}")
    lines.extend(["", "## Ограничения"])
    for item in payload["caveats"]:
        lines.append(f"- {item}")
    lines.append("")
    return "\n".join(lines)


def run_codex(prompt: str, args: argparse.Namespace) -> tuple[int, str, str]:
    output_file = Path(tempfile.mkstemp(prefix="aw-1c-recovery-brief-", suffix=".json")[1])
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
    return template.replace("{{CONTEXT_JSON}}", json.dumps(context, ensure_ascii=False, indent=2))


def save_artifacts(state_dir: Path, artifact: dict[str, Any], markdown: str) -> None:
    timestamp = datetime.fromisoformat(artifact["generated_at"]).strftime("%Y%m%dT%H%M%SZ")
    history_dir = state_dir / "history"
    history_dir.mkdir(parents=True, exist_ok=True)
    write_json(state_dir / "latest.json", artifact)
    write_text(state_dir / "latest.md", markdown)
    write_json(history_dir / f"{timestamp}.json", artifact)
    write_text(history_dir / f"{timestamp}.md", markdown)


def main() -> int:
    args = parse_args()
    state_dir = Path(args.state_dir)
    state_dir.mkdir(parents=True, exist_ok=True)
    client = ch_client(args)
    context = build_context(client, args)
    prompt = build_prompt(context)

    render_mode = "deterministic"
    model = "deterministic"
    payload = render_deterministic_recovery(context)
    codex_output = ""

    try:
        rc, stdout_stderr, reply = run_codex(prompt, args)
        codex_output = stdout_stderr
        if rc == 0 and reply:
            candidate = json.loads(reply)
            if candidate:
                payload = candidate
                render_mode = "codex"
                model = args.model
    except Exception as exc:  # noqa: BLE001
        codex_output = f"{codex_output}\nFALLBACK: {exc}".strip()

    generated_at = datetime.now(UTC).replace(microsecond=0).isoformat()
    markdown = render_markdown(payload, generated_at)
    artifact = {
        "generated_at": generated_at,
        "render_mode": render_mode,
        "model": model,
        "context": context,
        "recovery": payload,
        "markdown": markdown,
        "codex_output_excerpt": codex_output[-4000:] if codex_output else "",
    }
    save_artifacts(state_dir, artifact, markdown)
    print(json.dumps({"status": "ok", "render_mode": render_mode, "state_dir": str(state_dir)}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
