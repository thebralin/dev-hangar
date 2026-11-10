<script setup lang="ts">
import { computed, ref } from "vue";
import Icon from "./Icon.vue";
import type { GitOp, GitSummary, StatusEntry } from "../lib/api";
import type { ProjectEntry } from "../lib/store";
import { gitRun, gitState } from "../lib/gitops";

const props = defineProps<{ entry: ProjectEntry; git: GitSummary; repo: string; selFile: string | null }>();
const emit = defineEmits<{ select: [path: string | null] }>();

const busy = computed(() => !!gitState.busy[props.entry.path]);
const run = (op: GitOp, label?: string) => gitRun(props.entry.path, props.repo, op, label);

const isConflict = (s: StatusEntry) =>
  s.index === "U" || s.worktree === "U" || (s.index === "A" && s.worktree === "A") || (s.index === "D" && s.worktree === "D");
const conflicts = computed(() => props.git.status.filter(isConflict));
const staged = computed(() => props.git.status.filter((s) => !isConflict(s) && s.index !== " " && s.index !== "?"));
const unstaged = computed(() => props.git.status.filter((s) => !isConflict(s) && (s.worktree !== " " || s.index === "?")));

const message = computed({
  get: () => gitState.drafts[props.entry.path] ?? "",
  set: (v: string) => (gitState.drafts[props.entry.path] = v),
});
const amend = ref(false);
function toggleAmend() {
  amend.value = !amend.value;
  if (amend.value && !message.value.trim() && props.git.lastCommit) message.value = props.git.lastCommit.subject;
}

const willCommitAll = computed(() => staged.value.length === 0);
const commitCount = computed(() => (willCommitAll.value ? unstaged.value.length : staged.value.length));
const canCommit = computed(
  () => !busy.value && conflicts.value.length === 0 && (message.value.trim() !== "" || amend.value) && (commitCount.value > 0 || amend.value),
);
async function commit() {
  if (!canCommit.value) return;
  const ok = await run({ op: "commit", message: message.value.trim(), amend: amend.value, all: willCommitAll.value });
  if (ok) {
    message.value = "";
    amend.value = false;
  }
}

// two-click confirmation for discard
const armed = ref<string | null>(null);
let t: number | undefined;
function discard(key: string, paths: string[]) {
  if (armed.value !== key) {
    armed.value = key;
    clearTimeout(t);
    t = window.setTimeout(() => (armed.value = null), 3500);
    return;
  }
  armed.value = null;
  run({ op: "discard", paths });
}

const code = (s: StatusEntry, side: "index" | "worktree") => {
  const c = side === "index" ? s.index : s.worktree;
  return c === "?" ? "U" : c;
};
const color: Record<string, string> = { A: "green", M: "yellow", D: "red", R: "blue", C: "blue", U: "green" };
const fname = (p: string) => p.split("/").pop();
const fdir = (p: string) => p.split("/").slice(0, -1).join("/");
</script>

