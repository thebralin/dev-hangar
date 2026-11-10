<script setup lang="ts">
import { computed, ref, watch } from "vue";
import Icon from "./Icon.vue";
import { api, joinPath, relPath, sameSep, type Commit, type GitSummary, type ProjectInfo, type ProjectStats, type TodoHit } from "../lib/api";
import { state, loadCheckpoints, type ProjectEntry } from "../lib/store";
import { relTime, fullDate, plural } from "../lib/format";

const props = defineProps<{ entry: ProjectEntry; info: ProjectInfo; git?: GitSummary; stats: ProjectStats | null }>();
const emit = defineEmits<{ "open-file": [path: string, line?: number]; goto: [tab: string] }>();

const commits = ref<Commit[]>([]);
const todos = ref<TodoHit[]>([]);
const repo = computed(() => props.git?.root || props.entry.path);

const awayDays = computed(() => {
  const p = props.entry.prevOpenedAt;
  if (!p) return null;
  return Math.floor((Date.now() - p) / 86_400_000);
});

const changed = computed(() => props.git?.status ?? []);
const recent = computed(() => (props.stats?.recent ?? []).slice(0, 8));
const lastCheckpoint = computed(() => state.checkpoints[props.entry.path]?.[0]);

function statusOf(s: { index: string; worktree: string }) {
  if (s.index === "?" || s.worktree === "?") return "U";
  return s.index !== " " ? s.index : s.worktree;
}
const statusColor: Record<string, string> = { A: "green", U: "green", M: "yellow", D: "red", R: "blue" };

function absRepo(rel: string) {
  return joinPath(sameSep(repo.value, props.entry.path), rel);
}

async function loadCommits() {
  if (!props.git?.isRepo) {
    commits.value = [];
    return;
  }
  commits.value = await api.gitLog(repo.value, 0, 4).catch(() => []);
}

async function loadTodos() {
  const paths = new Set<string>();
  for (const s of changed.value) if (s.worktree !== "D" && s.index !== "D") paths.add(absRepo(s.path));
  for (const f of recent.value) paths.add(joinPath(props.entry.path, f.path));
  todos.value = paths.size ? await api.scanTodos([...paths], 12).catch(() => []) : [];
}

watch(() => props.git?.lastCommit?.hash, loadCommits, { immediate: true });
watch(() => [props.git?.status.length, props.stats?.lastModified], loadTodos, { immediate: true });
if (!state.checkpoints[props.entry.path]) loadCheckpoints(props.entry.path);
</script>

