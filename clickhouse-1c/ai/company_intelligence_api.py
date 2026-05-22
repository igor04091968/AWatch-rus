#!/usr/bin/env python3
from __future__ import annotations

import argparse
import html
import json
import os
from datetime import UTC, datetime
from decimal import Decimal
from pathlib import Path
from typing import Any
from urllib.parse import quote

import clickhouse_connect
import uvicorn
from fastapi import FastAPI, HTTPException, Query
from fastapi.responses import HTMLResponse, PlainTextResponse, Response


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


def manager_brief_state_dir() -> Path:
    root = Path(os.getenv("AW_1C_ROOT", "/opt/activitywatch/clickhouse-1c"))
    configured = os.getenv("AW_1C_MANAGER_BRIEF_STATE_DIR")
    if configured:
        return Path(configured)
    return root / "state" / "manager-brief"


def load_latest_manager_brief() -> dict[str, Any]:
    latest_path = manager_brief_state_dir() / "latest.json"
    if not latest_path.exists():
        raise HTTPException(status_code=404, detail="manager brief not generated yet")
    return json.loads(latest_path.read_text(encoding="utf-8"))


def load_brief_history_records(limit: int = 20) -> list[dict[str, Any]]:
    history_dir = manager_brief_state_dir() / "history"
    if not history_dir.exists():
        return []
    items: list[dict[str, Any]] = []
    for path in sorted(history_dir.glob("*.json"), reverse=True)[:limit]:
        payload = json.loads(path.read_text(encoding="utf-8"))
        items.append(
            {
                "generated_at": payload.get("generated_at"),
                "render_mode": payload.get("render_mode"),
                "model": payload.get("model"),
                "headline": payload.get("brief", {}).get("headline", ""),
                "path": path.name,
                "brief": payload.get("brief", {}),
            }
        )
    return items


def load_brief_history_record(name: str) -> dict[str, Any]:
    safe_name = Path(name).name
    if not safe_name.endswith(".json"):
        safe_name += ".json"
    path = manager_brief_state_dir() / "history" / safe_name
    if not path.exists():
        raise HTTPException(status_code=404, detail="manager brief history record not found")
    return json.loads(path.read_text(encoding="utf-8"))


def grafana_company_dashboard_url() -> str:
    return os.getenv(
        "AW_1C_MANAGER_BRIEF_GRAFANA_URL",
        "http://10.10.10.11:3000/d/1c-file-companies/1c-file-company-intelligence",
    )


def fmt_number(value: Any) -> str:
    if value is None or value == "":
        return "-"
    if isinstance(value, float):
        return f"{value:,.2f}".replace(",", " ").replace(".", ",")
    if isinstance(value, int):
        return f"{value:,}".replace(",", " ")
    return str(value)


def severity_badge(severity: str) -> str:
    tone = {
        "critical": "critical",
        "high": "high",
        "medium": "medium",
        "low": "low",
        "none": "none",
    }.get((severity or "").lower(), "none")
    return f'<span class="badge badge-{tone}">{html.escape(severity or "none")}</span>'


def company_detail_url(counterparty: str, infobase: str | None = None) -> str:
    base = f"/manager/company/{quote(counterparty)}"
    if infobase:
        return f"{base}?infobase={quote(infobase)}"
    return base


def manager_brief_history_html_url(name: str) -> str:
    safe_name = Path(name).name
    return f"/manager/briefs/{quote(safe_name)}"


