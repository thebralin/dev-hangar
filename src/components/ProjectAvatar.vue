<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { convertFileSrc } from "@tauri-apps/api/core";
import { displayName, type ProjectEntry } from "../lib/store";
import { hueOf, initials } from "../lib/format";

const props = withDefaults(defineProps<{ entry: ProjectEntry; size?: number }>(), { size: 34 });
const broken = ref(false);
watch(() => props.entry.cover, () => (broken.value = false));

const name = computed(() => displayName(props.entry));
const hue = computed(() => hueOf(name.value));
const src = computed(() => (props.entry.cover && !broken.value ? convertFileSrc(props.entry.cover) : ""));
</script>

<template>
  <span
    class="av"
    :style="{ width: size + 'px', height: size + 'px', '--h': hue, fontSize: Math.round(size * 0.36) + 'px' }"
  >
    <img v-if="src" :src="src" alt="" draggable="false" @error="broken = true" />
    <template v-else>{{ initials(name) }}</template>
  </span>
</template>

<style scoped>
.av {
  display: inline-flex; align-items: center; justify-content: center; flex: none; overflow: hidden;
  border-radius: 9px; font-weight: 700; letter-spacing: .02em; color: #fff;
  background: linear-gradient(135deg, hsl(var(--h) 55% 46%), hsl(calc(var(--h) + 40) 60% 36%));
}
.av img { width: 100%; height: 100%; object-fit: cover; object-position: top center; }
</style>
