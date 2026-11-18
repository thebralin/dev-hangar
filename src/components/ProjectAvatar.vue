<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { convertFileSrc } from "@tauri-apps/api/core";
import { displayName, logoOf, isLightLogo, type ProjectEntry } from "../lib/store";
import { hueOf, initials } from "../lib/format";

const props = withDefaults(defineProps<{ entry: ProjectEntry; size?: number }>(), { size: 34 });
const broken = ref<Record<string, boolean>>({});

const name = computed(() => displayName(props.entry));
const hue = computed(() => hueOf(name.value));
/** the cover, else the project's logo / favicon, else initials */
const pic = computed(() => {
  const c = props.entry.cover;
  if (c && !broken.value[c]) return { src: convertFileSrc(c), file: c, logo: false };
  const l = logoOf(props.entry.path);
  if (l && !broken.value[l]) return { src: convertFileSrc(l), file: l, logo: true };
  return null;
});
watch(() => props.entry.path, () => (broken.value = {}));
</script>

<template>
  <span
    class="av"
    :class="{ logo: pic?.logo, dark: pic?.logo && isLightLogo(pic.src) }"
    :style="{ width: size + 'px', height: size + 'px', '--h': hue, fontSize: Math.round(size * 0.36) + 'px' }"
  >
    <img v-if="pic" :src="pic.src" alt="" draggable="false" @error="broken[pic.file] = true" />
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
.av.logo { background: rgba(255,255,255,.94); padding: 14%; }
.av.logo.dark { background: rgba(14,18,24,.9); box-shadow: inset 0 0 0 1px rgba(255,255,255,.12); }
.av.logo img { object-fit: contain; object-position: center; }
</style>
