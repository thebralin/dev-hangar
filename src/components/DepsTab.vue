<script setup lang="ts">
import { computed, ref } from "vue";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { Dependency, ProjectInfo } from "../lib/api";

const props = defineProps<{ info: ProjectInfo }>();
const q = ref("");
const showDev = ref(true);

const groups = computed(() => {
  const f = q.value.trim().toLowerCase();
  const map = new Map<string, Dependency[]>();
  for (const d of props.info.dependencies) {
    if (!showDev.value && d.dev) continue;
    if (f && !d.name.toLowerCase().includes(f)) continue;
    const key = `${d.scope || "."}|${d.source}`;
    if (!map.has(key)) map.set(key, []);
    map.get(key)!.push(d);
  }
  return [...map.entries()].map(([k, deps]) => {
    const [scope, source] = k.split("|");
    deps.sort((a, b) => Number(a.dev) - Number(b.dev) || a.name.localeCompare(b.name));
    return { scope, source, deps };
  });
});

const sourceLabel: Record<string, string> = { npm: "npm · package.json", cargo: "Cargo.toml", pip: "Python", go: "go.mod", composer: "composer.json" };

function urlOf(d: Dependency) {
  switch (d.source) {
    case "npm": return `https://www.npmjs.com/package/${d.name}`;
    case "cargo": return `https://crates.io/crates/${d.name}`;
    case "pip": return `https://pypi.org/project/${d.name}/`;
    case "go": return `https://pkg.go.dev/${d.name}`;
    case "composer": return `https://packagist.org/packages/${d.name}`;
  }
  return "";
}
</script>

<template>
  <div class="deps">
    <div class="top">
      <input v-model="q" class="input" placeholder="Фильтр по имени пакета…" style="max-width: 320px" />
      <label class="chk"><input v-model="showDev" type="checkbox" /> dev-зависимости</label>
      <span class="spacer" />
      <span class="dim">Всего: {{ info.dependencies.length }}</span>
    </div>
    <div v-if="!info.dependencies.length" class="empty">Зависимости не найдены</div>
    <div class="grid">
      <section v-for="g in groups" :key="g.scope + g.source" class="card">
        <div class="card-h">
          {{ g.scope === "." ? "Корень" : g.scope }}
          <span class="muted">{{ sourceLabel[g.source] || g.source }}</span>
          <span class="spacer" />
          <span class="dim">{{ g.deps.length }}</span>
        </div>
        <table>
          <tr v-for="d in g.deps" :key="d.name">
            <td class="name"><a @click="openUrl(urlOf(d))">{{ d.name }}</a></td>
            <td class="ver mono">{{ d.version }}</td>
            <td class="tag"><span v-if="d.dev" class="badge">dev</span></td>
          </tr>
        </table>
      </section>
    </div>
  </div>
</template>

<style scoped>
.deps { height: 100%; overflow-y: auto; padding: 16px 20px; }
.top { display: flex; align-items: center; gap: 14px; margin-bottom: 14px; }
.chk { display: flex; gap: 6px; align-items: center; cursor: pointer; color: var(--text-2); }
.grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(420px, 1fr)); gap: 14px; align-items: start; }
table { width: 100%; border-collapse: collapse; }
td { padding: 4px 14px; border-top: 1px solid var(--border); font-size: 12.5px; }
.name { width: 55%; word-break: break-all; }
.ver { color: var(--text-2); font-size: 12px; }
.tag { width: 50px; text-align: right; }
</style>
