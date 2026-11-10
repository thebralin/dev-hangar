<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import Icon from "./Icon.vue";
import { api, type ArtifactDir } from "../lib/api";
import { state, activityOf, displayName, isRunning, refreshBrief, selectProject, toast, type ProjectEntry } from "../lib/store";
import { relTime, fullDate, size, plural } from "../lib/format";

interface Row extends ArtifactDir {
  project: ProjectEntry;
}

const rows = ref<Row[]>([]);
const scanned = ref(0);
const scanning = ref(false);
const selected = reactive<Record<string, boolean>>({});
const confirm = ref(false);
const deleting = ref(false);
const onlyDeps = ref(false);
const groupBy = ref<"project" | "size">("project");

const total = computed(() => rows.value.reduce((s, r) => s + r.size, 0));
const shown = computed(() => {
  let list = rows.value.filter((r) => !onlyDeps.value || r.kind === "deps");
  if (groupBy.value === "size") list = [...list].sort((a, b) => b.size - a.size);
  return list;
});
const selRows = computed(() => rows.value.filter((r) => selected[r.path]));
const selSize = computed(() => selRows.value.reduce((s, r) => s + r.size, 0));

const byProject = computed(() => {
  const m = new Map<string, { entry: ProjectEntry; rows: Row[]; size: number }>();
  for (const r of shown.value) {
    const k = r.project.path;
    if (!m.has(k)) m.set(k, { entry: r.project, rows: [], size: 0 });
    const g = m.get(k)!;
    g.rows.push(r);
    g.size += r.size;
  }
  return [...m.values()].sort((a, b) => b.size - a.size);
});

async function scan() {
  scanning.value = true;
  rows.value = [];
  scanned.value = 0;
  for (const k of Object.keys(selected)) delete selected[k];
  const queue = [...state.projects];
  const worker = async () => {
    while (queue.length) {
      const p = queue.shift()!;
      const found = await api.scanArtifacts(p.path).catch(() => []);
      rows.value.push(...found.map((f) => ({ ...f, project: p })));
      scanned.value++;
    }
  };
  await Promise.all([worker(), worker()]);
  rows.value.sort((a, b) => b.size - a.size);
  scanning.value = false;
}

function daysIdle(r: Row) {
  const a = activityOf(r.project.path);
  return a ? Math.floor((Date.now() - a) / 86_400_000) : 9999;
}

function selectIdle(days: number) {
  for (const r of rows.value) {
    selected[r.path] = daysIdle(r) >= days && !isRunning(r.project.path) && (!onlyDeps.value || r.kind === "deps");
  }
}

function clearSel() {
  for (const k of Object.keys(selected)) selected[k] = false;
}

async function doDelete() {
  if (!confirm.value) {
    confirm.value = true;
    return;
  }
  deleting.value = true;
  const list = selRows.value.filter((r) => !isRunning(r.project.path));
  const roots = [...new Set(list.map((r) => r.project.path))];
  try {
    const res = await api.deleteArtifacts(roots, list.map((r) => r.path));
    const ok = new Set(res.filter((x) => x.ok).map((x) => x.path));
    const freed = list.filter((r) => ok.has(r.path)).reduce((s, r) => s + r.size, 0);
    rows.value = rows.value.filter((r) => !ok.has(r.path));
    for (const p of ok) delete selected[p];
    const failed = res.filter((x) => !x.ok);
    toast(
      `Освобождено ${size(freed)}` + (failed.length ? `. Не удалось удалить: ${failed.length} (${failed[0].error})` : ""),
      failed.length ? "error" : "info",
    );
    for (const r of roots) refreshBrief(r);
  } catch (e) {
    toast(String(e), "error");
  } finally {
    deleting.value = false;
    confirm.value = false;
  }
}

const kindLabel: Record<string, string> = { deps: "зависимости", build: "сборка", cache: "кэш" };

onMounted(scan);
</script>

