# Release-readiness v0.1 architecture

```mermaid
flowchart LR
  subgraph Runtime["DetMir runtime"]
    AW["ActivityWatch / Worktime / DLP telemetry"]
    Portal["detmir-portal"]
    Readiness["detmir-readiness"]
    Metrics["Prometheus metrics"]
    Grafana["Grafana alerts"]
  end

  subgraph Bundle["Readiness bundle"]
    JSON["detmir-readiness-latest.json"]
    Act["act.md / act.html"]
    Sums["sha256sums.txt"]
    Sig["sha256sums.txt.sig"]
    Pub["public-key.pem"]
    Prom["detmir-readiness.prom"]
  end

  subgraph Release["Release readiness package"]
    Change["CHANGELOG_RU.md"]
    SBOM["docs/SBOM_V0.1_RU.md"]
    Install["docs/INSTALL_FOR_EXPERT_RU.md"]
    Runbook["adk-rust/RUNBOOK.md"]
    Arch["docs/ARCHITECTURE_RU.md"]
    Shots["docs/screenshots/release-v0.1"]
  end

  AW --> Readiness
  Readiness --> JSON
  Readiness --> Act
  Readiness --> Sums
  Sums --> Sig
  Pub --> Sig
  Readiness --> Prom
  Prom --> Metrics
  Metrics --> Grafana
  JSON --> Portal
  Sums --> Portal
  Sig --> Portal
  Pub --> Portal
  Portal --> Shots
  Bundle --> Release
  Change --> Release
  SBOM --> Release
  Install --> Release
  Runbook --> Release
  Arch --> Release
```

## Контрольные точки

1. `detmir-readiness` формирует bundle и Prometheus metrics.
2. Bundle имеет SHA-256 checksums и detached signature.
3. Portal показывает статус readiness и ручную проверку bundle.
4. Prometheus/Grafana поднимают alert при `detmir_readiness_ok == 0` или
   `detmir_readiness_signature_verified == 0`.
5. Release package содержит changelog, SBOM profile, install/runbook,
   architecture и обезличенные screenshots.
