<script setup lang="ts">
import { computed, onMounted, ref, watchEffect } from "vue";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import Sidebar from "./components/Sidebar.vue";
import ProjectView from "./components/ProjectView.vue";
import SettingsModal from "./components/SettingsModal.vue";
import DiskView from "./components/DiskView.vue";
import Icon from "./components/Icon.vue";
import { state, loadConfig, initProcessEvents, addProject, toast, findProject, selectProject, refreshBrief } from "./lib/store";
import { api, parentDir } from "./lib/api";
import { useDark } from "./lib/theme";

const dragging = ref(false);
const showSettings = ref(false);
const dark = useDark();

watchEffect(() => {
  document.documentElement.dataset.theme = dark.value ? "dark" : "light";
});

const current = computed(() => findProject(state.selected));

onMounted(async () => {
  await initProcessEvents();
  await loadConfig();
  if (!state.selected && state.projects.length) state.selected = state.projects[0].path;
  if (state.selected) selectProject(state.selected);

  await getCurrentWebview().onDragDropEvent(async (event) => {
    const p = event.payload;
    if (p.type === "enter" || p.type === "over") dragging.value = true;
    else if (p.type === "leave") dragging.value = false;
    else if (p.type === "drop") {
      dragging.value = false;
      let last: string | undefined;
      for (const path of p.paths) {
        const dir = (await api.isDir(path)) ? path : parentDir(path);
        await addProject(dir, false);
        last = dir;
      }
      if (last) {
        state.selected = findProject(last)?.path ?? last;
        toast(p.paths.length > 1 ? `Добавлено проектов: ${p.paths.length}` : "Проект добавлен");
      }
    }
  });

  // coming back from the IDE / AI agent: refresh git status & detection of the open project
  let lastFocus = 0;
  window.addEventListener("focus", () => {
    if (Date.now() - lastFocus < 3000) return;
    lastFocus = Date.now();
    if (state.selected && state.view === "project") refreshBrief(state.selected);
  });

  window.addEventListener("keydown", (e) => {
    if (e.key === "F5" || (e.ctrlKey && e.key.toLowerCase() === "r")) {
      if (!import.meta.env.DEV) e.preventDefault();
    }
  });
});
</script>

<template>
  <div class="app">
    <Sidebar @settings="showSettings = true" />
    <main class="main">
      <DiskView v-if="state.view === 'disk'" />
      <ProjectView v-else-if="current" :key="current.path" :entry="current" />
      <div v-else class="empty welcome">
        <Icon name="folder-open" :size="48" />
        <h2>Перетащите папку проекта сюда</h2>
        <p class="muted">
          Или нажмите <b>+</b> в боковой панели. Можно добавить сразу папку со множеством проектов —
          они будут найдены автоматически.
        </p>
      </div>
    </main>

    <div v-if="dragging" class="drop-overlay">
      <div class="drop-box">
        <Icon name="upload" :size="40" />
        <div>Отпустите, чтобы добавить проект</div>
      </div>
    </div>

    <SettingsModal v-if="showSettings" @close="showSettings = false" />

    <transition name="toast">
      <div v-if="state.toast" class="toast" :class="state.toast.kind">{{ state.toast.text }}</div>
    </transition>
  </div>
</template>

<style scoped>
.app { display: flex; height: 100%; }
.main { flex: 1; min-width: 0; display: flex; flex-direction: column; background: var(--bg); }
.welcome h2 { color: var(--text); margin: 6px 0 0; font-weight: 600; }
.welcome p { max-width: 420px; }
.drop-overlay {
  position: fixed; inset: 0; z-index: 100;
  background: rgba(10, 14, 20, 0.6); backdrop-filter: blur(2px);
  display: flex; align-items: center; justify-content: center; pointer-events: none;
}
.drop-box {
  border: 2px dashed var(--accent); border-radius: 16px; padding: 48px 64px;
  color: var(--accent); background: var(--panel);
  display: flex; flex-direction: column; align-items: center; gap: 12px; font-size: 15px; font-weight: 600;
}
.toast {
  position: fixed; bottom: 18px; right: 18px; z-index: 200;
  background: var(--panel); border: 1px solid var(--border-2); border-left: 3px solid var(--accent);
  padding: 10px 16px; border-radius: 8px; box-shadow: 0 8px 24px rgba(0,0,0,.35); max-width: 460px;
}
.toast.error { border-left-color: var(--red); }
.toast-enter-active, .toast-leave-active { transition: all .2s; }
.toast-enter-from, .toast-leave-to { opacity: 0; transform: translateY(8px); }
</style>
