(function () {
  window.__awRuPatchVersion = "template-v1";
  document.documentElement.setAttribute("data-aw-ru-patch", "template-v1");

  const exact = new Map([
    ["ActivityWatch", "АктивВотч"],
    ["Home", "Главная"],
    ["Activity", "Активность"],
    ["Timeline", "Таймлайн"],
    ["Trends", "Тренды"],
    ["Report", "Отчеты"],
    ["Settings", "Настройки"],
    ["Search", "Поиск"],
    ["Buckets", "Бакеты"],
    ["Stopwatch", "Секундомер"],
    ["Tools", "Инструменты"],
    ["Raw Data", "Сырые данные"],
    ["Summary", "Сводка"],
    ["All", "Все"],
    ["None", "Нет"],
    ["Date", "Дата"],
    ["Time", "Время"],
    ["Start", "Начало"],
    ["End", "Конец"],
    ["Download", "Скачать"],
    ["Upload", "Загрузить"],
    ["Show", "Показать"],
    ["Hide", "Скрыть"],
    ["Close", "Закрыть"],
    ["Week", "Неделя"],
    ["Month", "Месяц"],
    ["Year", "Год"],
    ["Today", "Сегодня"],
    ["Yesterday", "Вчера"],
    ["No data", "Нет данных"],
    ["Category", "Категория"],
    ["Categories", "Категории"],
    ["Duration", "Длительность"],
    ["Applications", "Приложения"],
    ["Hosts", "Хосты"],
    ["Date Range", "Диапазон дат"],
    ["Generate", "Сформировать"],
    ["Loading", "Загрузка"],
    ["Loading...", "Загрузка..."],
    ["Dark", "Темная"],
    ["Light", "Светлая"],
    ["Save", "Сохранить"],
    ["Cancel", "Отмена"],
    ["Delete", "Удалить"],
    ["API Browser", "API-браузер"],
    ["Documentation", "Документация"]
  ]);

  const partial = [
    ["Top apps", "Топ приложений"],
    ["Top titles", "Топ заголовков"],
    ["Top URLs", "Топ URL"],
    ["Top domains", "Топ доменов"],
    ["Top Browser Domains", "Топ доменов браузера"],
    ["Top Browser URLs", "Топ URL браузера"],
    ["Top Browser Titles", "Топ заголовков браузера"],
    ["Top Categories", "Топ категорий"],
    ["Category Tree", "Дерево категорий"],
    ["Timeline (barchart)", "Таймлайн (гистограмма)"],
    ["Calculate Work Time", "Рассчитать рабочее время"],
    ["Export CSV", "Экспорт CSV"],
    ["Export JSON", "Экспорт JSON"],
    ["No duplicate events found.", "Дубликаты событий не найдены."],
    ["No overlapping events found.", "Пересекающиеся события не найдены."],
    ["No zero-duration events found.", "События нулевой длительности не найдены."]
  ];

  function replaceText(text) {
    if (!text) return text;
    if (exact.has(text.trim())) {
      return text.replace(text.trim(), exact.get(text.trim()));
    }
    let result = text;
    for (const [en, ru] of partial) {
      result = result.split(en).join(ru);
    }
    return result;
  }

  function walk(root) {
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, null);
    const nodes = [];
    while (walker.nextNode()) nodes.push(walker.currentNode);
    for (const node of nodes) {
      const nextValue = replaceText(node.nodeValue);
      if (nextValue !== node.nodeValue) {
        node.nodeValue = nextValue;
      }
    }
  }

  function translateAttributes(root) {
    const elements = root.querySelectorAll("[title],[placeholder],[aria-label]");
    for (const element of elements) {
      ["title", "placeholder", "aria-label"].forEach(function (attr) {
        const value = element.getAttribute(attr);
        if (value) {
          element.setAttribute(attr, replaceText(value));
        }
      });
    }
  }

  function applyPatch() {
    walk(document.body);
    translateAttributes(document.body);
  }

  const observer = new MutationObserver(function () {
    applyPatch();
  });

  window.addEventListener("load", function () {
    applyPatch();
    observer.observe(document.body, { childList: true, subtree: true, characterData: true });
  });
})();
