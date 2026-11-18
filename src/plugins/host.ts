// Plugin host: discovers plugins in <config dir>/plugins, loads the enabled ones
// and gives each its own `hangar` API object (see hangar.d.ts).
//
// Trust model: plugins run in the app's webview like VS Code extensions do —
// permissions in plugin.json gate the `hangar.*` API and are shown to the user
// before enabling, but they are not a security sandbox. Only install plugins you trust.

import * as Vue from "vue";
import { markRaw, reactive, watch } from "vue";
import { getVersion } from "@tauri-apps/api/app";
import { convertFileSrc } from "@tauri-apps/api/core";
import { ask, open } from "@tauri-apps/plugin-dialog";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import type {
  ActionContribution, Disposable, Hangar, HangarEvents, PageContribution, Permission, PluginManifest, PluginModule,
  ProjectContext, StackContribution, StackDetectContext, TabContribution,
} from "./hangar";
import { api, joinPath, relPath, type DirEntry, type Module, type PluginEntry, type ProjectInfo } from "../lib/api";
import {
  analyzeProject, displayName, escapeHtml, findProject, reenrichAll, runAction, selectProject, setInfoEnricher, state, toast,
  type ProjectEntry,
} from "../lib/store";
import { relTime, size, STACK_COLORS } from "../lib/format";
import { ICONS, iconSvg } from "../lib/icons";
import { fillPrompt, loadPassport, passportHost, passports, taskPrompt } from "../lib/passport";
import * as bus from "./bus";

export const API_VERSION = 1;

export const PERMISSIONS: Record<Permission, string> = {
  "projects:read": "Видеть список проектов, их стек, зависимости и git-статус",
  "fs:read": "Читать файлы и папки на диске",
  "fs:write": "Создавать и изменять файлы на диске",
  "git:read": "Читать историю git: коммиты и diff",
  shell: "Запускать команды в терминале",
};

/** Built-in project tabs (plugins may switch to them with ui.showTab). */
export const BUILTIN_TABS = ["overview", "passport", "files", "graph", "git", "analytics", "deps", "console"];

export type PluginStatus = "disabled" | "active" | "loading" | "error" | "invalid";

export interface HostPlugin {
  folder: string;
  dir: string;
  manifest: PluginManifest | null;
  status: PluginStatus;
  error?: string;
  /** manifest problems that don't block loading */
  warnings: string[];
}

export interface HostTab {
  key: string; // "plugin:<pluginId>:<tabId>"
  pluginId: string;
  tab: TabContribution;
}

export interface HostAction {
  key: string;
  pluginId: string;
  pluginName: string;
  action: ActionContribution;
}

export interface HostPage {
  /** "page:<pluginId>:<pageId>" — also the value of state.view */
  key: `page:${string}`;
  pluginId: string;
  page: PageContribution;
}

export interface HostStack {
  key: string;
  pluginId: string;
  stack: StackContribution;
}

interface Runtime {
  module?: PluginModule;
  disposers: (() => void)[];
  style?: HTMLStyleElement;
}

export const host = reactive({
  plugins: [] as HostPlugin[],
  tabs: [] as HostTab[],
  actions: [] as HostAction[],
  pages: [] as HostPage[],
  stacks: [] as HostStack[],
  dir: "",
  appVersion: "",
  busy: false,
  /** ui.openFile → ProjectView picks this up */
  fileRequest: null as null | { path: string; line?: number },
});

const runtimes = new Map<string, Runtime>();

// ───────────────────────────── manifest ─────────────────────────────

const ID_RE = /^[a-z0-9][a-z0-9._-]{1,63}$/;

function semverGte(a: string, b: string) {
  const pa = a.split(/[.+-]/).map((x) => parseInt(x, 10) || 0);
  const pb = b.split(/[.+-]/).map((x) => parseInt(x, 10) || 0);
  for (let i = 0; i < 3; i++) {
    if ((pa[i] ?? 0) !== (pb[i] ?? 0)) return (pa[i] ?? 0) > (pb[i] ?? 0);
  }
  return true;
}

