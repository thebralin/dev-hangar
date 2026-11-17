import { reactive, watch } from "vue";
import { listen } from "@tauri-apps/api/event";
import { AnsiUp } from "ansi_up";
import { api, joinPath, baseName, type Action, type Checkpoint, type GitSummary, type ProjectInfo } from "./api";
import { openPath } from "@tauri-apps/plugin-opener";
import { emit } from "../plugins/bus";

/** Persisted plugin state (config.json → "plugins"). */
export interface PluginsConfig {
  enabled: Record<string, boolean>;
  /** hangar.storage, per plugin id */
  data: Record<string, Record<string, unknown>>;
  /** values for manifest.settings, per plugin id */
  settings: Record<string, Record<string, unknown>>;
}

export interface CustomCommand { id: string; label: string; command: string; cwd?: string; kind?: string }

export interface ProjectEntry {
  path: string;
  alias?: string;
  pinned?: boolean;
  addedAt: number;
  lastOpenedAt?: number;
  /** previous session's open time, for the "where I left off" card */
  prevOpenedAt?: number;
  notes?: string;
  customCommands?: CustomCommand[];
}

export interface Settings {
  editor: string;
  terminal: string;
  theme: "dark" | "light" | "system";
  groupByFolder: boolean;
  sortBy: "activity" | "name" | "added";
  showHidden: boolean;
  graphLayout: "force" | "layers";
}

export interface ProcLine { html: string; stream: string }
export interface Proc {
  id: string;
  projectPath: string;
  label: string;
  command: string;
  cwd: string;
  status: "running" | "exited" | "failed";
  exitCode?: number | null;
  startedAt: number;
  endedAt?: number;
  lines: ProcLine[];
  url?: string;
  ansi: AnsiUp;
  kind: string;
}

export interface Brief {
  info?: ProjectInfo;
  /** info as returned by Rust, before plugin stacks were applied */
  raw?: ProjectInfo;
  git?: GitSummary;
  loading: boolean;
  error?: string;
}

const defaultSettings: Settings = {
  editor: "code",
  terminal: "wt",
  theme: "dark",
  groupByFolder: true,
  sortBy: "activity",
  showHidden: true,
  graphLayout: "layers",
};

export const state = reactive({
  loaded: false,
  projects: [] as ProjectEntry[],
  settings: { ...defaultSettings } as Settings,
  selected: null as string | null,
  briefs: {} as Record<string, Brief>,
  procs: {} as Record<string, Proc>,
  procOrder: [] as string[],
  tab: "overview" as string,
  activeProc: {} as Record<string, string>, // projectPath -> proc id
  toast: null as null | { text: string; kind: "info" | "error" },
  detectedEditors: [] as string[],
  collapsedGroups: {} as Record<string, boolean>,
  view: "project" as "project" | "disk" | "plugins" | `page:${string}`,
  pluginConfig: { enabled: {}, data: {}, settings: {} } as PluginsConfig,
  checkpoints: {} as Record<string, Checkpoint[]>,
});

export async function loadCheckpoints(projectPath: string) {
  try {
    state.checkpoints[projectPath] = await api.checkpointList(projectPath);
  } catch {
    state.checkpoints[projectPath] = [];
  }
  return state.checkpoints[projectPath];
}

export async function createCheckpoint(projectPath: string, message: string) {
  try {
    const cp = await api.checkpointCreate(projectPath, message);
    await loadCheckpoints(projectPath);
    toast(`Чекпоинт создан: ${cp.message}`);
    return cp;
  } catch (e) {
    toast("Не удалось создать чекпоинт: " + e, "error");
  }
}

let toastTimer: number | undefined;
export function toast(text: string, kind: "info" | "error" = "info") {
  state.toast = { text, kind };
  clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => (state.toast = null), kind === "error" ? 6000 : 3000);
}

export function norm(p: string) {
  return p.replace(/[\\/]+$/, "").toLowerCase();
}

// ---------------- config ----------------

