<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from "vue";
import Icon from "./Icon.vue";
import CodeViewer from "./CodeViewer.vue";
import { api, relPath, sepOf, type DirEntry, type FileHit, type TextHit } from "../lib/api";
import { state, type ProjectEntry } from "../lib/store";
import { size } from "../lib/format";

const props = defineProps<{ entry: ProjectEntry; request: { path: string; line?: number } | null }>();
const emit = defineEmits<{ history: [rel: string] }>();

const children = reactive<Record<string, DirEntry[]>>({});
const expanded = reactive<Record<string, boolean>>({});
const selected = ref<string | null>(null);
const line = ref<number | undefined>();
const query = ref("");
const mode = ref<"files" | "text">("files");
const fileHits = ref<FileHit[]>([]);
const textHits = ref<TextHit[]>([]);
const searching = ref(false);
const width = ref(310);

async function loadDir(path: string) {
  try {
    children[path] = await api.listDir(path);
  } catch {
    children[path] = [];
  }
}

async function toggle(e: DirEntry) {
  if (!e.isDir) {
    selected.value = e.path;
    line.value = undefined;
    return;
  }
  if (expanded[e.path]) {
    expanded[e.path] = false;
    return;
  }
  if (!children[e.path]) await loadDir(e.path);
  expanded[e.path] = true;
}

interface Row { e: DirEntry; depth: number }
const rows = computed<Row[]>(() => {
  const out: Row[] = [];
  const walk = (dir: string, depth: number) => {
    for (const e of children[dir] ?? []) {
      if (!state.settings.showHidden && e.hidden) continue;
      out.push({ e, depth });
      if (e.isDir && expanded[e.path]) walk(e.path, depth + 1);
    }
  };
  walk(props.entry.path, 0);
  return out;
});

async function refresh() {
  const open = Object.keys(expanded).filter((k) => expanded[k]);
  for (const k of Object.keys(children)) delete children[k];
  await loadDir(props.entry.path);
  for (const k of open) await loadDir(k);
}

async function reveal(path: string) {
  const root = props.entry.path;
  const sep = sepOf(root);
  const rel = relPath(root, path).split("/");
  let cur = root;
  for (let i = 0; i < rel.length - 1; i++) {
    cur = cur + sep + rel[i];
    if (!children[cur]) await loadDir(cur);
    expanded[cur] = true;
  }
  setTimeout(() => document.querySelector(".tree .row.sel")?.scrollIntoView({ block: "nearest" }), 50);
}

let searchTimer: number | undefined;
watch([query, mode], () => {
  clearTimeout(searchTimer);
  const q = query.value.trim();
  if (!q) {
    fileHits.value = [];
    textHits.value = [];
    return;
  }
  searchTimer = window.setTimeout(async () => {
    searching.value = true;
    try {
      if (mode.value === "files") fileHits.value = await api.searchFiles(props.entry.path, q, 200);
      else textHits.value = await api.searchText(props.entry.path, q, 500);
    } finally {
      searching.value = false;
    }
  }, mode.value === "files" ? 120 : 350);
});

function openHit(path: string, ln?: number, isDir = false) {
  if (isDir) {
    query.value = "";
    reveal(path + sepOf(path) + "_").then(() => (expanded[path] = true));
    return;
  }
  selected.value = path;
  line.value = ln;
  reveal(path);
}

watch(
  () => props.request,
  (r) => {
    if (r) {
      selected.value = r.path;
      line.value = r.line;
      reveal(r.path);
    }
  },
);

onMounted(async () => {
  await loadDir(props.entry.path);
  if (props.request) {
    selected.value = props.request.path;
    line.value = props.request.line;
    reveal(props.request.path);
  }
});

const fileIcon = (name: string) => {
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  const colors: Record<string, string> = {
    ts: "#3178c6", tsx: "#3178c6", js: "#e8d44d", jsx: "#e8d44d", mjs: "#e8d44d", cjs: "#e8d44d", vue: "#42b883", astro: "#ff5d01",
    json: "#cbcb41", md: "#519aba", css: "#563d7c", scss: "#c6538c", html: "#e34c26", rs: "#dea584", go: "#00add8",
    py: "#3572a5", toml: "#9c4221", yml: "#cb171e", yaml: "#cb171e", svg: "#ffb13b", png: "#a074c4", jpg: "#a074c4",
    env: "#faf594", lock: "#777", sh: "#89e051", ps1: "#012456", bat: "#c1f12e", php: "#4f5d95", cs: "#178600",
  };
  return colors[ext] || "var(--text-3)";
};

// resizable split
let dragStart = 0, startW = 0;
function startDrag(ev: MouseEvent) {
  dragStart = ev.clientX;
  startW = width.value;
  const move = (e: MouseEvent) => (width.value = Math.min(600, Math.max(200, startW + e.clientX - dragStart)));
  const up = () => { window.removeEventListener("mousemove", move); window.removeEventListener("mouseup", up); };
  window.addEventListener("mousemove", move);
  window.addEventListener("mouseup", up);
}
</script>

