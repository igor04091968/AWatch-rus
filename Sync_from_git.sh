#!/bin/sh
set -eu

REPO_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

cd "$REPO_DIR"

echo "==> Fetch origin"
git fetch origin

CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)

echo "==> Pull origin/$CURRENT_BRANCH"
git pull --ff-only origin "$CURRENT_BRANCH"

echo "==> Done"
git status --short --branch
