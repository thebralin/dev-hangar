<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { openUrl } from "@tauri-apps/plugin-opener";
import Icon from "./Icon.vue";
import { state, stopProc, restartProc, closeProc, type ProjectEntry } from "../lib/store";
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
</script>

<template>
  <div v-if="!procs.length" class="empty">
    <Icon name="terminal" :size="36" />
    Здесь будет вывод запущенных команд.
    <span class="dim">Нажмите «Dev» или «Build» вверху, либо запустите любой скрипт во вкладке «Обзор».</span>
  </div>
  <div v-else class="con">
    <div class="ptabs">
      <button v-for="p in procs" :key="p.id" class="ptab" :class="{ on: p.id === activeId }" @click="activeId = p.id">
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
    <div ref="box" class="out term selectable" @scroll="onScroll">
      <div v-for="(l, i) in visibleLines" :key="i" class="ln" :class="l.stream" v-html="l.html" />
    </div>
    <button v-if="!follow" class="btn sm follow" @click="follow = true; box && (box.scrollTop = box.scrollHeight)">↓ к концу</button>
  </div>
</template>

<style scoped>
.con { display: flex; flex-direction: column; height: 100%; position: relative; }
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
.out { flex: 1; overflow: auto; padding: 10px 16px 30px; background: var(--bg); }
.ln { white-space: pre-wrap; word-break: break-word; min-height: 1.5em; }
.follow { position: absolute; right: 24px; bottom: 16px; box-shadow: 0 4px 14px rgba(0,0,0,.3); }
</style>
