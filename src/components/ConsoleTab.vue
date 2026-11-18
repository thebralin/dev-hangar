<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { openUrl } from "@tauri-apps/plugin-opener";
import Icon from "./Icon.vue";
import { state, stopProc, restartProc, closeProc, runShell, sendInput, pushHistory, type ProjectEntry } from "../lib/store";
import { relTime } from "../lib/format";

const props = defineProps<{ entry: ProjectEntry }>();

const procs = computed(() => state.procOrder.map((id) => state.procs[id]).filter((p) => p && p.projectPath === props.entry.path));
const activeId = computed({
  get: () => {
    const id = state.activeProc[props.entry.path];
    return id && state.procs[id] ? id : procs.value[0]?.id;
  },
  set: (v) => { if (v) state.activeProc[props.entry.path] = v; },
});
const active = computed(() => (activeId.value ? state.procs[activeId.value] : undefined));

const box = ref<HTMLElement | null>(null);
const follow = ref(true);
const find = ref("");

function onScroll() {
  const el = box.value;
  if (!el) return;
  follow.value = el.scrollTop + el.clientHeight >= el.scrollHeight - 30;
}

function toEnd() {
  follow.value = true;
  if (box.value) box.value.scrollTop = box.value.scrollHeight;
}

watch(
  () => [active.value?.lines.length, activeId.value],
  async () => {
    if (!follow.value) return;
    await nextTick();
    if (box.value) box.value.scrollTop = box.value.scrollHeight;
  },
  { immediate: true },
);

const visibleLines = computed(() => {
  const l = active.value?.lines ?? [];
  const f = find.value.trim().toLowerCase();
  if (!f) return l;
  return l.filter((x) => x.html.toLowerCase().includes(f));
});

