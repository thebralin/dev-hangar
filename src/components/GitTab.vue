<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { openUrl } from "@tauri-apps/plugin-opener";
import Icon from "./Icon.vue";
import DiffView from "./DiffView.vue";
import CodeViewer from "./CodeViewer.vue";
import GitToolbar from "./GitToolbar.vue";
import GitChanges from "./GitChanges.vue";
import Dropdown from "./Dropdown.vue";
import { api, joinPath, relPath, sameSep, type Checkpoint, type Commit, type CommitDetail, type GitOp, type GitSummary } from "../lib/api";
import { state, toast, refreshBrief, loadCheckpoints, type ProjectEntry } from "../lib/store";
import { gitRun, gitState } from "../lib/gitops";
import { relTime, fullDate } from "../lib/format";

const props = defineProps<{ entry: ProjectEntry; git?: GitSummary; fileRequest: string | null }>();
const emit = defineEmits<{ "open-file": [path: string, line?: number]; "clear-file": [] }>();

const repo = computed(() => props.git?.root || props.entry.path);
/** project dir relative to the repo root ("" when the project is the repo root) */
const prefix = computed(() => relPath(repo.value, props.entry.path));

const commits = ref<Commit[]>([]);
const loadingMore = ref(false);
const done = ref(false);
const filter = ref("");
const selected = ref<string | null>(null); // commit hash or "WORKTREE"
const detail = ref<CommitDetail | null>(null);
const selFile = ref<string | null>(null);
const diff = ref("");
const view = ref<"diff" | "file">("diff");
const fileAt = ref<string | null>(null);
const cps = computed(() => state.checkpoints[props.entry.path] ?? []);
const cpOpen = ref(true);
const cp = ref<Checkpoint | null>(null);
const cpFiles = ref<[string, string][]>([]);
const confirmRestore = ref(false);
const busy = ref(false);
const isCp = computed(() => selected.value?.startsWith("CP:") ?? false);

async function selectCheckpoint(c: Checkpoint) {
  selected.value = "CP:" + c.id;
  cp.value = c;
  detail.value = null;
  selFile.value = null;
  fileAt.value = null;
  view.value = "diff";
  confirmRestore.value = false;
  diff.value = "";
  cpFiles.value = [];
  try {
    const d = await api.checkpointDiff(props.entry.path, c.hash);
    cpFiles.value = d.files;
    diff.value = d.patch;
  } catch (e) {
    toast(String(e), "error");
  }
}

async function restoreCheckpoint() {
  if (!cp.value) return;
  if (!confirmRestore.value) {
    confirmRestore.value = true;
    return;
  }
  busy.value = true;
  try {
    const safety = await api.checkpointRestore(props.entry.path, cp.value.hash);
    toast(`Откатил к «${cp.value.message}». Прежнее состояние сохранено как чекпоинт.`);
    await Promise.all([loadCheckpoints(props.entry.path), refreshBrief(props.entry.path)]);
    const s = cps.value.find((x) => x.id === safety.id);
    if (s) selectCheckpoint(s);
  } catch (e) {
    toast("Откат не удался: " + e, "error");
  } finally {
    busy.value = false;
    confirmRestore.value = false;
  }
}

async function deleteCheckpoint(c: Checkpoint) {
  try {
    await api.checkpointDelete(props.entry.path, c.id);
    await loadCheckpoints(props.entry.path);
    if (selected.value === "CP:" + c.id) {
      selected.value = null;
      cp.value = null;
      diff.value = "";
    }
  } catch (e) {
    toast(String(e), "error");
  }
}

const fileFilter = computed(() => (props.fileRequest ? (prefix.value ? prefix.value + "/" : "") + props.fileRequest : undefined));

