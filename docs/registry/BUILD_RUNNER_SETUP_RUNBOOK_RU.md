# Build-runner setup runbook

Статус: registry-readiness runbook. Команды предназначены для нового
российского build-runner `awatch-build-01`. Runbook не меняет product runtime,
API, UI или deployment behavior.

## Базовая подготовка

```bash
hostnamectl set-hostname awatch-build-01
apt update
apt upgrade
```

## Базовые пакеты

```bash
apt install -y \
  curl \
  wget \
  git \
  jq \
  ca-certificates \
  gnupg \
  build-essential \
  pkg-config \
  libssl-dev \
  clang \
  cmake \
  protobuf-compiler \
  nodejs \
  npm \
  python3 \
  unzip \
  tar \
  rsync
```

`protobuf-compiler`, `nodejs` и `npm` нужны только если соответствующие
components/checks используются в текущем release candidate. Если package policy
организации требует другой способ установки, использовать корпоративный
approved mirror/toolchain и зафиксировать это в release evidence.

## Rust toolchain

Вариант через rustup допустим только если он разрешен политикой владельца
инфраструктуры:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/rustup-init.sh
sh /tmp/rustup-init.sh
```

Альтернатива: установить Rust toolchain из корпоративного mirror/package
repository и зафиксировать источник установки в release evidence. Скрипты
репозитория не должны выполнять `curl | sh` и не должны сами устанавливать
toolchain.

## Проверка инструментов

```bash
rustc --version
cargo --version
git --version
node --version
npm --version
jq --version
```

Если Node.js/npm не устанавливались, зафиксировать это как `skipped: tool not
installed` для соответствующих smoke checks.

## Пользователь build

```bash
useradd --create-home --shell /bin/bash build
usermod -aG sudo build
```

Доступ должен быть key-based SSH. Пароли, токены, приватные ключи и cookies не
хранить в репозитории.

## Безопасная модель доступа

- SSH key based access.
- No secrets in repo.
- GitHub tokens, Gitea tokens и SSH private keys только в environment variables
  или protected files outside repo.
- Protected files должны иметь ограниченные permissions и не попадать в
  source archive.
- Build scripts не должны менять remotes, git history, tags или выполнять
  auto-push.

## Подключение к Gitea

HTTPS clone:

```bash
git clone https://git.iri1968.dpdns.org/awatch-rus/AWatch-rus.git
```

SSH clone может быть добавлен отдельным шагом после настройки ключей Gitea.
SSH private key не хранить в репозитории.

## Первичная проверка репозитория

```bash
cd AWatch-rus
git remote -v
bash scripts/registry_readiness_check.sh
```

GitHub используется как public mirror only и не является primary registry
source/build/release contour.
