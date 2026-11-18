# Архитектура

Dev Hangar — приложение Tauri 2: Rust-процесс делает всю «тяжёлую» и системную работу,
Vue 3 + TypeScript в WebView2 рисует интерфейс и хранит состояние.

```
Vue (src/)  ──invoke──►  Rust-команды (src-tauri/src/lib.rs)  ──►  модули: detect, git, process, …
     ▲                                   │
     └──────── события proc://output ────┘   (вывод запущенных процессов)
```

## Бэкенд — `src-tauri/src/`

`lib.rs` регистрирует все `#[tauri::command]`; долгие операции выполняются через `spawn_blocking`
(хелпер `blocking`), чтобы не блокировать асинхронный рантайм. Ошибки возвращаются строками (`Result<T, String>`).

| Модуль | Что делает |
|---|---|
| `detect.rs` | Анализ проекта: стек(и), модули монорепы, команды (dev/build/test/…), зависимости, статистика |
| `php.rs` | PHP: composer, фреймворки, их команды |
| `git.rs` / `gitops.rs` | Чтение git (summary, log, diff, файл на коммите) / изменяющие операции (`git_op`) |
| `checkpoints.rs` | Снимки рабочей копии как скрытые коммиты в `refs/devhangar/checkpoints/*` |
| `process.rs` | Запуск команд через shell, стриминг stdout/stderr событием `proc://output`, stdin, остановка дерева (`taskkill` на Windows, группа процессов на Unix), остановка всего при выходе |
| `files.rs` | Дерево каталогов, поиск по именам и тексту, чтение файлов |
| `extra.rs` | Скан TODO/FIXME/HACK/XXX/BUG, дерево для ИИ-контекста, запись `CLAUDE.md`/`AGENTS.md`, уборка артефактов сборки |
| `analytics.rs` | Аналитика по git-истории и строкам кода |
| `graph.rs` | Граф импортов (JS/TS/Vue/Svelte/Astro, CSS, HTML, Python, Rust, Go, PHP; Tauri `invoke` → Rust-команда) |
| `covers.rs` | Обложки сетки: скриншот dev-сервера headless Edge/Chrome или своя картинка, в `<config>/covers/` |
| `plugins.rs` | Только файлы плагинов: список, чтение, установка/удаление, шаблон, встроенные плагины, `exec_capture`, запись текста |
| `util.rs` | `command` (без мигающей консоли на Windows), `shell` (`cmd /C` с `chcp 65001` / `sh -c`), кэш поиска в PATH |

Также в `lib.rs`: открытие в IDE (VS Code-подобные получают `-g file:line`), открытие терминала
(Windows Terminal, PowerShell, git-bash, cmd), поиск установленных редакторов, загрузка/сохранение конфига.

## Фронтенд — `src/`

- `lib/api.ts` — обёртки над `invoke`.
- `lib/store.ts` — реактивное состояние приложения: проекты, конфиг, процессы и консоль, обложки, чекпоинты.
- `components/` — экраны и вкладки проекта (Overview, Files, Git, Graph, Analytics, Deps, Console, Passport…),
  сетка (`GridView.vue`), сайдбар, экран плагинов.
- `lib/passport.ts` — реестр форматов паспорта, загрузка паспорта проекта, кнопки у задач от плагинов.
  `lib/passport-format.ts` — встроенный формат `.hangar/` (парсинг и правки markdown).
- `lib/aicontext.ts` — сборка markdown-контекста для ИИ.
- `plugins/host.ts` — загрузка плагинов и объект `hangar` для каждого; `plugins/hangar.d.ts` — типы Plugin API
  (единый источник правды, копируется и в шаблон плагина); `plugins/bus.ts` — шина событий без импортов.

## Плагины

Плагин — папка `<config dir>/plugins/<id>/` с `plugin.json` и `main.js`, экспортирующим `activate(hangar)`.
Хост читает код через Rust, грузит его как ES-модуль из blob URL (`import()`), опционально подключает `style`.
Разрешения из `plugin.json` ограничивают доступ к `hangar.*` (например, `shell`, `fs:write`).
Встроенные плагины (Claude Code) поставляются внутри приложения и ставятся одной кнопкой.

## Паспорт проекта

Форматы опрашиваются по порядку: сначала зарегистрированные плагинами, последним — встроенный `.hangar/`.
Первый, чей `detect` вернул истину, читает паспорт. Все пути доступа ограничены папкой проекта;
запись разрешена встроенному формату и плагинам с `fs:write`. Формат описан в `docs/PASSPORT.md`.

## Данные пользователя

- Конфиг: `%APPDATA%\com.bolat.devhangar\config.json` (запись через временный файл + rename).
- Обложки: `<config dir>/covers/`. Плагины: `<config dir>/plugins/`.
- Чекпоинты и паспорт — внутри самих проектов (git-refs и `.hangar/`).