async function loadCommits(reset = false) {
  if (!props.git?.isRepo) return;
  if (reset) {
    commits.value = [];
    done.value = false;
  }
  loadingMore.value = true;
  try {
    const page = await api.gitLog(repo.value, commits.value.length, 100, fileFilter.value);
    commits.value.push(...page);
    if (page.length < 100) done.value = true;
  } catch (e) {
    toast(String(e), "error");
    done.value = true;
  } finally {
    loadingMore.value = false;
  }
}

const shown = computed(() => {
  const f = filter.value.trim().toLowerCase();
  if (!f) return commits.value;
  return commits.value.filter((c) => (c.subject + " " + c.author + " " + c.short + " " + c.refs).toLowerCase().includes(f));
});

async function selectCommit(hash: string) {
  selected.value = hash;
  detail.value = null;
  selFile.value = null;
  diff.value = "";
  fileAt.value = null;
  view.value = "diff";
  if (hash === "WORKTREE") {
    diff.value = await api.gitWorkingDiff(repo.value).catch((e) => String(e));
    return;
  }
  try {
    detail.value = await api.gitCommit(repo.value, hash);
    const first = fileFilter.value && detail.value.files.find((f) => f.path === fileFilter.value);
    if (first) selectFile(first.path);
    else diff.value = await api.gitFileDiff(repo.value, ".", hash).catch((e) => String(e));
  } catch (e) {
    toast(String(e), "error");
  }
}

async function selectFile(path: string) {
  selFile.value = path;
  fileAt.value = null;
  diff.value = "";
  if (isCp.value && cp.value) {
    diff.value = await api.checkpointDiff(props.entry.path, cp.value.hash, path).then((d) => d.patch).catch((e) => String(e));
  } else {
    const hash = selected.value === "WORKTREE" ? undefined : selected.value!;
    diff.value = await api.gitFileDiff(repo.value, path, hash).catch((e) => String(e));
  }
  if (view.value === "file") await showFileVersion();
}

async function showFileVersion() {
  view.value = "file";
  if (!selFile.value || !selected.value || selected.value === "WORKTREE") return;
  const rev = isCp.value && cp.value ? cp.value.hash : selected.value;
  fileAt.value = await api.gitFileAt(repo.value, rev, selFile.value).catch((e) => `// не удалось получить версию: ${e}`);
}

function absFromRepo(rel: string) {
  return joinPath(sameSep(repo.value, props.entry.path), rel);
}

function refsOf(c: Commit) {
  return c.refs
    .split(",")
    .map((r) => r.trim().replace(/^HEAD -> /, ""))
    .filter((r) => r && r !== "HEAD");
}

const statusColor: Record<string, string> = { A: "green", M: "yellow", D: "red", R: "blue", C: "blue", "?": "green", U: "red" };
function copy(t: string) {
  navigator.clipboard.writeText(t);
  toast("Скопировано: " + t.slice(0, 12));
}

const webRemote = computed(() => props.git?.remotes.find((r) => r.webUrl)?.webUrl);
function openOnWeb(hash: string) {
  if (!webRemote.value) return;
  const base = webRemote.value;
  const path = base.includes("gitlab") ? `/-/commit/${hash}` : base.includes("bitbucket") ? `/commits/${hash}` : `/commit/${hash}`;
  openUrl(base + path);
}

function onScroll(e: Event) {
  const el = e.target as HTMLElement;
  if (!done.value && !loadingMore.value && el.scrollTop + el.clientHeight > el.scrollHeight - 200) loadCommits();
}

onMounted(async () => {
  loadCheckpoints(props.entry.path);
  await loadCommits(true);
  if (props.git?.status.length && !props.fileRequest) selectCommit("WORKTREE");
  else if (commits.value[0]) selectCommit(commits.value[0].hash);
});
watch(() => props.fileRequest, async () => {
  await loadCommits(true);
  if (commits.value[0]) selectCommit(commits.value[0].hash);
});
watch(() => props.git?.lastCommit?.hash, (n, o) => { if (n !== o) loadCommits(true); });
watch(() => props.git?.branch, (n, o) => { if (o !== undefined && n !== o) loadCommits(true); });