function validate(e: PluginEntry): HostPlugin {
  const p: HostPlugin = { folder: e.folder, dir: e.dir, manifest: null, status: "invalid", warnings: [] };
  if (e.error || !e.manifest) {
    p.error = e.error || "plugin.json не прочитан";
    return p;
  }
  const m = e.manifest as Partial<PluginManifest>;
  const errors: string[] = [];
  if (typeof m.id !== "string" || !ID_RE.test(m.id)) errors.push('"id": только a-z, 0-9, "-", "_", "." (2–64 символа)');
  else if (m.id !== e.folder) errors.push(`"id" (${m.id}) должен совпадать с именем папки (${e.folder})`);
  if (typeof m.name !== "string" || !m.name.trim()) errors.push('нет поля "name"');
  if (typeof m.version !== "string") errors.push('нет поля "version"');
  if (typeof m.apiVersion !== "number") errors.push('нет поля "apiVersion" (сейчас: 1)');
  else if (m.apiVersion > API_VERSION) errors.push(`нужен Plugin API v${m.apiVersion}, а в этой версии Hangar — v${API_VERSION}. Обновите приложение`);
  if (m.minAppVersion && host.appVersion && !semverGte(host.appVersion, m.minAppVersion))
    errors.push(`нужен Dev Hangar ${m.minAppVersion}+, установлен ${host.appVersion}`);
  if (m.permissions !== undefined && !Array.isArray(m.permissions)) errors.push('"permissions" должен быть массивом');
  for (const perm of m.permissions ?? []) {
    if (!(perm in PERMISSIONS)) p.warnings.push(`неизвестное разрешение "${perm}" — пропущено`);
  }
  if (m.settings !== undefined && !Array.isArray(m.settings)) errors.push('"settings" должен быть массивом');
  if (errors.length) {
    p.error = errors.join("; ");
    return p;
  }
  p.manifest = {
    main: "main.js",
    ...m,
    permissions: (m.permissions ?? []).filter((x) => x in PERMISSIONS),
  } as PluginManifest;
  p.status = "disabled";
  return p;
}

// ───────────────────────────── helpers ─────────────────────────────

function projectContext(entry: ProjectEntry): ProjectContext {
  const info = state.briefs[entry.path]?.info;
  return {
    path: entry.path,
    name: displayName(entry),
    stacks: info?.stacks.map((s) => s.id) ?? [],
    isGit: !!info?.isGit,
  };
}

export function currentProjectContext(): ProjectContext | null {
  const e = findProject(state.selected);
  return e ? projectContext(e) : null;
}

/** Evaluate a contribution's `when` safely. */
export function isVisible(c: { when?: (p: ProjectContext) => boolean }, entry: ProjectEntry) {
  if (!c.when) return true;
  try {
    return !!c.when(projectContext(entry));
  } catch (e) {
    console.error("[plugins] when() failed", e);
    return false;
  }
}

export function pluginOf(id: string) {
  return host.plugins.find((p) => p.folder === id);
}

function report(pluginId: string, where: string, e: unknown) {
  console.error(`[plugin ${pluginId}] ${where}:`, e);
  toast(`Плагин «${pluginOf(pluginId)?.manifest?.name ?? pluginId}»: ${where}: ${e instanceof Error ? e.message : e}`, "error");
}

export async function runPluginAction(a: HostAction, entry: ProjectEntry) {
  try {
    await a.action.run(projectContext(entry));
  } catch (e) {
    report(a.pluginId, `ошибка в «${a.action.label}»`, e);
  }
}

export function projectContextOf(entry: ProjectEntry) {
  return projectContext(entry);
}

export function settingValue(p: HostPlugin, key: string) {
  const v = state.pluginConfig.settings[p.folder]?.[key];
  return v !== undefined ? v : p.manifest?.settings?.find((s) => s.key === key)?.default;
}

export function setSetting(p: HostPlugin, key: string, value: unknown) {
  (state.pluginConfig.settings[p.folder] ??= {})[key] = value;
  bus.emit(`settings:${p.folder}`, { key, value });
}

// ───────────────────────────── API object ─────────────────────────────

