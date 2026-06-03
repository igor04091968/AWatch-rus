#!/usr/bin/env sh
set -eu

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
PWSH_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/powershell"
PROFILE="$PWSH_DIR/Microsoft.PowerShell_profile.ps1"
LOCAL_CFG="$PWSH_DIR/detmir-windows.psd1"
SNIPPET="$ROOT/scripts/powershell/detmir-powershell-profile.ps1"
EXAMPLE_CFG="$ROOT/scripts/powershell/detmir-windows.psd1.example"
INVENTORY="$ROOT/ansible/inventory.ini"
MARK_BEGIN="# >>> DetMir PowerShell MCP >>>"
MARK_END="# <<< DetMir PowerShell MCP <<<"

mkdir -p "$PWSH_DIR"
touch "$PROFILE"

TMP_PROFILE="$(mktemp)"
awk -v begin="$MARK_BEGIN" -v end="$MARK_END" '
    $0 == begin { skip = 1; next }
    $0 == end { skip = 0; next }
    skip != 1 { print }
' "$PROFILE" > "$TMP_PROFILE"

cat >> "$TMP_PROFILE" <<EOF

$MARK_BEGIN
\$detmirProjectProfile = '$SNIPPET'
if (Test-Path -LiteralPath \$detmirProjectProfile) {
    . \$detmirProjectProfile
}
$MARK_END
EOF

mv "$TMP_PROFILE" "$PROFILE"

if [ ! -f "$LOCAL_CFG" ]; then
    host=""
    user=""
    password=""

    if [ -f "$INVENTORY" ]; then
        line="$(awk '
            /^\[aw_windows\]/ { in_section = 1; next }
            /^\[/ { if (in_section) exit }
            in_section && $0 !~ /^[[:space:]]*#/ && NF { print; exit }
        ' "$INVENTORY")"

        if [ -n "$line" ]; then
            for token in $line; do
                case "$token" in
                    ansible_host=*) host="${token#ansible_host=}" ;;
                    ansible_user=*) user="${token#ansible_user=}" ;;
                    ansible_password=*) password="${token#ansible_password=}" ;;
                esac
            done
        fi
    fi

    if [ -n "$host" ] || [ -n "$user" ] || [ -n "$password" ]; then
        cat > "$LOCAL_CFG" <<EOF
@{
    Host = '${host:-198.51.100.18}'
    User = '${user:-Администратор}'
    Password = '${password:-CHANGE_ME}'
    Port = 22
    PowerShellPath = 'powershell.exe'
}
EOF
    else
        cp "$EXAMPLE_CFG" "$LOCAL_CFG"
    fi
fi

chmod 600 "$LOCAL_CFG"

printf 'Installed DetMir PowerShell MCP loader in %s\n' "$PROFILE"
printf 'DetMir local config: %s\n' "$LOCAL_CFG"
printf 'Reopen PowerShell or restart Codex session to pick up the profile.\n'
