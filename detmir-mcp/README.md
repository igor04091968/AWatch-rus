# DetMir MCP

Read-only MCP facade over the DetMir operational surfaces that usually regress under load:

- ActivityWatch core
- worktime API
- DLP policy and case services
- Grafana health
- 1C manager brief

This server exists to give `mcpdrill` a stable MCP target for reproducing timeout, saturation, stale-status, and mixed-read operator-path failures.

## Tools

- `aw_health_summary`
- `worktime_today`
- `worktime_management`
- `dlp_mode_get`
- `dlp_health_summary`
- `grafana_overview`
- `onec_manager_brief`

All tools are read-only.

## Run

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian/detmir-mcp
cp .env.example .env
sh run-local.sh
```

Default transport is `stdio`.

## Run for mcpdrill

`mcpdrill` wants HTTP, not stdio. Start the server like this:

```bash
cd /mnt/usb_hdd2/Projects/ActivityWatch-Russian/detmir-mcp
export DETMIR_MCP_TRANSPORT=streamable-http
export DETMIR_MCP_HOST=0.0.0.0
export DETMIR_MCP_PORT=8765
export DETMIR_MCP_STATELESS_HTTP=0
sh run-local.sh
```

Endpoint:

```text
http://<reachable-host-ip>:8765/mcp
```

Do not use `127.0.0.1` inside `mcpdrill` target configs; use a host/IP reachable from the Docker worker.

`run-local.sh` intentionally keeps the `uv` environment outside the repository tree. This machine stores the repo on a mounted filesystem where `.venv` creation inside the project is not reliable.

Example config:

- [examples/mcpdrill-detmir-readonly.json](/mnt/usb_hdd2/Projects/ActivityWatch-Russian/detmir-mcp/examples/mcpdrill-detmir-readonly.json)

## Quick checks

Tool list:

```bash
curl -sS http://127.0.0.1:8765/mcp \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
```

Example tool call:

```bash
curl -sS http://127.0.0.1:8765/mcp \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"dlp_mode_get","arguments":{}}}'
```

## What to stress first

- `worktime_today` + `worktime_management` to reproduce `5610` contention.
- `dlp_mode_get` + `dlp_health_summary` to expose stale policy/status views.
- `grafana_overview` + `onec_manager_brief` to check adjacent operator dependencies.
- Mixed runs across all tools to reproduce operator-path degradation.