let saveTimer: number | undefined;
function scheduleSave() {
  if (!state.loaded) return;
  clearTimeout(saveTimer);
  saveTimer = window.setTimeout(() => {
    api
      .saveConfig({
        projects: state.projects,
        settings: state.settings,
        collapsedGroups: state.collapsedGroups,
        selected: state.selected,
        plugins: state.pluginConfig,
      })
      .catch((e) => toast("Не удалось сохранить настройки: " + e, "error"));
  }, 300);
}

export async function loadConfig() {
  try {
    const cfg = await api.loadConfig();
    state.projects = Array.isArray(cfg.projects) ? cfg.projects : [];
    state.settings = { ...defaultSettings, ...(cfg.settings || {}) };
    state.collapsedGroups = cfg.collapsedGroups || {};
    if (cfg.selected && state.projects.some((p) => p.path === cfg.selected)) state.selected = cfg.selected;
    const pc = cfg.plugins || {};
    state.pluginConfig = { enabled: pc.enabled || {}, data: pc.data || {}, settings: pc.settings || {} };
  } catch (e) {
    toast("Ошибка чтения конфига: " + e, "error");
  }
  state.loaded = true;
  watch(() => [state.projects, state.settings, state.collapsedGroups, state.selected, state.pluginConfig], scheduleSave, { deep: true });
  api.detectEditors().then((e) => (state.detectedEditors = e)).catch(() => {});
  refreshAllBriefs();
}

// ---------------- projects ----------------

export function findProject(path: string | null) {
  if (!path) return undefined;
  return state.projects.find((p) => norm(p.path) === norm(path));
}

export async function addProject(path: string, select = true) {
  const existing = findProject(path);
  if (existing) {
    if (select) selectProject(existing.path);
    return existing;
  }
  const entry: ProjectEntry = { path: path.replace(/[\\/]+$/, ""), addedAt: Date.now() };
  state.projects.push(entry);
  await refreshBrief(entry.path);
  if (select) selectProject(entry.path);
  return entry;
}

export function removeProject(path: string) {
  state.projects = state.projects.filter((p) => norm(p.path) !== norm(path));
  delete state.briefs[path];
  if (state.selected && norm(state.selected) === norm(path)) state.selected = state.projects[0]?.path ?? null;
}

export function selectProject(path: string) {
  state.selected = path;
  const p = findProject(path);
  if (p) {
    // only count as a new "visit" if the last one was a while ago
    if (!p.lastOpenedAt || Date.now() - p.lastOpenedAt > 30 * 60_000) p.prevOpenedAt = p.lastOpenedAt;
    p.lastOpenedAt = Date.now();
  }
  state.view = "project";
}

// ---------------- plugin stacks ----------------

/** Set by the plugin host: applies plugin stack detectors on top of the Rust analysis. */
type InfoEnricher = (raw: ProjectInfo) => Promise<ProjectInfo>;
let enricher: InfoEnricher | null = null;
export function setInfoEnricher(fn: InfoEnricher | null) {
  enricher = fn;
}
async function enrich(raw: ProjectInfo): Promise<ProjectInfo> {
  if (!enricher) return raw;
  try {
    return await enricher(raw);
  } catch (e) {
    console.error("[plugins] stack detection failed", e);
    return raw;
  }
}
/** Analyze a folder: Rust detection + plugin stacks. */
export async function analyzeProject(path: string) {
  return enrich(await api.analyze(path));
}

/** Re-apply plugin stacks to every loaded project (after plugins were enabled/disabled). */
export async function reenrichAll() {
  const paths = Object.keys(state.briefs);
  const worker = async () => {
    while (paths.length) {
      const p = paths.shift()!;
      const b = state.briefs[p];
      if (b?.raw) b.info = await enrich(b.raw);
    }
  };
  await Promise.all([worker(), worker(), worker()]);
}

