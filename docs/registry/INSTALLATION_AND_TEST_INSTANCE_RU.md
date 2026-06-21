# Installation and test instance

Статус: registry-readiness document. Документ фиксирует минимальные требования
к тестовому стенду для экспертной проверки. Он не меняет runtime, API, UI или
deployment behavior продукта.

## Назначение тестового стенда

Тестовый стенд должен позволить проверить:

- получение исходного кода из российского Git-контура;
- воспроизводимость сборки по документированной процедуре;
- базовую установку AWatch-rus;
- отсутствие необходимости в cloud dependency для работы продукта;
- сбор release evidence и infrastructure evidence.

## Git source

Целевой источник для registry-readiness:

```text
https://git.iri1968.dpdns.org/awatch-rus/AWatch-rus
```

GitHub допускается как public mirror only и не должен описываться как primary
registry source/build/release contour.

## Проверки до признания стенда готовым

- Подтвержден доступ к self-hosted Gitea по HTTPS.
- Подтвержден commit hash и tag, если используется tagged release.
- Подтверждена процедура сборки из исходного кода.
- Подтверждено, что секреты и персональные данные не входят в test package.
- Подтверждено, что backup/restore runbook для Gitea доступен в
  `docs/registry/GITEA_BACKUP_AND_RESTORE_RUNBOOK_RU.md`.

## Оставшиеся пробелы

Перед финальной подачей требуется отдельно зафиксировать российский
build-runner, storage release artifacts в РФ, access control policy и
результат тестового восстановления Gitea backup на отдельном сервере.
