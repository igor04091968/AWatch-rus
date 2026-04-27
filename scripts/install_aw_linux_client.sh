#!/usr/bin/env sh
set -eu

VERSION="0.13.2"
SERVER_HOST="10.10.10.13"
SERVER_PORT="5600"
INSTALL_BASE="${HOME}/.local/opt/activitywatch"
BIN_DIR="${HOME}/.local/bin"
AUTOSTART_DIR="${HOME}/.config/autostart"
CONFIG_ROOT="${XDG_CONFIG_HOME:-$HOME/.config}/activitywatch"
FORCE="0"

usage() {
    cat <<'EOF'
Usage: install_aw_linux_client.sh [options]

Options:
  --server-host HOST     Remote AW server host (default: 10.10.10.13)
  --server-port PORT     Remote AW server port (default: 5600)
  --version VERSION      ActivityWatch version (default: 0.13.2)
  --install-base PATH    Install root (default: ~/.local/opt/activitywatch)
  --force                Reinstall selected version even if it exists
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
        --version)
            VERSION="$2"
            shift 2
            ;;
        --install-base)
            INSTALL_BASE="$2"
            shift 2
            ;;
        --force)
            FORCE="1"
            shift
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

ARCHIVE="activitywatch-v${VERSION}-linux-x86_64.zip"
DOWNLOAD_URL="https://github.com/ActivityWatch/activitywatch/releases/download/v${VERSION}/${ARCHIVE}"
VERSION_DIR="${INSTALL_BASE}/v${VERSION}"
CURRENT_LINK="${INSTALL_BASE}/current"
TMP_DIR="$(mktemp -d)"
ARCHIVE_PATH="${TMP_DIR}/${ARCHIVE}"
BUNDLE_DIR=""

cleanup() {
    rm -rf "${TMP_DIR}"
}
trap cleanup EXIT INT TERM

download_archive() {
    if command -v curl >/dev/null 2>&1; then
        curl -fL "${DOWNLOAD_URL}" -o "${ARCHIVE_PATH}"
        return
    fi

    if command -v wget >/dev/null 2>&1; then
        wget -O "${ARCHIVE_PATH}" "${DOWNLOAD_URL}"
        return
    fi

    echo "Neither curl nor wget is available" >&2
    exit 1
}

extract_archive() {
    mkdir -p "${VERSION_DIR}"

    if command -v unzip >/dev/null 2>&1; then
        unzip -q "${ARCHIVE_PATH}" -d "${VERSION_DIR}"
        return
    fi

    python3 -m zipfile -e "${ARCHIVE_PATH}" "${VERSION_DIR}"
}

resolve_bundle_dir() {
    if [ -x "${VERSION_DIR}/aw-qt" ]; then
        BUNDLE_DIR="${VERSION_DIR}"
        return
    fi

    if [ -x "${VERSION_DIR}/activitywatch/aw-qt" ]; then
        BUNDLE_DIR="${VERSION_DIR}/activitywatch"
        return
    fi

    echo "Cannot find aw-qt inside ${VERSION_DIR}" >&2
    exit 1
}

write_file() {
    target="$1"
    mkdir -p "$(dirname "${target}")"
    cat > "${target}"
}

if [ -d "${VERSION_DIR}" ] && [ "${FORCE}" != "1" ]; then
    echo "Version directory already exists: ${VERSION_DIR}" >&2
    echo "Use --force to reinstall the same version." >&2
    exit 1
fi

rm -rf "${VERSION_DIR}"
mkdir -p "${INSTALL_BASE}" "${BIN_DIR}" "${AUTOSTART_DIR}" "${CONFIG_ROOT}"

download_archive
extract_archive
resolve_bundle_dir
rm -f "${CURRENT_LINK}"
ln -sfn "${BUNDLE_DIR}" "${CURRENT_LINK}"

write_file "${CONFIG_ROOT}/aw-client/aw-client.toml" <<EOF
[server]
hostname = "${SERVER_HOST}"
port = "${SERVER_PORT}"
EOF

write_file "${CONFIG_ROOT}/aw-qt/aw-qt.toml" <<'EOF'
[aw-qt]
autostart_modules = ["aw-watcher-afk", "aw-watcher-window"]
EOF

write_file "${BIN_DIR}/activitywatch-remote-aw" <<EOF
#!/usr/bin/env sh
set -eu
AW_HOME="${CURRENT_LINK}"
export NO_PROXY="\${NO_PROXY:+\${NO_PROXY},}127.0.0.1,localhost,${SERVER_HOST}"
exec "\${AW_HOME}/aw-qt"
EOF
chmod 0755 "${BIN_DIR}/activitywatch-remote-aw"

write_file "${AUTOSTART_DIR}/activitywatch-remote-aw.desktop" <<EOF
[Desktop Entry]
Type=Application
Version=1.0
Name=ActivityWatch Remote
Comment=Start ActivityWatch watchers and report to ${SERVER_HOST}:${SERVER_PORT}
Exec=${BIN_DIR}/activitywatch-remote-aw
Terminal=false
X-GNOME-Autostart-enabled=true
EOF

"${CURRENT_LINK}/aw-qt" --help >/dev/null 2>&1 || true
"${CURRENT_LINK}/aw-watcher-afk/aw-watcher-afk" --help >/dev/null 2>&1 || true
"${CURRENT_LINK}/aw-watcher-window/aw-watcher-window" --help >/dev/null 2>&1 || true

echo "Installed ActivityWatch ${VERSION} into ${VERSION_DIR}"
echo "Remote server target: ${SERVER_HOST}:${SERVER_PORT}"
echo "Launcher: ${BIN_DIR}/activitywatch-remote-aw"
echo "Autostart: ${AUTOSTART_DIR}/activitywatch-remote-aw.desktop"