// ---------- git operations ----------
const gitBusy = computed(() => !!gitState.busy[props.entry.path]);
const runOp = (op: GitOp, label?: string) => gitRun(props.entry.path, repo.value, op, label);

// keep the working-copy diff fresh after stage/commit/discard
const statusKey = computed(() => props.git?.status.map((s) => s.index + s.worktree + s.path).join("|") ?? "");
watch(statusKey, () => {
  if (selected.value !== "WORKTREE") return;
  if (selFile.value && props.git?.status.some((s) => s.path === selFile.value)) selectFile(selFile.value);
  else selectCommit("WORKTREE");
});
function onChangesSelect(path: string | null) {
  if (path) selectFile(path);
  else selectCommit("WORKTREE");
}

const isStash = computed(() => selected.value?.startsWith("stash@") ?? false);
const stashOpen = ref(true);
const armed = ref<string | null>(null);
let armTimer: number | undefined;
function confirmClick(key: string, fn: () => void) {
  if (armed.value === key) {
    armed.value = null;
    fn();
    return;
  }
  armed.value = key;
  clearTimeout(armTimer);
  armTimer = window.setTimeout(() => (armed.value = null), 3500);
}
async function stashOp(op: GitOp) {
  const ok = await runOp(op);
  if (ok && isStash.value) {
    selected.value = null;
    detail.value = null;
    diff.value = "";
  }
}

// commit actions
const newRef = ref("");
const refKind = ref<"" | "branch" | "tag">("");
async function createRefHere(hash: string) {
  const name = newRef.value.trim();
  if (!name) return;
  const ok =
    refKind.value === "branch"
      ? await runOp({ op: "createBranch", name, from: hash, checkout: true })
      : await runOp({ op: "createTag", name, at: hash, message: null });
  if (ok) {
    newRef.value = "";
    refKind.value = "";
  }
}
async function resetTo(hash: string, mode: "soft" | "mixed" | "hard", close: () => void) {
  close();
  await runOp({ op: "reset", hash, mode }, `Reset --${mode}`);
}
</script>

