#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import shlex
import subprocess
import tempfile
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

from company_intelligence_api import build_weekly_trend_report, manager_brief_state_dir

ROOT = Path(__file__).resolve().parents[1]
PROMPT_PATH = ROOT / "ai" / "weekly_digest_prompt.md"
SCHEMA_PATH = ROOT / "ai" / "weekly_digest_schema.json"


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Generate weekly executive digest for analytics_1c")
    p.add_argument(
        "--brief-state-dir",
        default=os.getenv("AW_1C_MANAGER_BRIEF_STATE_DIR", str(manager_brief_state_dir())),
    )
    p.add_argument(
        "--state-dir",
        default=os.getenv("AW_1C_WEEKLY_DIGEST_STATE_DIR", str(ROOT / "state" / "weekly-digest")),
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
        "--days",
        type=int,
        default=int(os.getenv("AW_1C_WEEKLY_DIGEST_DAYS", "7")),
    )
    return p.parse_args()


def load_text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def write_text(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def write_json(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def load_latest_brief(brief_state_dir: Path) -> dict[str, Any]:
    latest_path = brief_state_dir / "latest.json"
    if not latest_path.exists():
        raise RuntimeError(f"missing latest manager brief: {latest_path}")
    return json.loads(latest_path.read_text(encoding="utf-8"))


def load_brief_history_payloads(brief_state_dir: Path, limit: int = 400) -> list[dict[str, Any]]:
    history_dir = brief_state_dir / "history"
    if not history_dir.exists():
        return []
    items: list[dict[str, Any]] = []
    for path in sorted(history_dir.glob("*.json"), reverse=True)[:limit]:
        payload = json.loads(path.read_text(encoding="utf-8"))
        payload["_path"] = path.name
        items.append(payload)
    return items


def classify_improvements(payloads: list[dict[str, Any]], limit: int = 5) -> list[dict[str, Any]]:
    improvements: dict[tuple[str, str], dict[str, Any]] = {}
    for payload in payloads:
        delta = payload.get("context", {}).get("delta") or {}
        generated_at = payload.get("generated_at")
        for item in delta.get("top_changes", []):
            change_type = str(item.get("change_type") or "")
            score_delta = float(item.get("score_delta") or 0)
            open_cases_delta = int(item.get("open_cases_delta") or 0)
            forecast_delta = float(item.get("forecast_delta") or 0)
            improved = False
            signal = ""
            if change_type == "severity_down":
                improved = True
                signal = f"severity снизилась {item.get('severity_before')} -> {item.get('severity_after')}"
            elif change_type == "forecast_growth" and forecast_delta > 0:
                improved = True
                signal = f"прогноз 30д вырос на {round(forecast_delta, 2)}"
            elif score_delta < 0 or open_cases_delta < 0:
                improved = True
                signal = (
                    f"score Δ {int(score_delta)}"
                    if score_delta < 0
                    else f"open cases Δ {open_cases_delta}"
                )
            if not improved:
                continue
            key = (str(item.get("infobase") or ""), str(item.get("company") or ""))
            candidate = {
                "company": item.get("company"),
                "infobase": item.get("infobase"),
                "signal": signal,
                "meaning": str(item.get("summary") or "Нагрузка по компании ослабла."),
                "generated_at": generated_at,
                "severity_after": item.get("severity_after"),
            }
            existing = improvements.get(key)
            if existing is None:
                improvements[key] = candidate
    return list(improvements.values())[:limit]


def build_context(brief_state_dir: Path, days: int) -> dict[str, Any]:
    latest_brief = load_latest_brief(brief_state_dir)
    history_payloads = load_brief_history_payloads(brief_state_dir, limit=400)
    weekly_report = build_weekly_trend_report(history_payloads, days=days)
    latest_context = latest_brief.get("context", {})
    latest_summary = latest_context.get("portfolio_summary", {})
    daily = weekly_report.get("daily", [])
    first_day = daily[0] if daily else {}
    last_day = daily[-1] if daily else {}
    week_delta = {
        "critical_total_delta": int(last_day.get("critical_total", 0) or 0) - int(first_day.get("critical_total", 0) or 0),
        "busy_total_delta": int(last_day.get("busy_total", 0) or 0) - int(first_day.get("busy_total", 0) or 0),
        "open_cases_total_delta": int(last_day.get("open_cases_total", 0) or 0) - int(first_day.get("open_cases_total", 0) or 0),
        "detections_total_delta": int(last_day.get("detections_total", 0) or 0) - int(first_day.get("detections_total", 0) or 0),
        "activity_30d_total_delta": round(float(last_day.get("activity_30d_total", 0) or 0) - float(first_day.get("activity_30d_total", 0) or 0), 2),
        "activity_forecast_30d_total_delta": round(float(last_day.get("activity_forecast_30d_total", 0) or 0) - float(first_day.get("activity_forecast_30d_total", 0) or 0), 2),
    }
    return {
        "generated_at": datetime.now(UTC).isoformat(),
        "period_start": weekly_report.get("period_start"),
        "period_end": weekly_report.get("period_end"),
        "days": days,
        "latest_summary": latest_summary,
        "freshness": latest_context.get("freshness", []),
        "top_weekly_changes": weekly_report.get("top_weekly_changes", []),
        "daily": daily,
        "week_delta": week_delta,
        "improvements": classify_improvements(history_payloads),
        "latest_brief_headline": latest_brief.get("brief", {}).get("headline", ""),
    }


def render_deterministic_digest(context: dict[str, Any]) -> dict[str, Any]:
    latest = context.get("latest_summary", {})
    week_delta = context.get("week_delta", {})
    top_changes = context.get("top_weekly_changes", [])
    improvements = context.get("improvements", [])
    stale_sources = [item["source"] for item in context.get("freshness", []) if item.get("stale")]

    headline = (
        f"Неделя: critical {latest.get('critical_total', 0)}, busy {latest.get('busy_total', 0)}, "
        f"кейсы {latest.get('open_cases_total', 0)}."
    )
    summary = [
        f"За неделю: critical {week_delta.get('critical_total_delta', 0):+d}, busy {week_delta.get('busy_total_delta', 0):+d}, кейсы {week_delta.get('open_cases_total_delta', 0):+d}, detections {week_delta.get('detections_total_delta', 0):+d}.",
        f"Текущий портфель: компаний {latest.get('companies_total', 0)}, direct {latest.get('direct_total', 0)}, alias {latest.get('alias_total', 0)}, manual {latest.get('manual_total', 0)}.",
        f"Активность 30д {latest.get('activity_30d_total', 0)}, прогноз 30д {latest.get('activity_forecast_30d_total', 0)}.",
    ]
    if stale_sources:
        summary.append(f"Есть просроченные источники: {', '.join(stale_sources)}.")
    else:
        summary.append("Источник данных по неделе свежий, контур не выглядит просроченным.")

    top_priorities = []
    for item in top_changes[:5]:
        top_priorities.append(
            {
                "company": str(item.get("company") or "-"),
                "priority": str(item.get("priority_tier") or "low"),
                "reason": str(item.get("priority_reason") or item.get("summary") or "-"),
                "recommended_action": (
                    "Проверить открытые кейсы, detections и фактическую занятость базы."
                    if int(item.get("open_cases_delta") or 0) > 0 or int(item.get("active_locks_delta") or 0) > 0
                    else "Проверить причину weekly-сдвига и подтвердить, что это не накопленный operational шум."
                ),
            }
        )

    actions = [
        "Сначала разбирать weekly priority critical/high, а не весь красный портфель подряд.",
        "Для компаний с ростом кейсов и блокировок подтвердить, это operational перегрузка или реальный бизнес-сбой.",
        "По manual-match компаниям не делать жёстких выводов без проверки реестровой привязки.",
    ]
    if top_priorities:
        actions.insert(0, f"Приоритет недели: {', '.join(item['company'] for item in top_priorities[:3])}.")

    caveats = [
        "Активность здесь operational-driven и не равна деньгам или выручке.",
        "Weekly приоритет строится по изменениям сигналов, кейсов, блокировок и прогноза, а не по бухгалтерскому результату.",
    ]
    if latest.get("manual_total", 0):
        caveats.append("Manual-match компании требуют осторожности при юридической трактовке соответствия реестру.")

    return {
        "headline": headline,
        "summary": summary[:7],
        "top_priorities": top_priorities,
        "improvements": improvements[:5],
        "actions": actions[:6],
        "caveats": caveats[:5],
    }


def render_markdown(payload: dict[str, Any], generated_at: str) -> str:
    lines = [
        "# Weekly Executive Digest 1C",
        "",
        f"_Сформировано: {generated_at}_",
        "",
        "## Заголовок",
        payload["headline"],
        "",
        "## Кратко",
    ]
    for item in payload["summary"]:
        lines.append(f"- {item}")
    lines.extend(["", "## Приоритет недели"])
    for idx, item in enumerate(payload["top_priorities"], start=1):
        lines.append(f"{idx}. {item['company']} [{item['priority']}] — {item['reason']} Действие: {item['recommended_action']}")
    lines.extend(["", "## Что улучшилось"])
    for idx, item in enumerate(payload["improvements"], start=1):
        lines.append(f"{idx}. {item['company']} — {item['signal']}. {item['meaning']}")
    lines.extend(["", "## Рекомендуемые действия"])
    for item in payload["actions"]:
        lines.append(f"- {item}")
    lines.extend(["", "## Ограничения"])
    for item in payload["caveats"]:
        lines.append(f"- {item}")
    lines.append("")
    return "\n".join(lines)


def run_codex(prompt: str, args: argparse.Namespace) -> tuple[int, str, str]:
    output_file = Path(tempfile.mkstemp(prefix="aw-1c-weekly-digest-", suffix=".json")[1])
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


def main() -> int:
    args = parse_args()
    state_dir = Path(args.state_dir)
    brief_state_dir = Path(args.brief_state_dir)
    context = build_context(brief_state_dir, args.days)
    prompt = build_prompt(context)

    render_mode = "deterministic"
    model = "deterministic"
    digest = render_deterministic_digest(context)
    codex_stdout = ""

    rc, stdout_stderr, reply = run_codex(prompt, args)
    codex_stdout = stdout_stderr
    if rc == 0 and reply:
        try:
            digest = json.loads(reply)
            render_mode = "codex"
            model = args.model
        except json.JSONDecodeError:
            pass

    generated_at = datetime.now(UTC).isoformat()
    markdown = render_markdown(digest, generated_at)
    payload = {
        "generated_at": generated_at,
        "render_mode": render_mode,
        "model": model,
        "context": context,
        "digest": digest,
        "markdown": markdown,
        "codex_stdout": codex_stdout,
    }

    latest_json = state_dir / "latest.json"
    latest_md = state_dir / "latest.md"
    history_json = state_dir / "history" / f"{generated_at.replace(':', '').replace('-', '').replace('+00:00', 'Z')}.json"
    history_md = state_dir / "history" / f"{generated_at.replace(':', '').replace('-', '').replace('+00:00', 'Z')}.md"

    write_json(latest_json, payload)
    write_text(latest_md, markdown)
    write_json(history_json, payload)
    write_text(history_md, markdown)
    print(json.dumps({"status": "ok", "generated_at": generated_at, "render_mode": render_mode}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
