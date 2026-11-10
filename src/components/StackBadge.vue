<script setup lang="ts">
import { STACK_COLORS } from "../lib/format";
import type { Stack } from "../lib/api";
defineProps<{ stack: Stack; mini?: boolean }>();
</script>

<template>
  <span class="badge stack" :class="{ mini }">
    <span class="dot" :style="{ background: STACK_COLORS[stack.id] || '#888' }" />
    {{ stack.label }}
  </span>
</template>

<style scoped>
.stack.mini { height: 17px; padding: 0 6px; font-size: 10.5px; gap: 4px; }
.stack.mini .dot { width: 6px; height: 6px; }
</style>