def render_manager_brief_html(payload: dict[str, Any]) -> str:
    brief = payload.get("brief", {})
    context = payload.get("context", {})
    summary = context.get("portfolio_summary", {})
    freshness = context.get("freshness", [])
    top_risks = brief.get("top_risks", [])
    top_forecasts = brief.get("top_forecasts", [])
    actions = brief.get("actions", [])
    caveats = brief.get("caveats", [])
    render_mode = payload.get("render_mode", "unknown")
    generated_at = payload.get("generated_at", "")
    history_url = "/api/1/analytics-1c/manager/brief/history"
    history_html_url = "/manager/briefs"
    problematic_1d_url = "/manager/problematic?days=1"
    problematic_7d_url = "/manager/problematic?days=7"
    json_url = "/api/1/analytics-1c/manager/brief/latest"
    md_url = "/api/1/analytics-1c/manager/brief/latest.md"
    grafana_url = grafana_company_dashboard_url()

    freshness_rows = []
    for item in freshness:
        status = "stale" if item.get("stale") else "fresh"
        freshness_rows.append(
            "<tr>"
            f"<td>{html.escape(str(item.get('source', '')))}</td>"
            f"<td>{html.escape(str(item.get('latest_ts', '-')))}</td>"
            f"<td>{html.escape(str(item.get('lag_hours', '-')))}</td>"
            f"<td><span class=\"freshness freshness-{status}\">{'просрочен' if status == 'stale' else 'свежий'}</span></td>"
            "</tr>"
        )

    risk_cards = []
    for item in top_risks:
        risk_cards.append(
            "<article class=\"stack-card\">"
            f"<div class=\"stack-card-head\"><h3>{html.escape(item.get('company', '-'))}</h3>{severity_badge(item.get('severity', ''))}</div>"
            f"<p class=\"stack-card-body\">{html.escape(item.get('reason', '-'))}</p>"
            f"<p class=\"stack-card-action\"><strong>Действие:</strong> {html.escape(item.get('recommended_action', '-'))}</p>"
            f"<p class=\"stack-card-action\"><a class=\"inline-link\" href=\"{company_detail_url(item.get('company', '-'))}\">Открыть карточку компании</a></p>"
            "</article>"
        )

    forecast_cards = []
    for item in top_forecasts:
        company = item.get("company", "-")
        forecast_cards.append(
            "<article class=\"stack-card\">"
            f"<div class=\"stack-card-head\"><h3>{html.escape(company)}</h3></div>"
            f"<p class=\"metric-line\"><strong>Прогноз 30д:</strong> {html.escape(item.get('forecast_30d', '-'))}</p>"
            f"<p class=\"stack-card-body\">{html.escape(item.get('interpretation', '-'))}</p>"
            f"<a class=\"inline-link\" href=\"{company_detail_url(company)}\">Карточка компании</a>"
            "</article>"
        )

    summary_items = "\n".join(f"<li>{html.escape(str(item))}</li>" for item in brief.get("summary", []))
    action_items = "\n".join(f"<li>{html.escape(str(item))}</li>" for item in actions)
    caveat_items = "\n".join(f"<li>{html.escape(str(item))}</li>" for item in caveats)

    return f"""<!doctype html>
<html lang="ru">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta http-equiv="refresh" content="300">
  <title>1C Executive Brief</title>
  <style>
    :root {{
      --bg: #f4f1ea;
      --paper: #fffdf8;
      --ink: #1c1a17;
      --muted: #6b655c;
      --line: #d8d0c4;
      --accent: #005f73;
      --accent-soft: #d9eef2;
      --critical: #9b2226;
      --high: #bb3e03;
      --medium: #ca6702;
      --low: #4d7c0f;
      --none: #687076;
      --fresh: #1d6f42;
      --stale: #9b2226;
      --shadow: 0 14px 40px rgba(28, 26, 23, 0.08);
    }}
    * {{ box-sizing: border-box; }}
    body {{
      margin: 0;
      font-family: \"IBM Plex Sans\", \"Segoe UI\", system-ui, sans-serif;
      background:
        radial-gradient(circle at top left, rgba(0, 95, 115, 0.08), transparent 28%),
        linear-gradient(180deg, #faf7f2 0%, var(--bg) 100%);
      color: var(--ink);
    }}
    .shell {{
      max-width: 1380px;
      margin: 0 auto;
      padding: 28px 22px 60px;
    }}
    .hero {{
      background: linear-gradient(135deg, rgba(0,95,115,0.94), rgba(10,77,104,0.92));
      color: #f8fbfc;
      border-radius: 24px;
      padding: 28px 30px;
      box-shadow: var(--shadow);
    }}
    .hero-top {{
      display: flex;
      justify-content: space-between;
      gap: 20px;
      align-items: flex-start;
      flex-wrap: wrap;
    }}
    .hero h1 {{
      margin: 0 0 12px;
      font-size: clamp(28px, 4vw, 44px);
      line-height: 1.05;
    }}
    .hero-meta {{
      color: rgba(248, 251, 252, 0.82);
      font-size: 14px;
    }}
    .hero-links {{
      display: flex;
      gap: 10px;
      flex-wrap: wrap;
    }}
    .hero-links a {{
      text-decoration: none;
      color: #f8fbfc;
      border: 1px solid rgba(248, 251, 252, 0.28);
      padding: 9px 12px;
      border-radius: 999px;
      font-size: 14px;
      backdrop-filter: blur(4px);
    }}
    .hero-links a:hover {{
      background: rgba(248, 251, 252, 0.12);
    }}
    .grid {{
      display: grid;
      grid-template-columns: repeat(12, minmax(0, 1fr));
      gap: 18px;
      margin-top: 22px;
    }}
    .panel {{
      background: var(--paper);
      border: 1px solid var(--line);
      border-radius: 22px;
      padding: 22px;
      box-shadow: var(--shadow);
    }}
    .panel h2 {{
      margin: 0 0 14px;
      font-size: 21px;
      line-height: 1.1;
    }}
    .panel h3 {{
      margin: 0;
      font-size: 18px;
      line-height: 1.2;
    }}
    .span-12 {{ grid-column: span 12; }}
    .span-8 {{ grid-column: span 8; }}
    .span-6 {{ grid-column: span 6; }}
    .span-4 {{ grid-column: span 4; }}
    .span-3 {{ grid-column: span 3; }}
    .summary-list,
    .action-list,
    .caveat-list {{
      margin: 0;
      padding-left: 20px;
      line-height: 1.55;
    }}
    .stats {{
      display: grid;
      grid-template-columns: repeat(4, minmax(0, 1fr));
      gap: 12px;
    }}
    .stat {{
      padding: 16px;
      border-radius: 18px;
      background: linear-gradient(180deg, #fff 0%, #f7f4ee 100%);
      border: 1px solid var(--line);
    }}
    .stat-label {{
      color: var(--muted);
      font-size: 13px;
      margin-bottom: 8px;
    }}
    .stat-value {{
      font-size: 26px;
      font-weight: 700;
      letter-spacing: -0.03em;
    }}
    .stack {{
      display: grid;
      gap: 14px;
    }}
    .stack-card {{
      border: 1px solid var(--line);
      border-radius: 18px;
      padding: 16px 18px;
      background: #fffdfa;
    }}
    .stack-card-head {{
      display: flex;
      justify-content: space-between;
      align-items: center;
      gap: 12px;
      margin-bottom: 10px;
    }}
    .stack-card-body,
    .stack-card-action,
    .metric-line {{
      margin: 0;
      line-height: 1.5;
    }}
    .stack-card-action {{
      margin-top: 10px;
      color: var(--muted);
    }}
    .badge {{
      display: inline-flex;
      align-items: center;
      justify-content: center;
      padding: 6px 10px;
      border-radius: 999px;
      font-size: 12px;
      font-weight: 700;
      text-transform: uppercase;
      letter-spacing: 0.06em;
      white-space: nowrap;
      color: #fff;
    }}
    .badge-critical {{ background: var(--critical); }}
    .badge-high {{ background: var(--high); }}
    .badge-medium {{ background: var(--medium); }}
    .badge-low {{ background: var(--low); }}
    .badge-none {{ background: var(--none); }}
    .freshness {{
      display: inline-flex;
      border-radius: 999px;
      padding: 4px 9px;
      font-size: 12px;
      font-weight: 700;
      text-transform: uppercase;
      letter-spacing: 0.04em;
    }}
    .freshness-fresh {{
      background: rgba(29, 111, 66, 0.12);
      color: var(--fresh);
    }}
    .freshness-stale {{
      background: rgba(155, 34, 38, 0.12);
      color: var(--stale);
    }}
    table {{
      width: 100%;
      border-collapse: collapse;
      font-size: 14px;
    }}
    th, td {{
      text-align: left;
      padding: 10px 8px;
      border-bottom: 1px solid var(--line);
      vertical-align: top;
    }}
    th {{
      color: var(--muted);
      font-weight: 600;
      font-size: 12px;
      text-transform: uppercase;
      letter-spacing: 0.06em;
    }}
    .inline-link {{
      color: var(--accent);
      text-decoration: none;
      font-weight: 600;
    }}
    .inline-link:hover {{
      text-decoration: underline;
    }}
    @media (max-width: 1100px) {{
      .span-8, .span-6, .span-4, .span-3 {{ grid-column: span 12; }}
      .stats {{ grid-template-columns: repeat(2, minmax(0, 1fr)); }}
    }}
    @media (max-width: 640px) {{
      .shell {{ padding: 16px 14px 40px; }}
      .hero {{ padding: 22px 18px; }}
      .stats {{ grid-template-columns: 1fr; }}
      .stack-card-head {{ flex-direction: column; align-items: flex-start; }}
      th:nth-child(2), td:nth-child(2),
      th:nth-child(3), td:nth-child(3) {{
        display: none;
      }}
    }}
  </style>
</head>
<body>
  <main class="shell">
    <section class="hero">
      <div class="hero-top">
        <div>
          <div class="hero-meta">AW-rus · 1C Executive Brief · render mode: {html.escape(render_mode)}</div>
          <h1>{html.escape(brief.get("headline", "Executive brief недоступен"))}</h1>
          <div class="hero-meta">Сформировано: {html.escape(generated_at)}</div>
        </div>
        <nav class="hero-links">
          <a href="{html.escape(json_url)}">JSON</a>
          <a href="{html.escape(md_url)}">Markdown</a>
          <a href="{html.escape(history_html_url)}">История brief</a>
          <a href="{html.escape(problematic_1d_url)}">Проблемные 1д</a>
          <a href="{html.escape(problematic_7d_url)}">Проблемные 7д</a>
          <a href="{html.escape(history_url)}">History API</a>
          <a href="{html.escape(grafana_url)}">Grafana</a>
        </nav>
      </div>
    </section>

    <section class="grid">
      <article class="panel span-8">
        <h2>Краткий комментарий</h2>
        <ol class="summary-list">
          {summary_items}
        </ol>
      </article>
      <article class="panel span-4">
        <h2>Портфель</h2>
        <div class="stats">
          <div class="stat">
            <div class="stat-label">Компаний</div>
            <div class="stat-value">{fmt_number(summary.get("companies_total"))}</div>
          </div>
          <div class="stat">
            <div class="stat-label">Critical</div>
            <div class="stat-value">{fmt_number(summary.get("critical_total"))}</div>
          </div>
          <div class="stat">
            <div class="stat-label">Кейсы</div>
            <div class="stat-value">{fmt_number(summary.get("open_cases_total"))}</div>
          </div>
          <div class="stat">
            <div class="stat-label">Прогноз 30д</div>
            <div class="stat-value">{fmt_number(summary.get("activity_forecast_30d_total"))}</div>
          </div>
        </div>
      </article>

      <article class="panel span-6">
        <h2>Компании риска</h2>
        <div class="stack">
          {''.join(risk_cards) or '<p>Нет данных.</p>'}
        </div>
      </article>

      <article class="panel span-6">
        <h2>Прогноз активности 30 дней</h2>
        <div class="stack">
          {''.join(forecast_cards) or '<p>Нет данных.</p>'}
        </div>
      </article>

      <article class="panel span-6">
        <h2>Что проверить руководителю</h2>
        <ul class="action-list">
          {action_items}
        </ul>
      </article>

      <article class="panel span-6">
        <h2>Ограничения интерпретации</h2>
        <ul class="caveat-list">
          {caveat_items}
        </ul>
      </article>

      <article class="panel span-12">
        <h2>Свежесть источников</h2>
        <table>
          <thead>
            <tr>
              <th>Источник</th>
              <th>Последняя отметка</th>
              <th>Отставание, ч</th>
              <th>Статус</th>
            </tr>
          </thead>
          <tbody>
            {''.join(freshness_rows)}
          </tbody>
        </table>
      </article>
    </section>
  </main>
</body>
</html>"""


