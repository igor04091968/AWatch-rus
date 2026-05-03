# Стратегический анализ: AWatch-rus vs InfoWatch Traffic Monitor

## Текущие возможности AWatch-rus (что уже есть)

| Область | Реализовано |
|---------|------------|
| Activity tracking | `aw-watcher-afk`, `aw-watcher-window` — время активности, окна |
| Browser monitoring | `browser-domains-native-collector.ps1` — URL/домены через UIAutomation |
| DLP Phase 1 | Rule-based политики, incident bucket, cooldown/dedup |
| DLP Phase 2 | USB/print/clipboard мониторинг (`dlp-endpoint-signals-collector.ps1`), incident screenshot |
| File operations | Прототип `file-operations-collector.ps1` (create/delete/rename/archive) |
| Worktime | Session-level presence (`worktime-session-collector.ps1`) |
| Network | pfSense poller (firewall events → AW bucket) |
| Aggregation | `aggregate_dlp_events.py` — SQLite/PostgreSQL centralized store |
| UI | Русификация WebUI + DLP incident review panel в `aw-ru-patch.js` |
| Deployment | Ansible + PowerShell ensemble + InnoSetup + Proxmox LXC |
| Linux | Remote worker, console/SSH logger, web category logger |

## Ключевые разрывы до InfoWatch TM уровня

### 🔴 Критические (без них это не DLP, а мониторинг)

#### 1. Блокировка в реальном времени (Enforcement)
**InfoWatch**: Перехватывает и блокирует отправку до завершения — email не уйдёт, файл на USB не запишется, печать не пройдёт.
**AWatch-rus**: Только detect + log. Нет inline-перехвата трафика, нет file system filter driver.

**Что делать:**
- **USB write-block** — реализуемо через Group Policy + PowerShell enforcement (`Set-StoragePolicy`, device lockdown via WMI). Не требует kernel driver.
- **Print block** — перехват через Print Spooler event (307) + `Cancel-PrintJob`. Уже есть event monitoring — нужен только enforcement шаг.
- **Clipboard block** — `SetClipboardData` hook через .NET или `Clear-Clipboard` при policy violation. Рискованно по UX, но возможно.
- **Email/web block** — требует MITM proxy (аналог InfoWatch ICAP gateway). Реалистичнее начать с email-выгрузки через Exchange Journal/Transport Rule → анализ → карантин, чем строить inline proxy.
- **Приоритет**: USB write-block + print cancel — быстрый win, 2-3 недели.

#### 2. Email перехват
**InfoWatch**: SMTP/MAPI/IMAP — полный контроль корпоративной почты, включая вложения, тело, получатели.
**AWatch-rus**: Отсутствует.

**Что делать:**
- **Exchange/M365**: Transport Rule с Journal → Python парсер EML → событие в AW bucket `aw-email-monitor_<host>`.
- **On-prem SMTP**: Milter на Postfix/Sendmail (Python, 200-300 строк).
- **MVP**: Только metadata (from/to/subject/attachment names/sizes), без content inspection. Потом добавить content analysis.
- **Приоритет**: Высокий — email это #1 канал утечки в enterprise.

#### 3. Контент-анализ (Content Inspection)
**InfoWatch**: Лингвистический анализ, ML-классификация, OCR, document fingerprinting, digital watermarks, EDM (Exact Data Matching).
**AWatch-rus**: Только regex-matching в DLP policy rules.

**Что делать поэтапно:**
- **Phase A**: Словарные пакеты (ПДн по 152-ФЗ: ИНН, СНИЛС, паспорт, банковские реквизиты) — regex + Luhn/checksum validation. 1-2 недели.
- **Phase B**: Keyword/weighted scoring — JSON pack с весами, threshold для incident. 1 неделя.
- **Phase C**: OCR pipeline — Tesseract + screenshot analysis. Скриншоты уже снимаются! Нужен только OCR + content check step. 2-3 недели.
- **Phase D**: Document fingerprinting — SimHash/MinHash для корпоративных шаблонов. Server-side Python service. 3-4 недели.
- **Phase E**: ML-классификация — fine-tuned модель на корпоративных документах. Долгосрочно.

