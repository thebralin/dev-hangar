<script setup lang="ts">
import { computed, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import Icon from "./Icon.vue";
import StackBadge from "./StackBadge.vue";
import ProjectAvatar from "./ProjectAvatar.vue";
import {
  state, addProject, removeProject, selectProject, scanFolder, activityOf, isRunning,
  displayName, toast, norm, refreshAllBriefs, filterProjects, type ProjectEntry,
} from "../lib/store";
import { parentDir, baseName } from "../lib/api";
import { relTime } from "../lib/format";
import { host, pluginOf } from "../plugins/host";
import { LATEST_VERSION } from "../lib/changelog";

const emit = defineEmits<{ settings: []; whatsNew: [] }>();
const unseen = computed(() => state.loaded && state.settings.lastSeenVersion !== LATEST_VERSION);
const q = ref("");
const menu = ref(false);
const scanning = ref(false);

const collapsed = computed(() => state.settings.sidebarCollapsed);
function toggleCollapsed() {
  state.settings.sidebarCollapsed = !state.settings.sidebarCollapsed;
  menu.value = false;
}

const filtered = computed(() => filterProjects(collapsed.value ? "" : q.value));

const groups = computed(() => {
  if (!state.settings.groupByFolder || q.value) return [{ key: "", name: "", items: filtered.value }];
  const pinned = filtered.value.filter((p) => p.pinned);
  const map = new Map<string, ProjectEntry[]>();
  for (const p of filtered.value) {
    if (p.pinned) continue;
    const g = parentDir(p.path);
    if (!map.has(g)) map.set(g, []);
    map.get(g)!.push(p);
  }
  const out = [] as { key: string; name: string; items: ProjectEntry[] }[];
  if (pinned.length) out.push({ key: "__pinned", name: "Закреплённые", items: pinned });
  const keys = [...map.keys()].sort((a, b) => baseName(a).localeCompare(baseName(b), "ru", { numeric: true }));
  for (const k of keys) out.push({ key: k, name: baseName(k) || k, items: map.get(k)! });
  return out;
});

const runningCount = computed(() => Object.values(state.procs).filter((p) => p.status === "running").length);

function isActive(p: ProjectEntry) {
  return state.view === "project" && !!state.selected && norm(state.selected) === norm(p.path);
}

function toggleView(v: "grid" | "disk" | "plugins") {
  state.view = state.view === v ? "project" : v;
}

async function addDialog() {
  menu.value = false;
  const res = await open({ directory: true, multiple: true, title: "Выберите папку проекта" });
  if (!res) return;
  const paths = Array.isArray(res) ? res : [res];
  for (const p of paths) await addProject(p, paths.length === 1);
}

async function scanDialog() {
  menu.value = false;
  const res = await open({ directory: true, multiple: false, title: "Папка, в которой лежат проекты" });
  if (!res || Array.isArray(res)) return;
  scanning.value = true;
  try {
    const n = await scanFolder(res);
    toast(n ? `Найдено и добавлено проектов: ${n}` : "Новых проектов не найдено");
  } finally {
    scanning.value = false;
  }
}

function remove(p: ProjectEntry) {
  if (isRunning(p.path)) {
    toast("Сначала остановите запущенные процессы", "error");
    return;
  }
  removeProject(p.path);
}

function toggleGroup(key: string) {
  state.collapsedGroups[key] = !state.collapsedGroups[key];
}

function dirtyCount(p: ProjectEntry) {
  return state.briefs[p.path]?.git?.status.length ?? 0;
}
</script>

<template>
  <!-- ───────── collapsed: narrow rail ───────── -->
  <aside v-if="collapsed" class="sidebar rail">
    <div class="rhead">
      <button class="rbtn logo-btn" title="Развернуть панель (Ctrl+B)" @click="toggleCollapsed">
        <svg class="mark" width="20" height="20" viewBox="0 0 24 24" aria-hidden="true">
          <path d="M3 20V11a9 9 0 0 1 18 0v9z" fill="currentColor" />
          <rect x="7.5" y="12.5" width="9" height="7.5" fill="var(--bg-2)" />
          <path d="M9.5 14.5l2 1.5-2 1.5M12.8 17.6h2" stroke="var(--text)" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" fill="none" />
        </svg>
      </button>
      <button class="rbtn" :class="{ on: state.view === 'grid' }" title="Все проекты сеткой" @click="toggleView('grid')"><Icon name="grid" :size="16" /></button>
      <div class="add-wrap">
        <button class="rbtn" title="Добавить" @click="menu = !menu"><Icon name="plus" :size="16" /></button>
        <div v-if="menu" class="menu rail-menu" @mouseleave="menu = false">
          <button @click="addDialog"><Icon name="folder" :size="14" /> Добавить проект…</button>
          <button @click="scanDialog"><Icon name="search" :size="14" /> Найти проекты в папке…</button>
          <button @click="menu = false; refreshAllBriefs()"><Icon name="refresh" :size="14" /> Обновить все</button>
        </div>
      </div>
    </div>

    <div class="rlist">
      <button
        v-for="p in filtered"
        :key="p.path"
        class="ritem"
        :class="{ active: isActive(p) }"
        :title="`${displayName(p)}\n${p.path}`"
        @click="selectProject(p.path)"
      >
        <ProjectAvatar :entry="p" :size="34" />
        <span v-if="isRunning(p.path)" class="rlive" />
        <span v-else-if="dirtyCount(p)" class="rdirty" />
      </button>
      <div v-if="scanning" class="rscan"><span class="spinner" /></div>
    </div>

    <div v-if="host.pages.length" class="rpages">
      <button
        v-for="p in host.pages"
        :key="p.key"
        class="rbtn"
        :class="{ on: state.view === p.key }"
        :title="`${p.page.label} — плагин «${pluginOf(p.pluginId)?.manifest?.name ?? p.pluginId}»`"
        @click="state.view = state.view === p.key ? 'project' : p.key"
      >
        <Icon :name="p.page.icon || 'puzzle'" :size="16" />
      </button>
    </div>

    <div class="rfoot">
      <span v-if="runningCount" class="badge green rcount" title="Запущенные процессы">{{ runningCount }}</span>
      <button class="rbtn" :class="{ on: state.view === 'disk' }" title="Уборка диска" @click="toggleView('disk')"><Icon name="drive" :size="16" /></button>
      <button class="rbtn" :class="{ on: state.view === 'plugins' }" title="Плагины" @click="toggleView('plugins')"><Icon name="puzzle" :size="16" /></button>
      <button
        class="rbtn"
        :title="state.settings.theme === 'dark' ? 'Светлая тема' : 'Тёмная тема'"
        @click="state.settings.theme = state.settings.theme === 'dark' ? 'light' : 'dark'"
      >
        <Icon :name="state.settings.theme === 'dark' ? 'sun' : 'moon'" :size="16" />
      </button>
      <button class="rbtn news" :title="unseen ? 'Что нового — есть обновления' : 'Что нового'" @click="emit('whatsNew')">
        <Icon name="sparkles" :size="16" /><span v-if="unseen" class="ndot" />
      </button>
      <button class="rbtn" title="Настройки" @click="emit('settings')"><Icon name="settings" :size="16" /></button>
    </div>
  </aside>

  <!-- ───────── expanded ───────── -->
  <aside v-else class="sidebar">
    <div class="head">
      <div class="logo">
        <svg class="mark" width="20" height="20" viewBox="0 0 24 24" aria-hidden="true">
          <path d="M3 20V11a9 9 0 0 1 18 0v9z" fill="currentColor" />
          <rect x="7.5" y="12.5" width="9" height="7.5" fill="var(--bg-2)" />
          <path d="M9.5 14.5l2 1.5-2 1.5M12.8 17.6h2" stroke="var(--text)" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" fill="none" />
        </svg>
        <span>Проекты</span>
        <span class="count">{{ state.projects.length }}</span>
      </div>
      <div class="head-tools">
        <button class="btn sm icon ghost" :class="{ on: state.view === 'grid' }" title="Все проекты сеткой" @click="toggleView('grid')"><Icon name="grid" :size="14" /></button>
        <div class="add-wrap">
          <button class="btn sm icon primary" title="Добавить" @click="menu = !menu"><Icon name="plus" :size="15" /></button>
          <div v-if="menu" class="menu" @mouseleave="menu = false">
            <button @click="addDialog"><Icon name="folder" :size="14" /> Добавить проект…</button>
            <button @click="scanDialog"><Icon name="search" :size="14" /> Найти проекты в папке…</button>
            <button @click="menu = false; refreshAllBriefs()"><Icon name="refresh" :size="14" /> Обновить все</button>
          </div>
        </div>
        <button class="btn sm icon ghost" title="Свернуть панель (Ctrl+B)" @click="toggleCollapsed"><Icon name="panel-left" :size="14" /></button>
      </div>
    </div>

    <div class="search">
      <Icon name="search" :size="14" class="si" />
      <input v-model="q" class="input" placeholder="Поиск проекта или стека…" />
    </div>

    <div v-if="scanning" class="scanning"><span class="spinner" /> Сканирую папку…</div>

    <div class="list">
      <template v-for="g in groups" :key="g.key">
        <div v-if="g.name" class="group" @click="toggleGroup(g.key)" :title="g.key">
          <Icon name="chevron" :size="12" class="chev" :class="{ open: !state.collapsedGroups[g.key] }" />
          <span class="ellipsis">{{ g.name }}</span>
          <span class="dim">{{ g.items.length }}</span>
        </div>
        <template v-if="!g.name || !state.collapsedGroups[g.key]">
          <div
            v-for="p in g.items"
            :key="p.path"
            class="item"
            :class="{ active: isActive(p) }"
            :title="p.path"
            @click="selectProject(p.path)"
          >
            <div class="line1">
              <span v-if="isRunning(p.path)" class="live" title="Запущен процесс" />
              <span class="name ellipsis">{{ displayName(p) }}</span>
              <span v-if="dirtyCount(p)" class="dirty" :title="`Незакоммиченных изменений: ${dirtyCount(p)}`">●{{ dirtyCount(p) }}</span>
              <div class="tools" @click.stop>
                <button class="btn sm icon ghost" :title="p.pinned ? 'Открепить' : 'Закрепить'" @click="p.pinned = !p.pinned">
                  <Icon name="pin" :size="12" />
                </button>
                <button class="btn sm icon ghost" title="Убрать из списка" @click="remove(p)"><Icon name="x" :size="12" /></button>
              </div>
            </div>
            <div class="line2">
              <template v-if="state.briefs[p.path]?.info?.exists === false">
                <span class="badge red">папка не найдена</span>
              </template>
              <template v-else>
                <StackBadge v-for="s in (state.briefs[p.path]?.info?.stacks ?? []).filter(s => !['ts','node','vite'].includes(s.id)).slice(0, 3)" :key="s.id" :stack="s" mini />
                <span class="spacer" />
                <span class="when dim">{{ relTime(activityOf(p.path)) }}</span>
              </template>
            </div>
          </div>
        </template>
      </template>
      <div v-if="!state.projects.length" class="hint">
        Перетащите папку в окно или нажмите <b>+</b>
      </div>
    </div>

    <div v-if="host.pages.length" class="pages">
      <button
        v-for="p in host.pages"
        :key="p.key"
        class="page"
        :class="{ on: state.view === p.key }"
        :title="`Плагин «${pluginOf(p.pluginId)?.manifest?.name ?? p.pluginId}»`"
        @click="state.view = state.view === p.key ? 'project' : p.key"
      >
        <Icon :name="p.page.icon || 'puzzle'" :size="14" />
        <span class="ellipsis">{{ p.page.label }}</span>
      </button>
    </div>

    <div class="foot">
      <button class="btn sm icon ghost" title="Настройки" @click="emit('settings')"><Icon name="settings" :size="14" /></button>
      <button class="btn sm ghost" :class="{ on: state.view === 'disk' }" title="Уборка node_modules, target, dist по всем проектам" @click="toggleView('disk')"><Icon name="drive" :size="14" /> Диск</button>
      <button class="btn sm ghost" :class="{ on: state.view === 'plugins' }" title="Плагины: свои вкладки, кнопки и инструменты" @click="toggleView('plugins')"><Icon name="puzzle" :size="14" /> Плагины</button>
      <span class="spacer" />
      <span v-if="runningCount" class="badge green" title="Запущенные процессы"><span class="dot" style="background: var(--green)" />{{ runningCount }}</span>
      <button class="btn sm icon ghost news" :title="unseen ? 'Что нового — есть обновления' : 'Что нового'" @click="emit('whatsNew')">
        <Icon name="sparkles" :size="14" /><span v-if="unseen" class="ndot" />
      </button>
      <button
        class="btn sm icon ghost"
        :title="state.settings.theme === 'dark' ? 'Светлая тема' : 'Тёмная тема'"
        @click="state.settings.theme = state.settings.theme === 'dark' ? 'light' : 'dark'"
      >
        <Icon :name="state.settings.theme === 'dark' ? 'sun' : 'moon'" :size="14" />
      </button>
    </div>
  </aside>
</template>

<style scoped>
.sidebar {
  width: 280px; flex: none; display: flex; flex-direction: column;
  background: var(--bg-2); border-right: 1px solid var(--border);
}
.head { display: flex; align-items: center; justify-content: space-between; padding: 12px 10px 8px 14px; }
.head-tools { display: flex; align-items: center; gap: 2px; }
.head-tools .btn.on { background: var(--active); color: var(--text); }
.logo { display: flex; align-items: center; gap: 8px; font-weight: 650; font-size: 14px; }
.logo svg, .logo-btn svg { color: var(--accent); }
.count { font-size: 11px; color: var(--text-3); font-weight: 500; }
.add-wrap { position: relative; }
.menu {
  position: absolute; right: 0; top: 30px; z-index: 50; min-width: 230px;
  background: var(--panel); border: 1px solid var(--border-2); border-radius: 8px; padding: 4px;
  box-shadow: 0 10px 30px rgba(0,0,0,.35);
}
.menu button {
  display: flex; align-items: center; gap: 8px; width: 100%; text-align: left;
  background: none; border: 0; padding: 7px 10px; border-radius: 5px; cursor: pointer;
}
.menu button:hover { background: var(--hover); }
.search { position: relative; padding: 0 10px 8px; }
.search .input { padding-left: 30px; height: 30px; background: var(--panel); }
.search .si { position: absolute; left: 20px; top: 8px; color: var(--text-3); }
.scanning { padding: 4px 14px 8px; color: var(--text-2); display: flex; gap: 8px; align-items: center; }
.list { flex: 1; overflow-y: auto; padding: 0 6px 8px; }
.group {
  display: flex; align-items: center; gap: 5px; padding: 10px 8px 4px; cursor: pointer;
  font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: .04em; color: var(--text-3);
}
.group:hover { color: var(--text-2); }
.chev { transition: transform .15s; flex: none; }
.chev.open { transform: rotate(90deg); }
.item { padding: 7px 9px; border-radius: 7px; cursor: pointer; margin-bottom: 1px; border: 1px solid transparent; }
.item:hover { background: var(--hover); }
.item.active { background: var(--active); border-color: var(--accent-border); }
.line1 { display: flex; align-items: center; gap: 6px; height: 22px; }
.name { font-weight: 600; flex: 1; }
.dirty { color: var(--yellow); font-size: 11px; font-weight: 600; }
.tools { display: none; gap: 0; }
.item:hover .tools { display: flex; }
.item:hover .dirty { display: none; }
.line2 { display: flex; align-items: center; gap: 4px; margin-top: 3px; min-height: 18px; overflow: hidden; }
.when { font-size: 11px; white-space: nowrap; flex: none; }
.line2 :deep(.badge) { flex-shrink: 1; min-width: 0; overflow: hidden; }
.line2 :deep(.dot) { flex: none; }
.live { width: 8px; height: 8px; border-radius: 50%; background: var(--green); box-shadow: 0 0 0 3px var(--green-soft); animation: pulse 1.6s infinite; flex: none; }
@keyframes pulse { 50% { box-shadow: 0 0 0 5px transparent; } }
.hint { padding: 24px 12px; text-align: center; color: var(--text-3); }
.pages { border-top: 1px solid var(--border); padding: 6px; max-height: 30%; overflow-y: auto; flex: none; }
.page {
  display: flex; align-items: center; gap: 8px; width: 100%; padding: 6px 9px; border: 0; border-radius: 6px;
  background: none; color: var(--text-2); cursor: pointer; text-align: left;
}
.page:hover { background: var(--hover); color: var(--text); }
.page.on { background: var(--active); color: var(--text); }
.foot .btn.on { background: var(--active); color: var(--text); }
.news { position: relative; }
.ndot {
  position: absolute; top: 3px; right: 3px; width: 7px; height: 7px; border-radius: 50%;
  background: var(--accent); box-shadow: 0 0 0 2px var(--bg-2);
}
.foot { display: flex; align-items: center; gap: 6px; padding: 8px 10px; border-top: 1px solid var(--border); }
.spinner { width: 12px; height: 12px; border: 2px solid var(--border-2); border-top-color: var(--accent); border-radius: 50%; animation: spin .8s linear infinite; display: inline-block; }
@keyframes spin { to { transform: rotate(360deg); } }

/* ───── rail ───── */
.rail { width: 58px; align-items: center; }
.rhead, .rfoot, .rpages { display: flex; flex-direction: column; align-items: center; gap: 4px; padding: 10px 0 8px; width: 100%; }
.rhead { border-bottom: 1px solid var(--border); }
.rfoot { border-top: 1px solid var(--border); padding-top: 8px; }
.rpages { border-top: 1px solid var(--border); padding: 6px 0; max-height: 25%; overflow-y: auto; flex: none; }
.rbtn {
  width: 36px; height: 34px; display: flex; align-items: center; justify-content: center;
  border: 0; border-radius: 8px; background: none; color: var(--text-2); cursor: pointer;
}
.rbtn:hover { background: var(--hover); color: var(--text); }
.rbtn.on { background: var(--active); color: var(--text); }
.rail-menu { left: 42px; right: auto; top: 0; }
.rlist { flex: 1; overflow-y: auto; overflow-x: hidden; width: 100%; padding: 8px 0; display: flex; flex-direction: column; align-items: center; gap: 6px; scrollbar-width: none; }
.ritem {
  position: relative; padding: 2px; border: 2px solid transparent; border-radius: 12px; background: none; cursor: pointer;
  opacity: .82; transition: opacity .12s, border-color .12s;
}
.ritem:hover { opacity: 1; }
.ritem.active { border-color: var(--accent); opacity: 1; }
.rlive, .rdirty {
  position: absolute; right: 0; bottom: 0; width: 10px; height: 10px; border-radius: 50%;
  border: 2px solid var(--bg-2);
}
.rlive { background: var(--green); }
.rdirty { background: var(--yellow); }
.rscan { padding: 6px; }
.rcount { font-size: 10.5px; padding: 0 6px; }
</style>
