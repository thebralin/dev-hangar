<script setup lang="ts">
import { computed, ref, watch } from "vue";
import Icon from "./Icon.vue";
import Dropdown from "./Dropdown.vue";
import type { GitOp, GitSummary } from "../lib/api";
import type { ProjectEntry } from "../lib/store";
import { gitRun, gitState } from "../lib/gitops";
import { relTime } from "../lib/format";

const props = defineProps<{ entry: ProjectEntry; git: GitSummary; repo: string }>();

const busy = computed(() => gitState.busy[props.entry.path]);
const log = computed(() => gitState.log[props.entry.path]);
const showLog = ref(false);
watch(log, (l) => { if (l && !l.ok) showLog.value = true; });

function run(op: GitOp, label?: string) {
  return gitRun(props.entry.path, props.repo, op, label);
}

// two-click confirmation for dangerous actions
const armed = ref<string | null>(null);
let armTimer: number | undefined;
function confirm(key: string, fn: () => void) {
  if (armed.value === key) {
    armed.value = null;
    fn();
    return;
  }
  armed.value = key;
  clearTimeout(armTimer);
  armTimer = window.setTimeout(() => (armed.value = null), 3500);
}

// ---------- branches ----------
const q = ref("");
const renaming = ref<string | null>(null);
const renameTo = ref("");
const local = computed(() => props.git.branches.filter((b) => b.toLowerCase().includes(q.value.trim().toLowerCase())));
const remote = computed(() => {
  const f = q.value.trim().toLowerCase();
  // hide remote branches that already have a local twin
  return props.git.remoteBranches.filter((b) => b.toLowerCase().includes(f) && !props.git.branches.includes(b.split("/").slice(1).join("/")));
});
const canCreate = computed(() => {
  const n = q.value.trim();
  return n && !/\s/.test(n) && !props.git.branches.includes(n);
});
const dirty = computed(() => props.git.status.length > 0);

async function checkout(b: string, close: () => void) {
  if (b === props.git.branch) return;
  close();
  await run({ op: "checkout", branch: b });
}
async function createBranch(close: () => void) {
  const name = q.value.trim();
  close();
  if (await run({ op: "createBranch", name, from: null, checkout: true })) q.value = "";
}
async function doRename(from: string) {
  const to = renameTo.value.trim();
  renaming.value = null;
  if (to && to !== from) await run({ op: "renameBranch", from, to });
}

// ---------- sync ----------
const hasRemote = computed(() => props.git.remotes.length > 0);
const pushLabel = computed(() => (props.git.upstream ? "Push" : "Опубликовать"));

// ---------- more ----------
const tagName = ref("");
const tagMsg = ref("");
const stashMsg = ref("");
const userName = ref("");
const userEmail = ref("");
const panel = ref<"" | "tag" | "stash" | "user">("");
function openPanel(p: typeof panel.value) {
  panel.value = panel.value === p ? "" : p;
  if (p === "user") {
    userName.value = props.git.userName ?? "";
    userEmail.value = props.git.userEmail ?? "";
  }
}
async function createTag(close: () => void) {
  close();
  if (await run({ op: "createTag", name: tagName.value.trim(), at: null, message: tagMsg.value.trim() || null })) {
    tagName.value = "";
    tagMsg.value = "";
  }
  panel.value = "";
}
async function stash(close: () => void) {
  close();
  if (await run({ op: "stashPush", message: stashMsg.value.trim() || null, includeUntracked: true })) stashMsg.value = "";
  panel.value = "";
}
async function saveUser(close: () => void) {
  close();
  await run({ op: "setUser", name: userName.value, email: userEmail.value });
  panel.value = "";
}

// ---------- operation in progress ----------
const conflicts = computed(() => props.git.status.filter((s) => s.index === "U" || s.worktree === "U" || (s.index === "A" && s.worktree === "A") || (s.index === "D" && s.worktree === "D")).length);
const opLabel: Record<string, string> = { merge: "Идёт слияние (merge)", rebase: "Идёт rebase", "cherry-pick": "Идёт cherry-pick", revert: "Идёт revert" };
</script>