function createApi(p: HostPlugin, rt: Runtime): Hangar {
  const m = p.manifest!;
  const id = m.id;
  const perms = new Set(m.permissions ?? []);
  const need = (perm: Permission) => {
    if (!perms.has(perm)) throw new Error(`плагину нужно разрешение "${perm}" в plugin.json → "permissions"`);
  };
  const track = (dispose: () => void): Disposable => {
    let done = false;
    const d = () => {
      if (done) return;
      done = true;
      dispose();
      rt.disposers = rt.disposers.filter((x) => x !== d);
    };
    rt.disposers.push(d);
    return { dispose: d };
  };
  const clone = <T>(v: T): T => (v === undefined ? v : JSON.parse(JSON.stringify(v)));
  const storageOf = () => (state.pluginConfig.data[id] ??= {});

  const hangar: Hangar = {
    apiVersion: API_VERSION,
    appVersion: host.appVersion,

    plugin: {
      id,
      manifest: clone(m),
      dir: p.dir,
      readFile: (rel) => api.pluginReadFile(p.folder, rel),
      assetUrl: async (rel) => convertFileSrc(await api.pluginFilePath(p.folder, rel)),
    },

    tabs: {
      register(tab) {
        if (!tab?.id || !tab.label || typeof tab.mount !== "function") throw new Error("tabs.register: нужны id, label и mount()");
        const key = `plugin:${id}:${tab.id}`;
        if (host.tabs.some((t) => t.key === key)) throw new Error(`tabs.register: вкладка "${tab.id}" уже зарегистрирована`);
        host.tabs.push(markRaw({ key, pluginId: id, tab: markRaw({ ...tab, icon: tab.icon || m.icon || "puzzle" }) }));
        return track(() => {
          host.tabs = host.tabs.filter((t) => t.key !== key);
        });
      },
    },

    actions: {
      register(action) {
        if (!action?.id || !action.label || typeof action.run !== "function") throw new Error("actions.register: нужны id, label и run()");
        const key = `plugin:${id}:${action.id}`;
        if (host.actions.some((a) => a.key === key)) throw new Error(`actions.register: команда "${action.id}" уже зарегистрирована`);
        host.actions.push(markRaw({ key, pluginId: id, pluginName: m.name, action: markRaw({ ...action }) }));
        return track(() => {
          host.actions = host.actions.filter((a) => a.key !== key);
        });
      },
    },

    pages: {
      register(page) {
        if (!page?.id || !page.label || typeof page.mount !== "function") throw new Error("pages.register: нужны id, label и mount()");
        const key = `page:${id}:${page.id}` as const;
        if (host.pages.some((x) => x.key === key)) throw new Error(`pages.register: экран "${page.id}" уже зарегистрирован`);
        host.pages.push(markRaw({ key, pluginId: id, page: markRaw({ ...page, icon: page.icon || m.icon || "puzzle" }) }));
        return track(() => {
          host.pages = host.pages.filter((x) => x.key !== key);
        });
      },
    },

    stacks: {
      register(stack) {
        if (!stack?.id || !stack.label || typeof stack.detect !== "function") throw new Error("stacks.register: нужны id, label и detect()");
        if (!/^[a-z0-9][a-z0-9._-]*$/.test(stack.id)) throw new Error('stacks.register: id стека — только a-z, 0-9, "-", "_", "."');
        const key = `${id}:${stack.id}`;
        if (host.stacks.some((x) => x.key === key)) throw new Error(`stacks.register: стек "${stack.id}" уже зарегистрирован`);
        const hadColor = stack.id in STACK_COLORS;
        if (stack.color && !hadColor) STACK_COLORS[stack.id] = stack.color;
        host.stacks.push(markRaw({ key, pluginId: id, stack: markRaw({ ...stack }) }));
        scheduleReenrich();
        return track(() => {
          host.stacks = host.stacks.filter((x) => x.key !== key);
          if (stack.color && !hadColor) delete STACK_COLORS[stack.id];
          scheduleReenrich();
        });
      },
      refresh: () => scheduleReenrich(),
    },

    events: {
      on(event, handler) {
        const off = bus.on(event, (payload) => {
          try {
            handler(payload);
          } catch (e) {
            report(id, `обработчик "${String(event)}"`, e);
          }
        });
        return track(off);
      },
    },

    storage: {
      get: (key, fallback) => {
        const v = storageOf()[key];
        return (v === undefined ? fallback : clone(v)) as any;
      },
      set: (key, value) => {
        storageOf()[key] = clone(value);
      },
      delete: (key) => {
        delete storageOf()[key];
      },
      keys: () => Object.keys(storageOf()),
    },

    settings: {
      get: (key) => clone(settingValue(p, key)) as any,
      all: () => Object.fromEntries((m.settings ?? []).map((s) => [s.key, clone(settingValue(p, s.key))])),
      onChange(handler) {
        return track(bus.on(`settings:${id}`, ({ key, value }) => {
          try {
            handler(key, value);
          } catch (e) {
            report(id, "settings.onChange", e);
          }
        }));
      },
    },

    ui: {
      toast: (text, kind = "info") => toast(String(text), kind),
      confirm: (message, title) => ask(message, { title: title ?? m.name, kind: "warning" }),
      openFile(path, line) {
        host.fileRequest = { path, line };
        state.view = "project";
      },
      showTab(tabId) {
        state.view = "project";
        state.tab = BUILTIN_TABS.includes(tabId) ? tabId : `plugin:${id}:${tabId}`;
      },
      showPage(pageId) {
        const key = `page:${id}:${pageId}` as const;
        if (!host.pages.some((x) => x.key === key)) throw new Error(`экран "${pageId}" не зарегистрирован`);
        state.view = key;
      },
      openUrl: (url) => openUrl(url),
      openInEditor: (path, line) => api.openInEditor(state.settings.editor, path, line),
      openPath: (path) => openPath(path),
      copy: (text) => navigator.clipboard.writeText(text),
      async pickFolder(title) {
        const r = await open({ directory: true, multiple: false, title });
        return typeof r === "string" ? r : null;
      },
      async pickFile(title, extensions) {
        const r = await open({
          multiple: false,
          title,
          filters: extensions?.length ? [{ name: extensions.join(", "), extensions }] : undefined,
        });
        return typeof r === "string" ? r : null;
      },
      theme: () => (document.documentElement.dataset.theme === "light" ? "light" : "dark"),
      icon: (name, sz) => iconSvg(name, sz),
      icons: () => Object.keys(ICONS),
    },

    projects: {
      list() {
        need("projects:read");
        return state.projects.map(projectContext);
      },
      current() {
        need("projects:read");
        return currentProjectContext();
      },
      async info(path) {
        need("projects:read");
        return state.briefs[path]?.info ?? (await analyzeProject(path));
      },
      async git(path) {
        need("projects:read");
        return state.briefs[path]?.git ?? (await api.gitSummary(path));
      },
      select(path) {
        need("projects:read");
        const e = findProject(path);
        if (!e) throw new Error(`проект не найден: ${path}`);
        selectProject(e.path);
      },
    },

    fs: {
      readText: async (path) => (need("fs:read"), api.readText(path)),
      listDir: async (path) => (need("fs:read"), api.listDir(path)),
      exists: async (path) => (need("fs:read"), api.pathExists(path)),
      searchFiles: async (root, q, limit) => (need("fs:read"), api.searchFiles(root, q, limit)),
      searchText: async (root, q, limit) => (need("fs:read"), api.searchText(root, q, limit)),
      writeText: async (path, content) => (need("fs:write"), api.writeTextFile(path, content)),
      join: (base, ...parts) => joinPath(base, ...parts),
      relative: (root, path) => relPath(root, path),
    },

    git: {
      log: async (path, o = {}) => (need("git:read"), api.gitLog(path, o.skip ?? 0, o.limit ?? 50, o.file)),
      commit: async (path, hash) => (need("git:read"), api.gitCommit(path, hash)),
      workingDiff: async (path) => (need("git:read"), api.gitWorkingDiff(path)),
      fileDiff: async (path, file, hash) => (need("git:read"), api.gitFileDiff(path, file, hash)),
    },

    shell: {
      exec: async (cwd, command, o = {}) => (need("shell"), api.execCapture(cwd, command, o.timeoutMs)),
      async run(projectPath, command, o = {}) {
        need("shell");
        const e = findProject(projectPath);
        if (!e) throw new Error(`проект не найден: ${projectPath}`);
        const cwd = o.cwd ? relPath(e.path, o.cwd) : "";
        let h = 0;
        for (const c of command + cwd) h = (h * 31 + c.charCodeAt(0)) | 0;
        await runAction(e.path, {
          id: `plugin-${id}-${(h >>> 0).toString(36)}`,
          label: o.label || command,
          command,
          cwd,
          kind: "plugin",
        });
      },
      async openTerminal(cwd, command) {
        need("shell");
        await api.openTerminal(cwd, state.settings.terminal, command || undefined);
      },
    },

    passport: {
      async get(projectPath) {
        need("projects:read");
        const s = passports[projectPath];
        return clone(s?.data && !s.loading ? s.data : await loadPassport(projectPath));
      },
      async refresh(projectPath) {
        await loadPassport(projectPath);
      },
      async taskPrompt(projectPath, task) {
        need("projects:read");
        return taskPrompt(projectPath, task);
      },
      async fillPrompt(projectPath) {
        need("projects:read");
        return fillPrompt(projectPath);
      },
      registerFormat(format) {
        if (!format?.id || !format.label || typeof format.detect !== "function" || typeof format.read !== "function")
          throw new Error("passport.registerFormat: нужны id, label, detect() и read()");
        const key = `${id}:${format.id}`;
        if (passportHost.formats.some((f) => f.key === key)) throw new Error(`passport.registerFormat: формат "${format.id}" уже зарегистрирован`);
        passportHost.formats.push(markRaw({ key, pluginId: id, format: markRaw({ ...format }), canWrite: perms.has("fs:write") }));
        return track(() => {
          passportHost.formats = passportHost.formats.filter((f) => f.key !== key);
        });
      },
      registerTaskAction(action) {
        if (!action?.id || !action.label || typeof action.run !== "function") throw new Error("passport.registerTaskAction: нужны id, label и run()");
        const key = `${id}:${action.id}`;
        if (passportHost.taskActions.some((a) => a.key === key)) throw new Error(`passport.registerTaskAction: "${action.id}" уже зарегистрирована`);
        passportHost.taskActions.push(markRaw({ key, pluginId: id, pluginName: m.name, action: markRaw({ ...action }) }));
        return track(() => {
          passportHost.taskActions = passportHost.taskActions.filter((a) => a.key !== key);
        });
      },
    },

    util: {
      escapeHtml,
      relTime: (input) => relTime(input),
      formatSize: (b) => size(b),
    },

    vue: Vue,

    onDispose(fn) {
      track(fn);
    },
  };
  return Object.freeze(hangar);
}

