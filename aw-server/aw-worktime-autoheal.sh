#!/usr/bin/env bash
set -euo pipefail

AW_URL="${AW_URL:-http://127.0.0.1:5600}"
HOST="${AW_WORKTIME_HOST:-SHARKON2025}"
PYTHON_BIN="${PYTHON_BIN:-/usr/bin/python3}"
WORKTIME_HEALTH_URL="${WORKTIME_HEALTH_URL:-http://127.0.0.1:5610/health}"
WORKTIME_REPORT_TIMEOUT_SECONDS="${WORKTIME_REPORT_TIMEOUT_SECONDS:-20}"
WORKTIME_MANAGEMENT_WARM_ENABLED="${WORKTIME_MANAGEMENT_WARM_ENABLED:-1}"
WORKTIME_MANAGEMENT_WARM_URL="${WORKTIME_MANAGEMENT_WARM_URL:-http://127.0.0.1:5610/reports/worktime/management?day=today&format=json}"
WORKTIME_MANAGEMENT_WARM_TIMEOUT_SECONDS="${WORKTIME_MANAGEMENT_WARM_TIMEOUT_SECONDS:-60}"
WORKTIME_TODAY_PROBE_ENABLED="${WORKTIME_TODAY_PROBE_ENABLED:-1}"
WORKTIME_TODAY_PROBE_URL="${WORKTIME_TODAY_PROBE_URL:-http://127.0.0.1:5610/reports/worktime/today?day=today&format=json}"
WORKTIME_TODAY_PROBE_TIMEOUT_SECONDS="${WORKTIME_TODAY_PROBE_TIMEOUT_SECONDS:-20}"
WORKTIME_SESSION_FRESHNESS_SECONDS="${WORKTIME_SESSION_FRESHNESS_SECONDS:-600}"
LOG_TAG="aw-worktime-autoheal"

log() {
  logger -t "$LOG_TAG" "$*"
  printf '%s %s\n' "$(date '+%F %T')" "$*"
}

probe_url() {
  local url="$1"
  local timeout="$2"
  curl -fsS --max-time "$timeout" "$url" >/dev/null 2>&1
}

probe_reports() {
  probe_url "$WORKTIME_HEALTH_URL" "$WORKTIME_REPORT_TIMEOUT_SECONDS" || return 1
  if [[ "$WORKTIME_TODAY_PROBE_ENABLED" == "1" ]]; then
    probe_url "$WORKTIME_TODAY_PROBE_URL" "$WORKTIME_TODAY_PROBE_TIMEOUT_SECONDS" || return 1
  fi
}

worktime_ok=1
if ! probe_reports; then
  log "worktime API probe failed, restarting aw-worktime-api.service"
  systemctl restart aw-worktime-api.service || true
  sleep 2
  if ! probe_reports; then
    log "worktime API still degraded after restart"
    worktime_ok=0
  else
    log "worktime API recovered after restart"
  fi
fi

if [[ "$worktime_ok" != "1" ]]; then
  log "skip warm/heal because worktime API is still unavailable"
  exit 0
fi

if [[ "$WORKTIME_MANAGEMENT_WARM_ENABLED" == "1" ]]; then
  if probe_url "$WORKTIME_MANAGEMENT_WARM_URL" "$WORKTIME_MANAGEMENT_WARM_TIMEOUT_SECONDS"; then
    log "management cache warm ok"
  else
    log "management cache warm failed"
  fi
fi

need_heal="$("$PYTHON_BIN" - <<'PY'
import json, urllib.request, datetime, os, sys
AW=os.environ.get("AW_URL","http://127.0.0.1:5600")
host=os.environ.get("HOST","SHARKON2025")
window_bucket=f"aw-rdp-window_{host}"
session_bucket=f"aw-worktime-sessions_{host}"
freshness=int(os.environ.get("WORKTIME_SESSION_FRESHNESS_SECONDS","600"))
msk=datetime.timezone(datetime.timedelta(hours=3))
start=datetime.datetime.now(msk).replace(hour=0,minute=0,second=0,microsecond=0).astimezone(datetime.timezone.utc)
now=datetime.datetime.now(datetime.timezone.utc)

def p(ts):
    if ts.endswith("Z"): ts=ts[:-1] + "+00:00"
    return datetime.datetime.fromisoformat(ts).astimezone(datetime.timezone.utc)

def is_active(d):
    if isinstance(d.get("active"), bool) and d.get("active"):
        return True
    st=str(d.get("state","")).strip().lower()
    if st in ("active","активно"):
        return True
    if st == "unknown":
        try:
            sid=int(d.get("sessionId"))
        except Exception:
            sid=-1
        u=str(d.get("username","")).strip().lower()
        sn=str(d.get("sessionName","")).strip().lower()
        if sid > 0 and u and (not u.endswith("$")) and (sn.startswith("rdp-") or sn == "console"):
            return True
    return False

try:
    with urllib.request.urlopen(f"{AW}/api/0/buckets/{window_bucket}/events?limit=12000", timeout=25) as r:
        window_events=json.loads(r.read().decode("utf-8"))
    with urllib.request.urlopen(f"{AW}/api/0/buckets/{session_bucket}/events?limit=12000", timeout=25) as r:
        session_events=json.loads(r.read().decode("utf-8"))
