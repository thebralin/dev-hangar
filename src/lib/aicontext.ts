import { api, joinPath, type GitSummary, type ProjectInfo } from "./api";
import type { ProjectEntry } from "./store";

export interface AiOptions {
  run: boolean;
  tree: boolean;
  treeDepth: number;
  deps: boolean;
  devDeps: boolean;
  commits: boolean;
  changes: boolean;
  notes: boolean;
  readme: boolean;
  env: boolean;
}

export const PRESET_PROMPT: AiOptions = {
  run: true, tree: true, treeDepth: 4, deps: true, devDeps: false, commits: true, changes: true, notes: true, readme: false, env: true,
};
export const PRESET_FILE: AiOptions = {
  run: true, tree: true, treeDepth: 3, deps: true, devDeps: true, commits: false, changes: false, notes: true, readme: false, env: true,
};

function fence(text: string, lang = "") {
  const f = text.includes("```") ? "~~~~" : "```";
  return `${f}${lang}\n${text.replace(/\n+$/, "")}\n${f}`;
}

export async function buildAiContext(entry: ProjectEntry, info: ProjectInfo, git: GitSummary | undefined, o: AiOptions): Promise<string> {
  const out: string[] = [];
  const name = entry.alias || info.name;
  out.push(`# ${name}`);
  const desc = info.root.description;
  if (desc) out.push(desc);
  const meta: string[] = [];
  meta.push(`- **Стек:** ${info.stacks.map((s) => s.label).join(", ")}`);
  if (info.root.packageManager) meta.push(`- **Менеджер пакетов:** ${info.root.packageManager}`);
  if (info.root.nodeVersion) meta.push(`- **Node:** ${info.root.nodeVersion}`);
  if (info.root.phpVersion) meta.push(`- **PHP:** ${info.root.phpVersion}`);
  if (info.root.version) meta.push(`- **Версия:** ${info.root.version}`);
  if (git?.isRepo && git.remotes[0]) meta.push(`- **Репозиторий:** ${git.remotes[0].webUrl || git.remotes[0].url}`);
  out.push(meta.join("\n"));

  const modules = [info.root, ...info.subprojects].filter((m) => m.actions.length || m.scripts.length || m.composerScripts?.length);

  if (o.run) {
    const lines: string[] = [];
    for (const m of modules) {
      const where = m.relPath ? ` (в папке \`${m.relPath}/\`)` : "";
      for (const a of m.actions.filter((a) => a.kind !== "open")) {
        lines.push(`- ${a.label}${where}: \`${a.command}\``);
      }
    }
    for (const c of entry.customCommands ?? []) {
      lines.push(`- ${c.label}${c.cwd ? ` (в папке \`${c.cwd}/\`)` : ""}: \`${c.command}\``);
    }
    if (lines.length) out.push(`## Как запускать\n${lines.join("\n")}`);

    for (const m of modules.filter((m) => m.scripts.length)) {
      const title = m.relPath ? `## Скрипты: ${m.relPath}` : "## Скрипты";
      out.push(`${title}\n${m.scripts.map(([k, v]) => `- \`${k}\`: \`${v}\``).join("\n")}`);
    }
    for (const m of modules.filter((m) => m.composerScripts?.length)) {
      const title = m.relPath ? `## Скрипты composer: ${m.relPath}` : "## Скрипты composer";
      out.push(`${title}\n${m.composerScripts.map(([k, v]) => `- \`composer run ${k}\`: \`${v}\``).join("\n")}`);
    }
  }

  if (o.tree) {
    const tree = await api.fileTree(entry.path, o.treeDepth, 300).catch(() => "");
    if (tree.trim()) out.push(`## Структура проекта\n${fence(tree, "text")}`);
  }

  if (o.deps) {
    const groups = new Map<string, string[]>();
    for (const d of info.dependencies) {
      if (d.dev && !o.devDeps) continue;
      const key = `${d.scope || "."}|${d.source}|${d.dev ? "dev" : "prod"}`;
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key)!.push(`${d.name}@${d.version}`);
    }
    if (groups.size) {
      const parts: string[] = [];
      for (const [k, v] of groups) {
        const [scope, source, kind] = k.split("|");
        parts.push(`- **${scope === "." ? "" : scope + " · "}${source}${kind === "dev" ? " (dev)" : ""}:** ${v.join(", ")}`);
      }
      out.push(`## Зависимости\n${parts.join("\n")}`);
    }
  }

  if (o.env) {
    const example = info.envFiles.find((f) => /^\.env\.(example|sample|template)$/i.test(f));
    const src = example || info.envFiles.find((f) => f === ".env");
    if (src) {
      try {
        const f = await api.readText(joinPath(entry.path, src));
        const keys = f.content
          .split(/\r?\n/)
          .map((l) => l.trim())
          .filter((l) => l && !l.startsWith("#") && l.includes("="))
          .map((l) => l.split("=")[0].replace(/^export\s+/, "").trim());
        if (keys.length) out.push(`## Переменные окружения (из ${src}, только имена)\n${keys.map((k) => `- \`${k}\``).join("\n")}`);
      } catch {
        /* ignore */
      }
    }
  }

  if (git?.isRepo && (o.commits || o.changes)) {
    const g: string[] = [];
    if (git.branch) g.push(`Текущая ветка: \`${git.branch}\``);
    if (o.commits) {
      const log = await api.gitLog(git.root || entry.path, 0, 10).catch(() => []);
      if (log.length) g.push("Последние коммиты:\n" + log.map((c) => `- ${c.date.slice(0, 10)} ${c.subject}`).join("\n"));
    }
    if (o.changes && git.status.length) {
      g.push(
        `Незакоммиченные изменения (${git.status.length}):\n` +
          git.status.slice(0, 40).map((s) => `- \`${s.index}${s.worktree}\` ${s.path}`).join("\n"),
      );
    }
    if (g.length) out.push(`## Состояние git\n${g.join("\n\n")}`);
  }

  if (o.notes && entry.notes?.trim()) out.push(`## Заметки\n${entry.notes.trim()}`);

  if (o.readme && info.readme) {
    try {
      const f = await api.readText(joinPath(entry.path, info.readme));
      const head = f.content.split(/\r?\n/).slice(0, 80).join("\n");
      out.push(`## README (начало)\n${fence(head, "markdown")}`);
    } catch {
      /* ignore */
    }
  }

  return out.join("\n\n") + "\n";
}
