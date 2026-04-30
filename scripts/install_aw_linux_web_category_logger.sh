#!/usr/bin/env sh
set -eu

SERVER_HOST="10.10.10.13"
SERVER_PORT="5600"
POLL_INTERVAL="5"
INSTALL_ROOT="${HOME}/.local/opt/aw-linux-web-category"
BIN_DIR="${HOME}/.local/bin"
STATE_DIR="${HOME}/.local/state/aw-linux-web-category"
LOG_DIR="${HOME}/.local/state/aw-linux-web-category/logs"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/aw-linux-web-category"
SYSTEMD_USER_DIR="${HOME}/.config/systemd/user"

usage() {
    cat <<'EOF'
Usage: install_aw_linux_web_category_logger.sh [options]

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

mkdir -p "${INSTALL_ROOT}" "${BIN_DIR}" "${STATE_DIR}" "${LOG_DIR}" "${CONFIG_DIR}" "${SYSTEMD_USER_DIR}"

write_file() {
    target="$1"
    mkdir -p "$(dirname "${target}")"
    cat > "${target}"
}

write_file "${CONFIG_DIR}/rules.json" <<'EOF'
{
  "rules": [
    {
      "id": "proxmox-webui",
      "categoryGroup": "work",
      "category": "Администрирование",
      "service": "proxmox",
      "interface": "https",
      "port": 8006,
      "rootDomain": "proxmox-webui",
      "windowClassRegex": "(?i)(firefox|chromium|chrome|brave|vivaldi|opera)",
      "titleRegex": "(?i)proxmox\\s+virtual\\s+environment|\\bproxmox\\b|\\bnode\\b.*\\bsummary\\b|\\bvirtual machine\\b"
    },
    {
      "id": "pfsense-webui",
      "categoryGroup": "work",
      "category": "Администрирование",
      "service": "pfsense",
      "interface": "https",
      "port": 443,
      "rootDomain": "pfsense-webui",
      "windowClassRegex": "(?i)(firefox|chromium|chrome|brave|vivaldi|opera)",
      "titleRegex": "(?i)\\bpfsense\\b|\\bfirewall\\b"
    },
    {
      "id": "grafana-webui",
      "categoryGroup": "work",
      "category": "Администрирование",
      "service": "grafana",
      "interface": "https",
      "port": 3000,
      "rootDomain": "grafana-webui",
      "windowClassRegex": "(?i)(firefox|chromium|chrome|brave|vivaldi|opera)",
      "titleRegex": "(?i)\\bgrafana\\b|dashboard"
    }
  ]
}
EOF

write_file "${INSTALL_ROOT}/config.json" <<EOF
{
  "server_host": "${SERVER_HOST}",
  "server_port": ${SERVER_PORT},
  "poll_interval_seconds": ${POLL_INTERVAL},
  "hostname": "$(hostname -s)",
  "username": "$(id -un)",
  "state_dir": "${STATE_DIR}",
  "rules_path": "${CONFIG_DIR}/rules.json",
  "raw_bucket": "aw-linux-web-context_$(hostname -s)",
  "category_bucket": "aw-detmir-web-category_$(hostname -s)"
}
EOF

write_file "${INSTALL_ROOT}/collector.py" <<'EOF'
#!/usr/bin/env python3
import datetime as dt
import json
import os
import pathlib
import re
import socket
import subprocess
import time
import urllib.error
import urllib.request


def iso_now():
    return dt.datetime.now(tz=dt.timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")


class Collector:
    def __init__(self, cfg):
        self.cfg = cfg
        self.server = f"http://{cfg['server_host']}:{cfg['server_port']}/api/0"
        self.host = cfg.get("hostname") or socket.gethostname().split(".")[0]
        self.user = cfg.get("username") or os.environ.get("USER", "unknown")
        self.poll_interval = max(1, int(cfg.get("poll_interval_seconds", 5)))
        self.state_dir = pathlib.Path(cfg["state_dir"]).expanduser()
        self.state_dir.mkdir(parents=True, exist_ok=True)
        self.raw_bucket = cfg.get("raw_bucket", f"aw-linux-web-context_{self.host}")
        self.category_bucket = cfg.get("category_bucket", f"aw-detmir-web-category_{self.host}")
        self.rules = self._load_rules(pathlib.Path(cfg["rules_path"]).expanduser())
        self.ensured = set()
        self.last_raw_key = None
        self.last_category_key = None

    def _load_rules(self, path):
        if not path.exists():
            return []
        payload = json.loads(path.read_text(encoding="utf-8"))
        rules = []
        for item in payload.get("rules", []):
            rules.append({
                "id": item.get("id", "rule"),
                "categoryGroup": item.get("categoryGroup", "work"),
                "category": item.get("category", "Работа"),
                "service": item.get("service", ""),
                "interface": item.get("interface", "https"),
                "port": item.get("port"),
                "rootDomain": item.get("rootDomain", item.get("service", "web-ui")),
                "windowClassRegex": re.compile(item.get("windowClassRegex", ".*")),
                "titleRegex": re.compile(item.get("titleRegex", ".*"))
            })
        return rules

    def _run(self, *cmd):
        try:
            return subprocess.check_output(cmd, text=True, stderr=subprocess.DEVNULL).strip()
        except Exception:
            return ""

    def get_active_window(self):
        win_id_line = self._run("xprop", "-root", "_NET_ACTIVE_WINDOW")
        match = re.search(r"0x[0-9a-fA-F]+", win_id_line)
        if not match:
            return None
        win_id = match.group(0)
        title = self._run("xprop", "-id", win_id, "_NET_WM_NAME")
        if not title:
            title = self._run("xprop", "-id", win_id, "WM_NAME")
        klass = self._run("xprop", "-id", win_id, "WM_CLASS")

        title_match = re.search(r'=\s*"(?P<value>.*)"\s*$', title)
        if title_match:
            title = title_match.group("value")
        else:
            title = title.split("=", 1)[-1].strip().strip('"')

        class_values = re.findall(r'"([^"]+)"', klass)
        window_class = " ".join(class_values) if class_values else klass.split("=", 1)[-1].strip()

        if not title and not window_class:
            return None

        return {
            "windowId": win_id,
            "title": title,
            "windowClass": window_class
        }

    def ensure_bucket(self, bucket_id, bucket_type):
        if bucket_id in self.ensured:
            return True
        payload = {"client": "aw-linux-web-category", "type": bucket_type, "hostname": self.host}
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

    def heartbeat(self, bucket_id, payload, bucket_type, pulse=60):
        if not self.ensure_bucket(bucket_id, bucket_type):
            return False
        req = urllib.request.Request(
            f"{self.server}/buckets/{bucket_id}/heartbeat?pulsetime={pulse}",
            data=json.dumps(payload, ensure_ascii=False).encode("utf-8"),
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        try:
            with urllib.request.urlopen(req, timeout=10):
                return True
        except urllib.error.URLError:
            return False

    def match_rule(self, window):
        title = window.get("title", "")
        window_class = window.get("windowClass", "")
        for rule in self.rules:
            if rule["windowClassRegex"].search(window_class) and rule["titleRegex"].search(title):
                return rule
        return None

    def run(self):
        while True:
            window = self.get_active_window()
            if window:
                raw_key = f"{window.get('windowClass','')}|{window.get('title','')}"
                if raw_key != self.last_raw_key:
                    raw_event = {
                        "timestamp": iso_now(),
                        "duration": 0,
                        "data": {
                            "source": "x11_active_window",
                            "username": self.user,
                            "host": self.host,
                            "windowId": window.get("windowId", ""),
                            "windowClass": window.get("windowClass", ""),
                            "title": window.get("title", "")
                        }
                    }
                    self.heartbeat(self.raw_bucket, raw_event, "aw.linux.web.context", pulse=20)
                    self.last_raw_key = raw_key

                rule = self.match_rule(window)
                if rule:
                    category_key = rule["id"] + "|" + window.get("title", "")
                    if category_key != self.last_category_key:
                        event = {
                            "timestamp": iso_now(),
                            "duration": 0,
                            "data": {
                                "source": "linux_window_title_rule",
                                "username": self.user,
                                "host": self.host,
                                "windowClass": window.get("windowClass", ""),
                                "title": window.get("title", ""),
                                "categoryGroup": rule["categoryGroup"],
                                "category": rule["category"],
                                "categoryRule": rule["id"],
                                "service": rule["service"],
                                "interface": rule["interface"],
                                "port": rule["port"],
                                "rootDomain": rule["rootDomain"]
                            }
                        }
                        self.heartbeat(self.category_bucket, event, "aw.web.category", pulse=30)
                        self.last_category_key = category_key

            time.sleep(self.poll_interval)


def main():
    config_path = pathlib.Path(os.environ.get("AW_LINUX_WEB_CATEGORY_CONFIG", "~/.local/opt/aw-linux-web-category/config.json")).expanduser()
    cfg = json.loads(config_path.read_text(encoding="utf-8"))
    Collector(cfg).run()


if __name__ == "__main__":
    main()
EOF
chmod 0755 "${INSTALL_ROOT}/collector.py"

write_file "${BIN_DIR}/aw-linux-web-category-start" <<EOF
#!/usr/bin/env sh
set -eu
export AW_LINUX_WEB_CATEGORY_CONFIG="${INSTALL_ROOT}/config.json"
mkdir -p "${LOG_DIR}"
if pgrep -u "$(id -u)" -f "aw-linux-web-category/collector.py" >/dev/null 2>&1; then
    exit 0
fi
nohup "${INSTALL_ROOT}/collector.py" >> "${LOG_DIR}/collector.log" 2>&1 &
EOF
chmod 0755 "${BIN_DIR}/aw-linux-web-category-start"

write_file "${BIN_DIR}/aw-linux-web-category-stop" <<'EOF'
#!/usr/bin/env sh
set -eu
pkill -u "$(id -u)" -f "aw-linux-web-category/collector.py" || true
EOF
chmod 0755 "${BIN_DIR}/aw-linux-web-category-stop"

write_file "${BIN_DIR}/aw-linux-web-category-status" <<'EOF'
#!/usr/bin/env sh
set -eu
pgrep -a -u "$(id -u)" -f "aw-linux-web-category/collector.py" || true
EOF
chmod 0755 "${BIN_DIR}/aw-linux-web-category-status"

write_file "${SYSTEMD_USER_DIR}/aw-linux-web-category.service" <<EOF
[Unit]
Description=AW Linux web-category logger (user space)
After=default.target

[Service]
Type=simple
Environment=AW_LINUX_WEB_CATEGORY_CONFIG=${INSTALL_ROOT}/config.json
ExecStart=${INSTALL_ROOT}/collector.py
Restart=always
RestartSec=3

[Install]
WantedBy=default.target
EOF

started_with_systemd="0"
if command -v systemctl >/dev/null 2>&1; then
    systemctl --user daemon-reload >/dev/null 2>&1 || true
    systemctl --user enable --now aw-linux-web-category.service >/dev/null 2>&1 || true
    if systemctl --user is-active --quiet aw-linux-web-category.service >/dev/null 2>&1; then
        started_with_systemd="1"
    fi
fi

if [ "${started_with_systemd}" != "1" ]; then
    "${BIN_DIR}/aw-linux-web-category-start"
fi

echo "Installed AW Linux web-category logger"
echo "Server target: ${SERVER_HOST}:${SERVER_PORT}"
echo "Raw bucket: aw-linux-web-context_$(hostname -s)"
echo "Category bucket: aw-detmir-web-category_$(hostname -s)"
echo "Rules: ${CONFIG_DIR}/rules.json"
echo "Log file: ${LOG_DIR}/collector.log"