// ───────────────────────────── plugin stacks ─────────────────────────────

let reenrichTimer: number | undefined;
function scheduleReenrich() {
  clearTimeout(reenrichTimer);
  reenrichTimer = window.setTimeout(() => reenrichAll(), 150);
}

/** Normalize a relative path ("./a\\b" → "a/b"), refusing absolute paths and "..". */
function cleanRel(rel: string) {
  const parts = rel.replace(/\\/g, "/").split("/").filter((x) => x && x !== ".");
  if (/^([a-zA-Z]:|[\\/])/.test(rel) || parts.includes("..")) throw new Error(`путь должен быть внутри папки проекта: ${rel}`);
  return parts.join("/");
}
function inside(dir: string, rel: string) {
  const r = cleanRel(rel);
  return r ? joinPath(dir, r) : dir;
}

function detectContext(projectPath: string, rel: string, mod: Module | undefined): StackDetectContext {
  const dir = joinPath(projectPath, rel);
  let listing: Promise<DirEntry[]> | undefined;
  const readText = async (r: string) => {
    const p = inside(dir, r);
    if (!(await api.pathExists(p))) return null;
    const f = await api.readText(p).catch(() => null);
    return f && !f.binary ? f.content : null;
  };
  return {
    projectPath,
    relPath: rel,
    dir,
    isRoot: !rel,
    stacks: mod?.stacks.map((s) => s.id) ?? [],
    has: (r) => api.pathExists(inside(dir, r)),
    list: () => (listing ??= api.listDir(dir).catch(() => [])),
    readText,
    async readJson(r) {
      const t = await readText(r);
      if (t == null) return null;
      try {
        return JSON.parse(t.replace(/^﻿/, ""));
      } catch {
        return null;
      }
    },
  };
}