<template>
  <div v-if="!git" class="empty">Загрузка…</div>
  <div v-else-if="!git.isRepo" class="empty">
    <Icon name="git" :size="36" />
    Это не git-репозиторий{{ git.gitAvailable ? "" : " (git не найден в PATH)" }}
  </div>
  <div v-else class="gwrap">
  <GitToolbar :entry="entry" :git="git" :repo="repo" />
  <div class="gt">
    <div class="left">
      <div v-if="fileRequest" class="file-filter">
        <Icon name="history" :size="13" />
        <span class="ellipsis grow mono">{{ fileRequest }}</span>
        <button class="btn sm icon ghost" title="Показать все коммиты" @click="emit('clear-file')"><Icon name="x" :size="12" /></button>
      </div>
      <div class="lsearch">
        <input v-model="filter" class="input" placeholder="Фильтр по сообщению, автору, хэшу…" />
        <button class="btn sm icon ghost" title="Обновить" @click="refreshBrief(entry.path); loadCommits(true)"><Icon name="refresh" :size="12" /></button>
      </div>
      <div class="clist" @scroll="onScroll">
        <template v-if="cps.length && !fileRequest">
          <div class="sect" @click="cpOpen = !cpOpen">
            <Icon name="chevron" :size="11" class="chev" :class="{ open: cpOpen }" />
            <Icon name="bookmark" :size="12" /> Чекпоинты <span class="dim">{{ cps.length }}</span>
          </div>
          <template v-if="cpOpen">
            <div v-for="c in cps" :key="c.id" class="c cp" :class="{ sel: selected === 'CP:' + c.id }" @click="selectCheckpoint(c)">
              <div class="subj ellipsis" :title="c.message">{{ c.message }}</div>
              <div class="meta">
                <span class="dim" :title="fullDate(c.date)">{{ relTime(c.date) }}</span>
                <span v-if="c.branch" class="dim">· {{ c.branch }}</span>
                <span class="spacer" />
                <span class="x" title="Удалить чекпоинт" @click.stop="deleteCheckpoint(c)"><Icon name="trash" :size="11" /></span>
              </div>
            </div>
          </template>
          <div class="sect nohover"><Icon name="commit" :size="12" /> Коммиты</div>
        </template>
        <template v-if="git.stashes?.length && !fileRequest">
          <div class="sect" @click="stashOpen = !stashOpen">
            <Icon name="chevron" :size="11" class="chev" :class="{ open: stashOpen }" />
            <Icon name="layers" :size="12" /> Stash <span class="dim">{{ git.stashes.length }}</span>
          </div>
          <template v-if="stashOpen">
            <div v-for="s in git.stashes" :key="s.index" class="c st-item" :class="{ sel: selected === `stash@{${s.index}}` }" @click="selectCommit(`stash@{${s.index}}`)">
              <div class="subj ellipsis" :title="s.message">{{ s.message }}</div>
              <div class="meta">
                <span class="dim" :title="fullDate(s.date)">{{ relTime(s.date) }}</span>
                <span class="spacer" />
                <span class="sacts">
                  <button class="tb" title="Применить и удалить из stash" :disabled="gitBusy" @click.stop="stashOp({ op: 'stashApply', index: s.index, pop: true })">pop</button>
                  <button class="tb" title="Применить, оставив в stash" :disabled="gitBusy" @click.stop="stashOp({ op: 'stashApply', index: s.index, pop: false })">apply</button>
                  <button class="tb del" :class="{ armed: armed === 'stash:' + s.index }" :title="armed === 'stash:' + s.index ? 'Нажмите ещё раз' : 'Удалить'" :disabled="gitBusy" @click.stop="confirmClick('stash:' + s.index, () => stashOp({ op: 'stashDrop', index: s.index }))">
                    {{ armed === "stash:" + s.index ? "удалить?" : "×" }}
                  </button>
                </span>
              </div>
            </div>
          </template>
        </template>
        <div v-if="!fileRequest" class="c wt" :class="{ sel: selected === 'WORKTREE' }" @click="selectCommit('WORKTREE')">
          <div class="subj"><span class="badge" :class="git.status.length ? 'yellow' : ''">●</span> Рабочая копия</div>
          <div class="meta dim">{{ git.status.length ? `${git.status.length} изм. — коммит, stage, отмена` : "нет изменений" }}</div>
        </div>
        <div v-for="c in shown" :key="c.hash" class="c" :class="{ sel: selected === c.hash }" @click="selectCommit(c.hash)">
          <div class="subj ellipsis" :title="c.subject">{{ c.subject }}</div>
          <div class="meta">
            <span class="dim">{{ c.author }}</span>
            <span class="dim" :title="fullDate(c.date)">· {{ relTime(c.date) }}</span>
            <span class="spacer" />
            <span v-if="c.insertions" class="plus">+{{ c.insertions }}</span>
            <span v-if="c.deletions" class="minus">−{{ c.deletions }}</span>
          </div>
          <div v-if="refsOf(c).length" class="refs">
            <span v-for="r in refsOf(c)" :key="r" class="badge" :class="r.startsWith('tag:') ? 'yellow' : 'blue'">{{ r.replace("tag: ", "🏷 ") }}</span>
          </div>
        </div>
        <div v-if="loadingMore" class="dim pad">Загрузка…</div>
        <div v-else-if="!commits.length" class="dim pad">Коммитов нет</div>
      </div>
    </div>

    <div class="mid" v-if="selected">
      <template v-if="selected === 'WORKTREE'">
        <GitChanges :entry="entry" :git="git" :repo="repo" :sel-file="selFile" @select="onChangesSelect" />
      </template>
      <template v-else-if="isCp && cp">
        <div class="dh">
          <div class="msg">{{ cp.message }}</div>
          <div class="small dim">{{ fullDate(cp.date) }} · {{ relTime(cp.date) }}{{ cp.branch ? " · ветка " + cp.branch : "" }}</div>
          <div class="small muted" style="margin: 8px 0">Ниже то, что изменилось с момента чекпоинта до текущего состояния файлов.</div>
          <div class="row">
            <button class="btn sm" :class="confirmRestore ? 'danger' : ''" :disabled="busy" @click="restoreCheckpoint">
              <Icon name="undo" :size="12" /> {{ busy ? "Откатываю…" : confirmRestore ? "Точно откатить?" : "Откатить к чекпоинту" }}
            </button>
            <button v-if="confirmRestore" class="btn sm ghost" @click="confirmRestore = false">Отмена</button>
          </div>
          <div v-if="confirmRestore" class="small warn-t">Файлы вернутся к состоянию чекпоинта, новые файлы после него удалятся. Текущее состояние сначала сохранится отдельным чекпоинтом, так что откат можно отменить.</div>
        </div>
        <div class="flist">
          <div class="fi" :class="{ sel: !selFile }" @click="selectCheckpoint(cp!)">
            <span class="ellipsis">Все изменения ({{ cpFiles.length }})</span>
          </div>
          <div v-for="[st, f] in cpFiles" :key="f" class="fi" :class="{ sel: selFile === f }" :title="f" @click="selectFile(f)">
            <span class="st" :class="statusColor[st]">{{ st }}</span>
            <span class="ellipsis fname">{{ f.split("/").pop() }}</span>
            <span class="ellipsis dim fdir">{{ f.split("/").slice(0, -1).join("/") }}</span>
          </div>
          <div v-if="!cpFiles.length" class="dim pad">С момента чекпоинта ничего не изменилось</div>
        </div>
      </template>
      <template v-else-if="detail">
        <div class="dh">
          <div class="msg selectable">{{ detail.message }}</div>
          <div class="small dim">
            {{ detail.author }} &lt;{{ detail.email }}&gt;
          </div>
          <div class="small dim">{{ fullDate(detail.date) }} · {{ relTime(detail.date) }}</div>
          <div class="row" style="margin-top: 6px">
            <code class="hash" title="Скопировать хэш" @click="copy(detail.hash)">{{ detail.hash.slice(0, 10) }}</code>
            <button v-if="webRemote && !isStash" class="btn sm ghost" @click="openOnWeb(detail.hash)"><Icon name="globe" :size="12" /> На сайте</button>
          </div>
          <div v-if="!isStash" class="cacts">
            <button class="btn sm" :disabled="gitBusy" title="Создать ветку от этого коммита и переключиться" @click="refKind = refKind === 'branch' ? '' : 'branch'"><Icon name="branch" :size="12" /> Ветка</button>
            <button class="btn sm" :disabled="gitBusy" title="Поставить тег на этот коммит" @click="refKind = refKind === 'tag' ? '' : 'tag'"><Icon name="bookmark" :size="12" /> Тег</button>
            <button class="btn sm" :disabled="gitBusy" :title="`Применить этот коммит поверх ${git.branch}`" @click="runOp({ op: 'cherryPick', hash: detail.hash })">Cherry-pick</button>
            <button class="btn sm" :disabled="gitBusy" title="Создать коммит, отменяющий этот" @click="runOp({ op: 'revert', hash: detail.hash })">Revert</button>
            <Dropdown align="right" :width="290">
              <template #trigger="{ toggle }">
                <button class="btn sm" :disabled="gitBusy" :title="`Передвинуть ${git.branch} на этот коммит`" @click="toggle">Reset <Icon name="down" :size="10" /></button>
              </template>
              <template #default="{ close }">
                <div class="mh">Передвинуть {{ git.branch }} сюда</div>
                <button class="mi" @click="resetTo(detail!.hash, 'soft', close)">soft <span class="hint">изменения останутся в индексе</span></button>
                <button class="mi" @click="resetTo(detail!.hash, 'mixed', close)">mixed <span class="hint">изменения останутся в файлах</span></button>
                <button class="mi danger" @click="confirmClick('hard', () => resetTo(detail!.hash, 'hard', close))">
                  {{ armed === "hard" ? "Точно? Файлы будут перезаписаны" : "hard" }} <span class="hint">с чекпоинтом</span>
                </button>
              </template>
            </Dropdown>
          </div>
          <div v-if="refKind && !isStash" class="row" style="margin-top: 6px">
            <input v-model="newRef" class="input grow" style="height: 26px" :placeholder="refKind === 'branch' ? 'имя новой ветки' : 'v1.0.0'" @keydown.enter="createRefHere(detail!.hash)" @keydown.esc="refKind = ''" />
            <button class="btn sm primary" :disabled="!newRef.trim() || gitBusy" @click="createRefHere(detail!.hash)">Создать</button>
          </div>
          <div v-if="isStash" class="small muted" style="margin-top: 6px">Содержимое stash. Применить или удалить его можно в списке слева.</div>
        </div>
        <div class="flist">
          <div class="fi" :class="{ sel: !selFile }" @click="selectCommit(detail.hash)">
            <span class="ellipsis">Весь коммит ({{ detail.files.length }})</span>
          </div>
          <div v-for="f in detail.files" :key="f.path" class="fi" :class="{ sel: selFile === f.path }" :title="(f.oldPath ? f.oldPath + ' → ' : '') + f.path" @click="selectFile(f.path)">
            <span class="st" :class="statusColor[f.status]">{{ f.status }}</span>
            <span class="ellipsis fname">{{ f.path.split("/").pop() }}</span>
            <span class="ellipsis dim fdir">{{ f.path.split("/").slice(0, -1).join("/") }}</span>
            <span class="spacer" />
            <span v-if="f.additions" class="plus small">+{{ f.additions }}</span>
            <span v-if="f.deletions" class="minus small">−{{ f.deletions }}</span>
          </div>
          <div v-if="!detail.files.length" class="dim pad">Нет изменённых файлов</div>
        </div>
      </template>
    </div>

    <div class="right">
      <div v-if="selFile" class="rbar">
        <span class="mono ellipsis grow">{{ selFile }}</span>
        <div class="seg">
          <button :class="{ on: view === 'diff' }" @click="view = 'diff'">Изменения</button>
          <button v-if="selected !== 'WORKTREE'" :class="{ on: view === 'file' }" @click="showFileVersion">{{ isCp ? "Файл в чекпоинте" : "Файл в этом коммите" }}</button>
        </div>
        <button class="btn sm" title="Открыть текущую версию файла" @click="emit('open-file', absFromRepo(selFile))"><Icon name="file" :size="12" /> Текущая</button>
      </div>
      <div class="rbody">
        <CodeViewer v-if="view === 'file' && selFile && fileAt != null" :content="fileAt" :label="`${selFile} @ ${isCp ? 'чекпоинт' : selected?.slice(0, 7)}`" :root="entry.path" hide-history />
        <div v-else class="diff-scroll"><DiffView :diff="diff" /></div>
      </div>
    </div>
  </div>
  </div>
