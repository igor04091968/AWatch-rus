# Enterprise Acceptance Checklist

Чеклист используется для приемки enterprise/pilot deployment AWatch-rus.

## Установка

- [ ] Release commit/tag зафиксирован.
- [ ] Release artifacts получены из утвержденного источника.
- [ ] Checksums проверены.
- [ ] Secrets не хранятся в Git.
- [ ] Deployment profile согласован.
- [ ] Optional integrations перечислены отдельно.

## Запуск

- [ ] Backend/portal service запущен.
- [ ] Reverse proxy настроен, если используется.
- [ ] TLS настроен, если portal доступен по сети.
- [ ] Required storage доступен.
- [ ] systemd services/timers активны where applicable.

## Доступность API

- [ ] `/healthz` отвечает.
- [ ] `/readyz` отвечает.
- [ ] `/metrics` отвечает.
- [ ] `/api/reports?role=executive` отвечает.
- [ ] `/api/actions?role=executive` отвечает.
- [ ] Role gates не отдают лишние данные.

## Портал

- [ ] Portal открывается.
- [ ] Role `Руководитель` показывает главный вывод.
- [ ] Role `Безопасность` показывает ИБ-контур.
- [ ] Role `Расследования` показывает investigation/evidence flow.
- [ ] Role `Админ` показывает readiness/operations view.
- [ ] Ошибки API отображаются контролируемо.

## Smoke

- [ ] `scripts/deployment-readiness-smoke.mjs` проходит.
- [ ] Production hardening smoke проходит.
- [ ] Pilot demo smoke проходит, если выполняется demo.
- [ ] Smoke results сохранены в acceptance evidence.

## Документация

- [ ] Enterprise deployment guide доступен.
- [ ] Topologies documented.
- [ ] Sizing assumptions documented.
- [ ] Backup and recovery documented.
- [ ] Operations runbook documented.
- [ ] Security hardening documented.
- [ ] Registry readiness docs доступны.
- [ ] Demo pack доступен.

## Резервирование

- [ ] Config backup настроен.
- [ ] Reports/state backup настроен.
- [ ] Evidence metadata backup настроен, если используется.
- [ ] Restore test выполнен.
- [ ] Rollback process documented.

## Ограничения

- [ ] Не заявляется полноценная DLP/SIEM/EDR.
- [ ] Не заявляется ML/LLM scoring.
- [ ] Optional integrations не показаны как implemented без приемки.
- [ ] pfSense не заявлен как обязательный source.
- [ ] Demo data не смешаны с production data.