/** Folders a monorepo may keep its packages in — mirrors candidate_subdirs in detect.rs. */
async function candidateDirs(root: string): Promise<string[]> {
  const out: string[] = [];
  const top = await api.listDir(root).catch(() => [] as DirEntry[]);
  const dirs = top.filter((e) => e.isDir && !e.hidden && !e.heavy);
  out.push(...dirs.map((e) => e.name));
  for (const g of ["apps", "packages", "services", "libs"]) {
    if (!dirs.some((e) => e.name === g)) continue;
    const sub = await api.listDir(joinPath(root, g)).catch(() => [] as DirEntry[]);
    out.push(...sub.filter((e) => e.isDir && !e.hidden && !e.heavy).map((e) => `${g}/${e.name}`));
  }
  return out.slice(0, 80);
}

function pushStack(list: { id: string; label: string }[], id: string, label: string) {
  if (!list.some((s) => s.id === id)) list.push({ id, label });
}

async function applyStacks(raw: ProjectInfo): Promise<ProjectInfo> {
  const providers = [...host.stacks];
  if (!providers.length || !raw.exists) return raw;
  const info: ProjectInfo = JSON.parse(JSON.stringify(raw));
  const modules = new Map<string, Module>([["", info.root], ...info.subprojects.map((m) => [m.relPath, m] as [string, Module])]);
  const needSubdirs = providers.some((p) => p.stack.subdirs !== false);
  const rels = ["", ...(needSubdirs ? await candidateDirs(info.path) : [])];

  for (const rel of rels) {
    const ctx = detectContext(info.path, rel, modules.get(rel));
    for (const { pluginId, stack } of providers) {
      if (rel && stack.subdirs === false) continue;
      let found = false;
      try {
        found = !!(await stack.detect(ctx));
      } catch (e) {
        console.error(`[plugin ${pluginId}] stack "${stack.id}" detect() failed in ${ctx.dir}`, e);
      }
      if (!found) continue;

      let mod = modules.get(rel);
      if (!mod) {
        mod = {
          relPath: rel, name: rel.split("/").pop() || info.name, stacks: [], scripts: [], actions: [],
          needsInstall: false, vendorMissing: false, composerScripts: [], missingTools: [],
        };
        modules.set(rel, mod);
        info.subprojects.push(mod);
      }
      pushStack(mod.stacks, stack.id, stack.label);

      try {
        const acts = (await stack.actions?.(ctx)) ?? [];
        const front: Module["actions"] = [];
        for (const a of acts) {
          if (!a?.id || !a.label || !a.command) continue;
          const action = {
            id: `plugin-${pluginId}-${stack.id}-${a.id}${rel ? "-" + rel.replace(/[\\/]/g, "_") : ""}`,
            label: a.label,
            command: a.command,
            kind: a.kind || "script",
            cwd: [rel, a.cwd ? cleanRel(a.cwd) : ""].filter(Boolean).join("/"),
          };
          if (mod.actions.some((x) => x.id === action.id)) continue;
          if (a.primary) front.push(action);
          else mod.actions.push(action);
        }
        mod.actions.unshift(...front);
      } catch (e) {
        console.error(`[plugin ${pluginId}] stack "${stack.id}" actions() failed`, e);
      }

      try {
        for (const d of (await stack.dependencies?.(ctx)) ?? []) {
          if (!d?.name) continue;
          info.dependencies.push({ name: d.name, version: String(d.version ?? ""), dev: !!d.dev, source: stack.label, scope: rel });
        }
      } catch (e) {
        console.error(`[plugin ${pluginId}] stack "${stack.id}" dependencies() failed`, e);
      }
    }
  }

  const all: { id: string; label: string }[] = [];
  for (const m of [info.root, ...info.subprojects]) for (const s of m.stacks) pushStack(all, s.id, s.label);
  info.stacks = all.length ? all : [{ id: "unknown", label: "Не определён" }];
  return info;
}

