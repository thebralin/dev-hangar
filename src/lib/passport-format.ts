// Built-in passport format (`.hangar/`): parsing and line-based editing.
// Pure module (no Vue / Tauri imports) so it can be tested with plain Node.

import type {
  PassportContext, PassportData, PassportFormat, PassportMilestone, PassportSection, PassportTask, ProjectStatus,
} from "../plugins/hangar";
// ───────────────────────────── helpers ─────────────────────────────

const TASK_RE = /^(\s*)[-*+]\s+\[( |x|X)\]\s?(.*)$/;
const HEADING_RE = /^(#{1,6})\s+(.+?)\s*#*\s*$/;

function lc(s: string) {
  return s.trim().toLowerCase().replace(/ё/g, "е");
}

function hash(s: string) {
  let h = 0;
  for (let i = 0; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) | 0;
  return (h >>> 0).toString(36);
}

// note: `\b` does not work with Cyrillic in JS regexps — use a Unicode lookahead instead
export function isDoneSection(title: string) {
  return /^(готово|сделано|выполнено|завершено|done|completed|finished)(?![\p{L}\p{N}])/iu.test(title.trim());
}
export function isDoingSection(title: string) {
  return /^(в работе|делаю|сейчас|in progress|doing|wip|current)(?![\p{L}\p{N}])/iu.test(title.trim());
}

const STATUS_WORDS: [ProjectStatus, RegExp][] = [
  ["idea", /^(idea|идея|черновик|draft)/],
  ["active", /^(active|активен|активный|в работе|разработка|development|wip)/],
  ["paused", /^(paused|пауза|на паузе|заморожен|frozen|on hold)/],
  ["done", /^(done|готов|завершен|released|выпущен|complete)/],
  ["archived", /^(archived|архив|deprecated|заброшен)/],
];
export function normStatus(s: string | undefined): ProjectStatus | undefined {
  if (!s) return undefined;
  const v = lc(s);
  return STATUS_WORDS.find(([, re]) => re.test(v))?.[0];
}

export const STATUS_LABELS: Record<ProjectStatus, { label: string; cls: string }> = {
  idea: { label: "Идея", cls: "purple" },
  active: { label: "В работе", cls: "green" },
  paused: { label: "На паузе", cls: "yellow" },
  done: { label: "Готов", cls: "blue" },
  archived: { label: "Архив", cls: "" },
};

function milestoneStatus(s: string): PassportMilestone["status"] | undefined {
  const v = lc(s);
  if (/^(план|planned|plan|todo|next|будущ|позже|later|idea|идея)/.test(v)) return "planned";
  if (/^(в работе|active|in progress|wip|current|текущ|делаю|doing)/.test(v)) return "active";
  if (/^(готово|done|released|выпущен|complete|завершен|shipped)/.test(v)) return "done";
  return undefined;
}

/** "Text #bug !high" → text + tags + priority */
export function parseTaskText(s: string) {
  const tags: string[] = [];
  let priority: PassportTask["priority"];
  const text = s
    .replace(/(^|\s)#([\p{L}\p{N}_][\p{L}\p{N}_\-./]*)/gu, (_, sp, t) => {
      tags.push(t);
      return sp;
    })
    .replace(/(^|\s)!(!|high|высокий|важно|срочно|urgent|low|низкий|потом)(?=\s|$)/giu, (_, sp, p) => {
      priority = /^(low|низкий|потом)$/i.test(p) ? "low" : "high";
      return sp;
    })
    .replace(/\s+/g, " ")
    .trim();
  return { text, tags, priority };
}

