#!/usr/bin/env bash
set -euo pipefail

# WinRM uses requests/pywinrm which may pick up local proxy settings (systemd env, shells, etc).
# If that happens, WinRM traffic can be sent to 127.0.0.1:<proxy> and time out.
# This wrapper hard-disables proxy env vars to make deploy deterministic.
#
# Also, NTLM auth requires MD4. On OpenSSL 3 builds where MD4 is disabled by default,
# pywinrm fails with "unsupported hash type md4". In that case we enable OpenSSL legacy
# provider only for this process.

if [[ -z "${AW_WINRM_PASSWORD:-}" ]]; then
  echo "ERROR: AW_WINRM_PASSWORD is not set" >&2
  echo "Usage: AW_WINRM_PASSWORD='...' ./run_deploy_aw_windows.sh [ansible-playbook args...]" >&2
  exit 2
fi

unset http_proxy https_proxy HTTP_PROXY HTTPS_PROXY all_proxy ALL_PROXY

if ! python3 - <<'PY' >/dev/null 2>&1
import hashlib
raise SystemExit(0 if 'md4' in hashlib.algorithms_available else 1)
PY
then
  cat >/tmp/openssl-legacy.cnf <<'EOF'
openssl_conf = openssl_init
[openssl_init]
providers = provider_sect
[provider_sect]
default = default_sect
legacy = legacy_sect
[default_sect]
activate = 1
[legacy_sect]
activate = 1
EOF
  export OPENSSL_CONF=/tmp/openssl-legacy.cnf
fi

retries="${AW_DEPLOY_RETRIES:-5}"
delay="${AW_DEPLOY_RETRY_DELAY_SEC:-30}"

attempt=1
while [[ "$attempt" -le "$retries" ]]; do
  echo "Deploy attempt $attempt/$retries"
  if ansible-playbook -i inventory.ini deploy_aw_windows.yml "$@"; then
    exit 0
  fi
  if [[ "$attempt" -lt "$retries" ]]; then
    echo "Deploy attempt $attempt failed; sleeping ${delay}s before retry..." >&2
    sleep "$delay"
  fi
  attempt=$((attempt + 1))
done

echo "Deploy failed after ${retries} attempts." >&2
exit 1