except Exception:
    print("1")
    sys.exit(0)

latest_ts=None
active_users=set()
for e in session_events:
    ts=e.get("timestamp")
    if not ts:
        continue
    try:
        cur=p(ts)
    except Exception:
        continue
    if latest_ts is None or cur > latest_ts:
        latest_ts=cur
        active_users.clear()
    if cur == latest_ts:
        d=e.get("data") or {}
        u=str(d.get("username","")).strip()
        if u and is_active(d):
            active_users.add(u)

if latest_ts is None or (now - latest_ts).total_seconds() > freshness:
    print("0")
    sys.exit(0)

if not active_users:
    print("0")
    sys.exit(0)

active=0.0
for e in window_events:
    ts=e.get("timestamp")
    if not ts:
        continue
    try:
        if p(ts) < start:
            continue
    except Exception:
        continue
    d=float(e.get("duration",0) or 0)
    title=((e.get("data") or {}).get("title") or "").strip().lower()
    if d > 0 and "rdp active" in title:
        active += d

print("1" if active <= 0 else "0")
PY
)"

if [[ "$need_heal" != "1" ]]; then
  log "health ok: activity present for ${HOST}, no action"
  exit 0
fi

log "detected zero activity for ${HOST}, running heal"
systemctl restart aw-worktime-ui-bridge.timer
systemctl start aw-worktime-ui-bridge.service || true

"$PYTHON_BIN" - <<'PY'
import json, urllib.request, datetime, os
AW=os.environ.get("AW_URL","http://127.0.0.1:5600")
host=os.environ.get("HOST","SHARKON2025")
sb=f"aw-worktime-sessions_{host}"
afk=f"aw-rdp-afk_{host}"
win=f"aw-rdp-window_{host}"
msk=datetime.timezone(datetime.timedelta(hours=3))
start=datetime.datetime.now(msk).replace(hour=0,minute=0,second=0,microsecond=0).astimezone(datetime.timezone.utc)

def req(method,path,payload=None):
    data=None; headers={}
    if payload is not None:
        data=json.dumps(payload,ensure_ascii=False).encode("utf-8"); headers["Content-Type"]="application/json"
    r=urllib.request.Request(AW+path,data=data,headers=headers,method=method)
    try:
        with urllib.request.urlopen(r,timeout=30) as resp:
            body=resp.read()
            return json.loads(body.decode("utf-8")) if body else None
    except urllib.error.HTTPError as e:
        if e.code in (304, 409):
            return None
        raise

def reset_bucket(bucket_id, event_type, client, hostname):
    try:
        req("DELETE", f"/api/0/buckets/{bucket_id}")
    except Exception:
        pass
    req("POST", f"/api/0/buckets/{bucket_id}", {
        "client": client,
        "type": event_type,
        "hostname": hostname,
    })

def parse(ts):
    if ts.endswith("Z"): ts=ts[:-1]+"+00:00"
    return datetime.datetime.fromisoformat(ts).astimezone(datetime.timezone.utc)

def is_active(d):
    if isinstance(d.get("active"), bool) and d.get("active"): return True
    st=str(d.get("state","")).strip().lower()
    if st in ("active","активно"): return True
    if st=="unknown":
        try: sid=int(d.get("sessionId"))
        except: sid=-1
        u=str(d.get("username","")).strip().lower()
        sn=str(d.get("sessionName","")).strip().lower()
        if sid>0 and u and (not u.endswith("$")) and (sn.startswith("rdp-") or sn=="console"): return True
    return False

rows=req("GET",f"/api/0/buckets/{sb}/events?limit=12000") or []
rows=[e for e in rows if e.get("timestamp") and parse(e["timestamp"])>=start]
if not rows:
    raise SystemExit(0)

# Hard normalization: drop corrupted/mixed watcher buckets and rebuild from source sessions.
reset_bucket(afk, "afkstatus", "aw-worktime-ui-bridge", host)
reset_bucket(win, "currentwindow", "aw-worktime-ui-bridge", host)

by={}
for e in rows:
    by.setdefault(e["timestamp"],[]).append(e)
keys=sorted(by.keys())
out_afk=[]; out_win=[]
for i,ts in enumerate(keys):
    cur=parse(ts)
    nxt=parse(keys[i+1]) if i+1<len(keys) else None
    dur=max(0.0,(nxt-cur).total_seconds()) if nxt else 10.0
    dur=min(30.0, dur if dur>0 else 10.0)
    act=[]
    for r in by[ts]:
        d=r.get("data") or {}
        u=str(d.get("username","")).strip()
        if u and is_active(d): act.append(u)
    act=sorted(set(act))
    active=bool(act)
    out_afk.append({"timestamp":ts,"duration":dur,"data":{"status":"not-afk" if active else "afk","source":"aw-worktime-autoheal"}})
    out_win.append({"timestamp":ts,"duration":dur,"data":{"app":"RDP","title":("RDP active (%d): %s"%(len(act),", ".join(act))) if active else "RDP idle","source":"aw-worktime-autoheal"}})

req("POST",f"/api/0/buckets/{afk}/events",out_afk)
req("POST",f"/api/0/buckets/{win}/events",out_win)
print(f"autoheal backfill posted afk={len(out_afk)} win={len(out_win)}")
PY

log "heal completed for ${HOST}"
