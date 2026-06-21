# Release evidence manifest

Статус: registry-readiness document. Документ описывает состав evidence,
который должен прикладываться к релизу или экспертному пакету. Он не
подтверждает юридическую достаточность evidence для реестра российского ПО.

## Infrastructure evidence

Для российского Git-контура и backup-контура фиксировать:

- Gitea URL: `https://git.iri1968.dpdns.org/awatch-rus/AWatch-rus`.
- Provider: REG.RU VPS / cloud server.
- Platform: self-hosted Gitea.
- HTTPS evidence для `https://git.iri1968.dpdns.org`.
- Nginx reverse proxy evidence.
- Firewall evidence: внешний `3000/tcp` не должен быть открыт после HTTPS
  validation.
- Service status evidence: `gitea`, `nginx`, `awatch-gitea-backup.timer`.
- Backup ZIP evidence из `/var/backups/gitea`.
- SHA256 checksum evidence для backup ZIP.
- Restore-runbook reference:
  `docs/registry/GITEA_BACKUP_AND_RESTORE_RUNBOOK_RU.md`.
- Timestamped evidence file, если он будет предоставлен владельцем.

## Source evidence

Фиксировать:

- primary source repository:
  `https://git.iri1968.dpdns.org/awatch-rus/AWatch-rus`;
- GitHub role: public mirror only;
- commit hash;
- tag, если релиз выпускается по tag;
- `git remote -v`;
- список измененных файлов для релиза;
- manifest JSON:
  `docs/registry/registry-evidence-manifest.json`.

## Build/release evidence

Для финального registry package дополнительно требуется подтвердить российский
build-runner и storage release artifacts в РФ или документировать принятую
владельцем схему. До этого нельзя утверждать, что source/build/release contour
полностью закрыт для подачи.
