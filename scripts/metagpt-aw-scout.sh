#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

OUT_DIR="${METAGPT_AW_OUT_DIR:-$ROOT/.ai/metagpt}"
METAGPT_BIN="${METAGPT_BIN:-~/bin/metagpt-lab}"
METAGPT_AW_INVESTMENT="${METAGPT_AW_INVESTMENT:-0.1}"
METAGPT_AW_N_ROUND="${METAGPT_AW_N_ROUND:-2}"
METAGPT_AW_TIMEOUT="${METAGPT_AW_TIMEOUT:-90}"
METAGPT_AW_ENGINE="${METAGPT_AW_ENGINE:-direct}"
METAGPT_CONFIG="${METAGPT_CONFIG:-~/.metagpt/config2.yaml}"

usage() {
  cat <<'USAGE'
Usage:
  scripts/metagpt-aw-scout.sh <preset|free task text>

Presets:
  qa-rollback     QA checklist and rollback plan for Windows collectors
  smoke           End-to-end smoke-test plan for AW server, RDP collectors, Grafana, Influx exporters
  grafana         Management-facing Grafana dashboard review plan
  install-kit     Install-kit rebuild and validation checklist
  windows-i18n    Windows localized Administrator / Cyrillic collector checks

Examples:
  scripts/metagpt-aw-scout.sh qa-rollback
  scripts/metagpt-aw-scout.sh "Review risk of changing aw-worktime-ui-bridge foreground cache"
USAGE
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" || $# -eq 0 ]]; then
  usage
  exit 0
fi

case "$1" in
  qa-rollback)
    TASK="Prepare a practical QA checklist and rollback plan for ActivityWatch-Russian Windows collectors after changes in PowerShell collectors, process events, localized Administrator task names, and recovery hardening."
    ;;
  smoke)
    TASK="Prepare a minimal but complete end-to-end smoke-test plan for ActivityWatch-Russian: AW server buckets, worktime API, RDP/WinRM collectors, Windows SSH access, Grafana dashboards, Influx worktime exporter, DLP exporter, and install-kit sanity."
    ;;
  grafana)
    TASK="Review the management-facing Grafana dashboards for ActivityWatch-Russian. Identify confusing technical labels, panels that should be hidden or renamed, and checks needed to prove 'true user work' by application."
    ;;
  install-kit)
    TASK="Prepare a rebuild and validation checklist for install-kit-awindows, including rebuild_install_kit.sh, check_install_kit_vs_repo.sh, validate_install_kit.sh, and optional InnoSetup exe rebuild."
    ;;
  windows-i18n)
    TASK="Prepare a validation checklist for Windows localized account names and Cyrillic handling: CP866 query.exe decoding, HOST-EXAMPLE_Администратор scheduled task, WinRM output, SSH checks, and ActivityWatch Launch fallback rules."
    ;;
  *)
    TASK="$*"
    ;;
esac

if [[ ! -x "$METAGPT_BIN" ]]; then
  echo "MetaGPT wrapper not found or not executable: $METAGPT_BIN" >&2
  exit 2
fi

mkdir -p "$OUT_DIR"
STAMP="$(date +%Y%m%d-%H%M%S)"
SLUG="$(printf '%s' "$TASK" | tr '[:upper:]' '[:lower:]' | tr -cs '[:alnum:]' '-' | sed 's/^-//; s/-$//; s/--*/-/g; s/^$/task/' | cut -c1-80)"
OUT="$OUT_DIR/$STAMP-$SLUG.md"

PROMPT="$(cat <<PROMPT_EOF
You are a technical scout for the ActivityWatch-Russian project.

Repository:
- Path: /mnt/usb_hdd2/Projects/ActivityWatch-Russian
- Purpose: enterprise ActivityWatch deployment with RU WebUI patching, Windows PowerShell collectors, Ansible automation, Grafana/Prometheus/Influx monitoring, 1C/DLP integrations, and install-kit-awindows packaging.

Hard constraints:
- Answer in Russian.
- Be concise, practical, and implementation-ready.
- Do not browse the web.
- Do not use Browser, Editor, Terminal, Search, or file-writing tools.
- Do not create project files inside MetaGPT workspace.
- Do not delegate to other agents; return the final answer directly.
- Do not invent secrets, credentials, or unverified production facts.
- Prefer checklists, exact commands, file paths, rollback points, and validation evidence.
- Assume Groq/free-tier constraints: keep the response compact.

Known operational facts:
- AW server: http://192.0.2.13:5600
- Worktime API: http://192.0.2.13:5610
- Grafana: http://192.0.2.11:3000
- RDP host: 198.51.100.18 / HOST-EXAMPLE
- Ansible inventory: ansible/inventory.ini
- Windows deploy root: C:\\Program Files\\AWatch-rus
- Windows state root: C:\\ProgramData\\AWatch-rus
- Localized built-in Administrator launch task: ActivityWatch Launch [HOST-EXAMPLE_Администратор]
- For localized Windows console output, prefer query.exe user via CP866 decoding and emit UTF-8.

Task:
$TASK

Required output:
1. Цель проверки или изменения.
2. Минимальный план действий.
3. Команды/файлы, которые надо использовать.
4. Критерии успеха.
5. Rollback or recovery path.
6. Risks and gaps that must be verified locally by Codex/operator.
PROMPT_EOF
)"

{
  echo "# MetaGPT AW Scout"
  echo
  echo "- Date: $(date -Is)"
  echo "- Task: $TASK"
  echo "- Engine: $METAGPT_AW_ENGINE"
  echo "- Command: direct LLM via $METAGPT_CONFIG"
  echo
  echo "## Output"
  echo
} > "$OUT"

set +e
if [[ "$METAGPT_AW_ENGINE" == "team" ]]; then
  timeout "$METAGPT_AW_TIMEOUT" "$METAGPT_BIN" --investment "$METAGPT_AW_INVESTMENT" --n-round "$METAGPT_AW_N_ROUND" --no-implement --project-name "aw-scout-$STAMP" "$PROMPT" 2>&1 | tee -a "$OUT"
else
  ~/labs/metagpt-lab/.venv/bin/python - "$METAGPT_CONFIG" "$PROMPT" <<'PY' 2>&1 | tee -a "$OUT"
import sys
from pathlib import Path

import yaml
from openai import OpenAI

config_path = Path(sys.argv[1])
prompt = sys.argv[2]
cfg = yaml.safe_load(config_path.read_text(encoding="utf-8"))["llm"]
client = OpenAI(
    api_key=cfg["api_key"],
    base_url=(cfg.get("base_url") or "https://api.openai.com/v1").rstrip("/"),
)
response = client.chat.completions.create(
    model=cfg.get("model") or "gpt-4.1-mini",
    messages=[
        {
            "role": "system",
            "content": (
                "Ты практичный технический ревьюер ActivityWatch-Russian. "
                "Отвечай по-русски, кратко, с командами и критериями проверки. "
                "Не выдумывай факты, помечай непроверенное как гипотезу."
            ),
        },
        {"role": "user", "content": prompt},
    ],
    temperature=0.2,
    max_tokens=1800,
)
print(response.choices[0].message.content.strip())
PY
fi
status=${PIPESTATUS[0]}
set -e

{
  echo
  echo "## Exit Status"
  echo
  echo "$status"
} >> "$OUT"

echo
echo "Saved: $OUT"
exit "$status"
