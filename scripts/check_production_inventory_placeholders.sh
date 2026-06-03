#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/check_production_inventory_placeholders.sh [--allow-missing] FILE...
  DETMIR_PRODUCTION_CONFIG_PATHS="file1:file2" scripts/check_production_inventory_placeholders.sh
  scripts/check_production_inventory_placeholders.sh --self-test

Fails when production inventory/env files contain public placeholder values:
TEST-NET addresses, HOST-EXAMPLE, WINDOWS_USER_EXAMPLE, CHANGE_ME, YOUR_*,
replace-me, or angle-bracket placeholders.
EOF
}

pattern='(192\.0\.2\.|198\.51\.100\.|203\.0\.113\.|HOST-EXAMPLE|WINDOWS_USER_EXAMPLE|CHANGE_ME|CHANGEME|REPLACE_ME|replace-me|YOUR_[A-Z0-9_]*|<[A-Z0-9_ -]+>)'
allow_missing=0
self_test=0
paths=()

while (($#)); do
  case "$1" in
    --allow-missing)
      allow_missing=1
      ;;
    --self-test)
      self_test=1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      paths+=("$1")
      ;;
  esac
  shift
done

run_scan() {
  local file
  local found=0
  for file in "$@"; do
    if [[ ! -e "$file" ]]; then
      if (( allow_missing == 0 )); then
        printf 'missing production config path: %s\n' "$file" >&2
        found=1
      fi
      continue
    fi
    if [[ -d "$file" ]]; then
      while IFS= read -r -d '' child; do
        if grep -nE "$pattern" "$child" >/dev/null; then
          printf 'placeholder found in %s\n' "$child" >&2
          grep -nE "$pattern" "$child" | sed -E 's/^([0-9]+):.*/  line \1: placeholder marker/' >&2
          found=1
        fi
      done < <(find "$file" -type f \( -name '*.env' -o -name '*.ini' -o -name '*.yml' -o -name '*.yaml' \) -print0)
    elif grep -nE "$pattern" "$file" >/dev/null; then
      printf 'placeholder found in %s\n' "$file" >&2
      grep -nE "$pattern" "$file" | sed -E 's/^([0-9]+):.*/  line \1: placeholder marker/' >&2
      found=1
    fi
  done
  return "$found"
}

if (( self_test == 1 )); then
  tmp_dir="$(mktemp -d)"
  trap 'rm -rf "$tmp_dir"' EXIT
  good="$tmp_dir/good.env"
  bad="$tmp_dir/bad.env"
  cat >"$good" <<'EOF'
AW_WORKTIME_INFLUX_URL=http://influxdb.internal:8086
AW_WORKTIME_INFLUX_HOSTS=WINDOWS-HOST
AW_WORKTIME_INFLUX_TOKEN=prod-write-token-value
EOF
  cat >"$bad" <<'EOF'
AW_WORKTIME_INFLUX_URL=http://192.0.2.10:8086
AW_WORKTIME_INFLUX_HOSTS=HOST-EXAMPLE
AW_WORKTIME_INFLUX_TOKEN=CHANGE_ME
EOF
  run_scan "$good"
  if run_scan "$bad" >/dev/null 2>&1; then
    echo "self-test failed: bad fixture was accepted" >&2
    exit 1
  fi
  echo "production inventory placeholder guard self-test: OK"
  exit 0
fi

if ((${#paths[@]} == 0)) && [[ -n "${DETMIR_PRODUCTION_CONFIG_PATHS:-}" ]]; then
  IFS=':' read -r -a paths <<<"$DETMIR_PRODUCTION_CONFIG_PATHS"
fi

if ((${#paths[@]} == 0)); then
  if (( allow_missing == 1 )); then
    echo "production inventory placeholder guard: skipped (no production paths)"
    exit 0
  fi
  usage >&2
  exit 2
fi

run_scan "${paths[@]}"
echo "production inventory placeholder guard: OK"
