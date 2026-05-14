# Hayabusa AW-rus Integration 2026-05-14

This document defines the bounded integration between Hayabusa DFIR and the normal AW-rus operator path.

## Purpose

Hayabusa is used as DFIR enrichment after incidents, not as a new real-time detector.

## When to use Hayabusa follow-up

Recommended triggers:

- high-severity DLP incidents that justify host-side forensic review;
- repeated incidents on the same host or user;
- suspicious print, USB, email, or document-export activity that needs Windows event corroboration;
- operator-driven escalation where case review needs EVTX-based timeline evidence.

Not recommended:

- routine low-signal incidents;
- replacing normal AW-rus health/runtime checks;
- pushing raw Sigma detections into AW buckets.

## Operator path

1. Export EVTX package on Windows with `export-evtx-for-hayabusa.ps1`.
2. Transfer the resulting zip package to `10.10.10.13`.
3. Run one of:

```bash
aw-hayabusa accept --package /path/to/HOST-YYYYMMDD-HHMMSS.zip --host HOST
aw-hayabusa process-inbox --mode incident
```

or from Telegram bot:

```text
/aw_dfir /path/to/HOST-YYYYMMDD-HHMMSS.zip HOST [CASE_ID] [MODE]
```

Default mode is `incident`.

## What gets linked to a case

Case management stores only bounded metadata:

- `tool=hayabusa`
- `host`
- `mode`
- `status`
- `intake_id`
- `package_path`
- `sha256`
- `report_dir`
- `summary_html`
- `timeline_path`
- `manifest_path`
- `linked_at`
- `link_source`

The raw Sigma output, full timelines, and extracted payloads stay under `/opt/hayabusa`, not inside AW buckets or case comments.

## UI behavior

Case Management shows a short `DFIR` field:

- `Hayabusa ok · incident`
- `Hayabusa failed-* · incident`

This is intentionally short; report paths remain operator-facing metadata, not primary UI content.

## Boundaries

- No raw forensic output is copied into normal AW runtime buckets.
- No automatic case creation from Hayabusa findings.
- Hayabusa remains an enrichment layer around incidents and investigations.
