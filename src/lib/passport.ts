// Project passport: description, status, tasks, roadmap and docs stored in the repository,
// readable by people and AI agents alike. Built-in format: the `.hangar/` folder (see PASSPORT_SPEC).
// Plugins can add their own formats (hangar.passport.registerFormat) and task buttons.

import { markRaw, reactive } from "vue";
import type {
  Passport, PassportContext, PassportData, PassportFormat, PassportMilestone, PassportSection, PassportSeed,
  PassportTask, ProjectStatus, TaskActionContribution,
} from "../plugins/hangar";
import { api, joinPath } from "./api";
import { displayName, findProject, state, toast } from "./store";
import { emit } from "../plugins/bus";

// ───────────────────────────── registry ─────────────────────────────

export interface RegisteredFormat {
  /** "hangar" for the built-in one, "<pluginId>:<formatId>" for plugins */
  key: string;
  pluginId: string | null;
  format: PassportFormat;
  /** may write files through ctx.writeText */
  canWrite: boolean;
}

export interface RegisteredTaskAction {
  key: string;
  pluginId: string;
  pluginName: string;
  action: TaskActionContribution;
}

export const passportHost = reactive({
  formats: [] as RegisteredFormat[],
  taskActions: [] as RegisteredTaskAction[],
});

export interface PassportState {
  loading: boolean;
  data: Passport | null;
  error?: string;
  formatKey?: string;
  loadedAt?: number;
}

/** projectPath → passport */
export const passports = reactive<Record<string, PassportState>>({});

/** Plugin formats first (they may override the built-in one), the built-in `.hangar/` last. */
export function formatsInOrder() {
  return [...passportHost.formats.filter((f) => f.pluginId), ...passportHost.formats.filter((f) => !f.pluginId)];
}

// ───────────────────────────── context ─────────────────────────────

function cleanRel(rel: string) {
  const parts = rel.replace(/\\/g, "/").split("/").filter((x) => x && x !== ".");
  if (/^([a-zA-Z]:|[\\/])/.test(rel) || parts.includes("..")) throw new Error(`путь должен быть внутри папки проекта: ${rel}`);
  return parts.join("/");
}

export function passportContext(projectPath: string, canWrite: boolean, who: string): PassportContext {
  const abs = (rel: string) => {
    const r = cleanRel(rel);
    return r ? joinPath(projectPath, r) : projectPath;
  };
  const entry = findProject(projectPath);
  return {
    projectPath,
    name: entry ? displayName(entry) : projectPath,
    has: (rel) => api.pathExists(abs(rel)),
    async readText(rel) {
      const p = abs(rel);
      if (!(await api.pathExists(p))) return null;
      const f = await api.readText(p);
      if (f.binary) return null;
      if (f.truncated) throw new Error(`${rel}: файл слишком большой`);
      return f.content;
    },
    list: (rel = "") => api.listDir(abs(rel)).catch(() => []),
    async writeText(rel, content) {
      if (!canWrite) throw new Error(`формату «${who}» нужно разрешение "fs:write"`);
      await api.writeTextFile(abs(rel), content);
    },
    abs,
  };
}

function ctxFor(rf: RegisteredFormat, projectPath: string) {
  return passportContext(projectPath, rf.canWrite, rf.format.label);
}

import { builtinFormat, isDoneSection, isDoingSection } from "./passport-format";
export * from "./passport-format";

passportHost.formats.push(markRaw({ key: "hangar", pluginId: null, format: markRaw(builtinFormat), canWrite: true }));

// ───────────────────────────── loading ─────────────────────────────

const inflight = new Map<string, Promise<Passport | null>>();

export function loadPassport(projectPath: string): Promise<Passport | null> {
  const running = inflight.get(projectPath);
  if (running) return running;
  const p = doLoad(projectPath).finally(() => inflight.delete(projectPath));
  inflight.set(projectPath, p);
  return p;
}