def problematic_companies(days: int = 7, limit: int = 50) -> list[dict[str, Any]]:
    client = ch_client()
    sql = f"""
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
        WHERE generated_at >= now() - INTERVAL {int(days)} DAY
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
        p.amount_30d,
        p.amount_forecast_30d,
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
    ORDER BY r.max_score DESC, r.signals_total DESC, p.amount_30d DESC, p.counterparty
    LIMIT {int(limit)}
    """
    items = rows_to_dict(client.query(sql))
    for item in items:
        if "infobase" not in item and "p.infobase" in item:
            item["infobase"] = item["p.infobase"]
        if "counterparty" not in item and "p.counterparty" in item:
            item["counterparty"] = item["p.counterparty"]
    return items


def render_brief_history_html(items: list[dict[str, Any]]) -> str:
    rows = []
    for item in items:
        rows.append(
            "<tr>"
            f"<td><a class=\"inline-link\" href=\"{manager_brief_history_html_url(str(item.get('path', '')))}\">{html.escape(str(item.get('generated_at', '-')))}</a></td>"
            f"<td>{html.escape(str(item.get('render_mode', '-')))}</td>"
            f"<td>{html.escape(str(item.get('model') or '-'))}</td>"
            f"<td>{html.escape(str(item.get('headline') or '-'))}</td>"
            f"<td><a class=\"inline-link\" href=\"/api/1/analytics-1c/manager/brief/history/{quote(str(item.get('path', '')))}\">JSON</a></td>"
            "</tr>"
        )
    return f"""<!doctype html>
<html lang="ru">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta http-equiv="refresh" content="300">
  <title>1C Brief History</title>
  <style>
    :root {{
      --bg: #f4f1ea; --paper: #fffdf8; --ink: #1c1a17; --muted: #6b655c; --line: #d8d0c4; --accent: #005f73; --shadow: 0 14px 40px rgba(28, 26, 23, 0.08);
    }}
    * {{ box-sizing: border-box; }}
    body {{ margin: 0; font-family: "IBM Plex Sans", "Segoe UI", system-ui, sans-serif; background: linear-gradient(180deg, #faf7f2 0%, var(--bg) 100%); color: var(--ink); }}
    .shell {{ max-width: 1380px; margin: 0 auto; padding: 28px 22px 60px; }}
    .hero {{ background: linear-gradient(135deg, rgba(0,95,115,0.94), rgba(10,77,104,0.92)); color: #f8fbfc; border-radius: 24px; padding: 28px 30px; box-shadow: var(--shadow); }}
    .hero h1 {{ margin: 0 0 10px; font-size: clamp(28px, 4vw, 42px); }}
    .hero-links {{ display: flex; gap: 10px; flex-wrap: wrap; }}
    .hero-links a {{ text-decoration: none; color: #f8fbfc; border: 1px solid rgba(248, 251, 252, 0.28); padding: 9px 12px; border-radius: 999px; font-size: 14px; }}
    .panel {{ margin-top: 22px; background: var(--paper); border: 1px solid var(--line); border-radius: 22px; padding: 22px; box-shadow: var(--shadow); }}
    table {{ width: 100%; border-collapse: collapse; font-size: 14px; }}
    th, td {{ text-align: left; padding: 10px 8px; border-bottom: 1px solid var(--line); vertical-align: top; }}
    th {{ color: var(--muted); font-weight: 600; font-size: 12px; text-transform: uppercase; letter-spacing: 0.06em; }}
    .inline-link {{ color: var(--accent); text-decoration: none; font-weight: 600; }}
    .inline-link:hover {{ text-decoration: underline; }}
  </style>
</head>
<body>
  <main class="shell">
    <section class="hero">
      <h1>История executive brief</h1>
      <div class="hero-links">
        <a href="/manager/brief">Текущий brief</a>
        <a href="/manager/problematic?days=1">Проблемные 1д</a>
        <a href="/manager/problematic?days=7">Проблемные 7д</a>
      </div>
    </section>
    <section class="panel">
      <table>
        <thead>
          <tr>
            <th>Сформировано</th>
            <th>Режим</th>
            <th>Модель</th>
            <th>Headline</th>
            <th>Raw</th>
          </tr>
        </thead>
        <tbody>
          {''.join(rows) or '<tr><td colspan="5">История пока пуста.</td></tr>'}
        </tbody>
      </table>
    </section>
  </main>
</body>
</html>"""


