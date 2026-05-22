# 1C Company Intelligence

Слой `1C Company Intelligence` расширяет `File 1C analytics` и добавляет:

- mart по `counterparty`;
- отдельный read-only dataset/table `companies`;
- прогноз по документам и объёму на `7/30` дней;
- health-signals по компаниям;
- read-only API для AI Investigator;
- source dashboard `1c-file-companies`.

Основной документ:

- [1C_COMPANY_INTELLIGENCE_RU.md](../1C_COMPANY_INTELLIGENCE_RU.md)

Ключевая граница:

- `counterparty` в file-based Detmir контуре по-прежнему telemetry-derived;
- `companies` даёт текущий срез базы: owner/path/size/locks/activity;
- если `counterparty` в live-выгрузках пустой, слой остаётся пустым честно и ничего не симулирует.