async function doLoad(projectPath: string): Promise<Passport | null> {
  const prev = passports[projectPath];
  passports[projectPath] = { ...(prev ?? { data: null }), loading: true };
  try {
    for (const rf of formatsInOrder()) {
      const ctx = ctxFor(rf, projectPath);
      let found = false;
      try {
        found = !!(await rf.format.detect(ctx));
      } catch (e) {
        console.error(`[passport] ${rf.key}.detect failed`, e);
      }
      if (!found) continue;
      const d = await rf.format.read(ctx);
      const f = rf.format;
      const data: Passport = {
        tags: [], description: "", now: "", sections: [], roadmap: [], docs: [], files: {},
        // plugin formats may leave fields out — keep the defaults then
        ...(d as Partial<PassportData>),
        format: rf.key,
        formatLabel: f.label,
        editable: { tasks: !!(f.setTaskDone && f.addTask && f.moveTask), status: !!f.setStatus },
      };
      passports[projectPath] = { loading: false, data, formatKey: rf.key, loadedAt: Date.now() };
      emit("passport:change", { projectPath, passport: data });
      return data;
    }
    passports[projectPath] = { loading: false, data: null, loadedAt: Date.now() };
    if (prev?.data) emit("passport:change", { projectPath, passport: null });
    return null;
  } catch (e) {
    passports[projectPath] = { loading: false, data: prev?.data ?? null, formatKey: prev?.formatKey, error: e instanceof Error ? e.message : String(e), loadedAt: Date.now() };
    return prev?.data ?? null;
  }
}

/** Load passports of many projects (grid view), 4 at a time, skipping fresh ones. */
export async function loadPassports(paths: string[], maxAgeMs = 60_000) {
  const queue = paths.filter((p) => {
    const s = passports[p];
    return !s || (!s.loading && Date.now() - (s.loadedAt ?? 0) > maxAgeMs);
  });
  const worker = async () => {
    while (queue.length) await loadPassport(queue.shift()!);
  };
  await Promise.all([worker(), worker(), worker(), worker()]);
}

// ───────────────────────────── editing ─────────────────────────────

function formatOf(projectPath: string) {
  const key = passports[projectPath]?.formatKey;
  const rf = passportHost.formats.find((f) => f.key === key);
  if (!rf) throw new Error("формат паспорта не найден (плагин выключен?)");
  return rf;
}

async function edit(projectPath: string, what: string, fn: (f: PassportFormat, ctx: PassportContext) => unknown) {
  try {
    const rf = formatOf(projectPath);
    await fn(rf.format, ctxFor(rf, projectPath));
  } catch (e) {
    toast(`${what}: ${e instanceof Error ? e.message : e}`, "error");
  }
  await loadPassport(projectPath);
}

export const passportOps = {
  setDone: (path: string, task: PassportTask, done: boolean) =>
    edit(path, "Не удалось отметить задачу", (f, ctx) => f.setTaskDone?.(ctx, task, done)),

  /** Check a task; if the board has a «Готово» column, move it there. */
  complete(path: string, task: PassportTask) {
    const p = passports[path]?.data;
    const done = p?.sections.find((s) => isDoneSection(s.title));
    if (done && !isDoneSection(task.section)) return passportOps.move(path, task, done.title);
    return passportOps.setDone(path, task, true);
  },

  add: (path: string, section: string, text: string) =>
    edit(path, "Не удалось добавить задачу", (f, ctx) => f.addTask?.(ctx, section, text)),

  move: (path: string, task: PassportTask, section: string) =>
    edit(path, "Не удалось перенести задачу", (f, ctx) => f.moveTask?.(ctx, task, section)),

  setStatus: (path: string, status: ProjectStatus) =>
    edit(path, "Не удалось сменить статус", (f, ctx) => f.setStatus?.(ctx, status)),
};

// ───────────────────────────── creating ─────────────────────────────

export function seedOf(projectPath: string): PassportSeed {
  const entry = findProject(projectPath);
  const info = state.briefs[projectPath]?.info;
  return {
    name: entry ? displayName(entry) : projectPath,
    summary: info?.root.description || "",
    stacks: (info?.stacks ?? []).filter((s) => s.id !== "unknown").map((s) => s.id),
    version: info?.root.version,
  };
}

const POINTER_START = "<!-- hangar:passport -->";
const POINTER = `${POINTER_START}
## Паспорт проекта

Описание, статус, задачи и роадмап проекта лежат в \`.hangar/\` (формат — в \`.hangar/README.md\`).
Перед работой прочитай \`.hangar/project.md\` и \`.hangar/tasks.md\`. По ходу работы отмечай задачи,
в конце сессии коротко обнови раздел «Сейчас» в \`.hangar/project.md\`.
<!-- /hangar:passport -->
`;

/** Agent instruction files a passport pointer can go into. */
export const AGENT_FILES = ["CLAUDE.md", "AGENTS.md", ".cursorrules", ".github/copilot-instructions.md", "GEMINI.md"];

export async function agentFilesOf(projectPath: string) {
  const out: { name: string; exists: boolean; linked: boolean }[] = [];
  for (const name of AGENT_FILES) {
    const p = joinPath(projectPath, name);
    const exists = await api.pathExists(p);
    let linked = false;
    if (exists) linked = (await api.readText(p).catch(() => null))?.content.includes(POINTER_START) ?? false;
    out.push({ name, exists, linked });
  }
  return out;
}

