<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import Icon from "./Icon.vue";
import { CHANGELOG } from "../lib/changelog";

const props = defineProps<{ current?: string }>();
const emit = defineEmits<{ close: [] }>();

const start = CHANGELOG.findIndex((r) => r.version === props.current);
const idx = ref(start >= 0 ? start : 0);
const rel = computed(() => CHANGELOG[idx.value]);

function go(d: number) {
  idx.value = Math.min(CHANGELOG.length - 1, Math.max(0, idx.value + d));
}
function fmtDate(s: string) {
  const d = new Date(s + "T00:00:00");
  return isNaN(+d) ? s : d.toLocaleDateString("ru-RU", { day: "numeric", month: "long", year: "numeric" });
}
function onKey(e: KeyboardEvent) {
  if (e.key === "Escape") emit("close");
  else if (e.key === "ArrowLeft") go(1);
  else if (e.key === "ArrowRight") go(-1);
}
onMounted(() => window.addEventListener("keydown", onKey));
onBeforeUnmount(() => window.removeEventListener("keydown", onKey));
</script>

<template>
  <div class="backdrop" @mousedown.self="emit('close')">
    <div class="modal">
      <div class="mh">
        <Icon name="sparkles" :size="16" /> <b>Что нового</b>
        <span class="spacer" />
        <button class="btn sm icon ghost" title="Закрыть (Esc)" @click="emit('close')"><Icon name="x" :size="14" /></button>
      </div>
      <div class="body">
        <nav class="versions">
          <button
            v-for="(r, i) in CHANGELOG"
            :key="r.version"
            class="vitem"
            :class="{ on: i === idx }"
            @click="idx = i"
          >
            <span class="mono">v{{ r.version }}</span>
            <span v-if="r.version === current" class="badge green">у вас</span>
          </button>
        </nav>
        <article class="rel">
          <header>
            <h2>v{{ rel.version }} <span class="t">— {{ rel.title }}</span></h2>
            <div class="dim">{{ fmtDate(rel.date) }}</div>
          </header>
          <section v-for="g in rel.groups" :key="g.title">
            <h3>{{ g.title }}</h3>
            <ul>
              <li v-for="(it, j) in g.items" :key="j">{{ it }}</li>
            </ul>
          </section>
        </article>
      </div>
      <div class="mf">
        <button class="btn sm" :disabled="idx >= CHANGELOG.length - 1" title="←" @click="go(1)">
          <Icon name="chevron" :size="13" style="transform: rotate(180deg)" /> Предыдущая версия
        </button>
        <span class="dim small">{{ idx + 1 }} / {{ CHANGELOG.length }}</span>
        <button class="btn sm" :disabled="idx <= 0" title="→" @click="go(-1)">
          Следующая версия <Icon name="chevron" :size="13" />
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.backdrop { position: fixed; inset: 0; background: rgba(0,0,0,.45); z-index: 90; display: flex; align-items: center; justify-content: center; }
.modal { width: 760px; max-width: calc(100vw - 40px); height: 560px; max-height: 85vh; display: flex; flex-direction: column; background: var(--panel); border: 1px solid var(--border-2); border-radius: 12px; box-shadow: 0 24px 60px rgba(0,0,0,.5); }
.mh { display: flex; align-items: center; gap: 8px; padding: 12px 16px; border-bottom: 1px solid var(--border); }
.mh svg { color: var(--accent); }
.body { flex: 1; min-height: 0; display: flex; }
.versions { width: 150px; flex: none; border-right: 1px solid var(--border); padding: 8px; overflow-y: auto; display: flex; flex-direction: column; gap: 2px; }
.vitem {
  display: flex; align-items: center; justify-content: space-between; gap: 6px; padding: 7px 10px; border-radius: 7px;
  background: none; border: 0; color: var(--text-2); cursor: pointer; text-align: left; font-size: 13px;
}
.vitem:hover { background: var(--hover, var(--active)); color: var(--text); }
.vitem.on { background: var(--active); color: var(--text); font-weight: 600; }
.vitem .badge { font-size: 10px; }
.rel { flex: 1; min-width: 0; overflow-y: auto; padding: 16px 22px 20px; }
.rel h2 { margin: 0; font-size: 18px; }
.rel h2 .t { font-weight: 500; color: var(--text-2); }
.rel header { margin-bottom: 6px; }
.rel h3 { font-size: 12px; text-transform: uppercase; letter-spacing: .05em; color: var(--text-3); margin: 16px 0 6px; }
.rel ul { margin: 0; padding-left: 18px; display: flex; flex-direction: column; gap: 6px; line-height: 1.45; }
.mf { display: flex; align-items: center; justify-content: space-between; padding: 10px 16px; border-top: 1px solid var(--border); }
.small { font-size: 12px; }
</style>