function clear() {
  if (active.value) active.value.lines = [];
}
function copyAll() {
  const text = (active.value?.lines ?? []).map((l) => l.html.replace(/<[^>]+>/g, "").replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&amp;/g, "&").replace(/&quot;/g, '"').replace(/&#39;/g, "'").replace(/&nbsp;/g, " ")).join("\n");
  navigator.clipboard.writeText(text);
}

// ---------- input ----------
const input = ref("");
const inputEl = ref<HTMLInputElement | null>(null);
/** "stdin" = type into the selected running process, "cmd" = run a new command in the project folder */
const wantMode = ref<"stdin" | "cmd">("stdin");
const canStdin = computed(() => active.value?.status === "running");
const mode = computed(() => (wantMode.value === "stdin" && canStdin.value ? "stdin" : "cmd"));
let histPos = -1;
let draft = "";

const placeholder = computed(() =>
  mode.value === "stdin"
    ? `Ввод в «${active.value?.label}» — Enter отправит строку`
    : "Команда в папке проекта, например: npm i lodash · git status · ↑ — история",
);

async function submit() {
  const text = input.value;
  input.value = "";
  histPos = -1;
  if (mode.value === "stdin" && active.value) {
    if (text.trim()) pushHistory(props.entry, text);
    toEnd();
    await sendInput(active.value.id, text);
    return;
  }
  const cmd = text.trim();
  if (!cmd) return;
  pushHistory(props.entry, cmd);
  if (/^(cls|clear)$/i.test(cmd)) {
    clear();
    return;
  }
  follow.value = true;
  await runShell(props.entry.path, cmd);
  // a new shell process got focus — keep typing into the command line
  wantMode.value = "cmd";
}

function history(dir: -1 | 1) {
  const h = props.entry.cmdHistory ?? [];
  if (!h.length) return;
  if (histPos === -1) {
    if (dir === 1) return;
    draft = input.value;
    histPos = h.length;
  }
  histPos += dir;
  if (histPos >= h.length) {
    histPos = -1;
    input.value = draft;
    return;
  }
  histPos = Math.max(0, histPos);
  input.value = h[histPos];
  nextTick(() => inputEl.value?.setSelectionRange(input.value.length, input.value.length));
}

function toggleMode() {
  wantMode.value = mode.value === "stdin" ? "cmd" : "stdin";
  inputEl.value?.focus();
}

function selectProc(id: string) {
  activeId.value = id;
  if (state.procs[id]?.status === "running" && state.procs[id]?.kind !== "shell") wantMode.value = "stdin";
}

/** Clicking empty space in the output focuses the input (unless the user is selecting text). */
function focusInput() {
  if (window.getSelection()?.toString()) return;
  inputEl.value?.focus();
}
</script>

<template>
  <div class="con">
    <template v-if="procs.length">
      <div class="ptabs">
        <button v-for="p in procs" :key="p.id" class="ptab" :class="{ on: p.id === activeId }" @click="selectProc(p.id)">
          <span class="st" :class="p.status" />
          <span class="ellipsis">{{ p.label }}</span>
          <span v-if="p.status !== 'running'" class="x" title="Закрыть" @click.stop="closeProc(p.id)"><Icon name="x" :size="11" /></span>
        </button>
      </div>
      <div v-if="active" class="pbar">
        <code class="ellipsis cmd" :title="active.cwd">$ {{ active.command }}</code>
        <span v-if="active.status === 'running'" class="badge green">работает · {{ relTime(active.startedAt) }}</span>
        <span v-else-if="active.status === 'exited'" class="badge">завершён (0)</span>
        <span v-else class="badge red">ошибка{{ active.exitCode != null ? ` (${active.exitCode})` : "" }}</span>
        <button v-if="active.url" class="btn sm primary" @click="openUrl(active.url!)"><Icon name="globe" :size="12" /> {{ active.url }}</button>
        <span class="spacer" />
        <input v-model="find" class="input find" placeholder="Фильтр вывода…" />
        <button class="btn sm icon ghost" title="Скопировать вывод" @click="copyAll"><Icon name="copy" :size="13" /></button>
        <button class="btn sm icon ghost" title="Очистить" @click="clear"><Icon name="trash" :size="13" /></button>
        <button class="btn sm" title="Перезапустить" @click="restartProc(active.id)"><Icon name="restart" :size="12" /> Перезапуск</button>
        <button v-if="active.status === 'running'" class="btn sm danger" @click="stopProc(active.id)"><Icon name="stop" :size="12" /> Стоп</button>
      </div>
      <div ref="box" class="out term selectable" @scroll="onScroll" @mouseup="focusInput">
        <div v-for="(l, i) in visibleLines" :key="i" class="ln" :class="l.stream" v-html="l.html" />
      </div>
      <button v-if="!follow" class="btn sm follow" @click="toEnd">↓ к концу</button>
    </template>
    <div v-else class="empty">
      <Icon name="terminal" :size="36" />
      Здесь будет вывод запущенных команд.
      <span class="dim">Нажмите «Dev» или «Build» вверху, запустите скрипт во вкладке «Обзор» — или введите команду ниже.</span>
    </div>

    <form class="inbar" :class="mode" @submit.prevent="submit">
      <button
        type="button"
        class="mode"
        :disabled="!canStdin"
        :title="canStdin
          ? (mode === 'stdin' ? 'Сейчас: ввод в запущенный процесс. Нажмите — запускать новую команду' : 'Сейчас: новая команда. Нажмите — писать в запущенный процесс')
          : 'Новая команда в папке проекта'"
        @click="toggleMode"
      >
        <template v-if="mode === 'stdin'"><Icon name="send" :size="12" /> в процесс</template>
        <template v-else><span class="dollar">$</span> команда</template>
      </button>
      <input
        ref="inputEl"
        v-model="input"
        class="cmdline mono"
        :placeholder="placeholder"
        spellcheck="false"
        autocomplete="off"
        @keydown.up.prevent="history(-1)"
        @keydown.down.prevent="history(1)"
        @keydown.esc="input = ''"
      />
      <button type="submit" class="btn sm" :disabled="mode === 'cmd' && !input.trim()">
        <Icon :name="mode === 'stdin' ? 'send' : 'play'" :size="12" /> {{ mode === "stdin" ? "Отправить" : "Запустить" }}
      </button>
    </form>
  </div>
</template>

<style scoped>
.con { display: flex; flex-direction: column; height: 100%; position: relative; }
.con > .empty { flex: 1; }
.ptabs { display: flex; gap: 2px; padding: 8px 12px 0; border-bottom: 1px solid var(--border); overflow-x: auto; background: var(--bg-2); }
.ptab {
  display: flex; align-items: center; gap: 7px; max-width: 260px; padding: 6px 10px; border: 1px solid transparent; border-bottom: 0;
  background: none; border-radius: 6px 6px 0 0; cursor: pointer; color: var(--text-2); font-size: 12.5px;
}
.ptab.on { background: var(--bg); border-color: var(--border); color: var(--text); margin-bottom: -1px; }
.ptab .x { display: flex; opacity: .6; } .ptab .x:hover { opacity: 1; }
.st { width: 8px; height: 8px; border-radius: 50%; flex: none; background: var(--text-3); }
.st.running { background: var(--green); box-shadow: 0 0 0 3px var(--green-soft); }
.st.failed { background: var(--red); }
.pbar { display: flex; align-items: center; gap: 8px; padding: 8px 14px; border-bottom: 1px solid var(--border); }
.cmd { font-size: 12px; color: var(--text-2); max-width: 40%; }
.find { width: 180px; height: 24px; font-size: 12px; }
.out { flex: 1; overflow: auto; padding: 10px 16px 16px; background: var(--bg); }
.ln { white-space: pre-wrap; word-break: break-word; min-height: 1.5em; }
.ln.stdin { color: var(--accent); }
.follow { position: absolute; right: 24px; bottom: 60px; box-shadow: 0 4px 14px rgba(0,0,0,.3); }

.inbar {
  display: flex; align-items: center; gap: 8px; padding: 8px 12px; flex: none;
  border-top: 1px solid var(--border); background: var(--bg-2);
}
.mode {
  display: flex; align-items: center; gap: 5px; height: 26px; padding: 0 9px; flex: none; cursor: pointer;
  border: 1px solid var(--border-2); border-radius: 6px; background: var(--panel); color: var(--text-2);
  font-size: 11.5px; white-space: nowrap;
}
.mode:hover:not(:disabled) { color: var(--text); border-color: var(--accent-border); }
.mode:disabled { cursor: default; }
.inbar.stdin .mode { color: var(--accent); border-color: var(--accent-border); }
.dollar { font-family: var(--mono); font-weight: 700; color: var(--green); }
.cmdline {
  flex: 1; min-width: 0; height: 28px; padding: 0 10px; font-size: 12.5px;
  background: var(--bg); color: var(--text); border: 1px solid var(--border); border-radius: 6px; outline: none;
}
.cmdline:focus { border-color: var(--accent-border); }
</style>