def render_problematic_companies_html(items: list[dict[str, Any]], days: int) -> str:
    rows = []
    for item in items:
        rows.append(
            "<tr>"
            f"<td><a class=\"inline-link\" href=\"{company_detail_url(str(item.get('counterparty', '-')), str(item.get('infobase', '')) if item.get('infobase') else None)}\">{html.escape(str(item.get('counterparty', '-')))}</a></td>"
            f"<td>{html.escape(str(item.get('normalized_counterparty') or '-'))}</td>"
            f"<td>{severity_badge(str(item.get('top_severity') or item.get('signal_severity') or 'none'))}</td>"
            f"<td>{fmt_number(item.get('max_score'))}</td>"
            f"<td>{fmt_number(item.get('signals_total'))}</td>"
            f"<td>{fmt_number(item.get('critical_total'))}</td>"
            f"<td>{fmt_number(item.get('amount_30d'))}</td>"
            f"<td>{fmt_number(item.get('amount_forecast_30d'))}</td>"
            f"<td>{html.escape(str(item.get('top_signal_type') or '-'))}</td>"
            f"<td>{html.escape(str(item.get('top_summary') or '-'))}</td>"
            "</tr>"
        )
    return f"""<!doctype html>
<html lang="ru">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta http-equiv="refresh" content="300">
  <title>1C Problem Companies</title>
  <style>
    :root {{
      --bg: #f4f1ea; --paper: #fffdf8; --ink: #1c1a17; --muted: #6b655c; --line: #d8d0c4; --accent: #005f73; --shadow: 0 14px 40px rgba(28, 26, 23, 0.08);
      --critical: #9b2226; --high: #bb3e03; --medium: #ca6702; --low: #4d7c0f; --none: #687076;
    }}
    * {{ box-sizing: border-box; }}
    body {{ margin: 0; font-family: "IBM Plex Sans", "Segoe UI", system-ui, sans-serif; background: linear-gradient(180deg, #faf7f2 0%, var(--bg) 100%); color: var(--ink); }}
    .shell {{ max-width: 1500px; margin: 0 auto; padding: 28px 22px 60px; }}
    .hero {{ background: linear-gradient(135deg, rgba(0,95,115,0.94), rgba(10,77,104,0.92)); color: #f8fbfc; border-radius: 24px; padding: 28px 30px; box-shadow: var(--shadow); }}
    .hero h1 {{ margin: 0 0 10px; font-size: clamp(28px, 4vw, 42px); }}
    .hero p {{ margin: 0 0 14px; line-height: 1.5; color: rgba(248,251,252,0.88); }}
    .hero-links {{ display: flex; gap: 10px; flex-wrap: wrap; }}
    .hero-links a {{ text-decoration: none; color: #f8fbfc; border: 1px solid rgba(248, 251, 252, 0.28); padding: 9px 12px; border-radius: 999px; font-size: 14px; }}
    .panel {{ margin-top: 22px; background: var(--paper); border: 1px solid var(--line); border-radius: 22px; padding: 22px; box-shadow: var(--shadow); }}
    table {{ width: 100%; border-collapse: collapse; font-size: 14px; }}
    th, td {{ text-align: left; padding: 10px 8px; border-bottom: 1px solid var(--line); vertical-align: top; }}
    th {{ color: var(--muted); font-weight: 600; font-size: 12px; text-transform: uppercase; letter-spacing: 0.06em; }}
    .inline-link {{ color: var(--accent); text-decoration: none; font-weight: 600; }}
    .inline-link:hover {{ text-decoration: underline; }}
    .badge {{
      display: inline-flex; align-items: center; justify-content: center; padding: 6px 10px; border-radius: 999px;
      font-size: 12px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.06em; color: #fff;
    }}
    .badge-critical {{ background: var(--critical); }}
    .badge-high {{ background: var(--high); }}
    .badge-medium {{ background: var(--medium); }}
    .badge-low {{ background: var(--low); }}
    .badge-none {{ background: var(--none); }}
  </style>
</head>
<body>
  <main class="shell">
    <section class="hero">
      <h1>Проблемные компании за {days} {('день' if days == 1 else 'дней')}</h1>
      <p>Список собран по live company signals. В приоритете max score, плотность сигналов и общий вес проблемного контура.</p>
      <div class="hero-links">
        <a href="/manager/brief">Текущий brief</a>
        <a href="/manager/briefs">История brief</a>
        <a href="/manager/problematic?days=1">Срез 1д</a>
        <a href="/manager/problematic?days=7">Срез 7д</a>
      </div>
    </section>
    <section class="panel">
      <table>
        <thead>
          <tr>
            <th>Компания</th>
            <th>Нормализация</th>
            <th>Severity</th>
            <th>Max score</th>
            <th>Signals</th>
            <th>Critical</th>
            <th>Активность 30д</th>
            <th>Прогноз 30д</th>
            <th>Top signal</th>
            <th>Комментарий</th>
          </tr>
        </thead>
        <tbody>
          {''.join(rows) or '<tr><td colspan="10">Нет сигналов за выбранный период.</td></tr>'}
        </tbody>
      </table>
    </section>
  </main>
</body>
</html>"""


