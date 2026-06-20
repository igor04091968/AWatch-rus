# Release readiness v0.3: audit package

Дата фиксации: `2026-06-03`.

`release-readiness-v0.3` добавляет audit-facing пакет документов для реестра
российского ПО и коммерческого пилота.

## 1. Состав v0.3

| Документ | Назначение |
|---|---|
| `docs/THIRD_PARTY_LICENSES_RU.md` | Таблица компонент / версия / лицензия / назначение / риск. |
| `docs/SECURITY_MODEL_RU.md` | Роли, границы доверия, собираемые данные, хранение и доступ. |
| `docs/REGISTRY_RUSSIAN_SO_POSITIONING_RU.md` | Класс подачи и безопасное позиционирование SIEM/UEBA/ИБ-мониторинга. |
| `docs/PILOT_DEPLOYMENT_CHECKLIST_RU.md` | Чек-лист внедрения у заказчика. |

## 2. Связь с v0.2

v0.2 дал проверяемые release assets:

- CycloneDX/SPDX SBOM;
- SHA256SUMS;
- detached signature;
- public key;
- signed Git tag;
- CI release-assets gate.

v0.3 добавляет объяснительный audit layer поверх этих artifacts.

## 3. GitHub Release wording

Для release description использовать формулировку:

> AWatch-rus v0.2 is an auditable release package for AWatch-rus with machine SBOM,
> checksums, detached signatures, a signed Git tag and pilot acceptance
> documentation. The package is prepared for expert review, commercial pilot
> onboarding and Russian software registry positioning.

## 4. Acceptance

- Документы v0.3 не содержат live IP/domains/secrets/evidence.
- pfSense описан только как optional integration/perimeter layer.
- SIEM/UEBA/DLP не заявлены как основной сертифицированный класс.
- Rust-primary runtime описан честно: PowerShell не заявлен полностью
  удаленным, а оставшиеся runtime/fallback/installer/repair scripts сохраняются
  до отдельного burn-in/canary/rollback/acceptance gate.
- Pilot checklist и acceptance act готовы для customer-facing заполнения.
