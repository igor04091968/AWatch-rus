# DLP Reliability Roadmap

## Scope
Roadmap for improving runtime reliability of:
- `windows/dlp-endpoint-signals-collector.ps1`
- `windows/file-operations-collector.ps1`

Date: 2026-05-04

---

## Stage 1 (1-2 days): Quick wins

### 1) Disk queue + sender loop + retry/backoff/jitter

**Goal:** no data loss on temporary network/server outages.

**Tasks**
- Add local append-only queue file per collector (`*.jsonl`) under ProgramData logs/artifacts root.
- Write events to queue first, then send asynchronously.
- Implement sender loop:
  - reads oldest unsent records,
  - sends in small batches,
  - marks sent records,
  - compacts queue periodically.
- Implement retry policy with exponential backoff + jitter.

**Acceptance criteria**
- When API is unavailable, queue grows and collector keeps running.
- When API recovers, queued events are flushed automatically.
- No collector crash during repeated network failures.

### 2) `eventId` + dedupe contract

**Goal:** at-least-once delivery without logical duplicates.

**Tasks**
- Add `eventId` (UUID), `eventCreatedAt`, `collectorType`, `hostname` to every payload.
- Define server dedupe contract:
  - dedupe key = `eventId`,
  - TTL for dedupe cache,
  - idempotent processing semantics.

**Acceptance criteria**
- Retried sends do not create duplicate incidents/events in downstream storage.
- Payload schema documentation updated.

### 3) Basic metrics/logging

**Goal:** visibility into health and data delivery.

**Tasks**
- Emit counters/gauges to log and heartbeat:
  - `queueDepth`,
  - `oldestUnsentAgeSec`,
  - `eventsEnqueued`,
  - `eventsSent`,
  - `sendFailures`,
  - `lastSendStatus`.

**Acceptance criteria**
- Operators can identify stuck queue and send failures from logs only.

---

## Stage 2: Hardening

### 1) Circuit breaker + health probes

**Tasks**
- Add transport circuit breaker (Closed/Open/HalfOpen).
- Open breaker after N consecutive failures.
- In Open state perform probe every M seconds.
- Close breaker on successful probe.

**Acceptance criteria**
- Reduced request storm during outage.
- Deterministic recovery behavior after outage.

### 2) Watcher auto-recreate

**Tasks**
- Handle `FileSystemWatcher` error/overflow events.
- Recreate watcher and subscriptions automatically.
- Keep watchdog timer to ensure watcher health.

**Acceptance criteria**
- Watcher resumes after overflow without manual restart.

### 3) Last-known-good policy

**Tasks**
- Validate new policy before apply.
- Cache last valid policy with checksum/version.
- Rollback to cached policy on parse/validation errors.

**Acceptance criteria**
- Broken policy cannot stop detection loop.

---

## Stage 3: Reliability operations

### 1) Chaos tests

Scenarios:
- network disconnect,
- API 5xx bursts,
- slow disk / queue write delay,
- headless UI context,
- forced collector restart.

**Acceptance criteria**
- For each scenario, documented expected behavior and observed result.
- No silent data loss in tested outage windows.

### 2) SLO + error budget process

**Initial SLO proposals**
- Event delivery latency P95 < 120s under normal conditions.
- Data loss = 0 for outages shorter than 30 minutes (with available disk).
- Collector liveness heartbeat every `pollSeconds * 3` max.

**Process**
- Define SLI dashboards.
- Define release gates tied to error budget burn.
- Freeze risky changes when budget exhausted.

---

## Suggested implementation order inside repository

1. `file-operations-collector.ps1`: queue + sender + metrics (simpler flow).
2. `dlp-endpoint-signals-collector.ps1`: queue + sender + metrics.
3. Shared helper module extraction (`windows/lib/aw-transport.psm1`) for queue, retry, breaker.
4. Policy cache and validation.
5. Chaos test scripts and runbook.

---

## Deliverables checklist

- [ ] Transport queue implementation in both collectors.
- [ ] Payload schema update with `eventId`.
- [ ] Dedupe contract documented for server side.
- [ ] Metrics fields added to heartbeat/logs.
- [ ] Circuit breaker implemented.
- [ ] Watcher auto-recreate implemented.
- [ ] Last-known-good policy implemented.
- [ ] Chaos test runbook and results.
- [ ] SLO/error budget document adopted.
