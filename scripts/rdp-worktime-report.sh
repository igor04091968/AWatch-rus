#!/usr/bin/env bash
set -euo pipefail

DAY=""
FROM=""
TO=""
AW_BASE_URL="${AW_BASE_URL:-http://10.10.10.13:5600/api/0}"
OUT_DIR="${OUT_DIR:-reports}"

usage() {
  cat <<EOF
Usage:
  $0 --day today|yesterday
  $0 --from YYYY-MM-DD --to YYYY-MM-DD
Env:
  AW_BASE_URL (default: ${AW_BASE_URL})
  OUT_DIR     (default: ${OUT_DIR})
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --day) DAY="${2:-}"; shift 2 ;;
    --from) FROM="${2:-}"; shift 2 ;;
    --to) TO="${2:-}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown arg: $1" >&2; usage; exit 2 ;;
  esac
done

if [[ -n "$DAY" ]]; then
  if [[ "$DAY" == "today" ]]; then
    FROM="$(date +%F)"
    TO="$FROM"
  elif [[ "$DAY" == "yesterday" ]]; then
    FROM="$(date -d 'yesterday' +%F)"
    TO="$FROM"
  else
    echo "Invalid --day: $DAY" >&2
    exit 2
  fi
fi

if [[ -z "$FROM" || -z "$TO" ]]; then
  usage
  exit 2
fi

mkdir -p "$OUT_DIR"
CSV_OUT="${OUT_DIR}/rdp-worktime-${FROM}_${TO}.csv"
JSON_OUT="${OUT_DIR}/rdp-worktime-${FROM}_${TO}.json"

python3 - "$AW_BASE_URL" "$FROM" "$TO" "$CSV_OUT" "$JSON_OUT" <<'PY'
import csv
import json
import sys
import urllib.request
from datetime import datetime, timedelta, timezone

base, from_d, to_d, csv_out, json_out = sys.argv[1:6]

def get_json(url: str):
    with urllib.request.urlopen(url, timeout=30) as r:
        return json.loads(r.read().decode())

def parse_ts(s):
    if not s:
        return None
    return datetime.fromisoformat(s.replace("Z", "+00:00")).astimezone(timezone.utc)

buckets = get_json(f"{base}/buckets")
sessions_bucket = None
for k in buckets.keys():
    if k.startswith("aw-worktime-sessions_"):
        sessions_bucket = k
        break

if not sessions_bucket:
    raise SystemExit("No aw-worktime-sessions_* bucket found")

start = datetime.fromisoformat(from_d + "T00:00:00+00:00")
end = datetime.fromisoformat(to_d + "T23:59:59+00:00")

ev = get_json(f"{base}/buckets/{sessions_bucket}/events?limit=20000")
by_user = {}
for e in ev:
    ts = parse_ts(e.get("timestamp"))
    if ts is None or ts < start or ts > end:
        continue
    d = e.get("data") or {}
    user = (d.get("username") or "").strip()
    if not user:
        continue
    state = (d.get("state") or "").strip().lower()
    is_active = ("актив" in state) or (state == "active")
    rec = by_user.setdefault(user, {"active_ts": set(), "first": None, "last": None, "rows": 0})
    rec["rows"] += 1
    if is_active:
        rec["active_ts"].add(ts.replace(microsecond=0))
        rec["first"] = ts if rec["first"] is None or ts < rec["first"] else rec["first"]
        rec["last"] = ts if rec["last"] is None or ts > rec["last"] else rec["last"]

rows = []
full_range = int((end - start).total_seconds())
for user in sorted(by_user.keys()):
    rec = by_user[user]
    active = len(rec["active_ts"])
    idle = max(0, full_range - active)
    rows.append({
        "user": user,
        "active_seconds": int(active),
        "active_hhmm": f"{int(active)//3600:02d}:{(int(active)%3600)//60:02d}",
        "first_activity": rec["first"].isoformat().replace("+00:00","Z") if rec["first"] else "",
        "last_activity": rec["last"].isoformat().replace("+00:00","Z") if rec["last"] else "",
        "idle_seconds": int(idle),
        "sessions_count": rec["rows"],
    })

with open(csv_out, "w", newline="", encoding="utf-8") as f:
    w = csv.DictWriter(f, fieldnames=[
        "user","active_seconds","active_hhmm","first_activity","last_activity","idle_seconds","sessions_count"
    ])
    w.writeheader()
    w.writerows(rows)

with open(json_out, "w", encoding="utf-8") as f:
    json.dump({
        "from": from_d,
        "to": to_d,
        "generated_at_utc": datetime.now(timezone.utc).isoformat().replace("+00:00","Z"),
        "rows": rows
    }, f, ensure_ascii=False, indent=2)

print(csv_out)
print(json_out)
PY

echo "CSV: ${CSV_OUT}"
echo "JSON: ${JSON_OUT}"