// ───────────────────────────── lifecycle ─────────────────────────────

async function activate(p: HostPlugin) {
  const m = p.manifest!;
  const rt: Runtime = { disposers: [] };
  runtimes.set(p.folder, rt);
  p.status = "loading";
  p.error = undefined;
  try {
    if (m.style) {
      const css = await api.pluginReadFile(p.folder, m.style);
      rt.style = document.createElement("style");
      rt.style.dataset.plugin = m.id;
      rt.style.textContent = css;
      document.head.appendChild(rt.style);
    }
    const code = await api.pluginReadFile(p.folder, m.main || "main.js");
    // A blob URL keeps the module same-origin; sourceURL makes stack traces readable in DevTools.
    const url = URL.createObjectURL(
      new Blob([code, `\n//# sourceURL=hangar-plugin://${m.id}/${m.main || "main.js"}`], { type: "text/javascript" }),
    );
    let mod: PluginModule;
    try {
      mod = await import(/* @vite-ignore */ url);
    } finally {
      URL.revokeObjectURL(url);
    }
    if (typeof mod.activate !== "function") throw new Error(`${m.main || "main.js"} должен экспортировать функцию activate(hangar)`);
    rt.module = mod;
    await mod.activate(createApi(p, rt));
    p.status = "active";
  } catch (e) {
    console.error(`[plugin ${p.folder}] activate failed`, e);
    await deactivate(p);
    p.status = "error";
    p.error = e instanceof Error ? e.message : String(e);
  }
}