export async function refreshBrief(path: string) {
  const b: Brief = state.briefs[path] || { loading: true };
  b.loading = true;
  state.briefs[path] = b;
  try {
    const [raw, git] = await Promise.all([api.analyze(path), api.gitSummary(path).catch(() => undefined)]);
    const info = await enrich(raw);
    state.briefs[path] = { info, raw, git, loading: false };
  } catch (e) {
    state.briefs[path] = { ...b, loading: false, error: String(e) };
  }
}

export async function refreshAllBriefs() {
  const queue = [...state.projects.map((p) => p.path)];
  // selected first
  if (state.selected) {
    const i = queue.indexOf(state.selected);
    if (i > 0) queue.unshift(...queue.splice(i, 1));
  }
  const worker = async () => {
    while (queue.length) {
      const p = queue.shift()!;
      await refreshBrief(p);
    }
  };
  await Promise.all([worker(), worker(), worker()]);
}

export function activityOf(path: string): number {
  const b = state.briefs[path];
  const c = b?.git?.lastCommit?.date ? Date.parse(b.git.lastCommit.date) : 0;
  const m = b?.info?.modified ?? 0;
  return Math.max(c, m);
}

/** Scan a parent folder and add every recognized project (depth ≤ 2). */
export async function scanFolder(parent: string): Promise<number> {
  let added = 0;
  const looksLikeProject = async (dir: string) => {
    const info = await analyzeProject(dir).catch(() => null);
    if (!info) return false;
    return info.isGit || !info.stacks.some((s) => s.id === "unknown");
  };
  const visit = async (dir: string, depth: number) => {
    const entries = await api.listDir(dir).catch(() => []);
    for (const e of entries) {
      if (!e.isDir || e.heavy || e.hidden) continue;
      if (await looksLikeProject(e.path)) {
        if (!findProject(e.path)) {
          await addProject(e.path, false);
          added++;
        }
      } else if (depth < 2) {
        await visit(e.path, depth + 1);
      }
    }
  };
  if (await looksLikeProject(parent)) {
    if (!findProject(parent)) {
      await addProject(parent, false);
      added++;
    }
  } else {
    await visit(parent, 1);
  }
  return added;
}

// ---------------- processes ----------------