<template>
  <div class="gtb">
    <div class="bar">
      <!-- branch switcher -->
      <Dropdown :width="340">
        <template #trigger="{ toggle }">
          <button class="btn sm branch" :disabled="!!busy" title="Ветки" @click="toggle">
            <Icon name="branch" :size="13" />
            <span class="ellipsis">{{ git.branch === "HEAD" ? "detached HEAD" : git.branch || "—" }}</span>
            <Icon name="down" :size="11" />
          </button>
        </template>
        <template #default="{ close }">
          <div class="dd-search">
            <input v-model="q" class="input" placeholder="Найти или создать ветку…" autofocus @keydown.enter="canCreate && createBranch(close)" />
          </div>
          <button v-if="canCreate" class="mi" @click="createBranch(close)">
            <Icon name="plus" :size="12" /> Создать <b>{{ q.trim() }}</b> от текущей и переключиться
          </button>
          <div v-if="dirty" class="note">Незакоммиченные изменения перенесутся в новую ветку; если они мешают переключению, git откажет — сделайте коммит или stash.</div>
          <div class="mh">Локальные</div>
          <div v-for="b in local" :key="b" class="brow" :class="{ cur: b === git.branch }">
            <template v-if="renaming === b">
              <input v-model="renameTo" class="input rn" @keydown.enter="doRename(b)" @keydown.esc="renaming = null" />
              <button class="btn sm" @click="doRename(b)">OK</button>
            </template>
            <template v-else>
              <button class="mi grow" :title="b === git.branch ? 'Текущая ветка' : 'Переключиться'" @click="checkout(b, close)">
                <Icon :name="b === git.branch ? 'check' : 'branch'" :size="12" />
                <span class="ellipsis">{{ b }}</span>
              </button>
              <span class="row-acts">
                <template v-if="b !== git.branch">
                  <button class="tb" :title="`Влить ${b} в ${git.branch}`" @click="close(); run({ op: 'merge', branch: b, noFf: false })">merge</button>
                  <button class="tb" :title="`Перебазировать ${git.branch} на ${b}`" @click="close(); run({ op: 'rebase', onto: b })">rebase</button>
                </template>
                <button class="tb" title="Переименовать" @click="renaming = b; renameTo = b"><Icon name="edit" :size="11" /></button>
                <button
                  v-if="b !== git.branch"
                  class="tb del"
                  :class="{ armed: armed === 'del:' + b }"
                  :title="armed === 'del:' + b ? 'Нажмите ещё раз для удаления' : 'Удалить ветку'"
                  @click="confirm('del:' + b, () => run({ op: 'deleteBranch', name: b, force: false }))"
                >{{ armed === "del:" + b ? "удалить?" : "" }}<Icon v-if="armed !== 'del:' + b" name="trash" :size="11" /></button>
              </span>
            </template>
          </div>
          <template v-if="remote.length">
            <div class="mh">Удалённые</div>
            <div v-for="b in remote" :key="b" class="brow">
              <button class="mi grow" title="Создать локальную ветку и переключиться" @click="checkout(b, close)">
                <Icon name="globe" :size="12" /> <span class="ellipsis">{{ b }}</span>
              </button>
              <span class="row-acts">
                <button class="tb" :title="`Влить ${b} в ${git.branch}`" @click="close(); run({ op: 'merge', branch: b, noFf: false })">merge</button>
              </span>
            </div>
          </template>
        </template>
      </Dropdown>

      <span v-if="git.upstream" class="dim small up" :title="'Отслеживает ' + git.upstream">→ {{ git.upstream }}</span>

      <span class="spacer" />

      <span v-if="busy" class="busy small"><span class="spin" /> {{ busy }}…</span>

      <button class="btn sm" :disabled="!!busy || !hasRemote" title="git fetch --all --prune" @click="run({ op: 'fetch' })">
        <Icon name="refresh" :size="12" /> Fetch
      </button>
      <button class="btn sm" :disabled="!!busy || !git.upstream" :title="git.upstream ? 'git pull (merge)' : 'У ветки нет upstream'" @click="run({ op: 'pull', rebase: false })">
        <Icon name="download" :size="12" /> Pull<span v-if="git.behind" class="cnt">↓{{ git.behind }}</span>
      </button>
      <button class="btn sm" :class="{ primary: git.ahead > 0 || (!git.upstream && hasRemote) }" :disabled="!!busy || !hasRemote" :title="git.upstream ? 'git push' : 'git push -u — опубликовать ветку'" @click="run({ op: 'push', force: false })">
        <Icon name="upload" :size="12" /> {{ pushLabel }}<span v-if="git.ahead" class="cnt">↑{{ git.ahead }}</span>
      </button>

      <Dropdown align="right" :width="300">
        <template #trigger="{ toggle }">
          <button class="btn sm icon" :disabled="!!busy" title="Ещё" @click="toggle(); panel = ''"><Icon name="list" :size="13" /></button>
        </template>
        <template #default="{ close }">
          <button class="mi" :disabled="!git.upstream" @click="close(); run({ op: 'pull', rebase: true }, 'Pull --rebase')">
            <Icon name="download" :size="12" /> Pull с rebase
          </button>
          <button class="mi danger" :disabled="!git.upstream" @click="confirm('force', () => { close(); run({ op: 'push', force: true }, 'Force push') })">
            <Icon name="upload" :size="12" /> {{ armed === "force" ? "Точно перезаписать remote?" : "Force push (with lease)" }}
          </button>
          <button class="mi" :disabled="!hasRemote || !git.tags.length" @click="close(); run({ op: 'pushTags' })">
            <Icon name="bookmark" :size="12" /> Push тегов
          </button>
          <div class="msep" />
          <button class="mi" :disabled="!dirty" @click="openPanel('stash')"><Icon name="layers" :size="12" /> Убрать изменения в stash…</button>
          <div v-if="panel === 'stash'" class="form">
            <input v-model="stashMsg" class="input" placeholder="Описание (необязательно)" @keydown.enter="stash(close)" />
            <button class="btn sm primary" @click="stash(close)">Stash</button>
          </div>
          <button class="mi" @click="openPanel('tag')"><Icon name="bookmark" :size="12" /> Тег на текущем коммите…</button>
          <div v-if="panel === 'tag'" class="form col">
            <input v-model="tagName" class="input" placeholder="v1.2.0" @keydown.enter="tagName.trim() && createTag(close)" />
            <input v-model="tagMsg" class="input" placeholder="Сообщение (для аннотированного тега)" />
            <button class="btn sm primary" :disabled="!tagName.trim()" @click="createTag(close)">Создать тег</button>
          </div>
          <button class="mi" :disabled="!git.lastCommit" @click="confirm('undo', () => { close(); run({ op: 'undoCommit' }) })">
            <Icon name="undo" :size="12" /> {{ armed === "undo" ? "Нажмите ещё раз" : "Отменить последний коммит" }}
            <span class="hint">изменения останутся</span>
          </button>
          <div class="msep" />
          <button class="mi" @click="openPanel('user')">
            <Icon name="edit" :size="12" /> Автор: {{ git.userName || "не задан" }}
          </button>
          <div v-if="panel === 'user'" class="form col">
            <input v-model="userName" class="input" placeholder="Имя" />
            <input v-model="userEmail" class="input" placeholder="email" @keydown.enter="saveUser(close)" />
            <button class="btn sm primary" :disabled="!userName.trim() || !userEmail.trim()" @click="saveUser(close)">Сохранить для репозитория</button>
          </div>
        </template>
      </Dropdown>

      <button v-if="log" class="btn sm ghost" :class="{ err: !log.ok }" title="Вывод последней git-операции" @click="showLog = !showLog">
        <Icon name="terminal" :size="12" />
      </button>
    </div>

    <div v-if="git.operation" class="opbar">
      <Icon name="info" :size="14" />
      <b>{{ opLabel[git.operation] }}</b>
      <span v-if="conflicts" class="red">— конфликтов: {{ conflicts }}. Разрешите их в «Рабочей копии».</span>
      <span v-else class="dim">— конфликтов нет, можно продолжить</span>
      <span class="spacer" />
      <button class="btn sm success" :disabled="!!busy || conflicts > 0" @click="run({ op: 'continue' })"><Icon name="check" :size="12" /> Продолжить</button>
      <button class="btn sm danger" :disabled="!!busy" @click="confirm('abort', () => run({ op: 'abort' }))">
        <Icon name="x" :size="12" /> {{ armed === "abort" ? "Точно отменить?" : "Отменить " + git.operation }}
      </button>
    </div>

    <div v-if="showLog && log" class="log" :class="{ err: !log.ok }">
      <div class="log-h">
        <b>{{ log.label }}</b> <span class="dim">· {{ log.ok ? "успешно" : "ошибка" }} · {{ relTime(log.at) }}</span>
        <span class="spacer" />
        <button class="btn sm icon ghost" @click="showLog = false"><Icon name="x" :size="12" /></button>
      </div>
      <pre class="selectable">{{ log.text || "(нет вывода)" }}</pre>
    </div>
  </div>
