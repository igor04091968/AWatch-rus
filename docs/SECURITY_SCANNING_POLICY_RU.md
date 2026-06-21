# Политика public secret scanning

Этот документ описывает публичную проверку репозитория на очевидные секреты.
Проверка нужна для инженерной прозрачности и снижения риска случайной
публикации токенов, паролей, cookies, private keys и похожих значений.

GitHub Actions используется как public mirror validation only. Основной
registry release evidence должен формироваться в российском build-контуре, а
не в GitHub Actions.

## Принцип проверки

- Scanner работает fail-closed: при подозрении на committed secret workflow
  должен завершаться ошибкой.
- Scanner выводит только `file:line:rule` и не печатает найденное значение.
- Реальные секреты, токены, пароли, cookies, API keys и private keys нельзя
  хранить в репозитории.
- Runtime-секреты должны передаваться через environment variables, защищенные
  файлы вне репозитория или внешний secret storage.
- Документационные примеры должны использовать `<SET_VIA_ENV>`, `<REDACTED>`,
  `example`, `dummy` или `redacted`.

## Тестовые значения

Для unit tests и fixtures допустимы только короткие безопасные значения:

- `dummy`
- `test`
- `example`
- `redacted`
- `secret`, если тест проверяет именно parsing поля и значение короткое

Не использовать длинные base64, hex, JWT-like или token-like строки даже в
тестах. Такие строки выглядят как настоящий secret и должны заменяться на
короткий dummy.

## Inline allow comments

Если строка безопасна, но scanner не может корректно определить контекст,
разрешен точечный inline allow comment:

```text
# public-secret-scan: allow dummy
```

```text
// public-secret-scan: allow dummy
```

Allow comment разрешен только для dummy/test fixtures, безопасных placeholder
values или runtime-derived значений, где секрет не хранится в репозитории.
Нельзя использовать allow comment для реального токена, пароля, cookie, private
key или customer evidence.

## Локальный запуск

```bash
python3 scripts/public_secret_pattern_check.py
```

Ожидаемый успешный результат:

```text
secret_pattern_check=ok
```

## Что делать при срабатывании

1. Проверить строку вручную.
2. Если значение настоящее, удалить его из истории рабочего изменения и
   заменить на env/config reference.
3. Если значение тестовое, заменить на короткий dummy.
4. Если это безопасный placeholder или runtime-derived value, переписать строку
   так, чтобы она не выглядела как секрет, либо добавить точечный inline allow
   comment.
5. Повторить локальный запуск scanner и registry readiness check.
