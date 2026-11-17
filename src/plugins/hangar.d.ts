/**
 * Dev Hangar Plugin API, apiVersion 1.
 *
 * This file is the single source of truth for the plugin API:
 *  - the app itself is type-checked against it (src/plugins/host.ts);
 *  - it is copied into every plugin created with "Создать плагин" for editor autocompletion.
 *
 * Usage in a plugin's main.js:
 *   /** @param {import("./hangar").Hangar} hangar *\/
 *   export function activate(hangar) { ... }
 */

// ───────────────────────────── manifest ─────────────────────────────

export type Permission =
  /** список проектов, их стек, зависимости, git-статус */
  | "projects:read"
  /** чтение файлов и папок, поиск по файлам */
  | "fs:read"
  /** запись файлов */
  | "fs:write"
  /** история git: коммиты, diff */
  | "git:read"
  /** запуск команд оболочки (в консоли Hangar или с захватом вывода) */
  | "shell";

export interface PluginSettingSchema {
  key: string;
  label: string;
  type: "string" | "number" | "boolean" | "select" | "text";
  default?: unknown;
  description?: string;
  /** для type: "select" */
  options?: { value: string; label: string }[];
  placeholder?: string;
}

export interface PluginManifest {
  /** уникальный id: a-z, 0-9, "-", "_", "." (2–64 символа); совпадает с именем папки */
  id: string;
  name: string;
  /** semver, например "1.2.0" */
  version: string;
  /** версия API, под которую написан плагин. Сейчас: 1 */
  apiVersion: number;
  description?: string;
  author?: string;
  homepage?: string;
  /** минимальная версия Dev Hangar, например "0.7.0" */
  minAppVersion?: string;
  /** ES-модуль с функцией activate. По умолчанию "main.js" */
  main?: string;
  /** CSS-файл, подключаемый, пока плагин включён */
  style?: string;
  /** имя иконки из набора Hangar (см. Hangar.ui.icons()) */
  icon?: string;
  permissions?: Permission[];
  /** настройки, которые пользователь редактирует на экране «Плагины» */
  settings?: PluginSettingSchema[];
}

// ───────────────────────────── data types ─────────────────────────────

export interface Stack { id: string; label: string }
export interface Action { id: string; label: string; command: string; kind: string; cwd: string }
export interface Dependency { name: string; version: string; dev: boolean; source: string; scope: string }
export interface Module {
  relPath: string;
  name: string;
  version?: string | null;
  description?: string | null;
  stacks: Stack[];
  packageManager?: string | null;
  scripts: [string, string][];
  actions: Action[];
  needsInstall: boolean;
  missingTools: string[];
}
export interface ProjectInfo {
  name: string;
  path: string;
  exists: boolean;
  root: Module;
  subprojects: Module[];
  stacks: Stack[];
  dependencies: Dependency[];
  envFiles: string[];
  readme?: string | null;
  notableFiles: string[];
  isGit: boolean;
  modified?: number | null;
}
export interface StatusEntry { path: string; index: string; worktree: string }
export interface Commit {
  hash: string; short: string; author: string; email: string; date: string;
  subject: string; refs: string; filesChanged: number; insertions: number; deletions: number;
}
export interface GitSummary {
  isRepo: boolean;
  branch?: string | null;
  upstream?: string | null;
  ahead: number;
  behind: number;
  branches: string[];
  tags: string[];
  status: StatusEntry[];
  lastCommit?: Commit | null;
  commitCount: number;
}
export interface ChangedFile { status: string; path: string; oldPath?: string | null; additions?: number | null; deletions?: number | null }
export interface CommitDetail { hash: string; author: string; email: string; date: string; message: string; parents: string[]; files: ChangedFile[] }
export interface DirEntry { name: string; path: string; isDir: boolean; size: number; modified: number; heavy: boolean; hidden: boolean }
export interface FileContent { path: string; content: string; size: number; modified: number; binary: boolean; truncated: boolean }
export interface FileHit { path: string; rel: string; score: number; isDir: boolean }
export interface TextHit { path: string; rel: string; line: number; text: string }
export interface ExecResult { code: number | null; stdout: string; stderr: string; timedOut: boolean }

/** Короткое описание проекта — доступно без разрешений. */
export interface ProjectContext {
  /** абсолютный путь к папке проекта */
  path: string;
  /** отображаемое имя (с учётом переименования пользователем) */
  name: string;
  /** id стеков: "vue", "rust", "tauri", "php", "laravel"… */
  stacks: string[];
  isGit: boolean;
}

// ───────────────────────────── contributions ─────────────────────────────

export interface Disposable { dispose(): void }

export interface TabContext {
  project: ProjectContext;
  /** срабатывает, когда вкладку закрыли — удобно для fetch/таймеров */
  signal: AbortSignal;
}