function splitFrontmatter(text: string) {
  const lines = text.replace(/^﻿/, "").split(/\r?\n/);
  const meta: Record<string, string> = {};
  if (lines[0]?.trim() !== "---") return { meta, body: lines, fmEnd: -1, lines };
  const end = lines.findIndex((l, i) => i > 0 && l.trim() === "---");
  if (end < 0) return { meta, body: lines, fmEnd: -1, lines };
  for (const l of lines.slice(1, end)) {
    const m = /^([\w.-]+)\s*:\s*(.*)$/.exec(l);
    if (m) meta[m[1].toLowerCase()] = m[2].trim().replace(/^["']|["']$/g, "");
  }
  return { meta, body: lines.slice(end + 1), fmEnd: end, lines };
}

function listValue(v: string | undefined) {
  if (!v) return [];
  return v.replace(/^\[|\]$/g, "").split(",").map((x) => x.trim().replace(/^["']|["']$/g, "")).filter(Boolean);
}

function trimBlock(lines: string[]) {
  return lines.join("\n").replace(/^\s*\n/, "").replace(/\s+$/, "");
}

// ───────────────────────────── built-in format: .hangar/ ─────────────────────────────

export const HANGAR_DIR = ".hangar";
const F = {
  project: ".hangar/project.md",
  tasks: ".hangar/tasks.md",
  roadmap: ".hangar/roadmap.md",
  readme: ".hangar/README.md",
  docs: ".hangar/docs",
};

const DESC_KEYS = /^(описание|description|о проекте|about|overview)$/;
const NOW_KEYS = /^(сейчас|now|статус|status|текущее состояние|current state|где остановились)$/;

function parseProject(text: string) {
  const { meta, body } = splitFrontmatter(text);
  let title: string | undefined;
  const chunks: { heading: string | null; lines: string[] }[] = [{ heading: null, lines: [] }];
  for (const line of body) {
    const h = HEADING_RE.exec(line);
    if (h && h[1].length === 1 && !title && chunks.length === 1 && !chunks[0].lines.some((l) => l.trim())) {
      title = h[2];
      continue;
    }
    if (h && h[1].length === 2) {
      chunks.push({ heading: h[2], lines: [] });
      continue;
    }
    chunks[chunks.length - 1].lines.push(line);
  }
  const desc: string[] = [];
  let now = "";
  for (const c of chunks) {
    const k = c.heading ? lc(c.heading) : "";
    if (c.heading && NOW_KEYS.test(k)) now = trimBlock(c.lines);
    else if (!c.heading || DESC_KEYS.test(k)) desc.push(trimBlock(c.lines));
    else desc.push(`## ${c.heading}\n${trimBlock(c.lines)}`);
  }
  return {
    name: meta.name || title,
    status: normStatus(meta.status),
    summary: meta.summary || meta.description || undefined,
    tags: listValue(meta.tags),
    description: desc.filter(Boolean).join("\n\n"),
    now,
  };
}

export function parseTasks(text: string): PassportSection[] {
  const sections: PassportSection[] = [];
  const seen = new Map<string, number>();
  let cur: PassportSection | null = null;
  let last: PassportTask | null = null;
  const section = (title: string) => {
    let s = sections.find((x) => lc(x.title) === lc(title));
    if (!s) sections.push((s = { title, tasks: [] }));
    return s;
  };
  text.split(/\r?\n/).forEach((line, i) => {
    const h = HEADING_RE.exec(line);
    if (h) {
      if (h[1].length >= 2) cur = section(h[2]);
      last = null;
      return;
    }
    const m = TASK_RE.exec(line);
    if (m && m[1].length < 2) {
      cur ??= section("Задачи");
      const p = parseTaskText(m[3]);
      const base = `${lc(cur.title)}|${lc(p.text)}`;
      const n = seen.get(base) ?? 0;
      seen.set(base, n + 1);
      last = {
        id: hash(base) + (n ? `-${n}` : ""),
        raw: line,
        done: m[2] !== " ",
        section: cur.title,
        line: i,
        ...p,
      };
      cur.tasks.push(last);
      return;
    }
    if (last && /^\s{2,}\S/.test(line)) {
      const sub = TASK_RE.exec(line);
      const t = sub ? `${sub[2] === " " ? "☐" : "☑"} ${sub[3]}` : line.trim();
      last.note = last.note ? `${last.note}\n${t}` : t;
      return;
    }
    if (line.trim()) last = null;
  });
  return sections;
}

function parseRoadmap(text: string): PassportMilestone[] {
  const out: PassportMilestone[] = [];
  let cur: { title: string; lines: string[] } | null = null;
  const flush = () => {
    if (!cur) return;
    let title = cur.title;
    let status: PassportMilestone["status"] | undefined;
    const tok = /\s*(?:`([^`]+)`|\[([^\]]+)\]|\(([^)]+)\))\s*$/.exec(title);
    if (tok) {
      const st = milestoneStatus(tok[1] || tok[2] || tok[3]);
      if (st) {
        status = st;
        title = title.slice(0, tok.index);
      }
    }
    if (!status) {
      if (/✅/.test(title)) status = "done";
      else if (/🚧/.test(title)) status = "active";
    }
    title = title.replace(/[✅🚧]/gu, "").replace(/[\s—–:-]+$/, "").trim();
    const items: PassportMilestone["items"] = [];
    const body: string[] = [];
    for (const l of cur.lines) {
      const m = TASK_RE.exec(l);
      if (m) items.push({ text: parseTaskText(m[3]).text, done: m[2] !== " " });
      else body.push(l);
    }
    if (!status) status = items.length && items.every((i) => i.done) ? "done" : "planned";
    const v = /^v?(\d+\.\d+(?:\.\d+)?(?:[-+][\w.]+)?)/.exec(title);
    out.push({ title, version: v?.[1], status, body: trimBlock(body), items });
  };
  for (const line of text.split(/\r?\n/)) {
    const h = HEADING_RE.exec(line);
    if (h && h[1].length === 2) {
      flush();
      cur = { title: h[2], lines: [] };
    } else if (cur) cur.lines.push(line);
  }
  flush();
  return out;
}

async function listDocs(ctx: PassportContext, rel: string, depth = 0): Promise<{ title: string; path: string }[]> {
  const out: { title: string; path: string }[] = [];
  for (const e of await ctx.list(rel)) {
    if (e.hidden) continue;
    if (e.isDir) {
      if (depth < 2) out.push(...(await listDocs(ctx, `${rel}/${e.name}`, depth + 1)));
    } else if (/\.(md|mdx|markdown|txt)$/i.test(e.name)) {
      const sub = rel.slice(F.docs.length + 1);
      const title = e.name.replace(/\.[^.]+$/, "").replace(/[-_]+/g, " ");
      out.push({ title: sub ? `${sub}/${title}` : title, path: e.path });
    }
  }
  return out.sort((a, b) => a.title.localeCompare(b.title, "ru", { numeric: true }));
}

// ---- editing (line based, keeps the rest of the file untouched) ----

async function editLines(ctx: PassportContext, rel: string, fn: (lines: string[]) => void, fallback = "") {
  const text = (await ctx.readText(rel)) ?? fallback;
  const eol = text.includes("\r\n") ? "\r\n" : "\n";
  const lines = text.split(/\r?\n/);
  fn(lines);
  let out = lines.join(eol);
  if (!out.endsWith(eol)) out += eol;
  await ctx.writeText(rel, out);
}

function sameTask(line: string, task: PassportTask) {
  const m = TASK_RE.exec(line);
  return !!m && m[1].length < 2 && parseTaskText(m[3]).text === task.text;
}

function locate(lines: string[], task: PassportTask) {
  if (task.line != null && lines[task.line] !== undefined && sameTask(lines[task.line], task)) return task.line;
  const i = lines.findIndex((l) => sameTask(l, task));
  if (i < 0) throw new Error("задача не найдена — файл изменился, обновите вкладку");
  return i;
}

function blockEnd(lines: string[], i: number) {
  let j = i + 1;
  while (j < lines.length && /^\s{2,}\S/.test(lines[j])) j++;
  return j;
}

/** Index to insert a new task line at the end of a section (creates the section if needed). */
function sectionInsertAt(lines: string[], title: string) {
  let h = lines.findIndex((l) => {
    const m = HEADING_RE.exec(l);
    return !!m && m[1].length >= 2 && lc(m[2]) === lc(title);
  });
  if (h < 0) {
    while (lines.length && !lines[lines.length - 1].trim()) lines.pop();
    lines.push("", `## ${title}`, "");
    return lines.length - 1;
  }
  let end = lines.findIndex((l, i) => i > h && /^#{1,6}\s/.test(l));
  if (end < 0) end = lines.length;
  let at = end;
  while (at > h + 1 && !lines[at - 1].trim()) at--;
  if (at === h + 1) {
    // empty section: keep one blank line after the heading
    lines.splice(h + 1, 0, "");
    return h + 2;
  }
  return at;
}

function setCheck(line: string, done: boolean) {
  return line.replace(/\[( |x|X)\]/, done ? "[x]" : "[ ]");
}

function sanitizeLine(s: string) {
  return s.replace(/\s*\r?\n\s*/g, " ").trim();
}

const TASKS_TEMPLATE = "# Задачи\n\n## В работе\n\n## Очередь\n\n## Готово\n";

export const PASSPORT_SPEC = `# Паспорт проекта (.hangar/)

Эта папка описывает проект для людей и ИИ-агентов (Claude Code, Cursor, Codex, …).
Её показывает приложение [Dev Hangar](https://github.com/thebralin/dev-hangar), но это обычный markdown —
его можно читать и править где угодно.

| Файл | Что внутри |
|---|---|
| \`project.md\` | Описание проекта, статус, раздел «Сейчас» |
| \`tasks.md\` | Задачи, сгруппированные по разделам |
| \`roadmap.md\` | Версии / этапы и их цели |
| \`docs/\` | Документация: архитектура, решения, инструкции (любые .md) |

## project.md

\`\`\`markdown
---
format: hangar/1
name: Название
status: active        # idea | active | paused | done | archived
summary: Одна строка — что это за проект
tags: web, vue, tauri
---

# Название

## Описание
Для кого проект, что умеет, как устроен — коротко.

## Сейчас
Что в работе и на чём остановились. Обновляй в конце каждой сессии.
\`\`\`

## tasks.md

Разделы \`## …\` — колонки доски (обычно «В работе», «Очередь», «Готово»). Задача — пункт-чекбокс без отступа:

\`\`\`markdown
## В работе
- [ ] Сворачивание сайдбара #ui !high
  Подробности — строками с отступом под задачей.

## Готово
- [x] Сетка проектов #ui
\`\`\`

- \`#тег\` — теги, \`!high\` / \`!low\` — приоритет.
- Начал задачу — перенеси её в «В работе». Закончил — отметь \`[x]\` и перенеси в «Готово».
- Не меняй формулировки чужих задач без нужды: по тексту задачи находит Dev Hangar.

## roadmap.md

\`\`\`markdown
## 0.9.0 — Режим ИИ \`в работе\`
Пара строк о цели версии.
- [x] Формат паспорта
- [ ] Вкладка в приложении

## 1.0.0 \`план\`
\`\`\`

Статус версии — в обратных кавычках в конце заголовка: \`план\`, \`в работе\`, \`готово\`
(или \`planned\`, \`active\`, \`done\`).

## Правила для ИИ-агента

1. Перед работой прочитай \`project.md\` и \`tasks.md\`.
2. По ходу работы обновляй задачи; новые найденные проблемы добавляй в «Очередь».
3. В конце сессии кратко обнови «Сейчас» в \`project.md\`; если закрыт этап роадмапа — отметь его.
4. Важные решения по архитектуре записывай в \`docs/\`.
`;

export const builtinFormat: PassportFormat = {
  id: "hangar",
  label: "Dev Hangar (.hangar/)",
  description: "Папка .hangar/ в корне проекта: project.md, tasks.md, roadmap.md, docs/",

  async detect(ctx) {
    return (await ctx.has(F.project)) || (await ctx.has(F.tasks));
  },

  async read(ctx): Promise<PassportData> {
    const [project, tasks, roadmap, hasReadme, hasDocs] = await Promise.all([
      ctx.readText(F.project),
      ctx.readText(F.tasks),
      ctx.readText(F.roadmap),
      ctx.has(F.readme),
      ctx.has(F.docs),
    ]);
    const p = project ? parseProject(project) : { tags: [], description: "", now: "" };
    return {
      ...p,
      sections: tasks ? parseTasks(tasks) : [],
      roadmap: roadmap ? parseRoadmap(roadmap) : [],
      docs: hasDocs ? await listDocs(ctx, F.docs) : [],
      files: {
        project: project != null ? ctx.abs(F.project) : undefined,
        tasks: tasks != null ? ctx.abs(F.tasks) : undefined,
        roadmap: roadmap != null ? ctx.abs(F.roadmap) : undefined,
        docs: hasDocs ? ctx.abs(F.docs) : undefined,
        readme: hasReadme ? ctx.abs(F.readme) : undefined,
      },
    };
  },

  async init(ctx, seed) {
    const date = new Date().toISOString().slice(0, 10);
    const summary = sanitizeLine(seed.summary);
    const next = (() => {
      const m = /^(\d+)\.(\d+)/.exec(seed.version ?? "");
      return m ? `${m[1]}.${+m[2] + 1}.0` : "0.1.0";
    })();
    const files: [string, string][] = [
      [F.readme, PASSPORT_SPEC],
      [
        F.project,
        `---\nformat: hangar/1\nname: ${sanitizeLine(seed.name)}\nstatus: active\nsummary: ${summary}\ntags: ${seed.stacks.join(", ")}\n---\n\n` +
          `# ${sanitizeLine(seed.name)}\n\n## Описание\n\n${summary || "Что это за проект, для кого он и что умеет."}\n\n` +
          `## Сейчас\n\nПаспорт создан ${date}. Здесь — что сейчас в работе и на чём остановились.\n`,
      ],
      [F.tasks, TASKS_TEMPLATE],
      [F.roadmap, `# Роадмап\n\n## ${next} \`план\`\n\nЦели следующей версии — пунктами «- [ ] …».\n`],
    ];
    for (const [rel, content] of files) {
      if (!(await ctx.has(rel))) await ctx.writeText(rel, content);
    }
  },

  async setTaskDone(ctx, task, done) {
    await editLines(ctx, F.tasks, (lines) => {
      const i = locate(lines, task);
      lines[i] = setCheck(lines[i], done);
    });
  },

  async addTask(ctx, section, text) {
    const t = sanitizeLine(text);
    if (!t) return;
    await editLines(
      ctx,
      F.tasks,
      (lines) => {
        const at = sectionInsertAt(lines, section);
        lines.splice(at, 0, `- [${isDoneSection(section) ? "x" : " "}] ${t}`);
      },
      TASKS_TEMPLATE,
    );
  },

  async moveTask(ctx, task, section) {
    await editLines(ctx, F.tasks, (lines) => {
      const i = locate(lines, task);
      const block = lines.splice(i, blockEnd(lines, i) - i);
      if (isDoneSection(section)) block[0] = setCheck(block[0], true);
      else if (isDoneSection(task.section)) block[0] = setCheck(block[0], false);
      const at = sectionInsertAt(lines, section);
      lines.splice(at, 0, ...block);
    });
  },

  async setStatus(ctx, status) {
    await editLines(
      ctx,
      F.project,
      (lines) => {
        if (lines[0]?.trim() === "---") {
          const end = lines.findIndex((l, i) => i > 0 && l.trim() === "---");
          if (end > 0) {
            const i = lines.findIndex((l, k) => k > 0 && k < end && /^status\s*:/i.test(l));
            if (i > 0) lines[i] = `status: ${status}`;
            else lines.splice(end, 0, `status: ${status}`);
            return;
          }
        }
        lines.unshift("---", "format: hangar/1", `status: ${status}`, "---", "");
      },
      `# ${ctx.name}\n`,
    );
  },
};

