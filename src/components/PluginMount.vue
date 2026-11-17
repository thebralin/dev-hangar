<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import Icon from "./Icon.vue";
import { pluginOf } from "../plugins/host";

// Hosts plugin UI (a project tab or a page): hands the plugin a bare element and cleans up after it.
type Cleanup = void | (() => void);
const props = defineProps<{
  pluginId: string;
  mount: (el: HTMLElement, signal: AbortSignal) => Cleanup | Promise<Cleanup>;
}>();
const el = ref<HTMLElement>();
const error = ref("");
let cleanup: (() => void) | undefined;
let ctrl: AbortController | undefined;

onMounted(async () => {
  ctrl = new AbortController();
  try {
    const r = await props.mount(el.value!, ctrl.signal);
    if (typeof r === "function") {
      if (ctrl.signal.aborted) r();
      else cleanup = r;
    }
  } catch (e) {
    console.error(`[plugin ${props.pluginId}] mount failed`, e);
    error.value = e instanceof Error ? e.message : String(e);
  }
});

onBeforeUnmount(() => {
  ctrl?.abort();
  try {
    cleanup?.();
  } catch (e) {
    console.error(`[plugin ${props.pluginId}] cleanup failed`, e);
  }
});
</script>

<template>
  <div class="plugin-mount" :data-plugin="pluginId">
    <div v-if="error" class="empty">
      <Icon name="alert" :size="32" />
      <div>Плагин «{{ pluginOf(pluginId)?.manifest?.name ?? pluginId }}» не смог отрисовать интерфейс</div>
      <code class="err selectable">{{ error }}</code>
    </div>
    <div ref="el" class="plugin-root" />
  </div>
</template>

<style scoped>
.plugin-mount { height: 100%; overflow: auto; user-select: text; }
.plugin-root { min-height: 100%; }
.err { color: var(--red); max-width: 640px; white-space: pre-wrap; }
</style>