export interface TabContribution {
  /** уникален в пределах плагина */
  id: string;
  label: string;
  /** имя иконки Hangar, по умолчанию иконка плагина */
  icon?: string;
  /** показывать вкладку только для подходящих проектов */
  when?: (project: ProjectContext) => boolean;
  /**
   * Нарисовать содержимое вкладки в `el`. Можно вернуть функцию очистки.
   * Вызывается при каждом открытии вкладки (и при смене проекта).
   */
  mount(el: HTMLElement, ctx: TabContext): void | (() => void) | Promise<void | (() => void)>;
}

export interface ActionContribution {
  id: string;
  label: string;
  icon?: string;
  /** true — кнопка в шапке проекта; иначе пункт в меню «Все команды» */
  toolbar?: boolean;
  /** подсказка при наведении */
  title?: string;
  when?: (project: ProjectContext) => boolean;
  run(project: ProjectContext): void | Promise<void>;
}

export interface PageContribution {
  /** уникален в пределах плагина */
  id: string;
  label: string;
  icon?: string;
  /** нарисовать экран в `el`; можно вернуть функцию очистки */
  mount(el: HTMLElement, ctx: { signal: AbortSignal }): void | (() => void) | Promise<void | (() => void)>;
}

/** Папка, которую проверяет детектор стека: корень проекта или подпапка монорепы. */
export interface StackDetectContext {
  /** корень проекта */
  projectPath: string;
  /** путь проверяемой папки относительно корня ("" — корень) */
  relPath: string;
  /** абсолютный путь проверяемой папки */
  dir: string;
  isRoot: boolean;
  /** стеки, которые Hangar уже нашёл в этой папке */
  stacks: string[];
  /** есть ли файл/папка (путь относительно проверяемой папки, можно "src/main.ts") */
  has(rel: string): Promise<boolean>;
  /** содержимое проверяемой папки */
  list(): Promise<DirEntry[]>;
  /** текст файла или null, если файла нет */
  readText(rel: string): Promise<string | null>;
  /** JSON-файл или null, если файла нет или он некорректен */
  readJson<T = any>(rel: string): Promise<T | null>;
}

export interface StackAction {
  /** уникален в пределах стека */
  id: string;
  label: string;
  command: string;
  /**
   * "dev" и "build" попадают на кнопки Dev / Build в шапке проекта (если у проекта ещё нет своих),
   * "open" — открыть файл `command` системным приложением, остальные — в меню «Все команды».
   * По умолчанию "script".
   */
  kind?: "dev" | "build" | "test" | "install" | "lint" | "serve" | "open" | "script";
  /** рабочая папка относительно проверяемой папки */
  cwd?: string;
  /** поставить команду первой — тогда именно она окажется на кнопке Dev/Build */
  primary?: boolean;
}

export interface StackDependency {
  name: string;
  version?: string;
  dev?: boolean;
}

export interface StackContribution {
  /** id стека, например "hugo". Попадает в ProjectContext.stacks */
  id: string;
  /** подпись на бейдже: "Hugo" */
  label: string;
  /** цвет точки на бейдже (CSS-цвет) */
  color?: string;
  /** искать стек и в подпапках (монорепы: apps/*, packages/* …). По умолчанию true */
  subdirs?: boolean;
  /** вернуть true, если папка — проект этого стека */
  detect(ctx: StackDetectContext): boolean | Promise<boolean>;
  /** команды для найденного проекта */
  actions?(ctx: StackDetectContext): StackAction[] | Promise<StackAction[]>;
  /** зависимости для вкладки «Зависимости» */
  dependencies?(ctx: StackDetectContext): StackDependency[] | Promise<StackDependency[]>;
}

export interface HangarEvents {
  /** пользователь открыл проект */
  "project:open": ProjectContext;
  /** запущена команда в консоли */
  "process:start": { projectPath: string; label: string; command: string; kind: string };
  /** команда завершилась */
  "process:exit": { projectPath: string; label: string; command: string; kind: string; code: number | null; durationMs: number };
  /** сменилась тема */
  "theme:change": { dark: boolean };
}

// ───────────────────────────── API ─────────────────────────────

export interface Hangar {
  /** версия Plugin API хоста */
  readonly apiVersion: number;
  /** версия приложения Dev Hangar */
  readonly appVersion: string;

  plugin: {
    readonly id: string;
    readonly manifest: PluginManifest;
    /** абсолютный путь к папке плагина */
    readonly dir: string;
    /** прочитать текстовый файл из папки плагина (шаблон, JSON, …) */
    readFile(rel: string): Promise<string>;
    /** URL для <img src>, <audio> и т.п. на файл из папки плагина */
    assetUrl(rel: string): Promise<string>;
  };

