# 1C Company Intelligence

Слой `1C Company Intelligence` расширяет `File 1C analytics` и добавляет:

- mart по `counterparty`;
- прогноз по документам и объёму на `7/30` дней;
- health-signals по компаниям;
- read-only API для AI Investigator;
- source dashboard `1c-file-companies`.

Основной документ:

- [1C_COMPANY_INTELLIGENCE_RU.md](../1C_COMPANY_INTELLIGENCE_RU.md)

Ключевая граница:

- если `counterparty` в live-выгрузках пустой, слой остаётся пустым честно и ничего не симулирует.
