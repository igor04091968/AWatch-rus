#!/usr/bin/env bash
set -euo pipefail

violations=()

while IFS= read -r -d '' path; do
  rest="${path#private-config/}"
  case "$path" in
    private-config/README.md|private-config/.gitkeep)
      continue
      ;;
  esac
  if [[ "$rest" != */* && ( "$rest" == *.example || "$rest" == *.template ) ]]; then
    continue
  fi
  violations+=("$path")
done < <(git ls-files -z -- private-config)

if (( ${#violations[@]} > 0 )); then
  printf 'private-config guard failed: tracked private files are forbidden. Allowed files are README.md, .gitkeep, *.example, *.template.\\n' >&2
  printf '%s\\n' "${violations[@]}" >&2
  exit 1
fi

echo "private-config guard: OK"