app = FastAPI(title="AW-rus 1C Company Intelligence API", version="1.0.0")


@app.get("/favicon.ico")
def favicon() -> Response:
    return Response(status_code=204)


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


@app.get("/api/1/analytics-1c/manager/brief/latest")
def manager_brief_latest() -> dict[str, Any]:
    return load_latest_manager_brief()


@app.get("/api/1/analytics-1c/manager/brief/latest.md", response_class=PlainTextResponse)
def manager_brief_latest_markdown() -> str:
    latest_md = manager_brief_state_dir() / "latest.md"
    if not latest_md.exists():
        raise HTTPException(status_code=404, detail="manager brief markdown not generated yet")
    return latest_md.read_text(encoding="utf-8")


@app.get("/api/1/analytics-1c/manager/brief/history")
def manager_brief_history(limit: int = Query(default=20, ge=1, le=200)) -> dict[str, Any]:
    items = load_brief_history_records(limit)
    return {"items": items, "count": len(items)}


@app.get("/api/1/analytics-1c/manager/brief/history/{name}")
def manager_brief_history_record(name: str) -> dict[str, Any]:
    return load_brief_history_record(name)


@app.get("/api/1/analytics-1c/companies/problematic")
def problematic_companies_api(
    days: int = Query(default=7, ge=1, le=30),
    limit: int = Query(default=50, ge=1, le=500),
) -> dict[str, Any]:
    items = problematic_companies(days=days, limit=limit)
    return {"items": items, "count": len(items), "days": days}


