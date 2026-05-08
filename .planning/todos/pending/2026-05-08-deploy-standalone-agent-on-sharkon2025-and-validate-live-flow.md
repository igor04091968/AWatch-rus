---
created: 2026-05-07T21:46:00Z
title: Deploy standalone agent on SHARKON2025 and validate live flow
area: tooling
files:
  - windows/installkit/innosetup/AWatch-rus-InnoSetup.iss
  - windows/install-standalone-service.ps1
  - windows/aw-standalone-service.ps1
  - windows/installkit/innosetup/BUILD.md
  - docs/windows/deployment.md
  - docs/windows/troubleshooting.md
---

## Problem

Standalone InnoSetup mode and service wrapper are implemented and pushed, and AW API write path was verified by manual heartbeat posts. But production value still depends on real endpoint rollout: installer must be deployed on SHARKON2025 (192.168.100.21), service must be running persistently, and UI/API must show continuously fresh events without manual seeding.

## Solution

Deploy the newly built installer `AWatch-rus-InstallKit.exe` to SHARKON2025, run installation with target `10.10.10.13:5600`, verify `AWatchRusStandaloneAgent` state, inspect `standalone-agent-service.log`, and confirm fresh `metadata.end` progression for `aw-dlp-endpoint-signals_SHARKON2025`, `aw-file-operations_SHARKON2025`, and `aw-worktime-sessions_SHARKON2025` over time.
