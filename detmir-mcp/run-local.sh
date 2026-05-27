#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
export UV_LINK_MODE="${UV_LINK_MODE:-copy}"
export UV_PROJECT_ENVIRONMENT="${UV_PROJECT_ENVIRONMENT:-$HOME/.local/share/uv-envs/detmir-mcp}"

mkdir -p "$UV_PROJECT_ENVIRONMENT"
cd "$SCRIPT_DIR"
exec /home/igor/.local/bin/uv run main.py