@app.get("/manager/brief", response_class=HTMLResponse)
def manager_brief_view() -> str:
    return render_manager_brief_html(load_latest_manager_brief())


@app.get("/manager/briefs", response_class=HTMLResponse)
def manager_brief_history_view(limit: int = Query(default=40, ge=1, le=200)) -> str:
    return render_brief_history_html(load_brief_history_records(limit))


@app.get("/manager/briefs/{name}", response_class=HTMLResponse)
def manager_brief_history_detail_view(name: str) -> str:
    return render_manager_brief_html(load_brief_history_record(name))


@app.get("/manager/problematic", response_class=HTMLResponse)
def manager_problematic_companies_view(
    days: int = Query(default=7, ge=1, le=30),
    limit: int = Query(default=50, ge=1, le=500),
) -> str:
    return render_problematic_companies_html(problematic_companies(days=days, limit=limit), days)


def render_company_detail_html(summary_payload: dict[str, Any], infobase: str | None = None) -> str:
    card = summary_payload["card"]
    company_state = summary_payload.get("company_state") or {}
    forecasts = summary_payload.get("forecasts") or []
    signals = summary_payload.get("signals") or []
    recent_documents = summary_payload.get("recent_documents") or []

    title = card.get("counterparty", "Карточка компании")
    subtitle = summary_payload.get("essence", "")
    grafana_url = grafana_company_dashboard_url()
    summary_url = f"/api/1/analytics-1c/companies/{quote(card['counterparty'])}/summary"
    if infobase:
        summary_url += f"?infobase={quote(infobase)}"
    timeline_url = f"/api/1/analytics-1c/companies/{quote(card['counterparty'])}/timeline"
    if infobase:
        timeline_url += f"?infobase={quote(infobase)}"
    forecast_url = f"/api/1/analytics-1c/companies/{quote(card['counterparty'])}/forecast"
    if infobase:
        forecast_url += f"?infobase={quote(infobase)}"

    if card.get("signal_severity") == "critical":
        manager_comment = "Компания требует немедленного внимания: контур считает её operational-critical."
    elif card.get("signal_severity") == "high":
        manager_comment = "Компания в зоне повышенного внимания: нужен короткий управленческий разбор причин сигнала."
    elif card.get("signal_severity") == "medium":
        manager_comment = "Компания не аварийная, но требует точечной проверки причин отклонения."
    else:
        manager_comment = "По компании нет выраженного аварийного сигнала; смотреть контекст и тренд активности."

    metrics = [
        ("Активность 7д", fmt_number(card.get("amount_7d"))),
        ("Активность 30д", fmt_number(card.get("amount_30d"))),
        ("Прогноз 30д", fmt_number(card.get("amount_forecast_30d"))),
        ("Документы 30д", fmt_number(card.get("docs_30d"))),
        ("Кейсы", fmt_number(card.get("open_cases_total"))),
        ("Detections", fmt_number(card.get("detections_total"))),
    ]
    metric_cards = "".join(
        f"<div class=\"stat\"><div class=\"stat-label\">{html.escape(label)}</div><div class=\"stat-value\">{html.escape(value)}</div></div>"
        for label, value in metrics
    )

    forecast_rows = []
    for item in forecasts:
        forecast_rows.append(
            "<tr>"
            f"<td>{html.escape(str(item.get('metric', '-')))}</td>"
            f"<td>{fmt_number(item.get('horizon_days'))}</td>"
            f"<td>{fmt_number(item.get('baseline_daily'))}</td>"
            f"<td>{fmt_number(item.get('predicted_total'))}</td>"
            f"<td>{fmt_number(item.get('confidence'))}</td>"
            f"<td>{html.escape(str(item.get('note', '-')))}</td>"
            "</tr>"
        )

    signal_rows = []
    for item in signals[:12]:
        signal_rows.append(
            "<tr>"
            f"<td>{html.escape(str(item.get('generated_at', '-')))}</td>"
            f"<td>{severity_badge(str(item.get('severity', 'none')))}</td>"
            f"<td>{fmt_number(item.get('score'))}</td>"
            f"<td>{html.escape(str(item.get('signal_type', '-')))}</td>"
            f"<td>{html.escape(str(item.get('summary', '-')))}</td>"
            "</tr>"
        )

    document_rows = []
    for item in recent_documents[:20]:
        document_rows.append(
            "<tr>"
            f"<td>{html.escape(str(item.get('ts', '-')))}</td>"
            f"<td>{html.escape(str(item.get('doc_type', '-')))}</td>"
            f"<td>{html.escape(str(item.get('operation_type', '-')))}</td>"
            f"<td>{fmt_number(item.get('amount'))}</td>"
            f"<td>{html.escape(str(item.get('status', '-')))}</td>"
            f"<td>{html.escape(str(item.get('author', '-')))}</td>"
            "</tr>"
        )

    registry_comment = (
        f"Сопоставление с реестром: {card.get('registry_match_mode', 'none')}. "
        f"Ответственный: {card.get('registry_assignee_name') or 'не указан'}."
    )
    if card.get("registry_match_mode") == "manual":
        registry_comment += " Требуется осторожность: запись заведена через manual override."

    return f"""<!doctype html>
<html lang="ru">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta http-equiv="refresh" content="300">
  <title>{html.escape(title)} · 1C Company Brief</title>
  <style>
    :root {{
      --bg: #f4f1ea;
      --paper: #fffdf8;
      --ink: #1c1a17;
      --muted: #6b655c;
      --line: #d8d0c4;
      --accent: #005f73;
      --critical: #9b2226;
      --high: #bb3e03;
      --medium: #ca6702;
      --low: #4d7c0f;
      --none: #687076;
      --shadow: 0 14px 40px rgba(28, 26, 23, 0.08);
    }}
    * {{ box-sizing: border-box; }}
    body {{
      margin: 0;
      font-family: "IBM Plex Sans", "Segoe UI", system-ui, sans-serif;
      color: var(--ink);
      background:
        radial-gradient(circle at top left, rgba(0, 95, 115, 0.08), transparent 28%),
        linear-gradient(180deg, #faf7f2 0%, var(--bg) 100%);
    }}
    .shell {{ max-width: 1380px; margin: 0 auto; padding: 28px 22px 60px; }}
    .hero {{
      background: linear-gradient(135deg, rgba(0,95,115,0.94), rgba(10,77,104,0.92));
      color: #f8fbfc;
      border-radius: 24px;
      padding: 28px 30px;
      box-shadow: var(--shadow);
    }}
    .hero-top {{
      display: flex; justify-content: space-between; gap: 20px; align-items: flex-start; flex-wrap: wrap;
    }}
    .hero h1 {{ margin: 0 0 12px; font-size: clamp(28px, 4vw, 42px); line-height: 1.05; }}
    .hero-meta {{ color: rgba(248, 251, 252, 0.82); font-size: 14px; }}
    .hero-links {{ display: flex; gap: 10px; flex-wrap: wrap; }}
    .hero-links a {{
      text-decoration: none; color: #f8fbfc; border: 1px solid rgba(248, 251, 252, 0.28);
      padding: 9px 12px; border-radius: 999px; font-size: 14px;
    }}
    .hero-links a:hover {{ background: rgba(248, 251, 252, 0.12); }}
    .grid {{ display: grid; grid-template-columns: repeat(12, minmax(0, 1fr)); gap: 18px; margin-top: 22px; }}
    .panel {{ background: var(--paper); border: 1px solid var(--line); border-radius: 22px; padding: 22px; box-shadow: var(--shadow); }}
    .panel h2 {{ margin: 0 0 14px; font-size: 21px; line-height: 1.1; }}
    .span-12 {{ grid-column: span 12; }}
    .span-8 {{ grid-column: span 8; }}
    .span-6 {{ grid-column: span 6; }}
    .span-4 {{ grid-column: span 4; }}
    .summary-box {{
      border: 1px solid var(--line); border-left: 5px solid var(--accent); border-radius: 18px; padding: 16px 18px;
      background: #fffdfa; line-height: 1.55;
    }}
    .stats {{ display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 12px; }}
    .stat {{ padding: 16px; border-radius: 18px; background: linear-gradient(180deg, #fff 0%, #f7f4ee 100%); border: 1px solid var(--line); }}
    .stat-label {{ color: var(--muted); font-size: 13px; margin-bottom: 8px; }}
    .stat-value {{ font-size: 24px; font-weight: 700; letter-spacing: -0.03em; }}
    .badge {{
      display: inline-flex; align-items: center; justify-content: center; padding: 6px 10px;
      border-radius: 999px; font-size: 12px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.06em; color: #fff;
    }}
    .badge-critical {{ background: var(--critical); }}
    .badge-high {{ background: var(--high); }}
    .badge-medium {{ background: var(--medium); }}
    .badge-low {{ background: var(--low); }}
    .badge-none {{ background: var(--none); }}
    table {{ width: 100%; border-collapse: collapse; font-size: 14px; }}
    th, td {{ text-align: left; padding: 10px 8px; border-bottom: 1px solid var(--line); vertical-align: top; }}
    th {{ color: var(--muted); font-weight: 600; font-size: 12px; text-transform: uppercase; letter-spacing: 0.06em; }}
    .inline-link {{ color: var(--accent); text-decoration: none; font-weight: 600; }}
    .inline-link:hover {{ text-decoration: underline; }}
    .meta-list {{ margin: 0; padding-left: 18px; line-height: 1.55; }}
    @media (max-width: 1100px) {{
      .span-8, .span-6, .span-4 {{ grid-column: span 12; }}
      .stats {{ grid-template-columns: repeat(2, minmax(0, 1fr)); }}
    }}
    @media (max-width: 640px) {{
      .shell {{ padding: 16px 14px 40px; }}
      .hero {{ padding: 22px 18px; }}
      .stats {{ grid-template-columns: 1fr; }}
      th:nth-child(2), td:nth-child(2) {{ display: none; }}
    }}
  </style>
</head>
<body>
  <main class="shell">
    <section class="hero">
      <div class="hero-top">
        <div>
          <div class="hero-meta">AW-rus · Company Brief · {severity_badge(str(card.get("signal_severity", "none")))}</div>
          <h1>{html.escape(title)}</h1>
          <div class="hero-meta">{html.escape(subtitle)}</div>
        </div>
        <nav class="hero-links">
          <a href="/manager/brief">К портфелю</a>
          <a href="/manager/briefs">История brief</a>
          <a href="/manager/problematic?days=7">Проблемные компании</a>
          <a href="{html.escape(summary_url)}">JSON summary</a>
          <a href="{html.escape(forecast_url)}">JSON forecast</a>
          <a href="{html.escape(timeline_url)}">JSON timeline</a>
          <a href="{html.escape(grafana_url)}">Grafana</a>
        </nav>
      </div>
    </section>

    <section class="grid">
      <article class="panel span-8">
        <h2>Управленческий комментарий</h2>
        <div class="summary-box">
          <p><strong>{html.escape(manager_comment)}</strong></p>
          <p>{html.escape(registry_comment)}</p>
          <p>Сейчас база в состоянии <strong>{html.escape(str(card.get("current_status", "-")))}</strong>, активных блокировок: <strong>{fmt_number(card.get("active_locks"))}</strong>, дней с последней активности: <strong>{fmt_number(card.get("days_since_last_activity"))}</strong>.</p>
        </div>
      </article>

      <article class="panel span-4">
        <h2>Ключевые метрики</h2>
        <div class="stats">
          {metric_cards}
        </div>
      </article>

      <article class="panel span-6">
        <h2>Карточка компании</h2>
        <ul class="meta-list">
          <li><strong>Компания:</strong> {html.escape(str(card.get("company_name") or card.get("counterparty") or "-"))}</li>
          <li><strong>Нормализованное имя:</strong> {html.escape(str(card.get("normalized_counterparty") or "-"))}</li>
          <li><strong>Инфобаза:</strong> {html.escape(str(card.get("infobase") or "-"))}</li>
          <li><strong>Ответственный:</strong> {html.escape(str(card.get("registry_assignee_name") or card.get("owner_user") or "-"))}</li>
          <li><strong>Match mode:</strong> {html.escape(str(card.get("registry_match_mode") or "-"))}</li>
          <li><strong>ИНН / КПП:</strong> {html.escape(str(card.get("registry_inn") or "-"))} / {html.escape(str(card.get("registry_kpp") or "-"))}</li>
          <li><strong>Путь базы:</strong> {html.escape(str(card.get("base_path") or "-"))}</li>
        </ul>
      </article>

      <article class="panel span-6">
        <h2>Состояние файловой базы</h2>
        <ul class="meta-list">
          <li><strong>Статус:</strong> {html.escape(str(company_state.get("current_status") or card.get("current_status") or "-"))}</li>
          <li><strong>Размер базы:</strong> {fmt_number(company_state.get("db_size_bytes") or card.get("db_size_bytes"))}</li>
          <li><strong>Размер reglog:</strong> {fmt_number(company_state.get("reglog_size_bytes") or card.get("reglog_size_bytes"))}</li>
          <li><strong>Активные блокировки:</strong> {fmt_number(company_state.get("active_locks") or card.get("active_locks"))}</li>
          <li><strong>Текущий activity score:</strong> {fmt_number(company_state.get("current_activity_score") or card.get("current_activity_score"))}</li>
          <li><strong>Последний snapshot:</strong> {html.escape(str(company_state.get("ts") or card.get("last_company_snapshot_at") or "-"))}</li>
        </ul>
      </article>

      <article class="panel span-12">
        <h2>Прогнозы</h2>
        <table>
          <thead>
            <tr>
              <th>Метрика</th>
              <th>Горизонт, д</th>
              <th>Baseline</th>
              <th>Прогноз total</th>
              <th>Confidence</th>
              <th>Примечание</th>
            </tr>
          </thead>
          <tbody>
            {''.join(forecast_rows) or '<tr><td colspan="6">Нет данных.</td></tr>'}
          </tbody>
        </table>
      </article>

      <article class="panel span-12">
        <h2>Сигналы</h2>
        <table>
          <thead>
            <tr>
              <th>Время</th>
              <th>Severity</th>
              <th>Score</th>
              <th>Тип</th>
              <th>Комментарий</th>
            </tr>
          </thead>
          <tbody>
            {''.join(signal_rows) or '<tr><td colspan="5">Нет данных.</td></tr>'}
          </tbody>
        </table>
      </article>

      <article class="panel span-12">
        <h2>Последние события</h2>
        <table>
          <thead>
            <tr>
              <th>Время</th>
              <th>Тип</th>
              <th>Операция</th>
              <th>Активность</th>
              <th>Статус</th>
              <th>Автор</th>
            </tr>
          </thead>
          <tbody>
            {''.join(document_rows) or '<tr><td colspan="6">Нет данных.</td></tr>'}
          </tbody>
        </table>
      </article>
    </section>
  </main>
</body>
</html>"""


@app.get("/manager/company/{counterparty}", response_class=HTMLResponse)
def manager_company_view(counterparty: str, infobase: str | None = None) -> str:
    return render_company_detail_html(company_summary(counterparty, infobase), infobase)


if __name__ == "__main__":
    args = parse_args()
    uvicorn.run(app, host=args.host, port=args.port)
