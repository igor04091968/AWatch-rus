# Контроль готовности промышленного внедрения

`detmir-readiness` - единая команда preflight-контроля перед внедрением,
релизом или изменением production runtime.

Команда проверяет:

- runtime env без public placeholders;
- активность обязательных systemd units;
- реальную запись в InfluxDB;
- health Grafana datasource.

## Базовый запуск

На AW server:

```bash
detmir-readiness --json
```

Ожидаемый результат:

```json
{
  "ok": true,
  "status": "OK"
}
```

Коды возврата:

- `0` - готово к промышленной эксплуатации;
- `2` - readiness check нашел `WARN`;
- `3` - readiness check нашел `FAIL`;
- `1` - сама команда не смогла выполниться.

## Private production inventory

Перед rollout private override-файлы проверяются отдельно:

```bash
scripts/check_production_inventory_placeholders.sh --strict \
  private-config/runtime.env \
  private-config/ansible-vars.yml
```

`--strict` предназначен только для private production-файлов. Публичные
tracked defaults и `.example` файлы могут содержать `HOST-EXAMPLE` и TEST-NET
адреса, потому что они не являются production source of truth.

## Что считается отказом

`detmir-readiness` возвращает `FAIL`, если:

- включенный Influx exporter получил пустой или example URL/org/bucket/token/host;
- systemd unit из обязательного списка не active;
- Influx write-probe не смог записать heartbeat;
- Grafana datasource health не `OK`.

## Акт готовности стенда

`detmir-readiness` может сохранить акт готовности в JSON, Markdown, HTML и PDF:

```bash
detmir-readiness --json \
  --output-json /var/lib/activitywatch/health/detmir-readiness-latest.json \
  --output-markdown /var/lib/activitywatch/health/detmir-readiness-act.md \
  --output-pdf /var/lib/activitywatch/health/detmir-readiness-act.pdf
```

PDF-вывод требует один из render tools на хосте: `weasyprint`, `chromium`,
`chromium-browser` или `google-chrome`. Если PDF renderer не установлен,
используйте `--output-markdown` и `--output-html` как обязательный минимальный
артефакт внедрения.

## Readiness bundle

Для промышленного контура предпочтителен единый bundle:

```bash
detmir-readiness --output-dir /var/lib/activitywatch/health/readiness-bundle
```

Команда создает:

- `detmir-readiness-latest.json` - машинный отчет;
- `detmir-readiness-act.md` - акт готовности для оператора;
- `detmir-readiness-act.html` - HTML-версия акта;
- `sha256sums.txt` - контрольные суммы bundle-файлов;
- `sha256sums.txt.sig` - detached signature для `sha256sums.txt`;
- `public-key.pem` - публичный ключ проверки подписи;
- `detmir-readiness-status.json` - короткий machine-readable статус bundle;
- `detmir-readiness.prom` - Prometheus textfile metric.

Архив хранится по датам:

```text
/var/lib/activitywatch/health/readiness-bundle/
  2026-06-03/
    062000Z/
      detmir-readiness-latest.json
      detmir-readiness-act.md
      detmir-readiness-act.html
      sha256sums.txt
      sha256sums.txt.sig
      public-key.pem
```

Файлы в корне `readiness-bundle/` являются latest-копией последнего архива.

Проверка целостности:

```bash
cd /var/lib/activitywatch/health/readiness-bundle
sha256sum -c sha256sums.txt
openssl dgst -sha256 -verify public-key.pem \
  -signature sha256sums.txt.sig sha256sums.txt
```

В JSON и акт добавляются технические поля `generated_by`, `host`, `version`,
`git_commit`, а также раздел `Ограничения проверки`. Секреты, токены и пароли
в артефакты не включаются.

Private signing key хранится только на сервере:

```text
/etc/detmir-readiness/signing-key.pem
```

Публичный ключ попадает в bundle как `public-key.pem`. Retention архивов
управляется переменной `DETMIR_READINESS_RETENTION_DAYS`.

Fingerprint публичного ключа считается как SHA-256 файла `public-key.pem` и
дублируется в `detmir-readiness-status.json`:

```bash
sha256sum /var/lib/activitywatch/health/readiness-bundle/public-key.pem
jq -r '.signature.public_key_fingerprint_sha256' \
  /var/lib/activitywatch/health/readiness-bundle/detmir-readiness-status.json
```

Для публичной поставки вместо live fingerprint используется placeholder
`<READINESS_PUBLIC_KEY_SHA256_FINGERPRINT>`; конкретный customer contour
фиксирует свой fingerprint в private acceptance package.

## Ежедневное формирование

При штатном развертывании Ansible устанавливает:

- `detmir-readiness.service`;
- `detmir-readiness.timer`.

Таймер ежедневно формирует readiness bundle в
`/var/lib/activitywatch/health/readiness-bundle`.

Операторская проверка:

```bash
systemctl list-timers detmir-readiness.timer
systemctl start detmir-readiness.service
systemctl status detmir-readiness.service --no-pager
```

Если Grafana находится на отдельном узле, параметры доступа передаются через
серверный private env-файл `/etc/detmir-grafana-check.env` или
`/etc/detmir-readiness.env`. Эти файлы не входят в публичный репозиторий.

Поддерживаемые private env-переключатели:

- `DETMIR_READINESS_SIGNING_KEY=/etc/detmir-readiness/signing-key.pem`;
- `DETMIR_READINESS_REQUIRE_SIGNATURE=true`;
- `DETMIR_READINESS_RETENTION_DAYS=30`;
- `DETMIR_READINESS_SKIP_SYSTEMD=true`;
- `DETMIR_READINESS_SKIP_INFLUX_WRITE=true`;
- `DETMIR_READINESS_ALLOW_DISABLED_INFLUX=true`;
- `DETMIR_READINESS_SKIP_GRAFANA=true`;
- `DETMIR_GRAFANA_DATASOURCE_UID=<uid>`;
- `DETMIR_GIT_COMMIT=<commit>`.

## Portal endpoints

`detmir-portal` публикует read-only endpoints:

- `/api/readiness/latest` - последний readiness JSON;
- `/api/readiness/bundle` - индекс latest bundle и список артефактов;
- `/api/readiness/verify` - проверка `sha256sum -c` и detached signature.

В UI портала карточка `Готовность системы` показывает статус `OK/WARN/FAIL`,
дату формирования, состояние checksum/signature и fingerprint публичного ключа.
Кнопка `Проверить bundle` запускает lightweight verify endpoint без повторного
запуска runtime checks.

## Prometheus alerts

Поставочный файл правил:

```text
aw-server/detmir-readiness-alerts.yml
```

Критичные условия:

```promql
detmir_readiness_ok == 0
detmir_readiness_signature_verified == 0
```

## Полезные параметры

```bash
detmir-readiness --json \
  --aw-env-file /etc/activitywatch/aw-server.env \
  --grafana-env-file /etc/detmir-grafana-check.env \
  --grafana-datasource-uid influxdb_aw
```

Для диагностики без write-probe:

```bash
detmir-readiness --json --skip-influx-write
```

Для контура, где Influx временно не входит в профиль внедрения:

```bash
detmir-readiness --json --allow-disabled-influx
```

Такой запуск допустим только как временный исключительный режим; для полного
commercial AWatch-rus contour Influx/Grafana должны быть зелеными.
