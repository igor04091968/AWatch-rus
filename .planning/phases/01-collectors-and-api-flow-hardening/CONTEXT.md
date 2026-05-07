# Phase 1 Context

## Phase

Phase 1: Collectors and API flow hardening

## Focus

- Endpoint collectors must continuously send data without silent hangs.
- AW server must accept/query data for UI pages consistently.
- Failure points around transport/CORS/runtime must be explicitly checked.

## Initial Acceptance Targets

- Endpoint collector heartbeats arrive regularly.
- Browser domains and endpoint signals appear in corresponding buckets.
- Activity page for target host shows non-zero timeline/events for active period.
