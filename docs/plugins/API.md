# Справочник Plugin API (v1)

Каждый плагин получает в `activate(hangar)` собственный объект `hangar`. Точные типы описаны в
[`src/plugins/hangar.d.ts`](../../src/plugins/hangar.d.ts). Руководство с примерами лежит в [README.md](README.md).

Методы, которые требуют разрешения, помечены 🔒 с его названием.

---

## Общее

| | |
|---|---|
| `hangar.apiVersion` | Версия Plugin API хоста (`1`). |
| `hangar.appVersion` | Версия Dev Hangar, например `"0.7.0"`. |
| `hangar.onDispose(fn)` | Вызвать `fn`, когда плагин выключат, перезагрузят или удалят. |

`register(...)` и `events.on(...)` возвращают `Disposable { dispose() }`, через который регистрацию
можно снять раньше. При выгрузке плагина Hangar снимает их сам.

### ProjectContext

Короткое описание проекта. Передаётся во вкладки, команды и `when` и не требует разрешений.

```ts
{ path: string; name: string; stacks: string[]; isGit: boolean }
```

`stacks` — это id стеков: `node`, `bun`, `ts`, `vue`, `nuxt`, `react`, `next`, `svelte`, `astro`, `vite`,
`tauri`, `electron`, `rust`, `go`, `python`, `django`, `php`, `laravel`, `symfony`, `dotnet`, `java`,
`flutter`, `docker` и другие. Полный список смотрите в `src-tauri/src/detect.rs`.

---

## hangar.plugin

| | |
|---|---|
| `id` | id плагина. |
| `manifest` | Содержимое `plugin.json` (копия). |
| `dir` | Абсолютный путь к папке плагина. |
| `readFile(rel): Promise<string>` | Прочитать текстовый файл из папки плагина. Выйти за пределы папки нельзя. |
| `assetUrl(rel): Promise<string>` | URL файла плагина для `<img src>`, `<audio>`, `background-image`. |

## hangar.stacks

### `register(stack): Disposable`

| Поле | Тип | |
|---|---|---|
| `id` | `string` | id стека (`a-z0-9._-`). Попадает в `ProjectContext.stacks`. |
| `label` | `string` | Подпись бейджа. |
| `color?` | `string` | Цвет точки на бейдже. |
| `subdirs?` | `boolean` | Искать стек и в подпапках монорепы. По умолчанию `true`. |
| `detect` | `(ctx) => boolean \| Promise<boolean>` | Является ли папка проектом этого стека. |
| `actions?` | `(ctx) => StackAction[] \| Promise<…>` | Команды проекта. |
| `dependencies?` | `(ctx) => StackDependency[] \| Promise<…>` | Зависимости для вкладки «Зависимости». |

### `refresh()`

Заново применяет стеки всех плагинов ко всем проектам. Нужен, если результат `detect` или `actions`
зависит от настроек.

**StackDetectContext:**

| | |
|---|---|
| `projectPath` | Корень проекта. |
| `relPath` | Проверяемая папка относительно корня (`""` — корень). |
| `dir` | Абсолютный путь проверяемой папки. |
| `isRoot` | `relPath === ""`. |
| `stacks` | id стеков, которые Hangar уже нашёл в этой папке. |
| `has(rel): Promise<boolean>` | Есть ли файл или папка. |
| `list(): Promise<DirEntry[]>` | Содержимое папки (кэшируется). |
| `readText(rel): Promise<string \| null>` | Текст файла или `null`. |
| `readJson(rel): Promise<any \| null>` | JSON-файл или `null`. |

Пути считаются относительно `dir`; `..` и абсолютные пути запрещены.

**StackAction:** `{ id, label, command, kind?, cwd?, primary? }`

- `kind` принимает значения `dev`, `build`, `test`, `install`, `lint`, `serve`, `open` или `script` (по умолчанию).
  `dev` и `build` попадают на кнопки Dev и Build. `open` открывает файл `command` системным приложением.
- `cwd` задаётся относительно проверяемой папки.
- `primary: true` ставит команду первой, и тогда именно она оказывается на кнопке Dev или Build.

**StackDependency:** `{ name, version?, dev? }`

## hangar.pages

### `register(page): Disposable`

| Поле | Тип | |
|---|---|---|
| `id` | `string` | Уникален в пределах плагина. |
| `label` | `string` | Подпись в боковой панели. |
| `icon?` | `string` | Иконка Hangar. По умолчанию берётся `icon` плагина. |
| `mount` | `(el, { signal }) => void \| cleanup \| Promise<…>` | Отрисовать экран в `el`. |

## hangar.tabs

### `register(tab): Disposable`

