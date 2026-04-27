#!/usr/bin/env sh
set -eu

SERVER_HOST="10.10.10.13"
SERVER_PORT="5600"
POLL_INTERVAL="5"
INSTALL_ROOT="${HOME}/.local/opt/aw-console-ssh-logger"
BIN_DIR="${HOME}/.local/bin"
STATE_DIR="${HOME}/.local/state/aw-console-ssh-logger"
LOG_DIR="${HOME}/.local/state/aw-console-ssh-logger/logs"
SYSTEMD_USER_DIR="${HOME}/.config/systemd/user"
BASHRC="${HOME}/.bashrc"
BASH_PROFILE="${HOME}/.bash_profile"

usage() {
    cat <<'EOF'
Usage: install_aw_console_ssh_logger.sh [options]

Options:
  --server-host HOST     AW server host (default: 10.10.10.13)
  --server-port PORT     AW server port (default: 5600)
  --poll-interval SEC    Poll interval for history/session tracking (default: 5)
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

mkdir -p "${INSTALL_ROOT}" "${BIN_DIR}" "${STATE_DIR}" "${LOG_DIR}" "${SYSTEMD_USER_DIR}"

write_file() {
    target="$1"
    mkdir -p "$(dirname "${target}")"
    cat > "${target}"
}

write_file "${INSTALL_ROOT}/config.json" <<EOF
{
  "server_host": "${SERVER_HOST}",
  "server_port": ${SERVER_PORT},
  "poll_interval_seconds": ${POLL_INTERVAL},
  "history_file": "${HOME}/.bash_history",
  "state_dir": "${STATE_DIR}",
  "hostname": "$(hostname -s)",
  "username": "$(id -un)"
}
EOF

write_file "${INSTALL_ROOT}/collector.py" <<'EOF'
#!/usr/bin/env python3
import datetime as dt
import json
import os
import pathlib
import shutil
import socket
import subprocess
import time
import urllib.error
import urllib.request


def to_iso(ts: float) -> str:
    return dt.datetime.fromtimestamp(ts, tz=dt.timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")


class Collector:
    def __init__(self, cfg):
        self.cfg = cfg
        self.server = f"http://{cfg['server_host']}:{cfg['server_port']}/api/0"
        self.host = cfg.get("hostname") or socket.gethostname().split(".")[0]
        self.user = cfg.get("username") or os.environ.get("USER", "unknown")
        self.history_bucket = f"aw-console-commands_{self.host}"
        self.ssh_bucket = f"aw-ssh-sessions_{self.host}"
        self.history_path = pathlib.Path(cfg["history_file"]).expanduser()
        self.state_dir = pathlib.Path(cfg["state_dir"]).expanduser()
        self.state_dir.mkdir(parents=True, exist_ok=True)
        self.history_state_path = self.state_dir / "history_state.json"
        self.ssh_state_path = self.state_dir / "ssh_state.json"
        self.history_state = self._read_json(self.history_state_path, {"inode": None, "offset": 0})
        self.ssh_state = self._read_json(self.ssh_state_path, {"active": {}})
        self.poll_interval = max(1, int(cfg.get("poll_interval_seconds", 5)))
        self.ensured_buckets = set()

    def _read_json(self, path, default):
        if not path.exists():
            return default
        try:
            return json.loads(path.read_text(encoding="utf-8"))
        except Exception:
            return default

    def _write_json(self, path, payload):
        path.write_text(json.dumps(payload, ensure_ascii=True), encoding="utf-8")

    def _heartbeat(self, bucket_id, payload):
        self._ensure_bucket(bucket_id)
        url = f"{self.server}/buckets/{bucket_id}/heartbeat?pulsetime=60"
        body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        req = urllib.request.Request(url, data=body, headers={"Content-Type": "application/json"}, method="POST")
        try:
            with urllib.request.urlopen(req, timeout=10):
                return True
        except urllib.error.URLError:
            return False

    def _ensure_bucket(self, bucket_id):
        if bucket_id in self.ensured_buckets:
            return True

        if bucket_id == self.history_bucket:
            bucket_type = "app.console.command"
        elif bucket_id == self.ssh_bucket:
            bucket_type = "app.ssh.session"
        else:
            bucket_type = "app.custom"

        payload = {
            "client": "aw-console-ssh-logger",
            "type": bucket_type,
            "hostname": self.host
        }
        url = f"{self.server}/buckets/{bucket_id}"
        body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        req = urllib.request.Request(url, data=body, headers={"Content-Type": "application/json"}, method="POST")
        try:
            with urllib.request.urlopen(req, timeout=10):
                self.ensured_buckets.add(bucket_id)
                return True
        except urllib.error.URLError:
            return False

    def process_history(self):
        if not self.history_path.exists():
            return

        st = self.history_path.stat()
        inode = int(st.st_ino)
        size = int(st.st_size)

        if self.history_state.get("inode") != inode or int(self.history_state.get("offset", 0)) > size:
            self.history_state = {"inode": inode, "offset": 0}

        offset = int(self.history_state.get("offset", 0))
        with self.history_path.open("r", encoding="utf-8", errors="replace") as f:
            f.seek(offset)
            lines = f.readlines()
            new_offset = f.tell()

        pending_ts = None
        now_iso = to_iso(time.time())
        for raw in lines:
            line = raw.rstrip("\n")
            if not line:
                continue
            if line.startswith("#") and line[1:].isdigit():
                pending_ts = int(line[1:])
                continue

            ts = pending_ts if pending_ts is not None else time.time()
            pending_ts = None
            event = {
                "timestamp": to_iso(ts),
                "duration": 0,
                "data": {
                    "source": "bash_history",
                    "user": self.user,
                    "host": self.host,
                    "command": line
                }
            }
            self._heartbeat(self.history_bucket, event)

        self.history_state["inode"] = inode
        self.history_state["offset"] = new_offset
        self._write_json(self.history_state_path, self.history_state)

    def get_sessions(self):
        who_bin = shutil.which("who") or "/usr/bin/who"
        try:
            out = subprocess.check_output([who_bin, "-u"], text=True, stderr=subprocess.DEVNULL)
        except Exception:
            return {}

        sessions = {}
        for row in out.splitlines():
            parts = row.split()
            if len(parts) < 2:
                continue
            user = parts[0]
            tty = parts[1]
            if not tty.startswith("pts/"):
                continue
            sessions[tty] = {
                "user": user,
                "tty": tty,
                "raw": row
            }
        return sessions

    def process_ssh_sessions(self):
        active_prev = dict(self.ssh_state.get("active", {}))
        active_now = self.get_sessions()

        for tty, meta in active_now.items():
            if tty in active_prev:
                continue
            event = {
                "timestamp": to_iso(time.time()),
                "duration": 0,
                "data": {
                    "source": "who",
                    "event": "login",
                    "user": meta["user"],
                    "tty": tty,
                    "host": self.host,
                    "raw": meta["raw"]
                }
            }
            self._heartbeat(self.ssh_bucket, event)

        for tty, meta in active_prev.items():
            if tty in active_now:
                continue
            event = {
                "timestamp": to_iso(time.time()),
                "duration": 0,
                "data": {
                    "source": "who",
                    "event": "logout",
                    "user": meta.get("user", self.user),
                    "tty": tty,
                    "host": self.host,
                    "raw": meta.get("raw", "")
                }
            }
            self._heartbeat(self.ssh_bucket, event)

        self.ssh_state["active"] = active_now
        self._write_json(self.ssh_state_path, self.ssh_state)

    def run(self):
        while True:
            self.process_history()
            self.process_ssh_sessions()
            time.sleep(self.poll_interval)


def main():
    config_path = pathlib.Path(os.environ.get("AW_CONSOLE_SSH_CONFIG", "~/.local/opt/aw-console-ssh-logger/config.json")).expanduser()
    cfg = json.loads(config_path.read_text(encoding="utf-8"))
    Collector(cfg).run()


if __name__ == "__main__":
    main()
EOF
chmod 0755 "${INSTALL_ROOT}/collector.py"

write_file "${BIN_DIR}/aw-console-ssh-logger-start" <<EOF
#!/usr/bin/env sh
set -eu
export AW_CONSOLE_SSH_CONFIG="${INSTALL_ROOT}/config.json"
mkdir -p "${LOG_DIR}"
if pgrep -u "$(id -u)" -f "collector.py" >/dev/null 2>&1; then
    exit 0
fi
nohup "${INSTALL_ROOT}/collector.py" >> "${LOG_DIR}/collector.log" 2>&1 &
EOF
chmod 0755 "${BIN_DIR}/aw-console-ssh-logger-start"

write_file "${BIN_DIR}/aw-console-ssh-logger-stop" <<'EOF'
#!/usr/bin/env sh
set -eu
pkill -u "$(id -u)" -f "aw-console-ssh-logger/collector.py" || true
EOF
chmod 0755 "${BIN_DIR}/aw-console-ssh-logger-stop"

write_file "${BIN_DIR}/aw-console-ssh-logger-status" <<'EOF'
#!/usr/bin/env sh
set -eu
pgrep -a -u "$(id -u)" -f "aw-console-ssh-logger/collector.py" || true
EOF
chmod 0755 "${BIN_DIR}/aw-console-ssh-logger-status"

write_file "${SYSTEMD_USER_DIR}/aw-console-ssh-logger.service" <<EOF
[Unit]
Description=AW console and SSH logger (user space)
After=default.target

[Service]
Type=simple
Environment=AW_CONSOLE_SSH_CONFIG=${INSTALL_ROOT}/config.json
ExecStart=${INSTALL_ROOT}/collector.py
Restart=always
RestartSec=3

[Install]
WantedBy=default.target
EOF

HIST_BLOCK_START="# >>> AW_CONSOLE_SSH_HISTORY >>>"
HIST_BLOCK_END="# <<< AW_CONSOLE_SSH_HISTORY <<<"
if ! grep -qF "${HIST_BLOCK_START}" "${BASHRC}" 2>/dev/null; then
    cat >> "${BASHRC}" <<EOF

${HIST_BLOCK_START}
export HISTSIZE=200000
export HISTFILESIZE=200000
export HISTTIMEFORMAT="%s "
shopt -s histappend
case "\${PROMPT_COMMAND-}" in
  *"history -a; history -n;"*) ;;
  "") PROMPT_COMMAND="history -a; history -n;" ;;
  *) PROMPT_COMMAND="history -a; history -n; \${PROMPT_COMMAND}" ;;
