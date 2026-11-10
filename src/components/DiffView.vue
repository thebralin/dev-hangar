<script setup lang="ts">
import { computed } from "vue";
const props = defineProps<{ diff: string }>();

const lines = computed(() => {
  const src = props.diff.length > 2_000_000 ? props.diff.slice(0, 2_000_000) + "\n… (обрезано)" : props.diff;
  return src.split("\n").map((l) => {
    let cls = "";
    if (l.startsWith("diff --git")) cls = "file";
    else if (l.startsWith("+++") || l.startsWith("---") || l.startsWith("index ") || l.startsWith("new file") || l.startsWith("deleted file") || l.startsWith("similarity") || l.startsWith("rename ")) cls = "meta";
    else if (l.startsWith("@@")) cls = "hunk";
    else if (l.startsWith("+")) cls = "add";
    else if (l.startsWith("-")) cls = "del";
    else if (l.startsWith("Binary files")) cls = "meta";
    return { l: cls === "file" ? l.replace(/^diff --git a\/(.+?) b\/.*$/, "$1") : l, cls };
  });
});
</script>

<template>
  <div v-if="diff.trim()" class="diff selectable">
    <div v-for="(x, i) in lines" :key="i" :class="x.cls">{{ x.l || " " }}</div>
  </div>
  <div v-else class="empty">Нет изменений для отображения</div>
</template>