async function deactivate(p: HostPlugin) {
  const rt = runtimes.get(p.folder);
  runtimes.delete(p.folder);
  p.status = "disabled";
  if (!rt) return;
  try {
    await rt.module?.deactivate?.();
  } catch (e) {
    console.error(`[plugin ${p.folder}] deactivate failed`, e);
  }
  for (const d of [...rt.disposers].reverse()) {
    try {
      d();
    } catch (e) {
      console.error(`[plugin ${p.folder}] dispose failed`, e);
    }
  }
  // safety net: drop anything still registered by this plugin
  host.tabs = host.tabs.filter((t) => t.pluginId !== p.folder);
  host.actions = host.actions.filter((a) => a.pluginId !== p.folder);
  host.pages = host.pages.filter((a) => a.pluginId !== p.folder);
  passportHost.formats = passportHost.formats.filter((f) => f.pluginId !== p.folder);
  passportHost.taskActions = passportHost.taskActions.filter((a) => a.pluginId !== p.folder);
  if (host.stacks.some((s) => s.pluginId === p.folder)) {
    host.stacks = host.stacks.filter((s) => s.pluginId !== p.folder);
    scheduleReenrich();
  }
  rt.style?.remove();
}

/** (Re)scan the plugins folder and activate the enabled ones. */
export async function loadPlugins() {
  host.busy = true;
  try {
    if (!host.appVersion) host.appVersion = await getVersion().catch(() => "");
    host.dir = await api.pluginsDir().catch(() => "");
    for (const p of host.plugins) if (runtimes.has(p.folder)) await deactivate(p);
    const entries = await api.pluginList().catch((e) => {
      toast("Не удалось прочитать папку плагинов: " + e, "error");
      return [] as PluginEntry[];
    });
    host.plugins = entries.map(validate);
    for (const p of host.plugins) {
      if (p.manifest && state.pluginConfig.enabled[p.folder]) await activate(p);
    }
  } finally {
    host.busy = false;
  }
}

export async function reloadPlugin(folder: string) {
  const old = pluginOf(folder);
  if (old && runtimes.has(folder)) await deactivate(old);
  const entry = (await api.pluginList()).find((e) => e.folder === folder);
  if (!entry) {
    host.plugins = host.plugins.filter((p) => p.folder !== folder);
    return;
  }
  const fresh = validate(entry);
  const i = host.plugins.findIndex((p) => p.folder === folder);
  if (i >= 0) host.plugins[i] = fresh;
  else host.plugins.push(fresh);
  const p = pluginOf(folder)!; // reactive proxy
  if (p.manifest && state.pluginConfig.enabled[folder]) await activate(p);
}