### 🟡 Важные (отличают серьёзный продукт от прототипа)

#### 4. Мессенджеры
**InfoWatch**: Skype, Telegram, WhatsApp, VK, Jabber, MS Teams.
**AWatch-rus**: Отсутствует.

**Что делать:**
- Перехват через `aw-watcher-window` + title parsing уже частично даёт metadata (видно, что пользователь в Telegram).
- Глубокий перехват текста мессенджеров требует accessibility API или memory scraping — сложно и ломко.
- Реалистичнее: интеграция с корпоративными мессенджерами через API (MS Teams webhook, Mattermost API).
- **MVP**: title-based detection "пользователь X общался в Telegram 2 часа" — уже почти есть.

#### 5. Облачные хранилища
**InfoWatch**: Контроль загрузки в DropBox, Google Drive, OneDrive, Яндекс.Диск.
**AWatch-rus**: Отсутствует.

**Что делать:**
- Browser-based detection: URL matching `drive.google.com/upload`, `disk.yandex.ru` — расширить `browser-domains-native-collector.ps1` правилами.
- Sync client detection: мониторинг процессов OneDrive/GoogleDrive + file watcher в sync-каталогах.
- **MVP**: Alert "файл был загружен в облако" через browser URL + file operation correlation. 1-2 недели.

#### 6. SIEM/SOAR интеграция
**InfoWatch**: CEF/syslog/API export, 75+ интеграций.
**AWatch-rus**: SQLite/PostgreSQL aggregator — foundation есть, но нет стандартных connectors.

**Что делать:**
- **CEF/Syslog exporter** — из `aggregate_dlp_events.py` в CEF format → rsyslog/Graylog/ELK. Python, 1-2 дня.
- **Webhook/HTTP callback** — при incident severity=high → POST в Teams/Telegram/PagerDuty. 1 день.
- **Grafana dashboards** — PostgreSQL уже поддерживается. Нужны готовые dashboard JSON. 2-3 дня.
- **Приоритет**: CEF exporter + Grafana — быстрые wins для демонстрации "enterprise-ready".

#### 7. RBAC и multi-tenant admin
**InfoWatch**: Ролевая модель, SoD, multi-tenant console, делегирование по отделам.
**AWatch-rus**: Single admin, нет ролей.

**Что делать:**
- ActivityWatch REST API не имеет auth из коробки. Нужен reverse proxy (nginx) с auth layer.
- **MVP**: Basic auth + nginx + IP whitelist. Достаточно для PoC.
- **Target**: Keycloak/LDAP auth proxy → role-based API access. Серьёзная работа, 4-6 недель.

### 🟢 Стратегические преимущества AWatch-rus (где вы уже лучше)

1. **Open-source база** — нет vendor lock-in, полный контроль над кодом.
2. **Лёгкий agent** — PowerShell collectors vs тяжёлый C++ agent InfoWatch (~200MB RAM).
3. **Гибкая DLP policy** — JSON rules, программируемые на лету, без перекомпиляции.
4. **Linux поддержка** — remote workers, SSH/console logging, web category — InfoWatch слабее в Linux.
5. **pfSense интеграция** — network visibility через firewall API, уникальная фича.
6. **Worktime tracking** — session-level presence, отработка с 1С (Grafana dashboards).

## Рекомендуемый стратегический roadmap

### Квартал 1 (ближайшие 3 месяца) — "Enforce & Detect"
| # | Задача | Усилие | Влияние |
|---|--------|--------|---------|
| 1 | USB write-block (GPO + enforcement step) | 2 нед | Критическое |
| 2 | Print job cancel при policy violation | 1 нед | Критическое |
| 3 | ПДн словарный пакет (ИНН/СНИЛС/паспорт regex) | 2 нед | Высокое |
| 4 | Email metadata collector (Exchange Journal) | 3 нед | Критическое |
| 5 | CEF/Syslog exporter для SIEM | 3 дня | Высокое |
| 6 | Grafana incident dashboards | 3 дня | Среднее |

