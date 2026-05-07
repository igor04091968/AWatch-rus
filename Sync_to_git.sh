#!/bin/sh
# shellcheck disable=SC1007
set -eu

REPO_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

cd "$REPO_DIR"

BRANCH=$(git rev-parse --abbrev-ref HEAD)

if [ "${1:-}" = "" ]; then
  echo "Usage: $0 \"commit message\"" >&2
  exit 1
fi

COMMIT_MESSAGE=$1

echo "==> Git status"
git status --short --branch

echo "==> Stage changes"
git add -A

if git diff --cached --quiet; then
  echo "No staged changes to commit."
  exit 0
fi

echo "==> Commit"
git commit -m "$COMMIT_MESSAGE"

echo "==> Push origin/$BRANCH"
git push origin "$BRANCH"

echo "==> Done"
