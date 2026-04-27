#!/usr/bin/env sh
set -eu

SERVER_HOST="10.10.10.13"
SERVER_PORT="5600"
POLL_INTERVAL="5"
INSTALL_ROOT="/opt/aw-pve-webadmin-logger"
STATE_DIR="/var/lib/aw-pve-webadmin-logger"
LOG_DIR="/var/log/aw-pve-webadmin-logger"
CONFIG_PATH="/etc/aw-pve-webadmin-logger/config.json"
SERVICE_PATH="/etc/systemd/system/aw-pve-webadmin-logger.service"

usage() {
    cat <<'EOF'
Usage: install_aw_pve_webadmin_logger.sh [options]

Options:
  --server-host HOST     AW server host (default: 10.10.10.13)
  --server-port PORT     AW server port (default: 5600)
  --poll-interval SEC    Poll interval in seconds (default: 5)
  -h, --help             Show this help
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --server-host)
            SERVER_HOST="$2"
            shift 2
            ;;
        --server-port)
            SERVER_PORT="$2"
            shift 2
            ;;
        --poll-interval)
            POLL_INTERVAL="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

if [ "$(id -u)" -ne 0 ]; then
    echo "Run as root" >&2
    exit 1
fi

mkdir -p "$INSTALL_ROOT" "$STATE_DIR" "$LOG_DIR" "$(dirname "$CONFIG_PATH")"

HOST_SHORT="$(hostname -s)"

cat > "$CONFIG_PATH" <<EOF
{
  "server_host": "${SERVER_HOST}",
  "server_port": ${SERVER_PORT},
  "poll_interval_seconds": ${POLL_INTERVAL},
  "host": "${HOST_SHORT}",
  "state_dir": "${STATE_DIR}",
  "access_log": "/var/log/pveproxy/access.log",
  "tasks_index": "/var/log/pve/tasks/index",
  "web_bucket": "aw-pve-webadmin-events_${HOST_SHORT}",
  "task_bucket": "aw-pve-task-events_${HOST_SHORT}"
}
EOF

cat > "${INSTALL_ROOT}/collector.py" <<'PY'
#!/usr/bin/env python3
import datetime as dt
import json
import pathlib
import re
import socket
import time
import urllib.error
import urllib.request

ACCESS_RE = re.compile(
    r'^(?P<ip>\S+)\s+-\s+(?P<user>\S+)\s+\[(?P<ts>[^\]]+)\]\s+"(?P<method>\S+)\s+(?P<path>\S+)\s+(?P<proto>[^"]+)"\s+(?P<status>\d{3})\s+(?P<size>\S+)'
)
NOISE_GET_PATHS = [
    re.compile(r"^/api2/json/version$"),
    re.compile(r"^/api2/json/cluster/resources$"),
    re.compile(r"^/api2/json/cluster/tasks$"),
    re.compile(r"^/api2/json/nodes/[^/]+/(qemu|lxc)/\d+/status/current$"),
    re.compile(r"^/api2/json/nodes/[^/]+/(qemu|lxc)/\d+/interfaces$"),
    re.compile(r"^/api2/json/nodes/[^/]+/(qemu|lxc)/\d+/rrddata(\?.*)?$"),
]

TASK_RE = re.compile(
    r'^UPID:(?P<node>[^:]+):(?P<pid>[^:]+):(?P<pstart>[^:]+):(?P<start>[^:]+):(?P<action>[^:]*):(?P<target>[^:]*):(?P<user>[^:]*):\s*(?P<msg>.*)$'
)