| Поле | Тип | |
|---|---|---|
| `id` | `string` | Уникален в пределах плагина. |
| `label` | `string` | Подпись вкладки. |
| `icon?` | `string` | Иконка Hangar. По умолчанию берётся `icon` плагина. |
| `when?` | `(p: ProjectContext) => boolean` | Показывать вкладку только для подходящих проектов. |
| `mount` | `(el, { project, signal }) => void \| cleanup \| Promise<…>` | Отрисовать вкладку в `el`. Можно вернуть функцию очистки. |

`mount` вызывается при каждом открытии вкладки и при переключении на другой проект.
`signal` срабатывает, когда вкладку закрыли.

## hangar.actions

### `register(action): Disposable`

| Поле | Тип | |
|---|---|---|
| `id` | `string` | Уникален в пределах плагина. |
| `label` | `string` | Текст кнопки или пункта меню. |
| `icon?` | `string` | Иконка (для кнопки в шапке). |
| `toolbar?` | `boolean` | `true` — кнопка в шапке проекта, иначе пункт меню ▾ «Все команды». |
| `title?` | `string` | Подсказка при наведении. |
| `when?` | `(p: ProjectContext) => boolean` | Показывать команду только для подходящих проектов. |
| `run` | `(p: ProjectContext) => void \| Promise<void>` | Обработчик. Ошибки показываются во всплывающем уведомлении. |

## hangar.events <a id="events"></a>

### `on(event, handler): Disposable`

| Событие | Данные |
|---|---|
| `project:open` | `ProjectContext`, когда пользователь открыл проект |
| `process:start` | `{ projectPath, label, command, kind }` |
| `process:exit` | `{ projectPath, label, command, kind, code, durationMs }` |
| `theme:change` | `{ dark: boolean }` |
| `passport:change` | `{ projectPath, passport }` — паспорт проекта перечитан *(с 0.9.0)* |

`kind` у процессов принимает значения `dev`, `build`, `install`, `test`, `script`, `plugin` и т. д.

## hangar.storage

Хранилище плагина: значения сохраняются в `config.json` и удаляются вместе с плагином.

| | |
|---|---|
| `get(key, fallback?)` | Вернуть значение (копию) или `fallback`. |
| `set(key, value)` | Сохранить значение (должно сериализоваться в JSON). |
| `delete(key)` | Удалить значение. |
| `keys()` | Список ключей. |

## hangar.settings

Значения настроек из `manifest.settings`, которые пользователь задаёт на экране «Плагины».

| | |
|---|---|
| `get(key)` | Значение пользователя или `default`. |
| `all()` | Все настройки объектом. |
| `onChange((key, value) => …)` | Подписка на изменения. |

Типы полей: `string`, `number`, `boolean`, `select` (с `options`), `text` (многострочный).

## hangar.ui

| | |
|---|---|
| `toast(text, kind?)` | Всплывающее уведомление; `kind`: `"info"` или `"error"`. |
| `confirm(message, title?): Promise<boolean>` | Диалог «ОК / Отмена». |
| `openFile(path, line?)` | Открыть файл на вкладке «Файлы» текущего проекта. |
| `showTab(id)` | Переключить вкладку: встроенную (`overview`, `passport`, `files`, `graph`, `git`, `analytics`, `deps`, `console`) или свою по её `id`. |
| `showPage(id)` | Открыть свой экран, зарегистрированный через `pages.register`. |
| `openUrl(url)` | Открыть ссылку в браузере. |
| `openInEditor(path, line?)` | Открыть файл или папку в IDE пользователя. |
| `openPath(path)` | Открыть файл или папку системным приложением. |
| `copy(text)` | Скопировать в буфер обмена. |
| `pickFolder(title?)` / `pickFile(title?, extensions?)` | Системный диалог выбора; вернёт `null`, если пользователь отменил выбор. |
| `theme()` | `"dark"` или `"light"`. |
| `icon(name, size?)` | SVG-строка иконки Hangar. |
| `icons()` | Список имён всех иконок. |

## hangar.projects 🔒 `projects:read`

| | |
|---|---|
| `list(): ProjectContext[]` | Все проекты Hangar. |
| `current(): ProjectContext \| null` | Открытый сейчас проект. |
| `info(path): Promise<ProjectInfo>` | Модули, команды, зависимости, env-файлы, README. |
| `git(path): Promise<GitSummary>` | Ветка, ahead/behind, изменённые файлы, последний коммит. |
| `select(path)` | Открыть проект в Hangar. |

## hangar.fs

| | Разрешение |
|---|---|
| `readText(path): Promise<FileContent>` (`{ content, size, binary, truncated, … }`) | 🔒 `fs:read` |
| `listDir(path): Promise<DirEntry[]>` | 🔒 `fs:read` |
| `exists(path): Promise<boolean>` | 🔒 `fs:read` |
| `searchFiles(root, query, limit?)` — нечёткий поиск по именам | 🔒 `fs:read` |
| `searchText(root, query, limit?)` — полнотекстовый поиск | 🔒 `fs:read` |
| `writeText(path, content)` — создаёт недостающие папки | 🔒 `fs:write` |
| `join(base, ...parts)`, `relative(root, path)` | — |

