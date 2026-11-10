<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { openPath, openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import Icon from "./Icon.vue";
import StackBadge from "./StackBadge.vue";
import OverviewTab from "./OverviewTab.vue";
import FilesTab from "./FilesTab.vue";
import GitTab from "./GitTab.vue";
import DepsTab from "./DepsTab.vue";
import ConsoleTab from "./ConsoleTab.vue";
import AnalyticsTab from "./AnalyticsTab.vue";
import GraphTab from "./GraphTab.vue";
import CheckpointButton from "./CheckpointButton.vue";
import AiContextModal from "./AiContextModal.vue";
import { state, refreshBrief, runAction, stopProc, procId, displayName, toast, type ProjectEntry } from "../lib/store";
import { api, type Action } from "../lib/api";

const props = defineProps<{ entry: ProjectEntry }>();
const brief = computed(() => state.briefs[props.entry.path]);
const info = computed(() => brief.value?.info);
const git = computed(() => brief.value?.git);
const moreOpen = ref(false);
const aiOpen = ref(false);
const editingName = ref(false);
const nameDraft = ref("");

const allModules = computed(() => (info.value ? [info.value.root, ...info.value.subprojects] : []));

function pickActions(kind: string): Action[] {
  const root = info.value?.root.actions.find((a) => a.kind === kind);
  if (root) return [root];
  const subs = (info.value?.subprojects ?? [])
    .map((m) => m.actions.find((a) => a.kind === kind))
    .filter(Boolean) as Action[];
  // for dev: run all subproject dev servers; for build: only the first
  return kind === "dev" ? subs : subs.slice(0, 1);
}
const devActions = computed(() => pickActions("dev"));
const buildActions = computed(() => pickActions("build"));

function runningOf(actions: Action[]) {
  return actions.filter((a) => state.procs[procId(props.entry.path, a.id)]?.status === "running");
}
const devRunning = computed(() => runningOf(devActions.value).length > 0);
const buildRunning = computed(() => runningOf(buildActions.value).length > 0);

function moduleLabel(a: Action) {
  return a.cwd ? a.cwd : "";
}

async function toggle(actions: Action[]) {
  const running = runningOf(actions);
  if (running.length) {
    for (const a of running) await stopProc(procId(props.entry.path, a.id));
    return;
  }
  for (const a of actions) await runAction(props.entry.path, a, moduleLabel(a));
}

const openActions = computed(() =>
  allModules.value.flatMap((m) => m.actions.map((a) => ({ a, module: m.relPath || "" }))),
);

const webRemote = computed(() => git.value?.remotes.find((r) => r.webUrl)?.webUrl);

async function openEditor() {
  try {
    await api.openInEditor(state.settings.editor, props.entry.path);
  } catch (e) {
    toast(String(e), "error");
  }
}
async function openTerminal() {
  try {
    await api.openTerminal(props.entry.path, state.settings.terminal);
  } catch (e) {
    toast(String(e), "error");
  }
}
async function openExplorer() {
  try {
    await openPath(props.entry.path);
  } catch {
    await revealItemInDir(props.entry.path);
  }
}
function copyPath() {
  navigator.clipboard.writeText(props.entry.path);
  toast("Путь скопирован");
}
function startRename() {
  nameDraft.value = displayName(props.entry);
  editingName.value = true;
}
function saveRename() {
  const v = nameDraft.value.trim();
  props.entry.alias = v && v !== info.value?.name ? v : undefined;
  editingName.value = false;
}

const consoleCount = computed(
  () => Object.values(state.procs).filter((p) => p.projectPath === props.entry.path && p.status === "running").length,
);

const tabs = computed(() => [
  { id: "overview", label: "Обзор", icon: "info" },
  { id: "files", label: "Файлы", icon: "folder" },
  { id: "graph", label: "Граф", icon: "graph" },
  { id: "git", label: "Git", icon: "git", count: git.value?.isRepo ? git.value.status.length || undefined : undefined },
  { id: "analytics", label: "Аналитика", icon: "chart" },
  { id: "deps", label: "Зависимости", icon: "package", count: info.value?.dependencies.length || undefined },
  { id: "console", label: "Консоль", icon: "terminal", count: consoleCount.value || undefined, live: consoleCount.value > 0 },
]);

// files tab can be asked to open a specific file
const fileRequest = ref<{ path: string; line?: number } | null>(null);
function openFile(path: string, line?: number) {
  fileRequest.value = { path, line };
  state.tab = "files";
}
const gitFileRequest = ref<string | null>(null);
function showFileHistory(rel: string) {
  gitFileRequest.value = rel;
  state.tab = "git";
}

watch(
  () => props.entry.path,
  () => {
    if (!brief.value) refreshBrief(props.entry.path);
  },
  { immediate: true },
);
</script>

<template>
  <div class="pv">
    <header class="ph">
      <div class="title-row">
        <div class="title grow">
          <input
            v-if="editingName"
            v-model="nameDraft"
            class="input name-input"
            autofocus
            @keydown.enter="saveRename"
            @keydown.esc="editingName = false"
            @blur="saveRename"
          />
          <h1 v-else class="ellipsis" title="Двойной клик — переименовать" @dblclick="startRename">{{ displayName(entry) }}</h1>
          <span v-if="info?.root.version" class="badge">v{{ info.root.version }}</span>
          <span v-if="git?.branch" class="badge blue" title="Текущая ветка"><Icon name="branch" :size="11" /> {{ git.branch }}</span>
          <span v-if="git && (git.ahead || git.behind)" class="badge yellow" :title="`Относительно ${git.upstream}`">↑{{ git.ahead }} ↓{{ git.behind }}</span>
          <span v-if="brief?.loading" class="dim">обновление…</span>
        </div>
        <div class="actions">
          <button
            v-if="devActions.length"
            class="btn"
            :class="devRunning ? 'danger' : 'success'"
            :title="devActions.map((a) => a.command).join('  &  ')"
            @click="toggle(devActions)"
          >
            <Icon :name="devRunning ? 'stop' : 'play'" :size="14" />
            {{ devRunning ? "Остановить" : devActions.length > 1 ? `Dev (${devActions.length})` : "Dev" }}
          </button>
          <button
            v-if="buildActions.length"
            class="btn"
            :class="{ danger: buildRunning }"
            :title="buildActions.map((a) => a.command).join('  &  ')"
            @click="toggle(buildActions)"
          >
            <Icon :name="buildRunning ? 'stop' : 'hammer'" :size="14" />
            {{ buildRunning ? "Остановить" : "Build" }}
          </button>
          <div class="more-wrap">
            <button class="btn icon" title="Все команды" @click="moreOpen = !moreOpen"><Icon name="down" :size="14" /></button>
            <div v-if="moreOpen" class="menu" @mouseleave="moreOpen = false">
              <div v-if="!openActions.length && !entry.customCommands?.length" class="menu-empty">Команды не найдены</div>
              <button v-for="{ a, module } in openActions" :key="a.id" @click="moreOpen = false; runAction(entry.path, a, module)">
                <span class="k">{{ a.kind }}</span>
                <span class="grow ellipsis">{{ module ? module + " · " : "" }}{{ a.label }}</span>
                <code class="dim ellipsis">{{ a.command }}</code>
              </button>
              <button v-for="c in entry.customCommands ?? []" :key="c.id" @click="moreOpen = false; runAction(entry.path, c)">
                <span class="k">свои</span>
                <span class="grow ellipsis">{{ c.label }}</span>
                <code class="dim ellipsis">{{ c.command }}</code>
              </button>
            </div>
          </div>
          <span class="sep" />
          <CheckpointButton :entry="entry" :git="git" @goto="(t: string) => (state.tab = t)" />
          <button class="btn" title="Собрать контекст проекта для ИИ / создать CLAUDE.md" :disabled="!info" @click="aiOpen = true"><Icon name="sparkles" :size="14" /> ИИ</button>
          <span class="sep" />
          <button class="btn" :title="`Открыть в ${state.settings.editor}`" @click="openEditor"><Icon name="code" :size="14" /> IDE</button>
          <button class="btn icon" title="Открыть в проводнике" @click="openExplorer"><Icon name="folder-open" :size="14" /></button>
          <button class="btn icon" title="Открыть терминал" @click="openTerminal"><Icon name="terminal" :size="14" /></button>
          <button v-if="webRemote" class="btn icon" :title="webRemote" @click="openUrl(webRemote!)"><Icon name="globe" :size="14" /></button>
          <button class="btn icon ghost" title="Обновить" @click="refreshBrief(entry.path)"><Icon name="refresh" :size="14" /></button>
        </div>
      </div>
      <div class="sub-row">
        <span class="path mono ellipsis" title="Скопировать путь" @click="copyPath">{{ entry.path }}</span>
        <StackBadge v-for="s in info?.stacks ?? []" :key="s.id" :stack="s" />
      </div>
      <nav class="tabs">
        <button v-for="t in tabs" :key="t.id" class="tab" :class="{ on: state.tab === t.id }" @click="state.tab = t.id">
          <Icon :name="t.icon" :size="14" />
          {{ t.label }}
          <span v-if="t.count" class="cnt" :class="{ live: t.live }">{{ t.count }}</span>
        </button>
      </nav>
    </header>

    <div class="body">
      <div v-if="info && !info.exists" class="empty">
        <Icon name="folder" :size="40" />
        <div>Папка не найдена: <span class="mono">{{ entry.path }}</span></div>
      </div>
      <template v-else-if="info">
        <OverviewTab v-if="state.tab === 'overview'" :entry="entry" :info="info" :git="git" @open-file="openFile" @goto="(t: string) => (state.tab = t)" />
        <FilesTab v-show="state.tab === 'files'" :entry="entry" :request="fileRequest" @history="showFileHistory" />
        <GraphTab v-if="state.tab === 'graph'" :entry="entry" @open-file="openFile" />
        <GitTab v-if="state.tab === 'git'" :entry="entry" :git="git" :file-request="gitFileRequest" @open-file="openFile" @clear-file="gitFileRequest = null" />
        <AnalyticsTab v-if="state.tab === 'analytics'" :entry="entry" :git="git" @open-file="openFile" @history="showFileHistory" />
        <DepsTab v-if="state.tab === 'deps'" :info="info" />
        <ConsoleTab v-if="state.tab === 'console'" :entry="entry" />
      </template>
      <div v-else class="empty">Анализ проекта…</div>
    </div>
    <AiContextModal v-if="aiOpen && info" :entry="entry" :info="info" :git="git" @close="aiOpen = false" @open-file="openFile" />
  </div>
</template>

<style scoped>
.pv { display: flex; flex-direction: column; height: 100%; min-height: 0; }
.ph { padding: 14px 20px 0; border-bottom: 1px solid var(--border); background: var(--bg-2); }
.title-row { display: flex; align-items: center; gap: 16px; }
.title { display: flex; align-items: center; gap: 8px; }
h1 { font-size: 19px; margin: 0; font-weight: 650; cursor: text; }
.name-input { max-width: 320px; height: 32px; font-size: 16px; font-weight: 600; }
.actions { display: flex; align-items: center; gap: 6px; }
.sep { width: 1px; height: 22px; background: var(--border-2); margin: 0 4px; }
.more-wrap { position: relative; }
.menu {
  position: absolute; right: 0; top: 34px; z-index: 60; width: 520px; max-height: 420px; overflow: auto;
  background: var(--panel); border: 1px solid var(--border-2); border-radius: 8px; padding: 4px;
  box-shadow: 0 12px 34px rgba(0,0,0,.4);
}
.menu button {
  display: flex; align-items: center; gap: 10px; width: 100%; text-align: left;
  background: none; border: 0; padding: 7px 10px; border-radius: 5px; cursor: pointer;
}
.menu button:hover { background: var(--hover); }
.menu code { max-width: 220px; font-size: 11.5px; }
.menu .k { font-size: 10px; text-transform: uppercase; color: var(--text-3); width: 44px; flex: none; }
.menu-empty { padding: 10px; color: var(--text-3); }
.sub-row { display: flex; align-items: center; gap: 6px; margin-top: 6px; flex-wrap: wrap; }
.path { color: var(--text-2); font-size: 12px; cursor: pointer; max-width: 520px; margin-right: 6px; }
.path:hover { color: var(--accent); }
.tabs { display: flex; gap: 2px; margin-top: 10px; }
.tab {
  display: flex; align-items: center; gap: 7px; padding: 8px 14px; background: none; border: 0;
  border-bottom: 2px solid transparent; color: var(--text-2); cursor: pointer; font-weight: 500;
}
.tab:hover { color: var(--text); }
.tab.on { color: var(--text); border-bottom-color: var(--accent); }
.cnt { font-size: 10.5px; background: var(--panel-2); border: 1px solid var(--border); padding: 0 6px; border-radius: 9px; color: var(--text-2); }
.cnt.live { background: var(--green-soft); color: var(--green); border-color: transparent; }
.body { flex: 1; min-height: 0; position: relative; }
</style>
