<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from "vue";

const props = withDefaults(defineProps<{ align?: "left" | "right"; width?: number }>(), { align: "left", width: 260 });
const open = ref(false);
const root = ref<HTMLElement | null>(null);

function onDoc(e: MouseEvent) {
  if (root.value && !root.value.contains(e.target as Node)) open.value = false;
}
function onKey(e: KeyboardEvent) {
  if (e.key === "Escape") open.value = false;
}
watch(open, (v) => {
  if (v) {
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onKey);
  } else {
    document.removeEventListener("mousedown", onDoc);
    document.removeEventListener("keydown", onKey);
  }
});
onBeforeUnmount(() => (open.value = false));

const close = () => (open.value = false);
defineExpose({ close });
</script>

<template>
  <div ref="root" class="dd">
    <slot name="trigger" :open="open" :toggle="() => (open = !open)" />
    <div v-if="open" class="dd-panel" :class="props.align" :style="{ width: props.width + 'px' }">
      <slot :close="close" />
    </div>
  </div>
</template>

<style scoped>
.dd { position: relative; display: inline-flex; }
.dd-panel {
  position: absolute; top: calc(100% + 4px); z-index: 50; max-height: 70vh; overflow-y: auto;
  background: var(--panel); border: 1px solid var(--border); border-radius: 8px; padding: 4px;
  box-shadow: 0 8px 28px rgba(0, 0, 0, 0.35);
}
.dd-panel.left { left: 0; }
.dd-panel.right { right: 0; }
.dd-panel :deep(.mi) {
  display: flex; align-items: center; gap: 8px; width: 100%; padding: 6px 8px; border: 0; background: none;
  border-radius: 5px; color: var(--text); font-size: 12.5px; text-align: left; cursor: pointer;
}
.dd-panel :deep(.mi:hover:not(:disabled)) { background: var(--hover); }
.dd-panel :deep(.mi:disabled) { opacity: 0.45; cursor: default; }
.dd-panel :deep(.mi.danger) { color: var(--red); }
.dd-panel :deep(.mi .hint) { margin-left: auto; color: var(--text-3); font-size: 11px; }
.dd-panel :deep(.msep) { height: 1px; background: var(--border); margin: 4px 2px; }
.dd-panel :deep(.mh) { padding: 6px 8px 3px; font-size: 10.5px; font-weight: 600; text-transform: uppercase; letter-spacing: .04em; color: var(--text-3); }
</style>
