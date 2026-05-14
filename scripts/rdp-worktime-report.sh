#!/usr/bin/env bash
set -euo pipefail

DAY=""
FROM=""
TO=""
AW_BASE_URL="${AW_BASE_URL:-http://10.10.10.13:5600/api/0}"
AW_WORKTIME_HOST="${AW_WORKTIME_HOST:-SHARKON2025}"
AW_WORKTIME_DEFAULT_SAMPLE_SECONDS="${AW_WORKTIME_DEFAULT_SAMPLE_SECONDS:-30}"
AW_WORKTIME_MAX_SAMPLE_SECONDS="${AW_WORKTIME_MAX_SAMPLE_SECONDS:-300}"
OUT_DIR="${OUT_DIR:-reports}"

usage() {
  cat <<EOF
Usage:
  $0 --day today|yesterday
  $0 --from YYYY-MM-DD --to YYYY-MM-DD
Env:
  AW_BASE_URL (default: ${AW_BASE_URL})
  AW_WORKTIME_HOST (default: ${AW_WORKTIME_HOST})
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

python3 - "$AW_BASE_URL" "$AW_WORKTIME_HOST" "$AW_WORKTIME_DEFAULT_SAMPLE_SECONDS" "$AW_WORKTIME_MAX_SAMPLE_SECONDS" "$FROM" "$TO" "$CSV_OUT" "$JSON_OUT" <<'PY'
import csv
import json
import sys
import urllib.request
from datetime import datetime, timedelta, timezone

base, host, default_sample, max_sample, from_d, to_d, csv_out, json_out = sys.argv[1:9]
base = (base or "http://10.10.10.13:5600").rstrip("/")
if not base.endswith("/api/0"):
    base = base + "/api/0"
default_sample = max(1.0, float(default_sample))
max_sample = max(default_sample, float(max_sample))

def get_json(url: str):
    with urllib.request.urlopen(url, timeout=30) as r:
        return json.loads(r.read().decode())

def parse_ts(s):
    if not s:
        return None
    return datetime.fromisoformat(s.replace("Z", "+00:00")).astimezone(timezone.utc)

def clamp_seconds(value, fallback=default_sample):
    try:
        seconds = float(value)
    except Exception:
        seconds = float(fallback)
    if seconds <= 0:
        seconds = float(fallback)
    return min(seconds, max_sample)

def merge_intervals(intervals):
    if not intervals:
        return []
    intervals = sorted(intervals, key=lambda item: item[0])
    merged = [intervals[0]]
    for start, end in intervals[1:]:
        last_start, last_end = merged[-1]
        if start <= last_end:
            if end > last_end:
                merged[-1] = (last_start, end)
            continue
        merged.append((start, end))
    return merged

def is_active(data):
    state = str(data.get("state") or "").strip().lower()
    if isinstance(data.get("active"), bool) and data.get("active"):
        return True
    return ("актив" in state) or (state == "active")

def normalize_user_id(data, host_name, username):
    raw = str(data.get("userId") or "").strip()
    if raw and "\\" in raw:
        _, right = raw.split("\\", 1)
        return f"{host_name}\\{right}"
    if raw:
        return raw
    return f"{host_name}\\{username}"

bucket_id = f"aw-worktime-sessions_{host}"
try:
    get_json(f"{base}/buckets/{bucket_id}")
except Exception:
    raise SystemExit(f"Bucket not found: {bucket_id}")

start = datetime.fromisoformat(from_d + "T00:00:00+00:00")
end = datetime.fromisoformat(to_d + "T23:59:59+00:00")

ev = get_json(f"{base}/buckets/{bucket_id}/events?limit=50000")
by_identity = {}
for e in ev:
    ts = parse_ts(e.get("timestamp"))
    if ts is None or ts < start or ts > end:
        continue
    d = e.get("data") or {}
    user = (d.get("username") or "").strip()
    if not user:
        continue
    session_id = str(d.get("sessionId") or "").strip() or "unknown"
    by_identity.setdefault((user, session_id), []).append({
        "ts": ts,
        "duration": e.get("duration"),
        "data": d,
    })

rows = []
full_range = int((end - start).total_seconds()) + 1
by_user = {}
for (user, session_id), samples in by_identity.items():
    samples = sorted(samples, key=lambda item: item["ts"])
    for idx, sample in enumerate(samples):
        data = sample["data"]
        rec = by_user.setdefault(user, {
            "user": user,
            "user_id": normalize_user_id(data, host, user),
            "sessions": set(),
            "samples_count": 0,
            "active_samples": 0,
            "intervals": [],
        })
        rec["sessions"].add(session_id)
        rec["samples_count"] += 1
        if not is_active(data):
            continue
        rec["active_samples"] += 1
        sample_seconds = None
        for key in ("sampleSeconds", "pollSeconds"):
            value = data.get(key)
            try:
                if float(value) > 0:
                    sample_seconds = clamp_seconds(value)
                    break
            except Exception:
                pass
        if sample_seconds is None:
            try:
                duration = float(sample.get("duration") or 0.0)
            except Exception:
                duration = 0.0
            if duration > 0:
                sample_seconds = clamp_seconds(duration)
            else:
                next_ts = samples[idx + 1]["ts"] if idx + 1 < len(samples) else None
                if next_ts is not None:
                    sample_seconds = clamp_seconds((next_ts - sample["ts"]).total_seconds())
                else:
                    sample_seconds = clamp_seconds(default_sample)
        interval_end = min(sample["ts"] + timedelta(seconds=sample_seconds), end + timedelta(seconds=1))
        if interval_end > sample["ts"]:
            rec["intervals"].append((sample["ts"], interval_end))

for user in sorted(by_user.keys()):
    rec = by_user[user]
    merged = merge_intervals(rec["intervals"])
    active = int(sum((finish - begin).total_seconds() for begin, finish in merged))
    active = min(active, full_range)
    idle = max(0, full_range - active)
    rows.append({
        "user": rec["user"],
        "user_id": rec["user_id"],
        "active_seconds": int(active),
        "active_hhmm": f"{int(active)//3600:02d}:{(int(active)%3600)//60:02d}",
        "first_activity": merged[0][0].isoformat().replace("+00:00","Z") if merged else "",
        "last_activity": merged[-1][1].isoformat().replace("+00:00","Z") if merged else "",
        "idle_seconds": int(idle),
        "sessions_count": len(rec["sessions"]),
        "samples_count": rec["samples_count"],
        "active_samples": rec["active_samples"],
    })

with open(csv_out, "w", newline="", encoding="utf-8") as f:
    w = csv.DictWriter(f, fieldnames=[
        "user","user_id","active_seconds","active_hhmm","first_activity","last_activity","idle_seconds","sessions_count","samples_count","active_samples"
    ])
    w.writeheader()
    w.writerows(rows)

with open(json_out, "w", encoding="utf-8") as f:
    json.dump({
        "host": host,
        "bucket_id": bucket_id,
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
