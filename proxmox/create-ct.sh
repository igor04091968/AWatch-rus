#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
DEFAULT_ENV_FILE="$SCRIPT_DIR/../secrets/deploy.secrets.env"

if [ "$#" -gt 1 ]; then
  echo "usage: $0 [ /path/to/deploy.secrets.env ]" >&2
  exit 1
fi

ENV_FILE="${1:-$DEFAULT_ENV_FILE}"
if [ ! -f "$ENV_FILE" ]; then
  echo "env file not found: $ENV_FILE" >&2
  exit 1
fi

# shellcheck disable=SC1090
. "$ENV_FILE"

required_vars="CT_ID CT_HOSTNAME CT_STORAGE CT_TEMPLATE CT_ROOTFS_SIZE CT_CORES CT_MEMORY CT_SWAP CT_BRIDGE CT_IP CT_GW CT_PASSWORD CT_UNPRIVILEGED CT_ONBOOT CT_FEATURES"
for var_name in $required_vars; do
  eval "var_value=\${$var_name:-}"
  if [ -z "$var_value" ]; then
    echo "missing required variable: $var_name" >&2
    exit 1
  fi
done

if pct status "$CT_ID" >/dev/null 2>&1; then
  echo "CT $CT_ID already exists" >&2
  exit 1
fi

NET0="name=eth0,bridge=$CT_BRIDGE,ip=$CT_IP,gw=$CT_GW"
if [ -n "${CT_VLAN:-}" ]; then
  NET0="$NET0,tag=$CT_VLAN"
fi

pct create "$CT_ID" "$CT_TEMPLATE" \
  --hostname "$CT_HOSTNAME" \
  --cores "$CT_CORES" \
  --memory "$CT_MEMORY" \
  --swap "$CT_SWAP" \
  --rootfs "$CT_STORAGE:$CT_ROOTFS_SIZE" \
  --password "$CT_PASSWORD" \
  --unprivileged "$CT_UNPRIVILEGED" \
  --onboot "$CT_ONBOOT" \
  --features "$CT_FEATURES" \
  --net0 "$NET0" \
  --nameserver "${CT_NAMESERVER:-}" \
  --searchdomain "${CT_SEARCHDOMAIN:-}" \
  --ostype debian

pct start "$CT_ID"
sleep 5

pct exec "$CT_ID" -- bash -lc '
  set -eu
  export DEBIAN_FRONTEND=noninteractive
  apt-get update
  apt-get install -y curl ca-certificates bash unzip xz-utils jq
  mkdir -p /root/bootstrap /etc/activitywatch
'

echo "CT $CT_ID created and bootstrapped"