Большие файлы `readText` отдаёт обрезанными (`truncated: true`), у бинарных файлов `binary: true`.

## hangar.git 🔒 `git:read`

| | |
|---|---|
| `log(path, { skip?, limit?, file? }): Promise<Commit[]>` | История коммитов (по умолчанию 50), можно для одного файла. |
| `commit(path, hash): Promise<CommitDetail>` | Коммит и список изменённых файлов. |
| `workingDiff(path): Promise<string>` | Diff незакоммиченных изменений. |
| `fileDiff(path, file, hash?): Promise<string>` | Diff файла в коммите или в рабочей копии. |

## hangar.shell 🔒 `shell`

| | |
|---|---|
| `exec(cwd, command, { timeoutMs? }): Promise<ExecResult>` | Выполнить команду и получить `{ code, stdout, stderr, timedOut }`. Таймаут по умолчанию — 60 с, по истечении процесс останавливается. |
| `run(projectPath, command, { label?, cwd? })` | Запустить команду во вкладке «Консоль» проекта: с живым выводом, остановкой и перезапуском. |
| `openTerminal(cwd, command?)` | Открыть внешний терминал из настроек Hangar в папке и выполнить команду. Для интерактивных программ (Claude Code, aider…), которым нужен настоящий терминал. *(с 0.9.0)* |

Команды выполняются через `cmd /C` (Windows) с UTF-8 выводом. Консоль Hangar — не терминал: ввод
программе приходит через pipe, поэтому программы, которые читают stdin до конца (например, `claude -p`),
запускайте с `< nul` (Windows) / `< /dev/null`.

## hangar.passport <a id="passport"></a> *(с 0.9.0)*

Паспорт проекта — описание, статус, задачи, роадмап и документы в файлах репозитория
(вкладка «Паспорт»). Встроенный формат — папка `.hangar/`
([описание формата](../PASSPORT.md)); плагин может добавить свой формат или кнопки у задач.

| | |
|---|---|
| `get(projectPath): Promise<Passport \| null>` 🔒 `projects:read` | Прочитать паспорт; `null` — его нет. |
| `refresh(projectPath)` | Перечитать паспорт и обновить интерфейс. |
| `taskPrompt(projectPath, task)` 🔒 `projects:read` | Промпт для ИИ-агента: взять задачу, работать по паспорту, отметить её и обновить «Сейчас». |
| `fillPrompt(projectPath)` 🔒 `projects:read` | Промпт «заполни паспорт по коду проекта». |
| `registerFormat(format): Disposable` | Свой формат хранения (см. ниже). Форматы плагинов проверяются раньше встроенного. |
| `registerTaskAction(action): Disposable` | Кнопка у каждой задачи на доске: `{ id, label, icon?, title?, when?(task, project), run(project, task, passport) }`. |

Событие `passport:change` — `{ projectPath, passport }` — приходит после каждого перечитывания.

### Свой формат

Если у вас уже есть своя структура (Notion-экспорт, `TODO.md` в корне, `docs/status.yml`…), не нужно
переезжать на `.hangar/` — опишите формат, и Hangar покажет его на той же вкладке:

```js
hangar.passport.registerFormat({
  id: "todo-md",
  label: "TODO.md в корне",
  detect: (ctx) => ctx.has("TODO.md"),
  async read(ctx) {
    const text = (await ctx.readText("TODO.md")) ?? "";
    const tasks = [...text.matchAll(/^- \[( |x)\] (.+)$/gm)].map((m, i) => ({
      id: String(i), text: m[2], raw: m[0], done: m[1] === "x", section: "TODO", tags: [],
    }));
    return {
      tags: [], description: "", now: "",
      sections: [{ title: "TODO", tasks }],
      roadmap: [], docs: [],
      files: { tasks: ctx.abs("TODO.md") },
    };
  },
  // необязательно: init, setTaskDone, addTask, moveTask, setStatus —
  // без них вкладка работает только на чтение. Запись через ctx.writeText требует "fs:write".
});
```

`ctx` (`PassportContext`) работает только внутри папки проекта: `has`, `readText`, `list`, `writeText`, `abs`.
Полные типы — `Passport`, `PassportTask`, `PassportFormat` в `hangar.d.ts`.

## hangar.util

| | |
|---|---|
| `escapeHtml(s)` | Экранировать строку для `innerHTML`. |
| `relTime(dateOrMs)` | «3 дня назад». |
| `formatSize(bytes)` | «12.4 МБ». |

## hangar.vue

Модуль Vue 3 из приложения: `createApp`, `h`, `ref`, `reactive`, `computed`, `watch` и т. д.
Это runtime-сборка без компилятора шаблонов, поэтому вместо шаблонов используйте render-функции на `h()`.
