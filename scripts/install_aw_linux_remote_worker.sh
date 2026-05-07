#!/usr/bin/env sh
set -eu

SERVER_HOST="10.10.10.13"
SERVER_PORT="5600"
POLL_INTERVAL="5"
AW_VERSION="0.13.2"

usage() {
    cat <<'EOF'
Usage: install_aw_linux_remote_worker.sh [options]

Options:
  --server-host HOST     Remote AW server host (default: 10.10.10.13)
  --server-port PORT     Remote AW server port (default: 5600)
  --poll-interval SEC    Poll interval for Linux loggers (default: 5)
  --version VERSION      ActivityWatch version for GUI watcher bundle (default: 0.13.2)
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
        --version)
            AW_VERSION="$2"
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

# shellcheck disable=SC1007
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

sh "${SCRIPT_DIR}/install_aw_linux_client.sh" \
    --server-host "${SERVER_HOST}" \
    --server-port "${SERVER_PORT}" \
    --version "${AW_VERSION}"

sh "${SCRIPT_DIR}/install_aw_console_ssh_logger.sh" \
    --server-host "${SERVER_HOST}" \
    --server-port "${SERVER_PORT}" \
    --poll-interval "${POLL_INTERVAL}"

sh "${SCRIPT_DIR}/install_aw_linux_web_category_logger.sh" \
    --server-host "${SERVER_HOST}" \
    --server-port "${SERVER_PORT}" \
    --poll-interval "${POLL_INTERVAL}"

echo "Linux remote worker full-stack install completed."
echo "Expected buckets on AW server:"
echo "  - aw-watcher-window_$(hostname -s)"
echo "  - aw-watcher-afk_$(hostname -s)"
echo "  - aw-console-commands_$(hostname -s)"
echo "  - aw-ssh-sessions_$(hostname -s)"
echo "  - aw-linux-web-context_$(hostname -s)"
echo "  - aw-detmir-web-category_$(hostname -s)"