esac
${HIST_BLOCK_END}
EOF
fi

AUTO_BLOCK_START="# >>> AW_CONSOLE_SSH_AUTOSTART >>>"
AUTO_BLOCK_END="# <<< AW_CONSOLE_SSH_AUTOSTART <<<"
if ! grep -qF "${AUTO_BLOCK_START}" "${BASH_PROFILE}" 2>/dev/null; then
    cat >> "${BASH_PROFILE}" <<EOF

${AUTO_BLOCK_START}
if [ -x "${BIN_DIR}/aw-console-ssh-logger-start" ]; then
  "${BIN_DIR}/aw-console-ssh-logger-start" || true
fi
${AUTO_BLOCK_END}
EOF
fi

if command -v systemctl >/dev/null 2>&1; then
    systemctl --user daemon-reload >/dev/null 2>&1 || true
    systemctl --user enable --now aw-console-ssh-logger.service >/dev/null 2>&1 || true
fi

"${BIN_DIR}/aw-console-ssh-logger-start"

echo "Installed AW console/ssh logger"
echo "Server target: ${SERVER_HOST}:${SERVER_PORT}"
echo "Collector: ${INSTALL_ROOT}/collector.py"
echo "State dir: ${STATE_DIR}"
echo "Log file: ${LOG_DIR}/collector.log"