</template>

<style scoped>
.gwrap { display: flex; flex-direction: column; height: 100%; min-height: 0; }
.gt { display: flex; flex: 1; min-height: 0; }
.cacts { display: flex; flex-wrap: wrap; gap: 5px; margin-top: 8px; }
.grow { flex: 1; min-width: 0; }
.sacts { display: flex; gap: 3px; opacity: 0; }
.c:hover .sacts { opacity: 1; }
.c.st-item { border-left: 2px solid var(--accent); border-radius: 0 6px 6px 0; }
.tb { border: 1px solid var(--border); background: var(--panel-2); color: var(--text-2); border-radius: 4px; font-size: 10.5px; padding: 0 5px; height: 18px; cursor: pointer; }
.tb:hover:not(:disabled) { color: var(--text); border-color: var(--text-3); }
.tb.del:hover, .tb.armed { color: var(--red); border-color: var(--red); }
.left { width: 340px; flex: none; display: flex; flex-direction: column; border-right: 1px solid var(--border); background: var(--bg-2); }
.file-filter { display: flex; align-items: center; gap: 6px; padding: 8px 10px; background: var(--accent-soft); color: var(--accent); font-size: 12px; }
.lsearch { display: flex; gap: 4px; padding: 10px; align-items: center; }
.clist { flex: 1; overflow-y: auto; padding: 0 6px 10px; }
.c { padding: 7px 9px; border-radius: 6px; cursor: pointer; margin-bottom: 1px; }
.c:hover { background: var(--hover); }
.c.sel { background: var(--active); }
.subj { font-weight: 500; }
.meta { display: flex; gap: 5px; font-size: 11.5px; margin-top: 2px; }
.refs { display: flex; flex-wrap: wrap; gap: 3px; margin-top: 4px; }
.plus { color: var(--green); font-size: 11.5px; font-family: var(--mono); }
.minus { color: var(--red); font-size: 11.5px; font-family: var(--mono); }
.pad { padding: 12px; }
.sect { display: flex; align-items: center; gap: 6px; padding: 8px 8px 4px; font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: .04em; color: var(--text-3); cursor: pointer; }
.sect.nohover { cursor: default; }
.chev { transition: transform .12s; }
.chev.open { transform: rotate(90deg); }
.c.cp { border-left: 2px solid var(--purple); border-radius: 0 6px 6px 0; }
.c .x { opacity: 0; color: var(--text-3); display: flex; }
.c:hover .x { opacity: 1; }
.c .x:hover { color: var(--red); }
.warn-t { color: var(--yellow); margin-top: 8px; line-height: 1.45; }
.mid { width: 320px; flex: none; display: flex; flex-direction: column; border-right: 1px solid var(--border); min-height: 0; }
.dh { padding: 12px 14px; border-bottom: 1px solid var(--border); }
.msg { white-space: pre-wrap; font-weight: 600; margin-bottom: 6px; max-height: 160px; overflow: auto; }
.small { font-size: 11.5px; }
.hash { font-size: 11.5px; background: var(--panel-2); padding: 2px 6px; border-radius: 4px; cursor: copy; }
.flist { flex: 1; overflow-y: auto; padding: 6px; }
.fi { display: flex; align-items: center; gap: 7px; padding: 4px 8px; border-radius: 5px; cursor: pointer; font-size: 12.5px; }
.fi:hover { background: var(--hover); }
.fi.sel { background: var(--active); }
.fname { flex: none; max-width: 55%; }
.fdir { font-size: 11px; min-width: 0; }
.st { font-family: var(--mono); font-size: 11px; font-weight: 700; width: 14px; text-align: center; flex: none; }
.st.green { color: var(--green); } .st.yellow { color: var(--yellow); } .st.red { color: var(--red); } .st.blue { color: var(--accent); }
.right { flex: 1; min-width: 0; display: flex; flex-direction: column; }
.rbar { display: flex; align-items: center; gap: 8px; padding: 6px 10px 6px 14px; border-bottom: 1px solid var(--border); background: var(--bg-2); min-height: 40px; font-size: 12.5px; }
.seg { display: flex; background: var(--panel); border: 1px solid var(--border); border-radius: 6px; padding: 2px; }
.seg button { border: 0; background: none; padding: 2px 10px; border-radius: 4px; cursor: pointer; color: var(--text-2); font-size: 12px; white-space: nowrap; }
.seg button.on { background: var(--active); color: var(--text); }
.rbody { flex: 1; min-height: 0; }
.diff-scroll { height: 100%; overflow: auto; }
</style>