<template>
  <div class="disk">
    <header class="dh">
      <div class="title">
        <Icon name="drive" :size="20" />
        <h1>Уборка диска</h1>
        <span class="muted">node_modules, target, dist, .next и прочее, что восстанавливается командами install или build</span>
      </div>
      <div class="bar">
        <div class="stat"><b>{{ size(total) }}</b><span class="dim"> в {{ rows.length }} папках</span></div>
        <span v-if="scanning" class="dim">сканирую {{ scanned }} / {{ state.projects.length }}…</span>
        <span class="spacer" />
        <span class="dim">Выбрать неактивные:</span>
        <button class="btn sm" @click="selectIdle(30)">30+ дней</button>
        <button class="btn sm" @click="selectIdle(60)">60+ дней</button>
        <button class="btn sm" @click="selectIdle(180)">полгода+</button>
        <button class="btn sm ghost" @click="clearSel">Сбросить</button>
        <label class="chk"><input v-model="onlyDeps" type="checkbox" /> только зависимости</label>
        <select v-model="groupBy" class="input sel">
          <option value="project">по проектам</option>
          <option value="size">по размеру</option>
        </select>
        <button class="btn sm icon ghost" title="Пересканировать" :disabled="scanning" @click="scan"><Icon name="refresh" :size="13" /></button>
      </div>
    </header>

    <div class="list">
      <template v-if="groupBy === 'project'">
        <section v-for="g in byProject" :key="g.entry.path" class="grp">
          <div class="gh">
            <a class="pname" @click="selectProject(g.entry.path)">{{ displayName(g.entry) }}</a>
            <span class="dim small" :title="fullDate(activityOf(g.entry.path))">активность {{ relTime(activityOf(g.entry.path)) }}</span>
            <span v-if="isRunning(g.entry.path)" class="badge green">запущен</span>
            <span class="spacer" />
            <b>{{ size(g.size) }}</b>
          </div>
          <label v-for="r in g.rows" :key="r.path" class="r" :class="{ dis: isRunning(r.project.path) }">
            <input v-model="selected[r.path]" type="checkbox" :disabled="isRunning(r.project.path)" />
            <span class="mono ellipsis grow">{{ r.rel }}</span>
            <span class="badge">{{ kindLabel[r.kind] }}</span>
            <span class="dim small files">{{ r.files.toLocaleString("ru") }} {{ plural(r.files, "файл", "файла", "файлов") }}</span>
            <span class="sz">{{ size(r.size) }}</span>
          </label>
        </section>
      </template>
      <template v-else>
        <label v-for="r in shown" :key="r.path" class="r flat" :class="{ dis: isRunning(r.project.path) }">
          <input v-model="selected[r.path]" type="checkbox" :disabled="isRunning(r.project.path)" />
          <span class="pn ellipsis">{{ displayName(r.project) }}</span>
          <span class="mono ellipsis grow">{{ r.rel }}</span>
          <span class="badge">{{ kindLabel[r.kind] }}</span>
          <span class="dim small files">{{ relTime(activityOf(r.project.path)) }}</span>
          <span class="sz">{{ size(r.size) }}</span>
        </label>
      </template>
      <div v-if="!scanning && !rows.length" class="empty">Нечего чистить</div>
    </div>

    <footer v-if="selRows.length" class="foot">
      <span>Выбрано: <b>{{ selRows.length }}</b> {{ plural(selRows.length, "папка", "папки", "папок") }}, <b style="white-space: nowrap">{{ size(selSize) }}</b></span>
      <span v-if="confirm" class="warn">Папки удалятся безвозвратно, минуя корзину. Восстановить их можно кнопкой Install или Build в проекте.</span>
      <span class="spacer" />
      <button v-if="confirm" class="btn ghost" @click="confirm = false">Отмена</button>
      <button class="btn danger" :disabled="deleting" @click="doDelete">
        <Icon name="trash" :size="13" /> {{ deleting ? "Удаляю…" : confirm ? `Да, удалить ${size(selSize)}` : "Удалить выбранное" }}
      </button>
    </footer>
  </div>
</template>

<style scoped>
.disk { display: flex; flex-direction: column; height: 100%; }
.dh { padding: 16px 20px 12px; border-bottom: 1px solid var(--border); background: var(--bg-2); }
.title { display: flex; align-items: center; gap: 10px; }
.title svg { color: var(--accent); }
h1 { font-size: 19px; margin: 0; }
.bar { display: flex; align-items: center; gap: 8px; margin-top: 12px; flex-wrap: wrap; }
.stat { font-size: 15px; margin-right: 10px; }
.chk { display: flex; align-items: center; gap: 5px; cursor: pointer; color: var(--text-2); margin-left: 6px; }
.sel { width: 140px; height: 24px; font-size: 12px; }
.list { flex: 1; overflow-y: auto; padding: 12px 20px 20px; }
.grp { background: var(--panel); border: 1px solid var(--border); border-radius: 8px; margin-bottom: 10px; }
.gh { display: flex; align-items: center; gap: 10px; padding: 8px 14px; border-bottom: 1px solid var(--border); }
.pname { font-weight: 600; color: var(--text); }
.small { font-size: 11.5px; }
.r { display: flex; align-items: center; gap: 10px; padding: 5px 14px; cursor: pointer; font-size: 12.5px; }
.r:hover { background: var(--hover); }
.r.dis { opacity: .5; cursor: default; }
.r.flat { border-bottom: 1px solid var(--border); }
.pn { width: 200px; flex: none; font-weight: 500; }
.files { width: 110px; text-align: right; flex: none; }
.sz { width: 80px; text-align: right; font-family: var(--mono); font-size: 12px; flex: none; }
.foot { display: flex; align-items: center; gap: 12px; padding: 10px 20px; border-top: 1px solid var(--border); background: var(--bg-2); }
.warn { color: var(--yellow); font-size: 12px; }
</style>