def iso_now():
    return dt.datetime.now(tz=dt.timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")


def parse_access_ts(value: str) -> str:
    try:
        parsed = dt.datetime.strptime(value, "%d/%b/%Y:%H:%M:%S %z")
        return parsed.astimezone(dt.timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")
    except Exception:
        return iso_now()


class TailState:
    def __init__(self, path: pathlib.Path):
        self.path = path
        self.data = {"inode": None, "offset": 0}
        if path.exists():
            try:
                self.data = json.loads(path.read_text(encoding="utf-8"))
            except Exception:
                self.data = {"inode": None, "offset": 0}

    def save(self):
        self.path.write_text(json.dumps(self.data, ensure_ascii=True), encoding="utf-8")


class Collector:
    def __init__(self, cfg):
        self.cfg = cfg
        self.server = f"http://{cfg['server_host']}:{cfg['server_port']}/api/0"
        self.host = cfg.get("host") or socket.gethostname().split(".")[0]
        self.poll = max(1, int(cfg.get("poll_interval_seconds", 5)))
        self.web_bucket = cfg["web_bucket"]
        self.task_bucket = cfg["task_bucket"]
        self.access_log = pathlib.Path(cfg["access_log"])
        self.tasks_index = pathlib.Path(cfg["tasks_index"])
        self.state_dir = pathlib.Path(cfg["state_dir"])
        self.state_dir.mkdir(parents=True, exist_ok=True)
        self.access_state = TailState(self.state_dir / "access_state.json")
        self.tasks_state = TailState(self.state_dir / "tasks_state.json")
        self.ensured = set()
        self.recent = {}

    def ensure_bucket(self, bucket_id: str, bucket_type: str):
        if bucket_id in self.ensured:
            return True
        payload = {"client": "aw-pve-webadmin-logger", "type": bucket_type, "hostname": self.host}
        req = urllib.request.Request(
            f"{self.server}/buckets/{bucket_id}",
            data=json.dumps(payload).encode("utf-8"),
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        try:
            with urllib.request.urlopen(req, timeout=10):
                self.ensured.add(bucket_id)
                return True
        except urllib.error.HTTPError as err:
            if err.code in (304, 409):
                self.ensured.add(bucket_id)
                return True
            return False
        except urllib.error.URLError:
            return False

    def heartbeat(self, bucket_id: str, payload: dict, bucket_type: str):
        if not self.ensure_bucket(bucket_id, bucket_type):
            return False
        req = urllib.request.Request(
            f"{self.server}/buckets/{bucket_id}/heartbeat?pulsetime=60",
            data=json.dumps(payload, ensure_ascii=False).encode("utf-8"),
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        try:
            with urllib.request.urlopen(req, timeout=10):
                return True
        except urllib.error.URLError:
            return False

    def read_new_lines(self, src: pathlib.Path, st: TailState):
        if not src.exists():
            return []
        fs = src.stat()
        inode = int(fs.st_ino)
        size = int(fs.st_size)
        prev_inode = st.data.get("inode")
        prev_off = int(st.data.get("offset", 0))
        if prev_inode != inode or prev_off > size:
            prev_off = 0
        with src.open("r", encoding="utf-8", errors="replace") as f:
            f.seek(prev_off)
            lines = f.readlines()
            st.data = {"inode": inode, "offset": f.tell()}
        st.save()
        return [ln.rstrip("\n") for ln in lines if ln.strip()]

    def process_access(self):
        for line in self.read_new_lines(self.access_log, self.access_state):
            m = ACCESS_RE.match(line)
            if not m:
                continue
            user = m.group("user")
            status = int(m.group("status"))
            method = m.group("method")
            path = m.group("path")
            if path == "/api2/json/version":
                continue
            if user == "-" and status < 400:
                continue
            # Proxmox UI does high-frequency read polling. Keep real actions and auth failures.
            if method == "GET" and status == 200 and any(rx.match(path) for rx in NOISE_GET_PATHS):
                continue
            event_kind = "auth_failed" if status in (401, 403) else "request"
            dedup_key = f"{event_kind}|{user}|{m.group('ip')}|{method}|{path}|{status}"
            now = time.time()
            if now - float(self.recent.get(dedup_key, 0)) < 30:
                continue
            self.recent[dedup_key] = now
            event = {
                "timestamp": parse_access_ts(m.group("ts")),
                "duration": 0,
                "data": {
                    "source": "pveproxy_access",
                    "event_kind": event_kind,
                    "host": self.host,
                    "user": user,
                    "remote_ip": m.group("ip"),
                    "method": method,
                    "path": path,
                    "status": status,
                    "protocol": m.group("proto"),
                    "raw": line,
                },
            }
            self.heartbeat(self.web_bucket, event, "app.pve.webadmin.event")

    def process_tasks(self):
        for line in self.read_new_lines(self.tasks_index, self.tasks_state):
            m = TASK_RE.match(line)
            if not m:
                continue
            msg = m.group("msg")
            event = {
                "timestamp": iso_now(),
                "duration": 0,
                "data": {
                    "source": "pve_tasks_index",
                    "host": self.host,
                    "node": m.group("node"),
                    "upid_pid": m.group("pid"),
                    "action": m.group("action"),
                    "target": m.group("target"),
                    "user": m.group("user"),
                    "message": msg,
                    "result": "ok" if " OK" in msg else ("error" if "error" in msg.lower() else "info"),
                    "raw": line,
                },
            }
            self.heartbeat(self.task_bucket, event, "app.pve.task.event")

    def run(self):
        while True:
            self.process_access()
            self.process_tasks()
            time.sleep(self.poll)


def main():
    cfg = json.loads(pathlib.Path("/etc/aw-pve-webadmin-logger/config.json").read_text(encoding="utf-8"))
    Collector(cfg).run()


if __name__ == "__main__":
    main()
PY

chmod 0755 "${INSTALL_ROOT}/collector.py"

cat > "$SERVICE_PATH" <<EOF
[Unit]
Description=AW PVE web-admin activity logger
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=${INSTALL_ROOT}/collector.py
Restart=always
RestartSec=3
WorkingDirectory=${INSTALL_ROOT}
StandardOutput=append:${LOG_DIR}/collector.log
StandardError=append:${LOG_DIR}/collector.log

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable --now aw-pve-webadmin-logger.service
systemctl --no-pager --full status aw-pve-webadmin-logger.service || true

echo "Installed aw-pve-webadmin-logger"
echo "Config: ${CONFIG_PATH}"
echo "Buckets: aw-pve-webadmin-events_${HOST_SHORT}, aw-pve-task-events_${HOST_SHORT}"