const URL_RE = /https?:\/\/(?:localhost|127\.0\.0\.1|0\.0\.0\.0|\[::1?\]|(?:\d{1,3}\.){3}\d{1,3})(?::\d+)?(?:\/[^\s'"`)\]]*)?/i;
// eslint-disable-next-line no-control-regex
const ANSI_RE = /\u001b\[[0-9;?]*[ -/]*[@-~]|\u001b\][^\u0007]*\u0007/g;
const MAX_LINES = 5000;

export function procId(projectPath: string, actionId: string) {
  return `${projectPath}::${actionId}`;
}

export async function runAction(projectPath: string, action: Action | CustomCommand, labelPrefix = "") {
  const kind = (action as Action).kind || (action as CustomCommand).kind || "script";
  const cwd = joinPath(projectPath, action.cwd || "");
  if (kind === "open") {
    try {
      await openPath(joinPath(cwd, action.command));
    } catch (e) {
      toast("Не удалось открыть: " + e, "error");
    }
    return;
  }
  const id = procId(projectPath, action.id);
  const existing = state.procs[id];
  if (existing?.status === "running") {
    state.activeProc[projectPath] = id;
    state.tab = "console";
    toast("Уже запущено");
    return;
  }
  const proc: Proc = {
    id,
    projectPath,
    label: (labelPrefix ? labelPrefix + " · " : "") + action.label,
    command: action.command,
    cwd,
    status: "running",
    startedAt: Date.now(),
    lines: [],
    ansi: new AnsiUp(),
    kind,
  };
  proc.ansi.use_classes = true;
  proc.lines.push({ html: `<span class="sys">$ ${escapeHtml(action.command)}   <span class="dim">(${escapeHtml(cwd)})</span></span>`, stream: "sys" });
  state.procs[id] = proc;
  if (!state.procOrder.includes(id)) state.procOrder.push(id);
  state.activeProc[projectPath] = id;
  state.tab = "console";
  emit("process:start", { projectPath, label: proc.label, command: proc.command, kind });
  try {
    await api.run(id, cwd, action.command);
  } catch (e) {
    const p = state.procs[id];
    p.status = "failed";
    p.lines.push({ html: `<span class="err">${escapeHtml(String(e))}</span>`, stream: "sys" });
  }
}

export async function stopProc(id: string) {
  try {
    await api.kill(id);
  } catch (e) {
    toast(String(e), "error");
  }
}

export async function restartProc(id: string) {
  const p = state.procs[id];
  if (!p) return;
  if (p.status === "running") {
    await stopProc(id);
    await new Promise<void>((resolve) => {
      const t = setInterval(() => {
        if (state.procs[id]?.status !== "running") {
          clearInterval(t);
          resolve();
        }
      }, 100);
      setTimeout(() => { clearInterval(t); resolve(); }, 5000);
    });
  }
  const actionId = id.slice(p.projectPath.length + 2);
  await runAction(p.projectPath, { id: actionId, label: p.label, command: p.command, cwd: p.cwd.slice(p.projectPath.length).replace(/^[\\/]+/, ""), kind: p.kind });
}

export function closeProc(id: string) {
  const p = state.procs[id];
  if (!p || p.status === "running") return;
  delete state.procs[id];
  state.procOrder = state.procOrder.filter((x) => x !== id);
  if (state.activeProc[p.projectPath] === id) {
    const next = state.procOrder.find((x) => state.procs[x]?.projectPath === p.projectPath);
    if (next) state.activeProc[p.projectPath] = next;
    else delete state.activeProc[p.projectPath];
  }
}

export function escapeHtml(s: string) {
  return s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!);
}

export function isRunning(projectPath: string) {
  return Object.values(state.procs).some((p) => p.projectPath === projectPath && p.status === "running");
}

export async function initProcessEvents() {
  await listen<{ id: string; stream: string; data: string }>("proc://output", (e) => {
    const p = state.procs[e.payload.id];
    if (!p) return;
    let data = e.payload.data.replace(/\r?\n$/, "");
    // handle carriage-return progress output: keep the last segment
    if (data.includes("\r")) {
      const segs = data.split("\r").filter((s) => s.length);
      data = segs[segs.length - 1] ?? "";
    }
    const plain = data.replace(ANSI_RE, "");
    if (!p.url) {
      const m = plain.match(URL_RE);
      if (m) p.url = m[0].replace("0.0.0.0", "localhost").replace(/[.,;:]+$/, "");
    }
    p.lines.push({ html: p.ansi.ansi_to_html(data) || "&nbsp;", stream: e.payload.stream });
    if (p.lines.length > MAX_LINES) p.lines.splice(0, p.lines.length - MAX_LINES);
  });
  await listen<{ id: string; code: number | null }>("proc://exit", (e) => {
    const p = state.procs[e.payload.id];
    if (!p) return;
    p.status = e.payload.code === 0 ? "exited" : "failed";
    p.exitCode = e.payload.code;
    p.endedAt = Date.now();
    const dur = ((p.endedAt - p.startedAt) / 1000).toFixed(1);
    p.lines.push({
      html: `<span class="sys">— процесс завершён${e.payload.code != null ? ` с кодом ${e.payload.code}` : ""} за ${dur} с —</span>`,
      stream: "sys",
    });
    emit("process:exit", {
      projectPath: p.projectPath, label: p.label, command: p.command, kind: p.kind,
      code: e.payload.code, durationMs: p.endedAt - p.startedAt,
    });
    if (p.kind === "build" || p.kind === "install") {
      toast(`${p.label}: ${e.payload.code === 0 ? "готово ✓" : "ошибка (код " + e.payload.code + ")"}`, e.payload.code === 0 ? "info" : "error");
    }
  });
}

export function displayName(entry: ProjectEntry) {
  return entry.alias || state.briefs[entry.path]?.info?.name || baseName(entry.path);
}
