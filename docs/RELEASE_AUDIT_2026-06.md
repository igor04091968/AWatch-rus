# Release audit 2026-06

Дата проверки: `2026-06-03 09:21:17 MSK`

Проверенный commit:

```text
a1801fca5f0502c9745baedf027fafc0b96428e3
```

Цель аудита: подтвердить, что публичный release-состав AWatch-rus не
содержит приватные hostnames, адреса личного стенда, публичный домен
правообладателя, локальные пути оператора и реальные секреты.

## 1. Область проверки

Release-аудит выполнялся по tracked-файлам Git. Это корректная область для
GitHub/release экспертизы: `.git`, build output, ignored локальные артефакты,
рабочие `.ops/`, `.playwright-cli/`, unpacked install-kit и локальные AI-логи
не входят в публичную поставку.

Запрошенный оператором общий шаблон:

```bash
grep -R "SHARKON2025\|10.10.10\|dm.iri\|/home/igor\|/root\|password\|token\|secret" .
```

Для release-аудита использован воспроизводимый эквивалент по tracked-файлам:

```bash
git grep -n -E 'SHARKON2025|10\.10\.10|dm\.iri|/home/igor|/root|password|token|secret' -- \
  ':!*.zip' ':!*.tar.gz' ':!adk-rust/target/**'
```

Причина: голый `grep -R .` также читает `.git`, ignored локальные каталоги и
сам audit-документ, поэтому он не является корректным критерием публичного
release-состава.

## 2. Приватные маркеры стенда

Команда:

```bash
git grep -n -E 'SHARKON2025|10\.10\.10|dm\.iri|/home/igor|/root' -- \
  ':!*.zip' ':!*.tar.gz' ':!adk-rust/target/**'
```

Фактический вывод:

```text
adk-rust/crates/verify-innosetup-installer/src/main.rs:260:        let root = std::path::Path::new("/tmp/root");
docs/ARCHITECTURE_RU.md:116:- canonical path/root allowlist;
docs/THREAT_MODEL_RU.md:168:- canonical path/root allowlist;
docs/THREAT_MODEL_RU.md:223:| T05 | Прямая выдача файлов по path traversal | Canonical path/root allowlist, no raw path route. |
proxmox/tsj_guardian_bot.py:960:            base_pat += r"|lxc-usernsexec.*(/var/lib/lxc/" + guest_pat + r"/rootfs|/run/lxc/)"
```

Результат:

- `SHARKON2025`: не найдено;
- `10.10.10`: не найдено;
- `dm.iri`: не найдено;
- `/home/igor`: не найдено;
- `/root` как приватный путь оператора: не найдено;
- оставшиеся совпадения являются ложноположительными:
  `path/root`, `/tmp/root`, LXC `rootfs`.

Вывод: приватных данных стенда в tracked release-составе не найдено.

## 3. Слова password/token/secret

Команда:

```bash
git grep -n -E 'password|token|secret' -- \
  ':!*.zip' ':!*.tar.gz' ':!adk-rust/target/**' | wc -l
```

Фактический вывод:

```text
396
```

Это ожидаемые технические совпадения. Они относятся к:

- названиям переменных CLI/API (`password`, `token`);
- Ansible lookup из окружения;
- `CHANGE_ME`, `replace-me`, `<PASSWORD>` и другим placeholder-значениям;
- документации по безопасному хранению секретов;
- тестам, проверяющим, что секреты не печатаются;
- DLP/regex terminology (`secret` как тип контролируемых данных).

Примеры допустимых совпадений:

```text
ansible/group_vars/aw_server.yml:4:ansible_password: "{{ lookup('env', 'AW_SSH_PASSWORD') }}"
ansible/group_vars/aw_windows.yml:4:ansible_password: "{{ lookup('env', 'AW_WINRM_PASSWORD') }}"
ansible/group_vars/proxmox-bot.example.yml:1:telegram_bot_token: "CHANGE_ME"
ansible/group_vars/pfsense-poller.example.yml:17:      api_secret: "<SET_VIA_ENV>"
docs/INSTALL_RU.md:66:- Telegram bot token;
docs/INSTALL_RU.md:67:- evidence upload token.
adk-rust/crates/detmir-portal/src/main.rs:1756:fn bearer_token(request: &Request) -> Option<String> {
```

Вывод: сами слова `password`, `token`, `secret` присутствуют в коде и
документации по назначению, но реальные значения секретов не обнаружены.

## 4. Проверка высокосигнальных секретов

Команда:

```bash
git grep -n -E 'sk_[A-Za-z0-9]{12,}|pk_[A-Za-z0-9]{12,}|BEGIN (RSA |OPENSSH |EC |DSA )?PRIVATE KEY|ghp_[A-Za-z0-9]{20,}|xox[baprs]-[A-Za-z0-9-]{20,}|[0-9]{8,10}:[A-Za-z0-9_-]{30,}' -- \
  ':!*.zip' ':!*.tar.gz' ':!adk-rust/target/**'
```

Фактический вывод:

```text
docs/SBOM_RELEASE_CHECKLIST_RU.md:65:git grep -n -E 'sk_[A-Za-z0-9]|pk_[A-Za-z0-9]|BEGIN OPENSSH PRIVATE KEY|password\\s*=' -- . || true
```

Результат: единственное совпадение находится в checklist-документе и является
самой командой проверки. Реальные API keys, GitHub tokens, Telegram bot tokens
и private keys не найдены.

Контрольная проверка без audit/checklist-документов:

```text
HIGH_SIGNAL_SECRETS_EXCLUDING_AUDIT=0
```

## 5. Проверка tracked env/inventory/secrets

Команда:

```bash
git ls-files | grep -E '(^|/)secrets(/|$)|\.env$|inventory\.ini$' || true
```

Фактический вывод:

```text
proxmox/ct-vars.example.env
```

Результат: найден только `*.example.env` шаблон. Он не является приватной
конфигурацией и не содержит реальных секретов.

## 6. Проверка assignment-паттернов

Команда:

```bash
git grep -n -E "(password|token|secret)[[:space:]]*[:=][[:space:]]*['\"][^'\"]{8,}['\"]" -- \
  ':!*.zip' ':!*.tar.gz' ':!adk-rust/target/**'
```

Результат: найдены только шаблоны, env lookups, generated runtime password
logic и `CHANGE_ME`/`replace-me`. Реальные статические пароли или токены не
обнаружены.

Характерные безопасные формы:

```text
ansible/group_vars/aw_server.yml: ansible_password берется из AW_SSH_PASSWORD
ansible/group_vars/aw_windows.yml: ansible_password берется из AW_WINRM_PASSWORD
ansible/group_vars/proxmox-bot.example.yml: telegram_bot_token: "CHANGE_ME"
ansible/group_vars/pfsense-poller.example.yml: api_secret: "<SET_VIA_ENV>"
ansible/deploy_proxmox_web_gateway.yml: password генерируется через openssl rand
```

## 7. Итог

Статус audit gate: `PASS`.

Вывод для эксперта:

- приватные hostnames, реальные адреса личного стенда, личный публичный домен,
  `/home/igor` и приватные `/root/...` пути в tracked release-составе не
  найдены;
- совпадения `password/token/secret` являются ожидаемыми техническими
  терминами, placeholders или env lookups;
- реальных секретов, private keys и API tokens не найдено;
- `secrets/`, локальные `.env`, приватный `inventory.ini` и runtime artifacts
  не входят в tracked Git-состав.