### Квартал 2 — "Content & Cloud"
| # | Задача | Усилие | Влияние |
|---|--------|--------|---------|
| 7 | Cloud storage upload detection | 2 нед | Высокое |
| 8 | OCR pipeline (Tesseract + screenshot analysis) | 3 нед | Высокое |
| 9 | Weighted keyword scoring engine | 1 нед | Среднее |
| 10 | Webhook/callback при critical incidents | 2 дня | Среднее |
| 11 | Clipboard enforcement (clear on violation) | 1 нед | Среднее |

### Квартал 3 — "Intelligence & Scale"
| # | Задача | Усилие | Влияние |
|---|--------|--------|---------|
| 12 | Policy engine service (server-side, versioned) | 4 нед | Критическое |
| 13 | Document fingerprinting (SimHash) | 3 нед | Высокое |
| 14 | UEBA / risk scoring (anomaly detection) | 4 нед | Высокое |
| 15 | RBAC auth proxy (Keycloak/LDAP) | 4 нед | Среднее |
| 16 | Correlation engine (user+channel+object+time) | 3 нед | Высокое |

### Квартал 4 — "Enterprise"
| # | Задача | Усилие | Влияние |
|---|--------|--------|---------|
| 17 | Case management (investigations) | 4 нед | Среднее |
| 18 | Evidence chain / immutable audit log | 2 нед | Среднее |
| 19 | Compliance report generator (152-ФЗ) | 3 нед | Высокое |
| 20 | ML document classifier | 6+ нед | Высокое |

## Архитектурная рекомендация

```
┌─────────────────────────────────────────────────┐
│              AWatch-rus Server (Rust)            │
│  ┌──────────┐  ┌───────────┐  ┌──────────────┐  │
│  │ AW API   │  │ Policy    │  │ Content      │  │
│  │ (buckets │  │ Engine    │  │ Analysis     │  │
│  │  events) │  │ (Python   │  │ Service      │  │
│  │          │  │  sidecar) │  │ (OCR/ML/     │  │
│  │          │  │           │  │  fingerprint)│  │
│  └────┬─────┘  └─────┬─────┘  └──────┬───────┘  │
│       │              │               │           │
│  ┌────┴──────────────┴───────────────┴────────┐  │
│  │           PostgreSQL / SQLite               │  │
│  └────┬──────────────┬───────────────┬────────┘  │
│       │              │               │           │
│  ┌────┴─────┐  ┌─────┴─────┐  ┌─────┴────────┐  │
│  │ Grafana  │  │ CEF/SIEM  │  │ Webhook      │  │
│  │ Dash     │  │ Export    │  │ Alerts       │  │
│  └──────────┘  └───────────┘  └──────────────┘  │
└─────────────────────────────────────────────────┘
           ▲              ▲              ▲
    ┌──────┴──────┐ ┌─────┴─────┐ ┌─────┴──────┐
    │ Windows     │ │ Linux     │ │ Network    │
    │ Endpoint    │ │ Endpoint  │ │ (pfSense,  │
    │ Collectors  │ │ Watchers  │ │  email     │
    │ + Enforce   │ │ + SSH log │ │  gateway)  │
    └─────────────┘ └───────────┘ └────────────┘
```

## Вывод

AWatch-rus уже покрывает ~25-30% функционала InfoWatch TM по каналам мониторинга. Критический разрыв — **отсутствие блокировки** (enforcement) и **контент-анализа**. Без этих двух компонентов продукт классифицируется как "мониторинг активности", а не "DLP".

Реалистичный путь к конкурентоспособности за 6-9 месяцев:
1. Добавить enforcement (USB/print/clipboard block) — переводит из "мониторинг" в "prevention"
2. Добавить content inspection (regex packs + OCR) — переводит из "метаданные" в "DLP"
3. Добавить email канал — закрывает #1 канал утечки
4. Добавить SIEM/Grafana export — делает продукт "enterprise-visible"

Сильная сторона проекта — **гибкость и лёгкость агента**. InfoWatch agent — тяжёлый C++ monolith. AWatch-rus collectors — лёгкие PowerShell/Python скрипты, которые можно быстро адаптировать. Это стратегическое преимущество для SMB/mid-market, где InfoWatch слишком дорог и тяжёл.
