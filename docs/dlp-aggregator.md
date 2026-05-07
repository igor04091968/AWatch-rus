# Central DLP aggregator prototype

`scripts/aggregate_dlp_events.py` collects Phase 2 DLP telemetry from ActivityWatch buckets and stores normalized rows in one database for Grafana/SIEM-style reporting.

## Streams

The prototype reads:

- `aw-file-operations_*` (`aw.file.operation`) — file create/delete/rename telemetry, including `archiveHint`.
- `aw-dlp-incidents_*` (`aw.dlp.incident`) — browser/endpoint DLP incidents and screenshot metadata when available.
- `aw-dlp-endpoint-signals_*` (`aw.dlp.endpoint.signal`) — endpoint signal heartbeats/events.
- `aw-email-monitor_*` (`aw.email.signal`) — outbound email signal stream.

## SQLite smoke test

SQLite is the default so the collector can be tested without deploying PostgreSQL:

```bash
python3 scripts/aggregate_dlp_events.py \
  --aw-url http://10.10.10.13:5600/api/0 \
  --sqlite-path data/dlp-events.sqlite3 \
  --lookback-hours 24
```

Useful checks:

```bash
sqlite3 data/dlp-events.sqlite3 \
  "select stream_type, hostname, count(*) from dlp_events group by 1,2 order by 3 desc;"

sqlite3 data/dlp-events.sqlite3 \
  "select event_ts, hostname, username, file_path from dlp_file_operations where archive_hint = 1 order by event_ts desc limit 20;"
```

## PostgreSQL mode

For centralized reporting, pass a DSN through an environment variable instead of committing secrets:

```bash
export DLP_AGGREGATOR_POSTGRES_DSN='postgresql://aw_dlp:${PASSWORD}@postgres.internal:5432/aw_dlp'
python3 -m pip install 'psycopg[binary]'
python3 scripts/aggregate_dlp_events.py \
  --aw-url http://10.10.10.13:5600/api/0
```

Minimum database bootstrap:

```sql
create database aw_dlp;
create user aw_dlp_ingest with password '<strong generated password>';
grant connect on database aw_dlp to aw_dlp_ingest;
grant usage, create on schema public to aw_dlp_ingest;
```

The script creates:

- table `dlp_events`
- view `dlp_file_operations`
- view `dlp_incidents`

## Incremental state

By default, the aggregator stores the last successful end timestamp in:

```text
data/dlp-aggregator-state.json
```

Future runs resume from that timestamp with a small overlap window to avoid missing late events. Duplicate inserts are ignored by `(bucket_id, event_id)`.

## Scheduling example

Cron every minute:

```cron
* * * * * cd /opt/AWatch-rus && /usr/bin/python3 scripts/aggregate_dlp_events.py --aw-url http://10.10.10.13:5600/api/0 >> /var/log/aw-dlp-aggregator.log 2>&1
```

## Example Grafana queries

Archive creation by user:

```sql
select
  date_trunc('minute', event_ts) as time,
  hostname,
  username,
  count(*) as archives
from dlp_file_operations
where archive_hint = true
group by 1, 2, 3
order by 1 desc;
```

DLP incidents by severity:

```sql
select
  date_trunc('hour', event_ts) as time,
  severity,
  count(*) as incidents
from dlp_incidents
group by 1, 2
order by 1 desc;
```
