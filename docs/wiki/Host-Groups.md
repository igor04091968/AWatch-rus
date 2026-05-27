# Host Groups

Группы хостов в `AWatch-rus` нужны для того, чтобы WebUI показывал не плоский список bucket'ов, а осмысленные operational сегменты.

Основной конфиг:

- `aw-server/aw-host-groups.json`

## Для чего это нужно

Host groups помогают:

- разделять Windows/RDP, Linux workers и инфраструктурные узлы
- показывать разные полезные ссылки для разных типов хостов
- не смешивать пользовательскую активность с инфраструктурным шумом
- сделать WebUI пригодным для DetMir operational use

## Структура конфига

Каждая группа содержит:

- `id`
- `name`
- `description`
- `patterns`
- `links`

`patterns` — это regex-маски по hostname.  
`links` — это действия и переходы, которые WebUI показывает для группы.

## Что уже используется в DetMir

В текущем конфиге есть группы:

- `pve-detmir`
- `windows-rdp`
- `linux-remote`
- `virtual-infra`

Примеры полезных ссылок внутри групп:

- `Активность`
- `DLP`
- `SSH сессии`
- `Команды shell`
- `Web категории`
- `Все бакеты`

## Где применяется

Этот контур используется в русифицированном WebUI и в DetMir host grouping path.

Связанные файлы:

- `aw-server/apply_webui_ru_patch.sh`
- `docs/wiki/WebUI-Russian-Patches.md`

## Канонические документы

- [WebUI Русификация](WebUI-Russian-Patches)
- [Конфиг групп хостов](../../aw-server/aw-host-groups.json)