<template>
  <div class="gc">
    <div class="commit">
      <textarea
        v-model="message"
        class="input msg"
        rows="3"
        :placeholder="conflicts.length ? 'Сначала разрешите конфликты' : 'Сообщение коммита  (Ctrl+Enter — закоммитить)'"
        @keydown.ctrl.enter.prevent="commit"
        @keydown.meta.enter.prevent="commit"
      />
      <div class="crow">
        <label class="chk" title="Изменить последний коммит вместо создания нового">
          <input type="checkbox" :checked="amend" @change="toggleAmend" /> amend
        </label>
        <span class="spacer" />
        <button class="btn sm primary" :disabled="!canCommit" @click="commit">
          <Icon name="check" :size="12" />
          {{ amend ? "Изменить коммит" : willCommitAll ? `Закоммитить всё (${commitCount})` : `Закоммитить (${commitCount})` }}
        </button>
      </div>
      <div v-if="willCommitAll && unstaged.length && !amend" class="hint">Ничего не добавлено в индекс — закоммитятся все изменения.</div>
    </div>

    <div class="flist">
      <div class="fi" :class="{ sel: !selFile }" @click="emit('select', null)">
        <span class="ellipsis">Весь diff</span>
      </div>

      <template v-if="conflicts.length">
        <div class="sh red"><Icon name="info" :size="12" /> Конфликты <span class="n">{{ conflicts.length }}</span></div>
        <div v-for="s in conflicts" :key="'c:' + s.path" class="fi" :class="{ sel: selFile === s.path }" :title="s.path" @click="emit('select', s.path)">
          <span class="st red">!</span>
          <span class="ellipsis fname">{{ fname(s.path) }}</span>
          <span class="ellipsis dim fdir">{{ fdir(s.path) }}</span>
          <span class="acts always">
            <button class="tb" title="Взять свою версию (ours)" :disabled="busy" @click.stop="run({ op: 'resolve', path: s.path, side: 'ours' })">ours</button>
            <button class="tb" title="Взять входящую версию (theirs)" :disabled="busy" @click.stop="run({ op: 'resolve', path: s.path, side: 'theirs' })">theirs</button>
            <button class="tb ok" title="Я поправил файл вручную — пометить решённым" :disabled="busy" @click.stop="run({ op: 'stage', paths: [s.path] }, 'Конфликт решён')"><Icon name="check" :size="11" /></button>
          </span>
        </div>
      </template>

      <template v-if="staged.length">
        <div class="sh">
          В индексе <span class="n">{{ staged.length }}</span>
          <span class="spacer" />
          <button class="tb" title="Убрать всё из индекса" :disabled="busy" @click="run({ op: 'unstage', paths: [] })">− все</button>
        </div>
        <div v-for="s in staged" :key="'s:' + s.path" class="fi" :class="{ sel: selFile === s.path }" :title="s.path" @click="emit('select', s.path)">
          <span class="st" :class="color[code(s, 'index')]">{{ code(s, "index") }}</span>
          <span class="ellipsis fname">{{ fname(s.path) }}</span>
          <span class="ellipsis dim fdir">{{ fdir(s.path) }}</span>
          <span class="acts">
            <button class="tb" title="Убрать из индекса" :disabled="busy" @click.stop="run({ op: 'unstage', paths: [s.path] })">−</button>
          </span>
        </div>
      </template>

      <template v-if="unstaged.length">
        <div class="sh">
          Изменения <span class="n">{{ unstaged.length }}</span>
          <span class="spacer" />
          <button class="tb" :class="{ armed: armed === 'all' }" title="Отменить все изменения (создастся чекпоинт)" :disabled="busy" @click="discard('all', [])">
            {{ armed === "all" ? "отменить всё?" : "↺ все" }}
          </button>
          <button class="tb" title="Добавить всё в индекс" :disabled="busy" @click="run({ op: 'stage', paths: [] })">+ все</button>
        </div>
        <div v-for="s in unstaged" :key="'u:' + s.path" class="fi" :class="{ sel: selFile === s.path }" :title="s.path" @click="emit('select', s.path)">
          <span class="st" :class="color[code(s, 'worktree')]">{{ code(s, "worktree") }}</span>
          <span class="ellipsis fname">{{ fname(s.path) }}</span>
          <span class="ellipsis dim fdir">{{ fdir(s.path) }}</span>
          <span class="acts" :class="{ always: armed === s.path }">
            <button class="tb" :class="{ armed: armed === s.path }" :title="armed === s.path ? 'Нажмите ещё раз' : 'Отменить изменения в файле (создастся чекпоинт)'" :disabled="busy" @click.stop="discard(s.path, [s.path])">
              {{ armed === s.path ? "точно?" : "↺" }}
            </button>
            <button class="tb" title="Добавить в индекс" :disabled="busy" @click.stop="run({ op: 'stage', paths: [s.path] })">+</button>
          </span>
        </div>
      </template>

      <div v-if="!git.status.length" class="dim pad">Нет изменений</div>
    </div>
  </div>
</template>

<style scoped>
.gc { display: flex; flex-direction: column; flex: 1; min-height: 0; }
.commit { padding: 10px 12px; border-bottom: 1px solid var(--border); }
.msg { width: 100%; resize: vertical; min-height: 54px; height: auto; padding: 6px 8px; font-family: inherit; line-height: 1.4; }
.crow { display: flex; align-items: center; gap: 8px; margin-top: 6px; }
.chk { display: flex; align-items: center; gap: 5px; font-size: 12px; color: var(--text-2); cursor: pointer; }
.chk input { margin: 0; accent-color: var(--accent); }
.hint { font-size: 11px; color: var(--text-3); margin-top: 5px; }
.flist { flex: 1; overflow-y: auto; padding: 6px; }
.sh { display: flex; align-items: center; gap: 6px; padding: 8px 8px 3px; font-size: 10.5px; font-weight: 600; text-transform: uppercase; letter-spacing: .04em; color: var(--text-3); }
.sh.red { color: var(--red); }
.sh .n { font-weight: 500; }
.fi { display: flex; align-items: center; gap: 7px; padding: 3px 6px 3px 8px; border-radius: 5px; cursor: pointer; font-size: 12.5px; min-height: 26px; }
.fi:hover { background: var(--hover); }
.fi.sel { background: var(--active); }
.fname { flex: none; max-width: 50%; }
.fdir { font-size: 11px; min-width: 0; flex: 1; }
.st { font-family: var(--mono); font-size: 11px; font-weight: 700; width: 14px; text-align: center; flex: none; }
.st.green { color: var(--green); } .st.yellow { color: var(--yellow); } .st.red { color: var(--red); } .st.blue { color: var(--accent); }
.acts { display: flex; gap: 3px; margin-left: auto; opacity: 0; flex: none; }
.fi:hover .acts, .acts.always { opacity: 1; }
.tb { border: 1px solid var(--border); background: var(--panel-2); color: var(--text-2); border-radius: 4px; font-size: 11px; min-width: 20px; padding: 0 5px; height: 20px; cursor: pointer; display: inline-flex; align-items: center; justify-content: center; text-transform: none; letter-spacing: 0; font-weight: 500; }
.tb:hover:not(:disabled) { color: var(--text); border-color: var(--text-3); }
.tb:disabled { opacity: .5; cursor: default; }
.tb.armed { color: var(--red); border-color: var(--red); }
.tb.ok:hover:not(:disabled) { color: var(--green); border-color: var(--green); }
.pad { padding: 12px; }
</style>
