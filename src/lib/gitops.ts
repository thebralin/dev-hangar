import { reactive } from "vue";
import { api, type GitOp } from "./api";
import { refreshBrief, loadCheckpoints, toast } from "./store";

export interface GitOpLog { label: string; ok: boolean; text: string; at: number }

/** per project: running operation label, and output of the last one */
export const gitState = reactive({
  busy: {} as Record<string, string>,
  log: {} as Record<string, GitOpLog>,
  /** commit message drafts (kept while switching tabs/projects) */
  drafts: {} as Record<string, string>,
});

const LABELS: Record<GitOp["op"], string> = {
  stage: "Stage", unstage: "Unstage", discard: "Отмена изменений", commit: "Коммит", fetch: "Fetch", pull: "Pull",
  push: "Push", checkout: "Переключение ветки", createBranch: "Новая ветка", deleteBranch: "Удаление ветки",
  renameBranch: "Переименование ветки", merge: "Merge", rebase: "Rebase", abort: "Отмена операции",
  continue: "Продолжение операции", resolve: "Разрешение конфликта", stashPush: "Stash", stashApply: "Stash apply",
  stashDrop: "Stash drop", createTag: "Тег", deleteTag: "Удаление тега", pushTags: "Push тегов",
  cherryPick: "Cherry-pick", revert: "Revert", reset: "Reset", undoCommit: "Отмена коммита", setUser: "Автор коммитов",
};

/** ops that are quick and frequent: no success toast */
const QUIET = new Set<GitOp["op"]>(["stage", "unstage"]);

/**
 * Run a git operation for a project. Refreshes the git summary afterwards (also on failure:
 * a failed merge still changes the state). Returns true on success.
 */
export async function gitRun(projectPath: string, repo: string, op: GitOp, label = LABELS[op.op]): Promise<boolean> {
  if (gitState.busy[projectPath]) {
    toast(`Подождите: ${gitState.busy[projectPath]}…`);
    return false;
  }
  gitState.busy[projectPath] = label;
  let ok = true;
  try {
    const out = await api.gitOp(repo, op);
    gitState.log[projectPath] = { label, ok: true, text: out, at: Date.now() };
    if (!QUIET.has(op.op)) toast(`${label}: готово ✓`);
  } catch (e) {
    ok = false;
    const text = String(e);
    gitState.log[projectPath] = { label, ok: false, text, at: Date.now() };
    toast(`${label}: ${firstLine(text)}`, "error");
  } finally {
    delete gitState.busy[projectPath];
    await refreshBrief(projectPath);
    if (op.op === "discard" || (op.op === "reset" && op.mode === "hard")) loadCheckpoints(projectPath);
  }
  return ok;
}

function firstLine(s: string) {
  const l = s.split(/\r?\n/).find((x) => /error|fatal|conflict|rejected|hint/i.test(x)) ?? s.split(/\r?\n/)[0];
  return l.length > 160 ? l.slice(0, 160) + "…" : l;
}