/** Append the "read .hangar/" instruction to CLAUDE.md / AGENTS.md (creates the file if missing). */
export async function linkAgentFile(projectPath: string, name: string) {
  const p = joinPath(projectPath, name);
  const cur = (await api.pathExists(p)) ? (await api.readText(p)).content : null;
  if (cur?.includes(POINTER_START)) return;
  const entry = findProject(projectPath);
  const base = cur ?? `# ${entry ? displayName(entry) : "Project"}\n`;
  const eol = base.includes("\r\n") ? "\r\n" : "\n";
  const text = base.replace(/\s*$/, "") + eol + eol + POINTER.replace(/\n/g, eol);
  await api.writeTextFile(p, text);
}

export async function initPassport(projectPath: string, formatKey = "hangar", link: string[] = []) {
  const rf = passportHost.formats.find((f) => f.key === formatKey);
  if (!rf?.format.init) throw new Error("этот формат не умеет создавать паспорт");
  await rf.format.init(ctxFor(rf, projectPath), seedOf(projectPath));
  for (const name of link) await linkAgentFile(projectPath, name);
  return loadPassport(projectPath);
}

// ───────────────────────────── prompts ─────────────────────────────

function rel(projectPath: string, abs?: string) {
  if (!abs) return undefined;
  const r = abs.slice(projectPath.length).replace(/^[\\/]+/, "").replace(/\\/g, "/");
  return r || undefined;
}

export async function taskPrompt(projectPath: string, task: PassportTask): Promise<string> {
  const p = passports[projectPath]?.data ?? (await loadPassport(projectPath));
  const entry = findProject(projectPath);
  const name = entry ? displayName(entry) : "";
  const tasksFile = rel(projectPath, p?.files.tasks) ?? "файле задач";
  const projectFile = rel(projectPath, p?.files.project);
  const doing = p?.sections.find((s) => isDoingSection(s.title))?.title;
  const done = p?.sections.find((s) => isDoneSection(s.title))?.title;
  const parts = [
    `Проект «${name}». Возьми задачу из ${tasksFile} (раздел «${task.section}»): «${task.text}».`,
  ];
  if (task.note) parts.push(`Подробности: ${task.note.replace(/\s*\n\s*/g, "; ")}.`);
  if (p?.files.readme) parts.push(`Формат паспорта проекта описан в ${rel(projectPath, p.files.readme)}.`);
  if (projectFile) parts.push(`Сначала прочитай ${projectFile}.`);
  parts.push(
    (doing ? `Перенеси задачу в раздел «${doing}», сделай её` : "Сделай задачу") +
      (done ? `, затем отметь выполненной и перенеси в «${done}».` : ", затем отметь её выполненной."),
  );
  if (projectFile) parts.push(`В конце коротко обнови раздел «Сейчас» в ${projectFile}.`);
  return parts.join(" ");
}

export async function fillPrompt(projectPath: string): Promise<string> {
  const p = passports[projectPath]?.data ?? (await loadPassport(projectPath));
  const where = p && p.format !== "hangar" ? `паспорт проекта (формат: ${p.formatLabel})` : "паспорт проекта в папке .hangar/ (формат описан в .hangar/README.md)";
  return [
    `Заполни ${where} по коду и файлам репозитория.`,
    "Опиши проект: summary, статус, теги, раздел «Описание» (что это, для кого, как устроено) и «Сейчас» (что в работе, судя по git и коду).",
    "Перенеси в задачи то, что найдёшь в TODO/FIXME, TODO.md, issues-заметках; планы версий (ROADMAP, CHANGELOG — для выпущенных) — в роадмап.",
    "Кратко опиши архитектуру и ключевые решения в .hangar/docs/.",
    "Не выдумывай: только то, что видно в репозитории. Существующее не удаляй, а дополняй.",
  ].join(" ");
}

// ───────────────────────────── stats for cards ─────────────────────────────

export function taskStats(p: Passport | null | undefined) {
  let total = 0;
  let done = 0;
  for (const s of p?.sections ?? []) {
    for (const t of s.tasks) {
      total++;
      if (t.done) done++;
    }
  }
  const doing = p?.sections.find((s) => isDoingSection(s.title))?.tasks.filter((t) => !t.done).length ?? 0;
  return { total, done, open: total - done, doing };
}

export function currentMilestone(p: Passport | null | undefined) {
  return p?.roadmap.find((m) => m.status === "active") ?? p?.roadmap.find((m) => m.status === "planned");
}
