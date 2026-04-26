(function () {
  window.__awRuPatchVersion = "template-v2";
  document.documentElement.setAttribute("data-aw-ru-patch", "template-v2");

  const exact = new Map([
    ["ActivityWatch", "АктивВотч"],
    ["Home", "Главная"],
    ["Activity", "Активность"],
    ["Developer settings", "Настройки разработчика"],
    ["Timeline", "Таймлайн"],
    ["Trends", "Тренды"],
    ["Report", "Отчеты"],
    ["Settings", "Настройки"],
    ["Search", "Поиск"],
    ["Buckets", "Бакеты"],
    ["Stopwatch", "Секундомер"],
    ["Categorization helper", "Помощник категоризации"],
    ["Categorization", "Категоризация"],
    ["Tools", "Инструменты"],
    ["Raw Data", "Сырые данные"],
    ["Summary", "Сводка"],
    ["All", "Все"],
    ["None", "Нет"],
    ["Date", "Дата"],
    ["Time", "Время"],
    ["to", "до"],
    ["Start of day", "Начало дня"],
    ["Start of week", "Начало недели"],
    ["Duration default value", "Значение длительности по умолчанию"],
    ["Landing page", "Стартовая страница"],
    ["Theme", "Тема"],
    ["New release notification", "Уведомление о новом релизе"],
    ["Use fallback colors", "Использовать резервные цвета"],
    ["Always count as active pattern", "Шаблон всегда считать активным"],
    ["Hostname", "Имя хоста"],
    ["Hostname:", "Имя хоста:"],
    ["Range", "Диапазон"],
    ["Range:", "Диапазон:"],
    ["Options", "Параметры"],
    ["Toggles", "Переключатели"],
    ["Enabled", "Включено"],
    ["Host", "Хост"],
    ["Version", "Версия"],
    ["day", "день"],
    ["week", "неделя"],
    ["month", "месяц"],
    ["year", "год"],
    ["Monday", "Понедельник"],
    ["Saturday", "Суббота"],
    ["Sunday", "Воскресенье"],
    ["Start", "Начало"],
    ["Stop", "Конец"],
    ["End", "Конец"],
    ["Refresh", "Обновить"],
    ["Apply", "Применить"],
    ["Filters", "Фильтры"],
    ["Show options", "Показать параметры"],
    ["Show last", "Показать за"],
    ["Show from", "Показать с"],
    ["Last update", "Последнее обновление"],
    ["Events shown", "Показано событий"],
    ["Window", "Окно"],
    ["New view", "Новый вид"],
    ["Edit view", "Изменить вид"],
    ["Show percent", "Показать проценты"],
    ["Top Window Titles", "Топ заголовков окон"],
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
    ["Documentation", "Документация"],
    ["Restore defaults", "Восстановить значения по умолчанию"],
    ["Import", "Импорт"],
    ["Export", "Экспорт"],
    ["Category Builder", "Конструктор категорий"],
    ["Query Explorer", "Конструктор запросов"],
    ["Event List", "Список событий"],
    ["Raw JSON", "Сырой JSON"],
    ["Expand list", "Развернуть список"],
    ["Alerts", "Оповещения"],
    ["Graph", "Граф"],
    ["Developer zone", "Зона разработчика"],
    ["History", "История"],
    ["Running", "Запущено"],
    ["Edit", "Изменить"],
    ["No stopwatch running", "Нет активного секундомера"],
    ["No label", "Без метки"],
    ["Start new", "Новый запуск"],
    ["Check", "Проверить"],
    ["Name", "Имя"],
    ["New alert", "Новое правило"],
    ["Custom regex", "Свое регулярное выражение"],
    ["Use existing categories", "Использовать существующие категории"],
    ["Choose a tag...", "Выберите тег..."],
    ["Rule", "Правило"],
    ["Open", "Открыть"],
    ["More", "Еще"],
    ["Bucket ID", "ID бакета"],
    ["Updated", "Обновлено"],
    ["First seen", "Впервые замечен"],
    ["Last updated", "Последнее обновление"],
    ["Toggle navigation", "Переключить навигацию"],
    ["unknown", "неизвестно"],
    ["Uncategorized", "Без категории"]
  ]);

  const partial = [
    ["Hello early user,", "Здравствуйте, ранний пользователь,"],
    ["early days for ActivityWatch.", "ранний этап для ActivityWatch."],
    ["still early days for ActivityWatch.", "все еще ранний этап для ActivityWatch."],
    ["It's still early days for ActivityWatch.", "ActivityWatch еще находится на раннем этапе."],
    ["We've come a long way but we need users (like you!) to provide feedback and help us turn ActivityWatch into a successful project.", "Мы уже прошли большой путь, но нам нужны пользователи вроде вас, чтобы давать обратную связь и помогать развивать проект."],
    ["Early users like you mean a lot to us, and we hope you'll reach out to us with any ideas you have for improvements!", "Такие ранние пользователи очень важны, и мы рассчитываем на ваши идеи по улучшению системы."],
    ["If you have a minute, we'd really appreciate you taking our short user survey!", "Если у вас есть минута, пожалуйста, пройдите наш короткий опрос пользователей."],
    ["If you have a minute to spare, please take the time to fill out our user survey, vote on features in the forum, or just share ActivityWatch with your friends and colleagues.", "Если у вас есть немного времени, пожалуйста, заполните наш пользовательский опрос, проголосуйте за функции на форуме или просто расскажите об ActivityWatch друзьям и коллегам."],
    ["Spread the word", "Расскажите другим"],
    ["It's still early days for ActivityWatch. We've come a long way but we need users (like you!) to provide feedback and help us turn ActivityWatch into a successful project. Early users like you mean a lot to us, and we hope you'll reach out to us with any ideas you have for improvements!", "ActivityWatch еще находится на раннем этапе. Мы уже прошли большой путь, но нам нужны пользователи вроде вас, чтобы давать обратную связь и помогать развивать проект. Такие ранние пользователи очень важны, и мы рассчитываем на ваши идеи по улучшению системы."],
    ["If you are a developer, we hope you can contribute by writing a watcher, visualization, or something else, and share it with us on the forum!", "Если вы разработчик, вы можете помочь проекту: написать watcher, визуализацию или что-то еще и поделиться этим на форуме."],
    ["Thank you for using ActivityWatch!", "Спасибо за использование ActivityWatch!"],
    ["If you are not interested in this message, then just ignore it. We won't show it very often.", "Если это сообщение вам не нужно, просто проигнорируйте его. Мы показываем его нечасто."],
    ["Trends for ", "Тренды за "],
    ["Activity for ", "Активность за "],
    ["Активность for ", "Активность за "],
    ["7 days", "7 дней"],
    ["30 days", "30 дней"],
    ["Time active:", "Активное время:"],
    ["This feature is still in early development.", "Эта функция пока находится на ранней стадии разработки."],
    ["This tool will help you create categories from your uncategorized time.", "Этот инструмент поможет создавать категории из некатегоризированного времени."],
    ["Note: These settings are meant for developers who (hopefully) know what they are doing, and as such, may break things unexpectedly.", "Примечание: эти настройки предназначены для разработчиков, которые понимают, что делают, и поэтому могут неожиданно что-нибудь сломать."],
    ["It works by fetching all uncategorized time for a recent timeperiod, and then finds the most common words (by time, not count) each of which may then either be ignored (if too broad/irrelevant), or used to create a new (sub)category, or to append the word to a pre-existing category rule. Words with less than 60s of time will not be shown.", "Инструмент получает некатегоризированное время за недавний период и ищет самые частые слова по длительности, а не по количеству. Их можно игнорировать, если они слишком общие, использовать для создания новой подкатегории или добавить в уже существующее правило. Слова с длительностью меньше 60 секунд не показываются."],
    ["When you're done, you can inspect the categories in the Settings page.", "После завершения вы сможете проверить категории на странице Настройки."],
    ['The time at which days "start", since humans don\'t always go to bed before midnight. Set to 04:00 by default.', 'Время, с которого начинается новый день, так как люди не всегда ложатся спать до полуночи. По умолчанию установлено 04:00.'],
    ["The weekday which starts a new week.", "День недели, с которого начинается новая неделя."],
    ["The default duration used for 'show last' in the timeline view.", "Длительность по умолчанию для режима 'показать последние' в таймлайне."],
    ["The page to open when opening ActivityWatch, or clicking the logo in the top menu.", "Страница, которая открывается при запуске ActivityWatch или при нажатии на логотип в верхнем меню."],
    ["Change color theme of the application (you need to change categories colors manually to be suitable with dark mode).", "Изменение цветовой темы приложения. Цвета категорий для темного режима нужно настраивать вручную."],
    ["Devmode enables some features that are still work-in-progress.", "Devmode включает некоторые функции, которые всё ещё находятся в стадии разработки."],
    ["Querying an entire year is a very heavy operation, and is likely to lead to timeouts. However, the query might be fast enough if you're running aw-server-rust.", "Запрос за целый год является очень тяжёлой операцией и может приводить к таймаутам. Но если у вас работает aw-server-rust, такой запрос может выполняться достаточно быстро."],
    ["Multidevice query is where events are collected from several hosts in the Activity view. It is an early experiment, that currently does not support browser buckets (or the audible-as-active feature).", "Multidevice query собирает события с нескольких хостов в представлении Активность. Это ранний эксперимент, который пока не поддерживает бакеты браузера и функцию активной вкладки со звуком."],
    ["The maximum amount of time a server request can take before timing out. Setting this to a high value can be useful for large queries. Note that you need to reload the web UI for it to apply.", "Максимальное время выполнения серверного запроса до срабатывания таймаута. Увеличенное значение может быть полезно для больших запросов. Чтобы изменение вступило в силу, нужно перезагрузить Web UI."],
    ["We will send you a notification if there is a new release available for download, this check will happen at most once per day.", "При появлении нового релиза для скачивания будет показано уведомление. Проверка выполняется не чаще одного раза в день."],
    ["Uses the old coloring style for some visualizations when uncategorized or no category color.", "Использует старую схему раскраски для некоторых визуализаций, когда категория не задана или у нее нет цвета."],
    ["Apps or titles matching this regular expression will never be counted as AFK.", "Приложения или заголовки, подходящие под это регулярное выражение, никогда не будут считаться AFK."],
    ["Can be used to count time as active, despite no input (like meetings, or games with controllers). An empty string disables it.", "Позволяет считать время активным даже без ввода, например на встречах или в играх с контроллером. Пустая строка отключает функцию."],
    ["Example expression:", "Пример выражения:"],
    ["Rules for categorizing events. An event can only have one category. If several categories match, the deepest one will be chosen.", "Правила категоризации событий. Событие может иметь только одну категорию. Если подходят несколько, будет выбрана самая глубокая."],
    ["You can use the Category Builder to quickly create categories from uncategorized activity.", "Через Конструктор категорий можно быстро создавать категории из некатегоризированной активности."],
    ["You can also find and share categorization rule presets on the forum.", "Готовые наборы правил категоризации можно находить и публиковать на форуме."],
    ["For help on how to write categorization rules, see the documentation.", "Как писать правила категоризации, описано в документации."],
    ["Generate a report of time spent on a certain category of device activity.", "Сформировать отчет по времени в выбранной категории активности устройства."],
    ["See the documentation for help on how to write queries.", "Как писать запросы, смотрите в документации."],
    ["See the documentation for help", "См. документацию для справки"],
    ["See the documentation", "См. документацию"],
    ["on how to write queries.", "по написанию запросов."],
    ["Number of events:", "Количество событий:"],
    ["Query", "Запрос"],
    ["EventsShowing", "Показано событий"],
    ["Drag to pan and scroll to zoom", "Перетаскивайте для прокрутки и используйте колесо мыши для масштабирования"],
    ["Made with", "Сделано с"],
    ["by the", "командой"],
    ["Сделано скомандой", "Сделано командой"],
    ["ActivityWatch developers", "разработчиков ActivityWatch"],
    ["Report a bug", "Сообщить об ошибке"],
    ["Ask for help", "Получить помощь"],
    ["Vote on features", "Голосовать за функции"],
    ["Donate", "Поддержать"],
    ["TwitterGitHub", "Twitter GitHub"],
    ["Using bucket:", "Используется бакет:"],
    ["This is an early experiment. Data entered here is not shown in the Activity view, yet.", "Это ранний эксперимент. Данные, введенные здесь, пока не отображаются в представлении Активность."],
    ["Started ", "Запущен "],
    ["hours ago", "часов назад"],
    ["days ago", "дней назад"],
    ["0s ago", "0 с назад"],
    ["minutes", "минут"],
    ["Generate a report", "Сформировать отчет"],
    ["Goal name:", "Имя цели:"],
    ["Category:", "Категория:"],
    ["Current:", "Текущее:"],
    ["Toggle autorefresh every ", "Автообновление каждые "],
    ["No events match selected criteria. Timeline is not updated.", "Нет событий, соответствующих выбранным критериям. Таймлайн не обновлен."],
    ["Last update:", "Последнее обновление:"],
    ["See PR aw-webui#365 for more information.", "Подробности см. в PR aw-webui#365."],
    ["See PR aw-webui#365", "См. PR aw-webui#365"],
    ["for more information.", "для дополнительной информации."],
    ["Displays a graph of categories and their transitions.", "Показывает граф категорий и переходов между ними."],
    ["Max category depth", "Максимальная глубина категории"],
    ["Exclude uncategorized", "Исключать некатегоризированное"],
    ["Just some tools to aid in development and debugging.", "Набор инструментов для разработки и отладки."],
    ["Nothing to see here right now...", "Сейчас здесь ничего полезного нет..."],
    ["Are you looking to collect more data? Check out the docs for more watchers.", "Нужно собирать больше данных? Посмотрите документацию по дополнительным watcher-модулям."],
    ["Are you looking to collect more data?", "Нужно собирать больше данных?"],
    ["Check out the docs for more watchers.", "Посмотрите документацию по дополнительным watcher-модулям."],
    ["Check out the docs for more watchers", "Посмотрите документацию по дополнительным watcher-модулям"],
    ["Click to sort ascending", "Нажмите для сортировки по возрастанию"],
    ["Host:", "Хост:"],
    ["Hostname:", "Имя хоста:"],
    ["Range:", "Диапазон:"],
    ["serverVersion", "Версия"],
    ["Version:", "Версия:"],
    ["Last updated:", "Последнее обновление:"],
    ["First seen:", "Впервые замечен:"],
    ["When you're done, you can inspect the categories", "После завершения вы сможете проверить категории"],
    ["in the Settings page.", "на странице Настройки."],
    ["Activity (", "Активность ("],
    ["Exclude time away from computer", "Исключать время отсутствия за компьютером"],
    ['Common words in "Uncategorized" events', 'Частые слова в событиях "Без категории"'],
    ["No words with significant duration. You're good to go!", "Нет слов со значимой длительностью. Здесь всё в порядке."],
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

  const hiddenNavLabels = new Set([
    "Raw Data",
    "Сырые данные"
  ]);

  const hiddenNavHrefPatterns = [
    /\/raw-data\b/i,
    /\/raw\b/i
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

  function injectStyles() {
    if (document.getElementById("aw-ru-hide-noise-style")) return;
    const style = document.createElement("style");
    style.id = "aw-ru-hide-noise-style";
    style.textContent = [
      '[href*="/raw-data"], [href*="/raw"], a[data-testid*="raw"], button[data-testid*="raw"] { display: none !important; }',
      '[aria-label="Raw Data"], [aria-label="Сырые данные"] { display: none !important; }'
    ].join("\n");
    document.head.appendChild(style);
  }

  function hideNoiseNavigation(root) {
    const links = root.querySelectorAll("a, button, [role='button']");
    for (const element of links) {
      const text = (element.textContent || "").trim();
      const href = (element.getAttribute("href") || "").trim();
      if (hiddenNavLabels.has(text) || hiddenNavHrefPatterns.some((pattern) => pattern.test(href))) {
        const container = element.closest("li, nav, div") || element;
        container.style.display = "none";
      }
    }
  }

  let trendsRedirectInFlight = false;

  function getTrendsHostFromSettings(settings) {
    if (!settings || typeof settings !== "object") return "";
    const landingpage = typeof settings.landingpage === "string" ? settings.landingpage : "";
    const match = landingpage.match(/\/activity\/([^/]+)/);
    return match && match[1] ? match[1] : "";
  }

  function getTrendsPath(hash) {
    if (!hash) return "";
    const normalized = hash.startsWith("#") ? hash.slice(1) : hash;
    return normalized.split("?")[0];
  }

  function shouldRedirectTrends(hash) {
    const path = getTrendsPath(hash);
    return path === "/trends" || path === "/trends/";
  }

  function redirectBareTrendsRoute() {
    if (trendsRedirectInFlight || !shouldRedirectTrends(window.location.hash)) return;
    trendsRedirectInFlight = true;
    fetch("/api/0/settings/", { credentials: "same-origin" })
      .then(function (response) {
        if (!response.ok) throw new Error("settings-fetch-failed");
        return response.json();
      })
      .then(function (settings) {
        const host = getTrendsHostFromSettings(settings);
        if (!host || !shouldRedirectTrends(window.location.hash)) return;
        const target = "#/trends/" + encodeURIComponent(host);
        if (window.location.hash !== target) {
          window.location.replace(target);
        }
      })
      .catch(function () {})
      .finally(function () {
        trendsRedirectInFlight = false;
      });
  }

  function applyPatch() {
    injectStyles();
    walk(document.body);
    translateAttributes(document.body);
    hideNoiseNavigation(document.body);
    redirectBareTrendsRoute();
  }

  const observer = new MutationObserver(function () {
    applyPatch();
  });

  window.addEventListener("load", function () {
    applyPatch();
    observer.observe(document.body, { childList: true, subtree: true, characterData: true });
  });
  window.addEventListener("hashchange", function () {
    redirectBareTrendsRoute();
  });
})();
