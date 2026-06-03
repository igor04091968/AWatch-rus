#!/usr/bin/env sh
set -eu

SERVER_HOST="192.0.2.13"
SERVER_PORT="5600"
POLL_INTERVAL="5"
AW_VERSION="0.13.2"
SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
ROOT_DIR="$(CDPATH= cd -- "${SCRIPT_DIR}/.." && pwd)"
SELF_PATH="${SCRIPT_DIR}/$(basename -- "$0")"

if [ "${1:-}" = "--apply-legacy" ]; then
    shift
else
    TARGET_ROOT="${CARGO_TARGET_DIR:-${ROOT_DIR}/adk-rust/target}"
    for candidate in \
        "${AW_LINUX_INSTALL_RUST:-}" \
        "${TARGET_ROOT}/release/aw-linux-install" \
        "${ROOT_DIR}/adk-rust/target/release/aw-linux-install" \
        "/usr/local/bin/aw-linux-install"; do
        if [ -n "$candidate" ] && [ -x "$candidate" ]; then
            exec "$candidate" --kind remote-worker --legacy-script "$SELF_PATH" "$@"
        fi
    done
    cat >&2 <<'EOF'
install_aw_linux_remote_worker.sh now requires the Rust planner for safe default runs.
Build it first:
  cd adk-rust && cargo build --release -p aw-linux-install

Safe dry-run:
  scripts/install_aw_linux_remote_worker.sh --json

Explicit install:
  scripts/install_aw_linux_remote_worker.sh --apply

Old shell install:
  scripts/install_aw_linux_remote_worker.sh --apply-legacy
EOF
    exit 2
fi

usage() {
    cat <<'EOF'
Usage: install_aw_linux_remote_worker.sh [options]

Options:
  --server-host HOST     Remote AW server host (default: 192.0.2.13)
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

sh "${SCRIPT_DIR}/install_aw_linux_client.sh" \
    --apply-legacy \
    --server-host "${SERVER_HOST}" \
    --server-port "${SERVER_PORT}" \
    --version "${AW_VERSION}"

sh "${SCRIPT_DIR}/install_aw_console_ssh_logger.sh" \
    --apply-legacy \
    --server-host "${SERVER_HOST}" \
    --server-port "${SERVER_PORT}" \
    --poll-interval "${POLL_INTERVAL}"

sh "${SCRIPT_DIR}/install_aw_linux_web_category_logger.sh" \
    --apply-legacy \
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