<template>
  <section class="card resume">
    <div class="card-h">
      <Icon name="bookmark" :size="14" />
      Где я остановился
      <span v-if="awayDays != null && awayDays >= 1" class="badge" :class="awayDays >= 14 ? 'yellow' : ''">
        не открывали {{ awayDays }} {{ plural(awayDays, "день", "дня", "дней") }}
      </span>
      <span class="spacer" />
      <span v-if="git?.branch" class="muted small"><Icon name="branch" :size="12" /> {{ git.branch }}</span>
      <span v-if="lastCheckpoint" class="muted small" :title="lastCheckpoint.message">· чекпоинт {{ relTime(lastCheckpoint.date) }}</span>
    </div>
    <div class="cols">
      <div class="col">
        <div class="h">Последние коммиты</div>
        <template v-if="git?.isRepo">
          <div v-for="c in commits" :key="c.hash" class="it" :title="`${c.subject}\n${c.author} · ${fullDate(c.date)}`" @click="emit('goto', 'git')">
            <span class="ellipsis grow">{{ c.subject }}</span>
            <span class="dim small nowrap">{{ relTime(c.date) }}</span>
          </div>
          <div v-if="!commits.length" class="dim small">коммитов пока нет</div>
        </template>
        <div v-else class="dim small">не git-репозиторий</div>
      </div>

      <div class="col">
        <div class="h">
          Не закоммичено
          <span v-if="changed.length" class="badge yellow">{{ changed.length }}</span>
        </div>
        <template v-if="git?.isRepo">
          <div v-for="s in changed.slice(0, 7)" :key="s.path" class="it" :title="s.path" @click="emit('open-file', absRepo(s.path))">
            <span class="st" :class="statusColor[statusOf(s)]">{{ statusOf(s) }}</span>
            <span class="ellipsis grow mono">{{ s.path.split("/").pop() }}</span>
            <span class="ellipsis dim small dir">{{ s.path.split("/").slice(0, -1).join("/") }}</span>
          </div>
          <a v-if="changed.length > 7" class="small" @click="emit('goto', 'git')">ещё {{ changed.length - 7 }} → в Git</a>
          <div v-if="!changed.length" class="ok small"><Icon name="check" :size="12" /> всё закоммичено</div>
        </template>
        <div v-else class="dim small">—</div>
      </div>

      <div class="col">
        <div class="h">Недавно изменённые файлы</div>
        <div v-for="f in recent" :key="f.path" class="it" :title="`${f.path}\n${fullDate(f.modified)}`" @click="emit('open-file', joinPath(entry.path, f.path))">
          <span class="ellipsis grow mono">{{ f.path.split("/").pop() }}</span>
          <span class="ellipsis dim small dir">{{ f.path.split("/").slice(0, -1).join("/") }}</span>
          <span class="dim small nowrap">{{ relTime(f.modified) }}</span>
        </div>
        <div v-if="!stats" class="dim small">сканирую…</div>
      </div>
    </div>

    <div v-if="todos.length" class="todos">
      <div class="h">TODO / FIXME в недавних и изменённых файлах</div>
      <div v-for="(t, i) in todos" :key="i" class="it" @click="emit('open-file', t.path, t.line)">
        <span class="kind" :class="t.kind === 'TODO' ? 'blue' : 'red'">{{ t.kind }}</span>
        <span class="ellipsis grow">{{ t.text.replace(/^(TODO|FIXME|HACK|XXX|BUG)\s*[:(]?\s*/, "") || t.text }}</span>
        <span class="dim small mono nowrap">{{ relPath(entry.path, t.path).split("/").pop() }}:{{ t.line }}</span>
      </div>
    </div>
  </section>
</template>

<style scoped>
.resume { grid-column: 1 / -1; }
.cols { display: grid; grid-template-columns: 1fr 1fr 1fr; }
@media (max-width: 1150px) { .cols { grid-template-columns: 1fr; } }
.col { padding: 10px 14px 12px; min-width: 0; }
.col + .col { border-left: 1px solid var(--border); }
.h { font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: .04em; color: var(--text-3); margin-bottom: 6px; display: flex; align-items: center; gap: 6px; }
.it { display: flex; align-items: center; gap: 7px; padding: 3px 6px; margin: 0 -6px; border-radius: 4px; cursor: pointer; font-size: 12.5px; min-width: 0; }
.it:hover { background: var(--hover); }
.mono { font-size: 12px; }
.small { font-size: 11.5px; }
.nowrap { white-space: nowrap; flex: none; }
.dir { max-width: 40%; }
.st { font-family: var(--mono); font-size: 11px; font-weight: 700; width: 12px; flex: none; text-align: center; }
.st.green { color: var(--green); } .st.yellow { color: var(--yellow); } .st.red { color: var(--red); } .st.blue { color: var(--accent); }
.ok { color: var(--green); display: flex; align-items: center; gap: 5px; }
.todos { border-top: 1px solid var(--border); padding: 10px 14px 12px; }
.kind { font-family: var(--mono); font-size: 10.5px; font-weight: 700; padding: 0 5px; border-radius: 4px; flex: none; }
.kind.blue { color: var(--accent); background: var(--accent-soft); }
.kind.red { color: var(--red); background: var(--red-soft); }
</style>