<template>
  <div class="files">
    <div class="left" :style="{ width: width + 'px' }">
      <div class="search">
        <div class="seg">
          <button :class="{ on: mode === 'files' }" @click="mode = 'files'">Файлы</button>
          <button :class="{ on: mode === 'text' }" @click="mode = 'text'">Текст</button>
        </div>
        <input v-model="query" class="input" :placeholder="mode === 'files' ? 'Найти файл… (напр. vite conf)' : 'Поиск по содержимому…'" />
      </div>
      <div class="tools">
        <span class="dim small" v-if="searching">поиск…</span>
        <span class="dim small" v-else-if="query && mode === 'files'">{{ fileHits.length }} найдено</span>
        <span class="dim small" v-else-if="query && mode === 'text'">{{ textHits.length }}{{ textHits.length >= 500 ? "+" : "" }} совпадений</span>
        <span class="spacer" />
        <label class="small dim chk"><input v-model="state.settings.showHidden" type="checkbox" /> скрытые</label>
        <button class="btn sm icon ghost" title="Обновить дерево" @click="refresh"><Icon name="refresh" :size="12" /></button>
      </div>

      <div v-if="query && mode === 'files'" class="list">
        <div v-for="h in fileHits" :key="h.path" class="hit" :class="{ sel: selected === h.path }" @click="openHit(h.path, undefined, h.isDir)">
          <Icon :name="h.isDir ? 'folder' : 'file'" :size="13" :style="{ color: h.isDir ? 'var(--accent)' : fileIcon(h.rel) }" />
          <span class="hname ellipsis">{{ h.rel.split("/").pop() }}</span>
          <span class="hdir ellipsis dim">{{ h.rel.split("/").slice(0, -1).join("/") }}</span>
        </div>
      </div>
      <div v-else-if="query && mode === 'text'" class="list">
        <div v-for="(h, i) in textHits" :key="i" class="thit" :class="{ sel: selected === h.path && line === h.line }" @click="openHit(h.path, h.line)">
          <div class="tpath ellipsis"><span class="mono">{{ h.rel }}</span><span class="dim">:{{ h.line }}</span></div>
          <div class="ttext ellipsis mono">{{ h.text }}</div>
        </div>
      </div>
      <div v-else class="list tree">
        <div
          v-for="{ e, depth } in rows"
          :key="e.path"
          class="row"
          :class="{ sel: selected === e.path, heavy: e.heavy, hidden: e.hidden }"
          :style="{ paddingLeft: 8 + depth * 14 + 'px' }"
          :title="e.isDir ? e.name : `${e.name} · ${size(e.size)}`"
          @click="toggle(e)"
        >
          <Icon v-if="e.isDir" name="chevron" :size="11" class="chev" :class="{ open: expanded[e.path] }" />
          <span v-else class="chev-sp" />
          <Icon :name="e.isDir ? (expanded[e.path] ? 'folder-open' : 'folder') : 'file'" :size="14" :style="{ color: e.isDir ? '#7aa2f7' : fileIcon(e.name) }" />
          <span class="ellipsis">{{ e.name }}</span>
        </div>
      </div>
    </div>
    <div class="splitter" @mousedown.prevent="startDrag" />
    <div class="right">
      <CodeViewer v-if="selected" :path="selected" :root="entry.path" :line="line" @history="(r: string) => emit('history', r)" />
      <div v-else class="empty">
        <Icon name="file" :size="36" />
        Выберите файл слева или найдите его через поиск
      </div>
    </div>
  </div>
</template>

<style scoped>
.files { display: flex; height: 100%; min-height: 0; }
.left { flex: none; display: flex; flex-direction: column; background: var(--bg-2); min-height: 0; }
.splitter { width: 4px; cursor: col-resize; border-left: 1px solid var(--border); flex: none; }
.splitter:hover { background: var(--accent-soft); }
.right { flex: 1; min-width: 0; }
.search { padding: 10px 10px 6px; display: flex; flex-direction: column; gap: 6px; }
.seg { display: flex; background: var(--panel); border: 1px solid var(--border); border-radius: 6px; padding: 2px; }
.seg button { flex: 1; border: 0; background: none; padding: 3px; border-radius: 4px; cursor: pointer; color: var(--text-2); font-size: 12px; }
.seg button.on { background: var(--active); color: var(--text); }
.tools { display: flex; align-items: center; gap: 6px; padding: 0 8px 6px 12px; min-height: 26px; }
.chk { display: flex; align-items: center; gap: 4px; cursor: pointer; }
.small { font-size: 11.5px; }
.list { flex: 1; overflow: auto; padding: 0 4px 10px; }
.row { display: flex; align-items: center; gap: 5px; height: 24px; padding-right: 8px; border-radius: 4px; cursor: pointer; white-space: nowrap; font-size: 12.8px; }
.row:hover, .hit:hover, .thit:hover { background: var(--hover); }
.row.sel, .hit.sel, .thit.sel { background: var(--active); }
.row.heavy, .row.hidden { opacity: .55; }
.chev { flex: none; transition: transform .12s; color: var(--text-3); }
.chev.open { transform: rotate(90deg); }
.chev-sp { width: 11px; flex: none; }
.hit { display: flex; align-items: center; gap: 6px; padding: 4px 8px; border-radius: 4px; cursor: pointer; }
.hname { flex: none; max-width: 60%; font-weight: 500; }
.hdir { font-size: 11.5px; }
.thit { padding: 5px 8px; border-radius: 4px; cursor: pointer; }
.tpath { font-size: 11.5px; }
.ttext { font-size: 12px; color: var(--text-2); }
</style>