export async function enablePlugin(folder: string, skipConfirm = false) {
  const p = pluginOf(folder);
  if (!p?.manifest) return;
  const perms = p.manifest.permissions ?? [];
  if (!skipConfirm) {
    const text =
      `Включить плагин «${p.manifest.name}» ${p.manifest.version}` +
      (p.manifest.author ? ` от ${p.manifest.author}` : "") +
      "?\n\n" +
      (perms.length ? "Плагин запрашивает доступ:\n" + perms.map((x) => "• " + PERMISSIONS[x]).join("\n") + "\n\n" : "") +
      "Плагины работают внутри приложения. Включайте только те, которым доверяете.";
    if (!(await ask(text, { title: "Включить плагин", kind: "warning", okLabel: "Включить", cancelLabel: "Отмена" }))) return;
  }
  state.pluginConfig.enabled[folder] = true;
  await activate(p);
  if (p.status === "active") toast(`Плагин «${p.manifest.name}» включён`);
}

export async function disablePlugin(folder: string) {
  const p = pluginOf(folder);
  delete state.pluginConfig.enabled[folder];
  if (p) await deactivate(p);
}

export async function uninstallPlugin(folder: string) {
  const p = pluginOf(folder);
  const name = p?.manifest?.name ?? folder;
  if (!(await ask(`Удалить плагин «${name}» и его папку с диска?`, { title: "Удаление плагина", kind: "warning", okLabel: "Удалить", cancelLabel: "Отмена" }))) return;
  if (p) await deactivate(p);
  try {
    await api.pluginUninstall(folder);
  } catch (e) {
    toast("Не удалось удалить: " + e, "error");
    return;
  }
  delete state.pluginConfig.enabled[folder];
  delete state.pluginConfig.data[folder];
  delete state.pluginConfig.settings[folder];
  host.plugins = host.plugins.filter((x) => x.folder !== folder);
  toast(`Плагин «${name}» удалён`);
}

/** Copy a plugin folder into the plugins dir (install or update). */
export async function installFromFolder() {
  const src = await open({ directory: true, multiple: false, title: "Папка плагина (с файлом plugin.json)" });
  if (typeof src !== "string") return;
  let id: string;
  try {
    id = await api.pluginInstall(src);
  } catch (e) {
    toast("Не удалось установить: " + e, "error");
    return;
  }
  const wasEnabled = !!state.pluginConfig.enabled[id];
  await reloadPlugin(id);
  const p = pluginOf(id);
  if (!p) return;
  if (!p.manifest) toast(`Плагин скопирован, но в нём ошибка: ${p.error}`, "error");
  else if (wasEnabled) toast(`Плагин «${p.manifest.name}» обновлён до ${p.manifest.version}`);
  else await enablePlugin(id);
}

/** Create a starter plugin and open it in the editor. */
export async function createPlugin(id: string, name: string) {
  const dir = await api.pluginScaffold(id, name);
  state.pluginConfig.enabled[id] = true;
  await reloadPlugin(id);
  api.openInEditor(state.settings.editor, dir).catch(() => openPath(dir));
  return dir;
}

// ───────────────────────────── app events → plugins ─────────────────────────────

let started = false;
export function startPluginEvents() {
  if (started) return;
  started = true;
  setInfoEnricher(applyStacks);
  let lastOpened = "";
  watch(
    () => {
      const e = findProject(state.selected);
      return e && state.view === "project" && state.briefs[e.path]?.info ? e.path : null;
    },
    (path) => {
      if (!path || path === lastOpened) return;
      lastOpened = path;
      const e = findProject(path);
      if (e) bus.emit("project:open", projectContext(e) satisfies HangarEvents["project:open"]);
    },
    { immediate: true },
  );
  new MutationObserver(() => {
    bus.emit("theme:change", { dark: document.documentElement.dataset.theme !== "light" });
  }).observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
}
