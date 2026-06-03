#!/usr/bin/env bash
set -Eeuo pipefail

HEARTBEAT_FILE="${HEARTBEAT_FILE:-/opt/infra-admin/.state/tsj_guardian_heartbeat}"
MAX_AGE_SEC="${MAX_AGE_SEC:-180}"
SERVICE_NAME="${SERVICE_NAME:-tsj-guardian-bot.service}"
GOST_SERVICE_NAME="${GOST_SERVICE_NAME:-gost-tg.service}"
GOST_DUP_PATTERN="${GOST_DUP_PATTERN:-/usr/local/bin/gost -L http+socks5://127.0.0.1:11090 -F socks5+wss://gw.example.local:4443}"

dedupe_gost_instances() {
  local main_pid
  main_pid="$(systemctl show -p MainPID --value "${GOST_SERVICE_NAME}" 2>/dev/null || true)"
  mapfile -t pids < <(pgrep -f -- "${GOST_DUP_PATTERN}" 2>/dev/null || true)
  if [[ "${#pids[@]}" -le 1 ]]; then
    return 0
  fi

  local keep_pid=""
  if [[ "${main_pid}" =~ ^[0-9]+$ ]] && [[ "${main_pid}" -gt 1 ]]; then
    keep_pid="${main_pid}"
  else
    keep_pid="${pids[0]}"
  fi

  for pid in "${pids[@]}"; do
    [[ "${pid}" == "${keep_pid}" ]] && continue
    kill -TERM "${pid}" 2>/dev/null || true
  done

  sleep 2

  for pid in "${pids[@]}"; do
    [[ "${pid}" == "${keep_pid}" ]] && continue
    kill -0 "${pid}" 2>/dev/null || continue
    kill -KILL "${pid}" 2>/dev/null || true
  done
}

dedupe_gost_instances

if [[ ! -f "${HEARTBEAT_FILE}" ]]; then
  systemctl restart "${SERVICE_NAME}"
  exit 0
fi

now="$(date +%s)"
hb="$(cat "${HEARTBEAT_FILE}" 2>/dev/null || printf '0')"
if [[ ! "${hb}" =~ ^[0-9]+$ ]]; then
  systemctl restart "${SERVICE_NAME}"
  exit 0
fi

age=$((now - hb))
if [[ "${age}" -gt "${MAX_AGE_SEC}" ]]; then
  systemctl restart "${SERVICE_NAME}"
fi