</template>

<style scoped>
.gtb { border-bottom: 1px solid var(--border); background: var(--bg-2); }
.bar { display: flex; align-items: center; gap: 6px; padding: 7px 10px; }
.branch { max-width: 260px; font-weight: 600; }
.up { max-width: 220px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.small { font-size: 11.5px; }
.cnt { margin-left: 4px; font-family: var(--mono); font-size: 11px; }
.busy { display: flex; align-items: center; gap: 6px; color: var(--text-2); }
.spin { width: 11px; height: 11px; border: 2px solid var(--border); border-top-color: var(--accent); border-radius: 50%; animation: sp .8s linear infinite; }
@keyframes sp { to { transform: rotate(360deg); } }
.btn.err { color: var(--red); }
.dd-search { padding: 4px 4px 6px; }
.dd-search .input { width: 100%; }
.note { font-size: 11px; color: var(--text-3); padding: 2px 8px 6px; line-height: 1.4; }
.brow { display: flex; align-items: center; border-radius: 5px; }
.brow:hover { background: var(--hover); }
.brow .mi:hover { background: none; }
.brow.cur .mi { font-weight: 600; color: var(--accent); }
.grow { flex: 1; min-width: 0; }
.row-acts { display: flex; gap: 2px; padding-right: 4px; opacity: 0; }
.brow:hover .row-acts { opacity: 1; }
.tb { border: 1px solid var(--border); background: var(--panel-2); color: var(--text-2); border-radius: 4px; font-size: 10.5px; padding: 1px 5px; cursor: pointer; display: flex; align-items: center; gap: 3px; height: 20px; }
.tb:hover { color: var(--text); border-color: var(--text-3); }
.tb.del:hover, .tb.armed { color: var(--red); border-color: var(--red); }
.rn { flex: 1; height: 24px; margin: 2px 4px; }
.form { display: flex; gap: 6px; padding: 4px 8px 8px; }
.form.col { flex-direction: column; }
.form .input { flex: 1; height: 26px; }
.opbar { display: flex; align-items: center; gap: 8px; padding: 7px 12px; background: var(--yellow-soft); color: var(--yellow); font-size: 12.5px; border-top: 1px solid var(--border); }
.opbar .red { color: var(--red); }
.log { border-top: 1px solid var(--border); background: var(--panel); }
.log.err { border-left: 3px solid var(--red); }
.log-h { display: flex; align-items: center; gap: 6px; padding: 5px 10px; font-size: 12px; }
.log pre { margin: 0; padding: 4px 12px 10px; max-height: 180px; overflow: auto; font-family: var(--mono); font-size: 11.5px; white-space: pre-wrap; color: var(--text-2); }
</style>