  /** вкладка в каждом проекте */
  tabs: { register(tab: TabContribution): Disposable };
  /** кнопка в шапке проекта или пункт меню «Все команды» */
  actions: { register(action: ActionContribution): Disposable };
  /** свой экран в боковой панели (не привязан к проекту) */
  pages: { register(page: PageContribution): Disposable };
  /**
   * Поддержка нового стека: распознавание, бейдж, команды (кнопки Dev/Build), зависимости.
   * Детектор читает файлы только внутри проекта — отдельное разрешение не нужно.
   */
  stacks: {
    register(stack: StackContribution): Disposable;
    /** заново применить стеки ко всем проектам (например, после смены настроек плагина) */
    refresh(): void;
  };

  events: {
    on<K extends keyof HangarEvents>(event: K, handler: (payload: HangarEvents[K]) => void): Disposable;
  };

  /** Простое хранилище плагина (JSON), сохраняется в config.json. Без разрешений. */
  storage: {
    get<T = unknown>(key: string, fallback?: T): T;
    set(key: string, value: unknown): void;
    delete(key: string): void;
    keys(): string[];
  };

  /** Значения из manifest.settings, которые пользователь меняет на экране «Плагины». */
  settings: {
    get<T = unknown>(key: string): T;
    all(): Record<string, unknown>;
    onChange(handler: (key: string, value: unknown) => void): Disposable;
  };

  ui: {
    toast(text: string, kind?: "info" | "error"): void;
    /** диалог подтверждения */
    confirm(message: string, title?: string): Promise<boolean>;
    /** открыть файл на вкладке «Файлы» текущего проекта */
    openFile(path: string, line?: number): void;
    /** переключить вкладку: встроенную ("overview", "files", "git", …) или свою по её id */
    showTab(tabId: string): void;
    /** открыть свой экран (pages.register) по его id */
    showPage(pageId: string): void;
    openUrl(url: string): Promise<void>;
    openInEditor(path: string, line?: number): Promise<void>;
    /** открыть файл/папку системным приложением */
    openPath(path: string): Promise<void>;
    copy(text: string): Promise<void>;
    pickFolder(title?: string): Promise<string | null>;
    pickFile(title?: string, extensions?: string[]): Promise<string | null>;
    theme(): "dark" | "light";
    /** SVG-разметка иконки Hangar */
    icon(name: string, size?: number): string;
    /** все доступные имена иконок */
    icons(): string[];
  };

  /** Требует "projects:read" */
  projects: {
    list(): ProjectContext[];
    /** открытый сейчас проект */
    current(): ProjectContext | null;
    info(path: string): Promise<ProjectInfo>;
    git(path: string): Promise<GitSummary>;
    /** открыть проект в Hangar */
    select(path: string): void;
  };

  /** "fs:read" — чтение, "fs:write" — запись */
  fs: {
    readText(path: string): Promise<FileContent>;
    listDir(path: string): Promise<DirEntry[]>;
    exists(path: string): Promise<boolean>;
    searchFiles(root: string, query: string, limit?: number): Promise<FileHit[]>;
    searchText(root: string, query: string, limit?: number): Promise<TextHit[]>;
    writeText(path: string, content: string): Promise<void>;
    join(base: string, ...parts: string[]): string;
    relative(root: string, path: string): string;
  };

  /** Требует "git:read" */
  git: {
    log(path: string, opts?: { skip?: number; limit?: number; file?: string }): Promise<Commit[]>;
    commit(path: string, hash: string): Promise<CommitDetail>;
    workingDiff(path: string): Promise<string>;
    fileDiff(path: string, file: string, hash?: string): Promise<string>;
  };

  /** Требует "shell" */
  shell: {
    /** выполнить команду и получить вывод (таймаут по умолчанию 60 с) */
    exec(cwd: string, command: string, opts?: { timeoutMs?: number }): Promise<ExecResult>;
    /** запустить команду во вкладке «Консоль» проекта */
    run(projectPath: string, command: string, opts?: { label?: string; cwd?: string }): Promise<void>;
  };

  util: {
    escapeHtml(s: string): string;
    /** «3 дня назад» */
    relTime(input: number | string): string;
    /** «12.4 МБ» */
    formatSize(bytes: number): string;
  };

  /**
   * Vue 3 приложения (runtime-сборка, без компилятора шаблонов — используйте h()).
   * Позволяет писать реактивный UI без своего бандла Vue.
   */
  vue: typeof import("vue");

  /** вызвать fn при выключении/перезагрузке плагина */
  onDispose(fn: () => void): void;
}

/** Что экспортирует main.js плагина */
export interface PluginModule {
  activate(hangar: Hangar): void | Promise<void>;
  deactivate?(): void | Promise<void>;
}
