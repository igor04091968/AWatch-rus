(function () {
  window.__awRuPatchVersion = "template-v12-activity-heading-ru";
  document.documentElement.setAttribute("data-aw-ru-patch", "template-v12-activity-heading-ru");

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
    ["Timespiral", "Временная спираль"],
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
    ["This is a work-in-progress experiment.", "Это экспериментальная функция, она ещё не доведена до готового состояния."],
    ["Bucket: ", "Бакет: "],
    ["Events: ", "События: "],
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

  const dlpVerdictOptions = [
    { value: "false_positive", label: "Ложное срабатывание" },
    { value: "allowed", label: "Разрешено" },
    { value: "review_needed", label: "На проверку" },
    { value: "incident", label: "Инцидент" }
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
      '[aria-label="Raw Data"], [aria-label="Сырые данные"] { display: none !important; }',
      '.aw-ru-dlp-center { margin: 16px 0; padding: 16px; border: 1px solid rgba(120,120,120,.35); border-radius: 8px; background: rgba(20,20,20,.03); }',
      '.aw-ru-dlp-toolbar { display: flex; flex-wrap: wrap; gap: 12px; align-items: center; margin-bottom: 12px; }',
      '.aw-ru-dlp-toolbar input, .aw-ru-dlp-toolbar select, .aw-ru-dlp-toolbar textarea { min-height: 32px; }',
      '.aw-ru-dlp-toolbar button, .aw-ru-dlp-row button { min-height: 32px; padding: 4px 10px; }',
      '.aw-ru-dlp-table { width: 100%; border-collapse: collapse; font-size: 13px; }',
      '.aw-ru-dlp-table th, .aw-ru-dlp-table td { border: 1px solid rgba(120,120,120,.25); padding: 6px; vertical-align: top; }',
      '.aw-ru-dlp-table td input, .aw-ru-dlp-table td select { width: 100%; box-sizing: border-box; }',
      '.aw-ru-dlp-muted { opacity: .55; }',
      '.aw-ru-dlp-pill { display: inline-block; padding: 2px 8px; border-radius: 999px; background: rgba(90,140,255,.15); font-size: 12px; }',
      '.aw-ru-dlp-status { margin-left: auto; font-size: 12px; opacity: .8; }',
      '.aw-ru-dlp-message { margin-top: 8px; font-size: 12px; }',
      '.aw-ru-dlp-actions { display: flex; gap: 6px; flex-wrap: wrap; }',
      '.aw-ru-dlp-section { margin-top: 18px; }',
      '.aw-ru-dlp-section h5 { margin: 0 0 8px; }',
      '.aw-ru-host-groups { margin: 16px 0; padding: 16px; border: 1px solid rgba(120,120,120,.35); border-radius: 8px; background: rgba(20,20,20,.03); }',
      '.aw-ru-host-groups-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(280px, 1fr)); gap: 16px; }',
      '.aw-ru-host-group-card { border: 1px solid rgba(120,120,120,.25); border-radius: 8px; padding: 12px; background: rgba(255,255,255,.02); }',
      '.aw-ru-host-group-card h4 { margin: 0 0 8px; }',
      '.aw-ru-host-group-card p { margin: 0 0 12px; font-size: 13px; opacity: .85; }',
      '.aw-ru-host-list { display: flex; flex-direction: column; gap: 8px; }',
      '.aw-ru-host-item { border: 1px solid rgba(120,120,120,.2); border-radius: 6px; padding: 8px; }',
      '.aw-ru-host-item-title { font-weight: 600; margin-bottom: 6px; }',
      '.aw-ru-host-links { display: flex; flex-wrap: wrap; gap: 6px; }',
      '.aw-ru-host-links a { display: inline-block; padding: 4px 8px; border-radius: 999px; background: rgba(90,140,255,.15); text-decoration: none; }',
      '.aw-ru-pve-audit { margin: 16px 0; padding: 16px; border: 1px solid rgba(120,120,120,.35); border-radius: 8px; background: rgba(10,20,40,.04); }',
      '.aw-ru-pve-audit-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 12px; margin: 12px 0 16px; }',
      '.aw-ru-pve-audit-card { border: 1px solid rgba(120,120,120,.22); border-radius: 8px; padding: 12px; background: rgba(255,255,255,.02); }',
      '.aw-ru-pve-audit-card h5 { margin: 0 0 6px; font-size: 13px; opacity: .8; }',
      '.aw-ru-pve-audit-value { font-size: 24px; font-weight: 700; }',
      '.aw-ru-pve-audit-table { width: 100%; border-collapse: collapse; margin-top: 8px; }',
      '.aw-ru-pve-audit-table th, .aw-ru-pve-audit-table td { padding: 6px 8px; border-bottom: 1px solid rgba(120,120,120,.18); vertical-align: top; text-align: left; font-size: 13px; }',
      '.aw-ru-pve-audit-muted { opacity: .72; font-size: 13px; }'
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

  function getCurrentHostFromHash() {
    const hash = window.location.hash || "";
    const activityMatch = hash.match(/#\/activity\/([^/?#]+)/);
    if (activityMatch && activityMatch[1]) return decodeURIComponent(activityMatch[1]);
    const trendsMatch = hash.match(/#\/trends\/([^/?#]+)/);
    if (trendsMatch && trendsMatch[1]) return decodeURIComponent(trendsMatch[1]);
    return "";
  }

  function isPveLikeHost(host) {
    return /^pve[-_]/i.test(String(host || ""));
  }

  function isLikelyClientHost(host) {
    const value = String(host || "").trim();
    if (!value) return false;
    if (/^(?:unknown|undefined|null)$/i.test(value)) return false;
    if (/^(?:localhost|127\.0\.0\.1|0\.0\.0\.0|::1)$/i.test(value)) return false;
    if (/^(?:\d{1,3}\.){3}\d{1,3}$/.test(value)) return false;
    if (value.indexOf(":") !== -1 && /^[0-9a-f:\[\]]+$/i.test(value)) return false;
    return true;
  }

  function enforceSafeActivityViewForPveHost() {
    const hash = window.location.hash || "";
    const match = hash.match(/^#\/activity\/([^/]+)\/day\/([^/]+)\/view\/([^/?#]+)/i);
    if (!match) return;
    const host = decodeURIComponent(match[1] || "");
    const day = decodeURIComponent(match[2] || "");
    const viewId = decodeURIComponent(match[3] || "");
    if (!isPveLikeHost(host)) return;
    const safeHash = "#/activity/" + encodeURIComponent(host) + "/day/" + encodeURIComponent(day) + "/view/" + encodeURIComponent("pve_audit");
    if (safeHash !== hash && !/^pve_audit$/i.test(viewId)) {
      window.location.replace(safeHash);
    }
  }

  function getDlpHostFromSettings(settings) {
    const routeHost = getCurrentHostFromHash();
    if (isLikelyClientHost(routeHost)) return routeHost;
    const bucketHost = getDlpHostFromBucketId(getDlpBucketIdFromHash());
    if (isLikelyClientHost(bucketHost)) return bucketHost;
    return getTrendsHostFromSettings(settings);
  }

  function getDlpHref(host) {
    if (!host) return "#/buckets";
    return "#/buckets/" + encodeURIComponent("aw-dlp-endpoint-signals_" + host);
  }

  function isDlpSignalBucketRoute() {
    return /^#\/buckets\/aw-dlp-endpoint-signals_/i.test(window.location.hash || "");
  }

  function isAlertsRoute() {
    return /^#\/alerts(?:[/?#]|$)/i.test(window.location.hash || "");
  }

  function getDlpBucketIdFromHash() {
    const hash = window.location.hash || "";
    const match = hash.match(/^#\/buckets\/([^/?#]+)/i);
    return match && match[1] ? decodeURIComponent(match[1]) : "";
  }

  function getDlpHostFromBucketId(bucketId) {
    const prefix = "aw-dlp-endpoint-signals_";
    return bucketId.startsWith(prefix) ? bucketId.slice(prefix.length) : "";
  }

  function escapeHtml(value) {
    return String(value == null ? "" : value)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;")
      .replace(/'/g, "&#39;");
  }

  function normalizeText(value) {
    return String(value == null ? "" : value).trim();
  }

  function buildDlpKey(event) {
    const data = event && event.data ? event.data : {};
    return [
      event && event.timestamp || "",
      data.signalType || "",
      data.username || "",
      data.owner || "",
      data.documentName || "",
      data.printerName || ""
    ].join("|");
  }

  function generateDlpId(prefix) {
    return prefix + "-" + Date.now().toString(36) + "-" + Math.random().toString(36).slice(2, 10);
  }

  async function awApiJson(url, options) {
    const response = await fetch(url, Object.assign({
      credentials: "same-origin",
      headers: {
        "Content-Type": "application/json; charset=utf-8"
      }
    }, options || {}));
    if (response.status === 304 || response.status === 409) {
      return null;
    }
    if (!response.ok) {
      throw new Error("aw-api-" + response.status);
    }
    if (response.status === 204) return null;
    const text = await response.text();
    return text ? JSON.parse(text) : null;
  }

  async function ensureAwBucket(bucketId, clientName, bucketType, hostname) {
    await awApiJson("/api/0/buckets/" + encodeURIComponent(bucketId), {
      method: "POST",
      body: JSON.stringify({
        client: clientName,
        type: bucketType,
        hostname: hostname
      })
    });
  }

  async function saveAwHeartbeat(bucketId, payload, pulsetimeSeconds) {
    const pulsetime = pulsetimeSeconds || 1;
    await awApiJson("/api/0/buckets/" + encodeURIComponent(bucketId) + "/heartbeat?pulsetime=" + pulsetime, {
      method: "POST",
      body: JSON.stringify(payload)
    });
  }

  async function loadBucketEvents(bucketId, limit) {
    const data = await awApiJson("/api/0/buckets/" + encodeURIComponent(bucketId) + "/events?limit=" + (limit || 100), {
      method: "GET",
      headers: {}
    });
    return Array.isArray(data) ? data : [];
  }

  function getRuleMatchFields(event) {
    const data = event && event.data ? event.data : {};
    return {
      signalType: normalizeText(data.signalType),
      username: normalizeText(data.username),
      owner: normalizeText(data.owner),
      documentName: normalizeText(data.documentName),
      printerName: normalizeText(data.printerName),
      hostname: normalizeText(data.hostname)
    };
  }

  function serializeRuleMatch(match) {
    const normalized = match || {};
    return [
      normalized.signalType || "",
      normalized.username || "",
      normalized.owner || "",
      normalized.documentName || "",
      normalized.printerName || "",
      normalized.hostname || ""
    ].join("|");
  }

  function getRuleId(ruleEvent) {
    const data = ruleEvent && ruleEvent.data ? ruleEvent.data : {};
    return normalizeText(data.ruleId) || [
      serializeRuleMatch(data.match || {}),
      normalizeText(data.category),
      normalizeText(data.action)
    ].join("|");
  }

  function getReviewId(reviewEvent) {
    const data = reviewEvent && reviewEvent.data ? reviewEvent.data : {};
    const review = data.review || {};
    return normalizeText(review.reviewId) || [
      data.sourceEvent && data.sourceEvent.timestamp || "",
      data.sourceEvent && data.sourceEvent.data ? buildDlpKey({ timestamp: data.sourceEvent.timestamp, data: data.sourceEvent.data }) : "",
      normalizeText(review.verdict),
      normalizeText(review.category)
    ].join("|");
  }

  function collapseRuleEvents(events) {
    const ordered = (events || []).slice().sort(function (a, b) {
      return String(a.timestamp).localeCompare(String(b.timestamp));
    });
    const map = new Map();
    ordered.forEach(function (event) {
      map.set(getRuleId(event), event);
    });
    return Array.from(map.values()).sort(function (a, b) {
      return String(b.timestamp).localeCompare(String(a.timestamp));
    });
  }

  function collapseReviewEvents(events) {
    const ordered = (events || []).slice().sort(function (a, b) {
      return String(a.timestamp).localeCompare(String(b.timestamp));
    });
    const map = new Map();
    ordered.forEach(function (event) {
      map.set(getReviewId(event), event);
    });
    return Array.from(map.values()).sort(function (a, b) {
      return String(b.timestamp).localeCompare(String(a.timestamp));
    });
  }

  function ruleMatchesEvent(rule, event) {
    const eventFields = getRuleMatchFields(event);
    const match = rule && rule.data && rule.data.match ? rule.data.match : {};
    return Object.keys(eventFields).every(function (key) {
      const ruleValue = normalizeText(match[key]);
      return !ruleValue || ruleValue === eventFields[key];
    });
  }

  function getSuppressionState() {
    if (!window.__awRuDlpState) {
      window.__awRuDlpState = {
        rules: [],
        activeRules: [],
        reviews: [],
        events: [],
        loading: false
      };
    }
    return window.__awRuDlpState;
  }

  function removeBadDlpLinks(root) {
    const badLinks = root.querySelectorAll("a[href*='/view/DLP']");
    badLinks.forEach(function (link) {
      const item = link.closest("li") || link;
      item.remove();
    });
  }

  function updateDlpLinks(root, href) {
    const links = root.querySelectorAll("a[data-aw-ru-dlp-link='1']");
    for (const link of links) {
      if (link.getAttribute("href") !== href) {
        link.setAttribute("href", href);
      }
    }
  }

  function buildDlpNavItem(templateItem, href) {
    const templateLink = templateItem.querySelector("a[href], [role='link']");
    if (!templateLink) return null;

    const item = document.createElement("li");
    item.setAttribute("data-aw-ru-dlp-item", "1");
    item.className = templateItem.className || "";

    const link = document.createElement("a");
    link.setAttribute("href", href);
    link.setAttribute("data-aw-ru-dlp-link", "1");
    link.textContent = "DLP";
    link.className = templateLink.className || "";

    item.appendChild(link);
    return item;
  }

  function findPrimaryNavList(root) {
    const navLists = Array.from(root.querySelectorAll("nav ul"));
    return navLists.find(function (list) {
      const labels = Array.from(list.querySelectorAll("a"))
        .map(function (link) { return normalizeText(link.textContent); })
        .filter(Boolean);
      return labels.includes("Главная") ||
        labels.includes("Home") ||
        labels.includes("Активность") ||
        labels.includes("Activity");
    }) || null;
  }

  function injectDlpNavigation(root) {
    const hostForDlp = window.__awRuPatchSettingsHost || getCurrentHostFromHash();
    if (hostForDlp && isPveLikeHost(hostForDlp)) {
      removeBadDlpLinks(root);
      const ownItem = root.querySelector("[data-aw-ru-dlp-item='1']");
      if (ownItem) ownItem.remove();
      return;
    }
    const href = getDlpHref(hostForDlp);
    removeBadDlpLinks(root);
    updateDlpLinks(root, href);
    if (root.querySelector("[data-aw-ru-dlp-item='1']")) return;

    const primaryNav = findPrimaryNavList(root);
    if (primaryNav) {
      const templateItem = primaryNav.querySelector("li") || primaryNav.parentElement;
      const dlpItem = templateItem ? buildDlpNavItem(templateItem, href) : null;
      if (dlpItem) {
        primaryNav.appendChild(dlpItem);
        return;
      }
      const item = document.createElement("li");
      item.setAttribute("data-aw-ru-dlp-item", "1");
      const link = document.createElement("a");
      link.setAttribute("href", href);
      link.setAttribute("data-aw-ru-dlp-link", "1");
      link.textContent = "DLP";
      item.appendChild(link);
      primaryNav.appendChild(item);
    }
  }

  function isHomeRoute() {
    const hash = window.location.hash || "";
    return !hash || /^#\/home(?:[/?#]|$)/i.test(hash);
  }

  function getDefaultHostGroupsConfig() {
    return {
      groups: [
        {
          id: "windows-rdp",
          name: "Windows RDP",
          description: "Пользовательские Windows/RDP хосты.",
          patterns: ["^(SHARKON|WIN|RDP|TERM|TS-|WS-)"],
          links: [
            { label: "Активность", type: "activity" },
            { label: "DLP", type: "bucket", bucket_prefix: "aw-dlp-endpoint-signals_" }
          ]
        },
        {
          id: "linux-remote",
          name: "Linux remote workers",
          description: "Linux-хосты удалённых сотрудников: GUI активность, SSH/console и browser admin UI.",
          patterns: ["^(LINUX-WS|LINUX-DESKTOP|LX-|DESKTOP-|ADMIN-|WORKSTATION-|DEVBOX-)"],
          links: [
            { label: "Активность", type: "activity" },
            { label: "SSH сессии", type: "bucket", bucket_prefix: "aw-ssh-sessions_" },
            { label: "Команды shell", type: "bucket", bucket_prefix: "aw-console-commands_" },
            { label: "Web категории", type: "bucket", bucket_prefix: "aw-detmir-web-category_" },
            { label: "Все бакеты", type: "buckets" }
          ]
        },
        {
          id: "virtual-infra",
          name: "Virtual servers + Proxmox",
          description: "Инфраструктурные VM, Proxmox и сетевые узлы.",
          patterns: ["^(PFSENSE|PVE|PROXMOX|DEBIAN|UBUNTU|LINUX|VM-|SRV-|INFRA-)"],
          links: [
            { label: "pfSense health", type: "bucket", bucket_prefix: "aw-pfsense-health_" },
            { label: "pfSense gateways", type: "bucket", bucket_prefix: "aw-pfsense-gateways_" },
            { label: "Все бакеты", type: "buckets" }
          ]
        }
      ],
      ungrouped_name: "Прочие хосты"
    };
  }

  function getHostGroupsState() {
    if (!window.__awRuHostGroupsState) {
      window.__awRuHostGroupsState = {
        config: null,
        buckets: null,
        loading: false
      };
    }
    return window.__awRuHostGroupsState;
  }

  async function ensureHostGroupsData() {
    const state = getHostGroupsState();
    if (state.loading) return state;
    if (state.config && state.buckets) return state;
    state.loading = true;
    try {
      if (!state.config) {
        try {
          state.config = await awApiJson("/js/aw-host-groups.json?v=" + encodeURIComponent(window.__awRuPatchVersion), { method: "GET", headers: {} });
        } catch (error) {
          state.config = getDefaultHostGroupsConfig();
        }
      }
      if (!state.buckets) {
        state.buckets = await awApiJson("/api/0/buckets/", { method: "GET", headers: {} });
      }
    } finally {
      state.loading = false;
    }
    return state;
  }

  function isPveActivityRoute() {
    const hash = window.location.hash || "";
    const match = hash.match(/^#\/activity\/([^/]+)/i);
    return !!(match && isPveLikeHost(decodeURIComponent(match[1] || "")));
  }

  function extractHostFromBucket(bucketId, bucketMeta) {
    if (bucketMeta && bucketMeta.hostname) return String(bucketMeta.hostname);
    const prefixes = [
      "aw-watcher-window_",
      "aw-watcher-afk_",
      "aw-console-commands_",
      "aw-ssh-sessions_",
      "aw-linux-web-context_",
      "aw-detmir-web-category_",
      "aw-dlp-endpoint-signals_",
      "aw-session-events_",
      "aw-worktime-sessions_",
      "aw-pve-webadmin-events_",
      "aw-pve-task-events_",
      "aw-dlp-incidents_",
      "aw-pfsense-health_",
      "aw-pfsense-gateways_",
      "aw-pfsense-interfaces_"
    ];
    for (const prefix of prefixes) {
      if (bucketId.indexOf(prefix) === 0) {
        return bucketId.slice(prefix.length);
      }
    }
    return "";
  }

  function buildHostBucketMap(rawBuckets) {
    const result = new Map();
    const entries = Array.isArray(rawBuckets)
      ? rawBuckets.map(function (item) { return [item.id || "", item]; })
      : Object.entries(rawBuckets || {});
    entries.forEach(function (entry) {
      const bucketId = entry[0];
      const meta = entry[1] || {};
      const host = extractHostFromBucket(bucketId, meta);
      if (!host) return;
      if (!result.has(host)) result.set(host, []);
      result.get(host).push(bucketId);
    });
    return result;
  }

  function hostHasBucketPrefix(hostBuckets, prefix) {
    return (hostBuckets || []).some(function (bucketId) {
      return String(bucketId || "").indexOf(prefix) === 0;
    });
  }

  function matchHostGroup(host, groups, hostBuckets) {
    const bucketList = hostBuckets || [];
    if (hostHasBucketPrefix(bucketList, "aw-dlp-endpoint-signals_") || hostHasBucketPrefix(bucketList, "aw-session-events_")) {
      return "windows-rdp";
    }
    if (
      hostHasBucketPrefix(bucketList, "aw-console-commands_") ||
      hostHasBucketPrefix(bucketList, "aw-ssh-sessions_") ||
      hostHasBucketPrefix(bucketList, "aw-linux-web-context_") ||
      hostHasBucketPrefix(bucketList, "aw-detmir-web-category_")
    ) {
      if (!hostHasBucketPrefix(bucketList, "aw-pve-webadmin-events_") && !hostHasBucketPrefix(bucketList, "aw-pve-task-events_")) {
        return "linux-remote";
      }
    }
    for (const group of groups) {
      const patterns = Array.isArray(group.patterns) ? group.patterns : [];
      for (const pattern of patterns) {
        try {
          if (new RegExp(pattern, "i").test(host)) {
            return group.id;
          }
        } catch (error) {
        }
      }
    }
    return "";
  }

  function buildHostLink(host, hostBuckets, linkDef) {
    if (!linkDef || !linkDef.type) return "";
    if (linkDef.type === "activity") {
      const viewId = linkDef.view ? String(linkDef.view) : "summary";
      return '#/activity/' + encodeURIComponent(host) + '/day/' + encodeURIComponent(new Date().toISOString().slice(0, 10)) + '/view/' + encodeURIComponent(viewId);
    }
    if (linkDef.type === "buckets") {
      return "#/buckets";
    }
    if (linkDef.type === "bucket" && linkDef.bucket_prefix) {
      const bucketId = String(linkDef.bucket_prefix) + host;
      return hostBuckets.indexOf(bucketId) >= 0 ? '#/buckets/' + encodeURIComponent(bucketId) : "";
    }
    return "";
  }

  function renderHostGroupCards(state) {
    const config = state.config || getDefaultHostGroupsConfig();
    const groups = Array.isArray(config.groups) ? config.groups : [];
    const hostBuckets = buildHostBucketMap(state.buckets);
    const grouped = new Map();

    groups.forEach(function (group) {
      grouped.set(group.id, []);
    });
    grouped.set("__ungrouped__", []);

    Array.from(hostBuckets.keys()).sort().forEach(function (host) {
      const groupId = matchHostGroup(host, groups, hostBuckets.get(host) || []) || "__ungrouped__";
      grouped.get(groupId).push(host);
    });

    const cards = [];
    groups.forEach(function (group) {
      const hosts = grouped.get(group.id) || [];
      const items = hosts.map(function (host) {
        const links = (group.links || []).map(function (linkDef) {
          const href = buildHostLink(host, hostBuckets.get(host) || [], linkDef);
          return href ? '<a href="' + escapeHtml(href) + '">' + escapeHtml(linkDef.label || "Открыть") + '</a>' : "";
        }).filter(Boolean).join("");
        return '<div class="aw-ru-host-item">' +
          '<div class="aw-ru-host-item-title">' + escapeHtml(host) + '</div>' +
          '<div class="aw-ru-host-links">' + links + '</div>' +
        '</div>';
      }).join("");
      cards.push(
        '<section class="aw-ru-host-group-card">' +
          '<h4>' + escapeHtml(group.name || group.id) + '</h4>' +
          '<p>' + escapeHtml(group.description || "") + '</p>' +
          '<div class="aw-ru-host-list">' + (items || '<div class="aw-ru-host-item">Хосты пока не обнаружены.</div>') + '</div>' +
        '</section>'
      );
    });

    const ungroupedHosts = grouped.get("__ungrouped__") || [];
    if (ungroupedHosts.length) {
      cards.push(
        '<section class="aw-ru-host-group-card">' +
          '<h4>' + escapeHtml(config.ungrouped_name || "Прочие хосты") + '</h4>' +
          '<p>Хосты, которые пока не попали под шаблоны группировки.</p>' +
          '<div class="aw-ru-host-list">' +
            ungroupedHosts.map(function (host) {
              return '<div class="aw-ru-host-item"><div class="aw-ru-host-item-title">' + escapeHtml(host) + '</div></div>';
            }).join("") +
          '</div>' +
        '</section>'
      );
    }

    return cards.join("");
  }

  async function injectHostGroupsCenter(root) {
    if (!isHomeRoute()) return;
    const heading = root.querySelector("h3");
    if (!heading) return;

    let center = root.querySelector("[data-aw-ru-host-groups='1']");
    if (!center) {
      center = document.createElement("section");
      center.className = "aw-ru-host-groups";
      center.setAttribute("data-aw-ru-host-groups", "1");
      center.innerHTML =
        '<h4>Разделы хостов</h4>' +
        '<p>Здесь хосты разделены на Windows RDP, Linux remote workers и инфраструктурные узлы.</p>' +
        '<div class="aw-ru-host-groups-grid" data-aw-ru-host-groups-grid><section class="aw-ru-host-group-card"><p>Загрузка...</p></section></div>';
      heading.parentElement.insertBefore(center, heading.nextSibling);
    }

    const state = await ensureHostGroupsData();
    center.querySelector("[data-aw-ru-host-groups-grid]").innerHTML = renderHostGroupCards(state);
  }

  function renderDlpTableRows(center, host) {
    const state = getSuppressionState();
    const tbody = center.querySelector("[data-aw-ru-dlp-events]");
    if (!tbody) return;
    const hideSuppressed = center.querySelector("[data-aw-ru-hide-suppressed]") && center.querySelector("[data-aw-ru-hide-suppressed]").checked;
    const rows = [];
    for (const event of state.events) {
      const matchedRule = state.activeRules.find(function (rule) { return ruleMatchesEvent(rule, event); }) || null;
      if (hideSuppressed && matchedRule) continue;
      const data = event.data || {};
      const eventKey = buildDlpKey(event);
      rows.push(
        '<tr class="aw-ru-dlp-row' + (matchedRule ? ' aw-ru-dlp-muted' : '') + '" data-aw-ru-dlp-key="' + escapeHtml(eventKey) + '">' +
          "<td>" + escapeHtml(new Date(event.timestamp).toLocaleString()) + "</td>" +
          "<td>" + escapeHtml(data.signalType || "") + "</td>" +
          "<td>" + escapeHtml(data.username || data.owner || "") + "</td>" +
          "<td>" + escapeHtml(data.documentName || "") + "</td>" +
          "<td>" + escapeHtml(data.printerName || "") + "</td>" +
          "<td>" + (matchedRule ? '<span class="aw-ru-dlp-pill">Подавлено правилом</span>' : "") + "</td>" +
          '<td><select data-aw-ru-dlp-verdict>' +
            dlpVerdictOptions.map(function (option) {
              return '<option value="' + option.value + '">' + option.label + '</option>';
            }).join("") +
          "</select></td>" +
          '<td><input type="text" data-aw-ru-dlp-category placeholder="например, safe.print.invoice" /></td>' +
          '<td><input type="text" data-aw-ru-dlp-comment placeholder="комментарий" /></td>' +
          '<td class="aw-ru-dlp-actions">' +
            '<button type="button" data-aw-ru-save-review>Сохранить</button>' +
            '<button type="button" data-aw-ru-save-rule>Правило</button>' +
            '<button type="button" data-aw-ru-create-case>Кейс</button>' +
          "</td>" +
        "</tr>"
      );
    }
    tbody.innerHTML = rows.length ? rows.join("") : '<tr><td colspan="10">Нет DLP-событий в выборке.</td></tr>';
    center.querySelector("[data-aw-ru-dlp-status]").textContent =
      "Событий: " + state.events.length + " · правил: " + state.activeRules.length + "/" + state.rules.length + " · review: " + state.reviews.filter(function (review) { return !(review.data && review.data.review && review.data.review.archived); }).length + "/" + state.reviews.length;
    bindDlpRowActions(center, host);
    renderDlpRuleManager(center, host);
    renderDlpReviewManager(center, host);
  }

  async function saveDlpReview(host, event, row) {
    const bucketId = "aw-dlp-review_" + host;
    await ensureAwBucket(bucketId, "aw-dlp-review", "aw.dlp.review", host);
    const verdict = row.querySelector("[data-aw-ru-dlp-verdict]").value;
    const category = row.querySelector("[data-aw-ru-dlp-category]").value.trim();
    const comment = row.querySelector("[data-aw-ru-dlp-comment]").value.trim();
    await saveAwHeartbeat(bucketId, {
      timestamp: new Date().toISOString(),
      duration: 0,
      data: {
        host: host,
        sourceBucket: getDlpBucketIdFromHash(),
        sourceEvent: {
          timestamp: event.timestamp,
          data: event.data || {}
        },
        review: {
          reviewId: generateDlpId("review"),
          verdict: verdict,
          category: category,
          comment: comment,
          archived: false
        }
      }
    }, 1);
    if (verdict === "incident") {
      await saveDlpIncident(host, event, {
        verdict: verdict,
        category: category,
        comment: comment
      });
    }
  }

  async function saveDlpIncident(host, event, review) {
    const bucketId = "aw-dlp-incidents_" + host;
    await ensureAwBucket(bucketId, "aw-dlp-incidents", "aw.dlp.incident", host);
    await saveAwHeartbeat(bucketId, {
      timestamp: new Date().toISOString(),
      duration: 0,
      data: {
        host: host,
        sourceBucket: getDlpBucketIdFromHash(),
        sourceEvent: {
          timestamp: event.timestamp,
          data: event.data || {}
        },
        incident: {
          incidentId: generateDlpId("incident"),
          verdict: review && review.verdict || "incident",
          category: review && review.category || "",
          comment: review && review.comment || "",
          status: "open"
        }
      }
    }, 1);
  }

  function getCaseApiBase() {
    if (window.__awCaseApiBase && typeof window.__awCaseApiBase === "string") {
      return window.__awCaseApiBase.replace(/\/+$/, "");
    }
    try {
      const origin = window.location.origin || "";
      if (/:\d+$/.test(origin)) return origin.replace(/:\d+$/, ":5602");
      return origin + ":5602";
    } catch (error) {
      return "http://127.0.0.1:5602";
    }
  }

  async function caseApi(path, init) {
    const response = await fetch(getCaseApiBase() + path, Object.assign({ credentials: "omit" }, init || {}));
    if (!response.ok) throw new Error("Case API HTTP " + response.status);
    if (response.status === 204) return null;
    return response.json();
  }

  async function createCaseFromEvent(host, event, row) {
    const data = event.data || {};
    const verdict = row.querySelector("[data-aw-ru-dlp-verdict]").value;
    const category = row.querySelector("[data-aw-ru-dlp-category]").value.trim();
    const comment = row.querySelector("[data-aw-ru-dlp-comment]").value.trim();
    const incidentId = buildDlpKey(event);
    const title = "DLP " + (data.signalType || "incident") + " · " + (data.username || data.owner || host || "unknown");
    return caseApi("/api/0/dlp/cases", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        incident_id: incidentId,
        host: host,
        title: title,
        severity: verdict === "incident" ? "high" : "medium",
        source_bucket: getDlpBucketIdFromHash(),
        source_event_ts: event.timestamp,
        evidence: {
          signalType: data.signalType || "",
          username: data.username || data.owner || "",
          documentName: data.documentName || "",
          printerName: data.printerName || "",
          category: category,
          comment: comment
        }
      })
    });
  }

  async function saveDlpRule(host, event, row) {
    const bucketId = "aw-dlp-rules_" + host;
    await ensureAwBucket(bucketId, "aw-dlp-rules", "aw.dlp.rule", host);
    const verdict = row.querySelector("[data-aw-ru-dlp-verdict]").value;
    const category = row.querySelector("[data-aw-ru-dlp-category]").value.trim();
    const comment = row.querySelector("[data-aw-ru-dlp-comment]").value.trim();
    await saveAwHeartbeat(bucketId, {
      timestamp: new Date().toISOString(),
      duration: 0,
      data: {
        host: host,
        ruleId: generateDlpId("rule"),
        enabled: true,
        action: verdict,
        category: category,
        comment: comment,
        match: getRuleMatchFields(event)
      }
    }, 1);
  }

  function bindDlpRowActions(center, host) {
    const state = getSuppressionState();
    const rows = center.querySelectorAll("[data-aw-ru-dlp-key]");
    rows.forEach(function (row) {
      if (row.getAttribute("data-aw-ru-bound") === "1") return;
      row.setAttribute("data-aw-ru-bound", "1");
      const eventKey = row.getAttribute("data-aw-ru-dlp-key");
      const event = state.events.find(function (item) { return buildDlpKey(item) === eventKey; });
      if (!event) return;
      row.querySelector("[data-aw-ru-save-review]").addEventListener("click", async function () {
        const message = center.querySelector("[data-aw-ru-dlp-message]");
        try {
          await saveDlpReview(host, event, row);
          state.reviews = collapseReviewEvents(await loadBucketEvents("aw-dlp-review_" + host, 200));
          renderDlpTableRows(center, host);
          message.textContent = "Review сохранен.";
        } catch (error) {
          message.textContent = "Ошибка сохранения review: " + error.message;
        }
      });
      row.querySelector("[data-aw-ru-save-rule]").addEventListener("click", async function () {
        const message = center.querySelector("[data-aw-ru-dlp-message]");
        try {
          await saveDlpRule(host, event, row);
          state.rules = collapseRuleEvents(await loadBucketEvents("aw-dlp-rules_" + host, 200));
          state.activeRules = state.rules.filter(function (rule) { return !(rule.data && rule.data.enabled === false); });
          message.textContent = "Правило сохранено.";
          renderDlpTableRows(center, host);
        } catch (error) {
          message.textContent = "Ошибка сохранения правила: " + error.message;
        }
      });
      row.querySelector("[data-aw-ru-create-case]").addEventListener("click", async function () {
        const message = center.querySelector("[data-aw-ru-dlp-message]");
        try {
          const created = await createCaseFromEvent(host, event, row);
          await renderCaseManager(center, host);
          message.textContent = "Кейс создан: #" + (created && created.id ? created.id : "?");
        } catch (error) {
          message.textContent = "Ошибка создания кейса: " + error.message;
        }
      });
    });
  }

  async function renderCaseManager(center, host) {
    const tbody = center.querySelector("[data-aw-ru-dlp-cases]");
    if (!tbody) return;
    try {
      const cases = await caseApi("/api/0/dlp/cases?host=" + encodeURIComponent(host) + "&limit=100", { method: "GET" });
      const rows = (cases || []).map(function (c) {
        return (
          "<tr>" +
          "<td>" + escapeHtml(String(c.id || "")) + "</td>" +
          "<td>" + escapeHtml(String(c.status || "")) + "</td>" +
          "<td>" + escapeHtml(String(c.severity || "")) + "</td>" +
          "<td>" + escapeHtml(String(c.title || "")) + "</td>" +
          "<td>" + escapeHtml(String(c.assignee || "")) + "</td>" +
          "<td>" + escapeHtml(String(c.incident_id || "")) + "</td>" +
          "<td>" + escapeHtml(String(c.updated_at || c.created_at || "")) + "</td>" +
          "</tr>"
        );
      });
      tbody.innerHTML = rows.length ? rows.join("") : '<tr><td colspan="7">Кейсов нет.</td></tr>';
      const status = center.querySelector("[data-aw-ru-dlp-cases-status]");
      if (status) status.textContent = "Кейсов: " + (cases || []).length;
    } catch (error) {
      tbody.innerHTML = '<tr><td colspan="7">Ошибка загрузки кейсов: ' + escapeHtml(error.message) + '</td></tr>';
      const status = center.querySelector("[data-aw-ru-dlp-cases-status]");
      if (status) status.textContent = "Кейсы недоступны";
    }
  }

  async function setDlpRuleEnabled(host, ruleEvent, enabled) {
    const bucketId = "aw-dlp-rules_" + host;
    await ensureAwBucket(bucketId, "aw-dlp-rules", "aw.dlp.rule", host);
    const data = ruleEvent.data || {};
    await saveAwHeartbeat(bucketId, {
      timestamp: new Date().toISOString(),
      duration: 0,
      data: {
        host: host,
        ruleId: getRuleId(ruleEvent),
        enabled: enabled,
        action: data.action || "",
        category: data.category || "",
        comment: data.comment || "",
        match: data.match || {}
      }
    }, 1);
  }

  async function setDlpReviewArchived(host, reviewEvent, archived) {
    const bucketId = "aw-dlp-review_" + host;
    await ensureAwBucket(bucketId, "aw-dlp-review", "aw.dlp.review", host);
    const data = reviewEvent.data || {};
    const review = data.review || {};
    await saveAwHeartbeat(bucketId, {
      timestamp: new Date().toISOString(),
      duration: 0,
      data: {
        host: host,
        sourceBucket: data.sourceBucket || getDlpBucketIdFromHash(),
        sourceEvent: data.sourceEvent || {},
        review: {
          reviewId: getReviewId(reviewEvent),
          verdict: review.verdict || "",
          category: review.category || "",
          comment: review.comment || "",
          archived: archived
        }
      }
    }, 1);
  }

  function renderDlpRuleManager(center, host) {
    const state = getSuppressionState();
    const tbody = center.querySelector("[data-aw-ru-dlp-rules]");
    if (!tbody) return;
    const showDisabled = center.querySelector("[data-aw-ru-show-disabled-rules]") && center.querySelector("[data-aw-ru-show-disabled-rules]").checked;
    const rows = [];
    state.rules.forEach(function (ruleEvent) {
      const data = ruleEvent.data || {};
      const isEnabled = data.enabled !== false;
      if (!isEnabled && !showDisabled) return;
      rows.push(
        '<tr data-aw-ru-rule-id="' + escapeHtml(getRuleId(ruleEvent)) + '">' +
          '<td>' + escapeHtml(new Date(ruleEvent.timestamp).toLocaleString()) + '</td>' +
          '<td>' + (isEnabled ? '<span class="aw-ru-dlp-pill">Активно</span>' : '<span class="aw-ru-dlp-pill">Отключено</span>') + '</td>' +
          '<td>' + escapeHtml(data.action || "") + '</td>' +
          '<td>' + escapeHtml(data.category || "") + '</td>' +
          '<td>' + escapeHtml(serializeRuleMatch(data.match || {})) + '</td>' +
          '<td>' + escapeHtml(data.comment || "") + '</td>' +
          '<td class="aw-ru-dlp-actions">' +
            '<button type="button" data-aw-ru-toggle-rule>' + (isEnabled ? 'Отключить' : 'Включить') + '</button>' +
          '</td>' +
        '</tr>'
      );
    });
    tbody.innerHTML = rows.length ? rows.join("") : '<tr><td colspan="7">Сохраненных правил нет.</td></tr>';
    tbody.querySelectorAll("[data-aw-ru-toggle-rule]").forEach(function (button) {
      button.addEventListener("click", async function () {
        const row = button.closest("[data-aw-ru-rule-id]");
        const ruleId = row && row.getAttribute("data-aw-ru-rule-id");
        const ruleEvent = state.rules.find(function (item) { return getRuleId(item) === ruleId; });
        if (!ruleEvent) return;
        const message = center.querySelector("[data-aw-ru-dlp-message]");
        try {
          await setDlpRuleEnabled(host, ruleEvent, ruleEvent.data && ruleEvent.data.enabled === false);
          state.rules = collapseRuleEvents(await loadBucketEvents("aw-dlp-rules_" + host, 200));
          state.activeRules = state.rules.filter(function (rule) { return !(rule.data && rule.data.enabled === false); });
          renderDlpTableRows(center, host);
          message.textContent = "Статус правила обновлен.";
        } catch (error) {
          message.textContent = "Ошибка обновления правила: " + error.message;
        }
      });
    });
  }

  function renderDlpReviewManager(center, host) {
    const state = getSuppressionState();
    const tbody = center.querySelector("[data-aw-ru-dlp-reviews]");
    if (!tbody) return;
    const showArchived = center.querySelector("[data-aw-ru-show-archived-reviews]") && center.querySelector("[data-aw-ru-show-archived-reviews]").checked;
    const rows = [];
    state.reviews.forEach(function (reviewEvent) {
      const data = reviewEvent.data || {};
      const review = data.review || {};
      const archived = review.archived === true;
      if (archived && !showArchived) return;
      const sourceData = data.sourceEvent && data.sourceEvent.data ? data.sourceEvent.data : {};
      rows.push(
        '<tr data-aw-ru-review-id="' + escapeHtml(getReviewId(reviewEvent)) + '">' +
          '<td>' + escapeHtml(new Date(reviewEvent.timestamp).toLocaleString()) + '</td>' +
          '<td>' + (archived ? '<span class="aw-ru-dlp-pill">Архив</span>' : '<span class="aw-ru-dlp-pill">Активно</span>') + '</td>' +
          '<td>' + escapeHtml(review.verdict || "") + '</td>' +
          '<td>' + escapeHtml(review.category || "") + '</td>' +
          '<td>' + escapeHtml(review.comment || "") + '</td>' +
          '<td>' + escapeHtml(sourceData.signalType || "") + ' · ' + escapeHtml(sourceData.documentName || sourceData.printerName || sourceData.username || "") + '</td>' +
          '<td class="aw-ru-dlp-actions">' +
            '<button type="button" data-aw-ru-toggle-review>' + (archived ? 'Вернуть' : 'Архивировать') + '</button>' +
          '</td>' +
        '</tr>'
      );
    });
    tbody.innerHTML = rows.length ? rows.join("") : '<tr><td colspan="7">Сохраненных review нет.</td></tr>';
    tbody.querySelectorAll("[data-aw-ru-toggle-review]").forEach(function (button) {
      button.addEventListener("click", async function () {
        const row = button.closest("[data-aw-ru-review-id]");
        const reviewId = row && row.getAttribute("data-aw-ru-review-id");
        const reviewEvent = state.reviews.find(function (item) { return getReviewId(item) === reviewId; });
        if (!reviewEvent) return;
        const message = center.querySelector("[data-aw-ru-dlp-message]");
        try {
          await setDlpReviewArchived(host, reviewEvent, !(reviewEvent.data && reviewEvent.data.review && reviewEvent.data.review.archived === true));
          state.reviews = collapseReviewEvents(await loadBucketEvents("aw-dlp-review_" + host, 200));
          renderDlpTableRows(center, host);
          message.textContent = "Статус review обновлен.";
        } catch (error) {
          message.textContent = "Ошибка обновления review: " + error.message;
        }
      });
    });
  }

  async function refreshDlpCenter(center, host) {
    const state = getSuppressionState();
    if (state.loading) return;
    state.loading = true;
    center.querySelector("[data-aw-ru-dlp-message]").textContent = "Загрузка DLP-событий...";
    try {
      state.events = (await loadBucketEvents(getDlpBucketIdFromHash(), 200))
        .sort(function (a, b) { return String(b.timestamp).localeCompare(String(a.timestamp)); });
      try {
        state.rules = collapseRuleEvents(await loadBucketEvents("aw-dlp-rules_" + host, 200));
        state.activeRules = state.rules.filter(function (rule) { return !(rule.data && rule.data.enabled === false); });
      } catch (error) {
        state.rules = [];
        state.activeRules = [];
      }
      try {
        state.reviews = collapseReviewEvents(await loadBucketEvents("aw-dlp-review_" + host, 200));
      } catch (error) {
        state.reviews = [];
      }
      renderDlpTableRows(center, host);
      await renderCaseManager(center, host);
      center.querySelector("[data-aw-ru-dlp-message]").textContent = "DLP review центр обновлен.";
    } catch (error) {
      center.querySelector("[data-aw-ru-dlp-message]").textContent = "Ошибка загрузки DLP-событий: " + error.message;
    } finally {
      state.loading = false;
    }
  }

  function injectDlpReviewCenter(root) {
    if (!isDlpSignalBucketRoute()) return;
    const bucketId = getDlpBucketIdFromHash();
    const host = getDlpHostFromBucketId(bucketId);
    if (!host) return;

    const heading = root.querySelector("h3");
    if (!heading) return;

    let center = root.querySelector("[data-aw-ru-dlp-center='1']");
    if (!center) {
      center = document.createElement("section");
      center.className = "aw-ru-dlp-center";
      center.setAttribute("data-aw-ru-dlp-center", "1");
      center.innerHTML =
        '<h4>DLP review и правила</h4>' +
        '<div class="aw-ru-dlp-toolbar">' +
          '<span class="aw-ru-dlp-pill">bucket: ' + escapeHtml(bucketId) + '</span>' +
          '<label><input type="checkbox" data-aw-ru-hide-suppressed /> скрывать события, совпавшие с правилами</label>' +
          '<button type="button" data-aw-ru-refresh-dlp>Обновить DLP</button>' +
          '<div class="aw-ru-dlp-status" data-aw-ru-dlp-status>Событий: 0 · правил: 0 · review: 0</div>' +
        '</div>' +
        '<p>Здесь можно категорировать DLP-события и сохранять suppress/rule записи прямо в AW. Это снижает ложные сработки на уровне review-потока.</p>' +
        '<table class="aw-ru-dlp-table">' +
          '<thead><tr><th>Время</th><th>Тип</th><th>Пользователь</th><th>Документ</th><th>Принтер/канал</th><th>Статус</th><th>Вердикт</th><th>Категория</th><th>Комментарий</th><th>Действия</th></tr></thead>' +
          '<tbody data-aw-ru-dlp-events><tr><td colspan="10">Загрузка...</td></tr></tbody>' +
        '</table>' +
        '<div class="aw-ru-dlp-section">' +
          '<div class="aw-ru-dlp-toolbar">' +
            '<h5>DLP Rules</h5>' +
            '<label><input type="checkbox" data-aw-ru-show-disabled-rules /> показывать отключённые</label>' +
          '</div>' +
          '<table class="aw-ru-dlp-table">' +
            '<thead><tr><th>Время</th><th>Статус</th><th>Действие</th><th>Категория</th><th>Match</th><th>Комментарий</th><th>Управление</th></tr></thead>' +
            '<tbody data-aw-ru-dlp-rules><tr><td colspan="7">Загрузка...</td></tr></tbody>' +
          '</table>' +
        '</div>' +
        '<div class="aw-ru-dlp-section">' +
          '<div class="aw-ru-dlp-toolbar">' +
            '<h5>DLP Review</h5>' +
            '<label><input type="checkbox" data-aw-ru-show-archived-reviews /> показывать архив</label>' +
          '</div>' +
          '<table class="aw-ru-dlp-table">' +
            '<thead><tr><th>Время</th><th>Статус</th><th>Вердикт</th><th>Категория</th><th>Комментарий</th><th>Источник</th><th>Управление</th></tr></thead>' +
            '<tbody data-aw-ru-dlp-reviews><tr><td colspan="7">Загрузка...</td></tr></tbody>' +
          '</table>' +
        '</div>' +
        '<div class="aw-ru-dlp-section">' +
          '<div class="aw-ru-dlp-toolbar">' +
            '<h5>Case Management</h5>' +
            '<div class="aw-ru-dlp-status" data-aw-ru-dlp-cases-status>Кейсов: 0</div>' +
          '</div>' +
          '<table class="aw-ru-dlp-table">' +
            '<thead><tr><th>ID</th><th>Статус</th><th>Severity</th><th>Заголовок</th><th>Исполнитель</th><th>Incident ID</th><th>Обновлено</th></tr></thead>' +
            '<tbody data-aw-ru-dlp-cases><tr><td colspan="7">Загрузка...</td></tr></tbody>' +
          '</table>' +
        '</div>' +
        '<div class="aw-ru-dlp-message" data-aw-ru-dlp-message></div>';
      heading.parentElement.insertBefore(center, heading.nextSibling);
      center.querySelector("[data-aw-ru-refresh-dlp]").addEventListener("click", function () {
        refreshDlpCenter(center, host);
      });
      center.querySelector("[data-aw-ru-hide-suppressed]").addEventListener("change", function () {
        renderDlpTableRows(center, host);
      });
      center.querySelector("[data-aw-ru-show-disabled-rules]").addEventListener("change", function () {
        renderDlpRuleManager(center, host);
      });
      center.querySelector("[data-aw-ru-show-archived-reviews]").addEventListener("change", function () {
        renderDlpReviewManager(center, host);
      });
    }

    if (center.getAttribute("data-aw-ru-loaded") !== "1") {
      center.setAttribute("data-aw-ru-loaded", "1");
      refreshDlpCenter(center, host);
    }
  }

  async function refreshDlpAlertsCenter(center, host) {
    center.querySelector("[data-aw-ru-dlp-alerts-message]").textContent = "Загрузка DLP-инцидентов...";
    try {
      const events = (await loadBucketEvents("aw-dlp-incidents_" + host, 100))
        .sort(function (a, b) { return String(b.timestamp).localeCompare(String(a.timestamp)); });
      const rows = events.map(function (event) {
        const data = event.data || {};
        const incident = data.incident || {};
        const sourceData = data.sourceEvent && data.sourceEvent.data ? data.sourceEvent.data : {};
        return '<tr>' +
          '<td>' + escapeHtml(new Date(event.timestamp).toLocaleString()) + '</td>' +
          '<td>' + escapeHtml(incident.status || "open") + '</td>' +
          '<td>' + escapeHtml(incident.category || "") + '</td>' +
          '<td>' + escapeHtml(incident.comment || "") + '</td>' +
          '<td>' + escapeHtml(sourceData.username || sourceData.owner || "") + '</td>' +
          '<td>' + escapeHtml(sourceData.signalType || "") + '</td>' +
          '<td>' + escapeHtml(sourceData.documentName || sourceData.printerName || "") + '</td>' +
        '</tr>';
      });
      center.querySelector("[data-aw-ru-dlp-alerts-events]").innerHTML = rows.length
        ? rows.join("")
        : '<tr><td colspan="7">Операторских DLP-инцидентов пока нет.</td></tr>';
      center.querySelector("[data-aw-ru-dlp-alerts-status]").textContent = "Инцидентов: " + events.length;
      center.querySelector("[data-aw-ru-dlp-alerts-message]").textContent = "Список DLP-инцидентов обновлен.";
    } catch (error) {
      center.querySelector("[data-aw-ru-dlp-alerts-events]").innerHTML = '<tr><td colspan="7">Не удалось загрузить DLP-инциденты.</td></tr>';
      center.querySelector("[data-aw-ru-dlp-alerts-message]").textContent = "Ошибка загрузки DLP-инцидентов: " + error.message;
    }
  }

  async function refreshPveAuditCenter(center, host) {
    const message = center.querySelector("[data-aw-ru-pve-audit-message]");
    const recentBody = center.querySelector("[data-aw-ru-pve-audit-events]");
    message.textContent = "Загрузка audit-событий...";
    try {
      const [webEvents, taskEvents, sshEvents, cmdEvents] = await Promise.all([
        loadBucketEvents("aw-pve-webadmin-events_" + host, 50).catch(function () { return []; }),
        loadBucketEvents("aw-pve-task-events_" + host, 50).catch(function () { return []; }),
        loadBucketEvents("aw-ssh-sessions_" + host, 50).catch(function () { return []; }),
        loadBucketEvents("aw-console-commands_" + host, 50).catch(function () { return []; })
      ]);
      const data = {
        web: webEvents || [],
        tasks: taskEvents || [],
        ssh: sshEvents || [],
        cmd: cmdEvents || []
      };
      center.querySelector("[data-aw-ru-pve-web-count]").textContent = String(data.web.length);
      center.querySelector("[data-aw-ru-pve-task-count]").textContent = String(data.tasks.length);
      center.querySelector("[data-aw-ru-pve-ssh-count]").textContent = String(data.ssh.length);
      center.querySelector("[data-aw-ru-pve-cmd-count]").textContent = String(data.cmd.length);
      const recent = []
        .concat(data.web.map(function (event) { return { kind: "Web-admin", event: event, text: (event.data && (event.data.method || "") + " " + (event.data.path || "")) || "" }; }))
        .concat(data.tasks.map(function (event) { return { kind: "PVE task", event: event, text: (event.data && ((event.data.action || "") + " " + (event.data.target || ""))) || "" }; }))
        .concat(data.ssh.map(function (event) { return { kind: "SSH", event: event, text: (event.data && ((event.data.event || "") + " " + (event.data.tty || ""))) || "" }; }))
        .concat(data.cmd.slice(0, 25).map(function (event) { return { kind: "Shell", event: event, text: (event.data && (event.data.command || "")) || "" }; }))
        .sort(function (a, b) { return String(b.event && b.event.timestamp || "").localeCompare(String(a.event && a.event.timestamp || "")); })
        .slice(0, 25);
      recentBody.innerHTML = recent.length ? recent.map(function (item) {
        const ev = item.event || {};
        const d = ev.data || {};
        return "<tr>" +
          "<td>" + escapeHtml(new Date(ev.timestamp).toLocaleString()) + "</td>" +
          "<td>" + escapeHtml(item.kind) + "</td>" +
          "<td>" + escapeHtml(d.user || d.username || "-") + "</td>" +
          "<td>" + escapeHtml(d.remote_ip || d.tty || d.host || "-") + "</td>" +
          "<td>" + escapeHtml(item.text) + "</td>" +
        "</tr>";
      }).join("") : '<tr><td colspan="5">Пока нет audit-событий.</td></tr>';
      message.textContent = "Audit-панель обновлена.";
    } catch (error) {
      recentBody.innerHTML = '<tr><td colspan="5">Не удалось загрузить audit-события.</td></tr>';
      message.textContent = "Ошибка загрузки audit-событий: " + error.message;
    }
  }

  function injectPveAuditCenter(root) {
    if (!isPveActivityRoute()) return;
    const host = getCurrentHostFromHash();
    if (!host) return;
    const heading = root.querySelector("h3");
    if (!heading || !heading.parentElement) return;
    let center = root.querySelector("[data-aw-ru-pve-audit='1']");
    if (!center) {
      center = document.createElement("section");
      center.className = "aw-ru-pve-audit";
      center.setAttribute("data-aw-ru-pve-audit", "1");
      center.innerHTML =
        "<h4>PVE Audit</h4>" +
        '<p class="aw-ru-pve-audit-muted">Для Proxmox-хоста показывается audit-панель вместо desktop-виджетов ActivityWatch, так как у этого хоста нет window/afk watcher данных.</p>' +
        '<div class="aw-ru-pve-audit-grid">' +
          '<div class="aw-ru-pve-audit-card"><h5>Web-admin</h5><div class="aw-ru-pve-audit-value" data-aw-ru-pve-web-count>0</div></div>' +
          '<div class="aw-ru-pve-audit-card"><h5>PVE tasks</h5><div class="aw-ru-pve-audit-value" data-aw-ru-pve-task-count>0</div></div>' +
          '<div class="aw-ru-pve-audit-card"><h5>SSH events</h5><div class="aw-ru-pve-audit-value" data-aw-ru-pve-ssh-count>0</div></div>' +
          '<div class="aw-ru-pve-audit-card"><h5>Shell commands</h5><div class="aw-ru-pve-audit-value" data-aw-ru-pve-cmd-count>0</div></div>' +
        "</div>" +
        '<table class="aw-ru-pve-audit-table">' +
          "<thead><tr><th>Время</th><th>Тип</th><th>Пользователь</th><th>Источник</th><th>Детали</th></tr></thead>" +
          '<tbody data-aw-ru-pve-audit-events><tr><td colspan="5">Загрузка...</td></tr></tbody>' +
        "</table>" +
        '<div class="aw-ru-dlp-message" data-aw-ru-pve-audit-message></div>';
      heading.parentElement.insertBefore(center, heading.nextSibling);
    }
    Array.from(heading.parentElement.children).forEach(function (child) {
      if (child === heading || child === center) return;
      child.style.display = "none";
    });
    const routeKey = host + "|" + (window.location.hash || "");
    if (center.getAttribute("data-aw-ru-pve-route") !== routeKey) {
      center.setAttribute("data-aw-ru-pve-route", routeKey);
      refreshPveAuditCenter(center, host);
    }
  }

  function injectDlpAlertsCenter(root) {
    if (!isAlertsRoute()) return;
    const host = window.__awRuPatchSettingsHost || getCurrentHostFromHash();
    if (!host) return;

    const heading = root.querySelector("h3");
    if (!heading) return;

    let center = root.querySelector("[data-aw-ru-dlp-alerts='1']");
    if (!center) {
      center = document.createElement("section");
      center.className = "aw-ru-dlp-center";
      center.setAttribute("data-aw-ru-dlp-alerts", "1");
      center.innerHTML =
        '<h4>DLP-инциденты оператора</h4>' +
        '<div class="aw-ru-dlp-toolbar">' +
          '<span class="aw-ru-dlp-pill">bucket: ' + escapeHtml("aw-dlp-incidents_" + host) + '</span>' +
          '<button type="button" data-aw-ru-refresh-dlp-alerts>Обновить инциденты</button>' +
          '<div class="aw-ru-dlp-status" data-aw-ru-dlp-alerts-status>Инцидентов: 0</div>' +
        '</div>' +
        '<p>Здесь выводятся DLP-события, которые оператор вручную признал инцидентами через review-центр.</p>' +
        '<table class="aw-ru-dlp-table">' +
          '<thead><tr><th>Время</th><th>Статус</th><th>Категория</th><th>Комментарий</th><th>Пользователь</th><th>Тип</th><th>Документ/канал</th></tr></thead>' +
          '<tbody data-aw-ru-dlp-alerts-events><tr><td colspan="7">Загрузка...</td></tr></tbody>' +
        '</table>' +
        '<div class="aw-ru-dlp-message" data-aw-ru-dlp-alerts-message></div>';
      heading.parentElement.insertBefore(center, heading.nextSibling);
      center.querySelector("[data-aw-ru-refresh-dlp-alerts]").addEventListener("click", function () {
        refreshDlpAlertsCenter(center, host);
      });
    }

    if (center.getAttribute("data-aw-ru-loaded") !== "1") {
      center.setAttribute("data-aw-ru-loaded", "1");
      refreshDlpAlertsCenter(center, host);
    }
  }

  let trendsRedirectInFlight = false;
  let settingsHostFetchInFlight = false;
  let applyPatchScheduled = false;
  let networkPatchesInstalled = false;

  function getTrendsHostFromSettings(settings) {
    if (!settings || typeof settings !== "object") return "";
    const landingpage = typeof settings.landingpage === "string" ? settings.landingpage : "";
    const match = landingpage.match(/\/activity\/([^/]+)/);
    const host = match && match[1] ? decodeURIComponent(match[1]) : "";
    return isLikelyClientHost(host) ? host : "";
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
        window.__awRuPatchSettingsHost = getDlpHostFromSettings(settings);
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

  function ensureSettingsHost() {
    if (window.__awRuPatchSettingsHost || settingsHostFetchInFlight) return;
    settingsHostFetchInFlight = true;
    fetch("/api/0/settings/", { credentials: "same-origin" })
      .then(function (response) {
        if (!response.ok) throw new Error("settings-fetch-failed");
        return response.json();
      })
      .then(function (settings) {
        window.__awRuPatchSettingsHost = getDlpHostFromSettings(settings);
      })
      .catch(function () {})
      .finally(function () {
        settingsHostFetchInFlight = false;
        injectDlpNavigation(document.body);
      });
  }

  function getPreferredWindowHostFromBuckets() {
    const state = getHostGroupsState();
    const rawBuckets = state && state.buckets ? state.buckets : {};
    const settingsHost = normalizeText(window.__awRuPatchSettingsHost || "");
    const bucketIds = Array.isArray(rawBuckets)
      ? rawBuckets.map(function (item) { return item && item.id ? String(item.id) : ""; })
      : Object.keys(rawBuckets || {});
    const hosts = bucketIds
      .filter(function (bucketId) { return /^aw-watcher-window_/i.test(bucketId); })
      .map(function (bucketId) { return bucketId.replace(/^aw-watcher-window_/i, ""); })
      .filter(Boolean)
      .filter(function (host) { return !/^unknown$/i.test(host); });
    if (isLikelyClientHost(settingsHost) && hosts.indexOf(settingsHost) >= 0) return settingsHost;
    hosts.sort();
    return hosts[0] || "";
  }

  function rewriteUnknownCategoryBuilderQueryBody(body) {
    if (typeof body !== "string") return body;
    function stripUnknownBucketQueries(raw) {
      return raw
        .replace(/flood\(query_bucket\(find_bucket\(\\"aw-watcher-window_unknown\\"\)\)\)/g, '[]')
        .replace(/flood\(query_bucket\(find_bucket\(\\"aw-watcher-afk_unknown\\"\)\)\)/g, '[]')
        .replace(/query_bucket\(find_bucket\(\\"aw-watcher-window_unknown\\"\)\)/g, '[]')
        .replace(/query_bucket\(find_bucket\(\\"aw-watcher-afk_unknown\\"\)\)/g, '[]')
        .replace(/flood\(query_bucket\(\\"aw-watcher-window_unknown\\"\)\)/g, '[]')
        .replace(/flood\(query_bucket\(\\"aw-watcher-afk_unknown\\"\)\)/g, '[]')
        .replace(/query_bucket\(\\"aw-watcher-window_unknown\\"\)/g, '[]')
        .replace(/query_bucket\(\\"aw-watcher-afk_unknown\\"\)/g, '[]');
    }
    if (body.indexOf("undefined") !== -1) {
      body = body
        .replace(/flood\(query_bucket\(find_bucket\(\\"undefined\\"\)\)\)/g, '[]')
        .replace(/query_bucket\(find_bucket\(\\"undefined\\"\)\)/g, '[]')
        .replace(/flood\(query_bucket\(\\"undefined\\"\)\)/g, '[]')
        .replace(/query_bucket\(\\"undefined\\"\)/g, '[]');
      const ph = getPreferredWindowHostFromBuckets();
      if (ph) {
        body = body
          .replace(/aw-watcher-window_undefined/g, "aw-watcher-window_" + ph)
          .replace(/aw-watcher-afk_undefined/g, "aw-watcher-afk_" + ph);
      }
    }
    if (body.indexOf("aw-watcher-window_unknown") !== -1 || body.indexOf("aw-watcher-afk_unknown") !== -1) {
      const preferredHost = getPreferredWindowHostFromBuckets();
      if (preferredHost) {
        body = body
          .replace(/aw-watcher-window_unknown/g, "aw-watcher-window_" + preferredHost)
          .replace(/aw-watcher-afk_unknown/g, "aw-watcher-afk_" + preferredHost);
      } else {
        body = stripUnknownBucketQueries(body);
      }
    }
    return body;
  }

  function installCategoryBuilderNetworkPatch() {
    if (networkPatchesInstalled) return;
    networkPatchesInstalled = true;

    const originalFetch = window.fetch ? window.fetch.bind(window) : null;
    if (originalFetch) {
      window.fetch = function (input, init) {
        try {
          const url = typeof input === "string" ? input : String(input && input.url || "");
          if (/\/api\/0\/query\/?$/i.test(url) && init && typeof init.body === "string") {
            init = Object.assign({}, init, {
              body: rewriteUnknownCategoryBuilderQueryBody(init.body)
            });
          }
        } catch (error) {
        }
        return originalFetch(input, init);
      };
    }

    if (window.XMLHttpRequest && window.XMLHttpRequest.prototype) {
      const proto = window.XMLHttpRequest.prototype;
      if (!proto.__awRuCategoryBuilderPatched) {
        const originalOpen = proto.open;
        const originalSend = proto.send;
        proto.open = function (method, url) {
          this.__awRuMethod = method;
          this.__awRuUrl = url;
          return originalOpen.apply(this, arguments);
        };
        proto.send = function (body) {
          try {
            const url = String(this.__awRuUrl || "");
            if (/\/api\/0\/query\/?$/i.test(url) && typeof body === "string") {
              body = rewriteUnknownCategoryBuilderQueryBody(body);
            }
          } catch (error) {
          }
          return originalSend.call(this, body);
        };
        proto.__awRuCategoryBuilderPatched = true;
      }
    }
  }

  function patchCategoryBuilderHostLabel(root) {
    if (!/^#\/settings\/category-builder(?:[/?#]|$)/i.test(window.location.hash || "")) return;
    const preferredHost = getPreferredWindowHostFromBuckets();
    if (!preferredHost) return;
    Array.from(root.querySelectorAll("*")).forEach(function (element) {
      if (element.children.length) return;
      const text = element.textContent || "";
      if (!/Имя хоста:\s*(unknown|неизвестно)\b|Hostname:\s*unknown\b/i.test(text)) return;
      const next = text
        .replace(/Имя хоста:\s*(unknown|неизвестно)\b/i, "Имя хоста: " + preferredHost)
        .replace(/Hostname:\s*unknown\b/i, "Hostname: " + preferredHost);
      if (next !== text) {
        element.textContent = next;
      }
    });
  }

  function patchActivityHeading(root) {
    const heading = root.querySelector("h3");
    if (!heading) return;
    const inlineParts = heading.querySelectorAll("span");
    inlineParts.forEach(function (element) {
      const text = (element.textContent || "").trim();
      if (text === "for") {
        element.textContent = "за ";
      }
    });
  }

  function applyPatch() {
    enforceSafeActivityViewForPveHost();
    ensureSettingsHost();
    ensureHostGroupsData().catch(function () {});
    installCategoryBuilderNetworkPatch();
    injectStyles();
    walk(document.body);
    translateAttributes(document.body);
    hideNoiseNavigation(document.body);
    patchActivityHeading(document.body);
    patchCategoryBuilderHostLabel(document.body);
    injectPveAuditCenter(document.body);
    injectDlpNavigation(document.body);
    injectDlpReviewCenter(document.body);
    injectDlpAlertsCenter(document.body);
    injectHostGroupsCenter(document.body).catch(function () {});
    redirectBareTrendsRoute();
  }

  function scheduleApplyPatch() {
    if (applyPatchScheduled) return;
    applyPatchScheduled = true;
    window.setTimeout(function () {
      applyPatchScheduled = false;
      applyPatch();
    }, 50);
  }

  const observer = new MutationObserver(function () {
    scheduleApplyPatch();
  });

  window.addEventListener("load", function () {
    applyPatch();
    observer.observe(document.body, { childList: true, subtree: true });
  });
  window.addEventListener("hashchange", function () {
    redirectBareTrendsRoute();
    scheduleApplyPatch();
  });
})();
