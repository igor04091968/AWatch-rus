# Russian WebUI Patch and Localization

## 2.3 Russian WebUI patch

`aw-server/aw-ru-patch.js` продолжает быть runtime patch'ем поверх ActivityWatch WebUI, но после `cc9e4a0` логика стала более явной: текстовые переводы и навигационные исправления сведены в отдельную функцию `applyTextAndNavigationPatches(root)`.

## `applyTextAndNavigationPatches`

Функция выполняет общий набор patch'ей для переданного DOM root:

- обход текстовых узлов через `walk(root)`;
- перевод атрибутов через `translateAttributes(root)`;
- исправление DLP navigation links;
- повторное применение при route change в SPA.

Это снижает риск, что часть UI останется на английском после client-side перехода без полной перезагрузки страницы.

## Исправление DLP links

DLP navigation получила защиту от битых ссылок:

- исправляются ссылки вида `/activity/dlp` и другие broken DLP activity refs;
- DLP item помечается `data-aw-ru-dlp-item="1"`, чтобы patch не дублировал элемент;
- patch различает label `DLP` внутри activity tabs и реальные broken links.

## Улучшения локализации

Патч теперь повторно применяет текстовые и навигационные изменения:

- на первичной загрузке `document.body`;
- после смены route key;
- после восстановления settings host/host groups state.

Это особенно важно для WebUI страниц, где ActivityWatch перерисовывает DOM без reload: activity views, category builder, settings и DLP navigation.
