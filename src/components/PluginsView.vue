<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import Icon from "./Icon.vue";
import { state, toast } from "../lib/store";
import { api } from "../lib/api";
import {
  host, PERMISSIONS, API_VERSION, loadPlugins, reloadPlugin, enablePlugin, disablePlugin, uninstallPlugin,
  installFromFolder, createPlugin, settingValue, setSetting, type HostPlugin,
} from "../plugins/host";

const DOCS_URL = "https://github.com/thebralin/dev-hangar/blob/main/docs/plugins/README.md";

const creating = ref(false);
const newName = ref("");
const newId = ref("");
const idTouched = ref(false);
const expanded = ref<Record<string, boolean>>({});

const TRANSLIT: Record<string, string> = {
  а: "a", б: "b", в: "v", г: "g", д: "d", е: "e", ё: "e", ж: "zh", з: "z", и: "i", й: "y", к: "k", л: "l", м: "m",
  н: "n", о: "o", п: "p", р: "r", с: "s", т: "t", у: "u", ф: "f", х: "h", ц: "c", ч: "ch", ш: "sh", щ: "sch",
  ъ: "", ы: "y", ь: "", э: "e", ю: "yu", я: "ya",
};
function slug(s: string) {
  return s
    .toLowerCase()
    .split("")
    .map((c) => TRANSLIT[c] ?? c)
    .join("")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 64);
}
watch(newName, (v) => {
  if (!idTouched.value) newId.value = slug(v);
});
const idValid = computed(() => /^[a-z0-9][a-z0-9._-]{1,63}$/.test(newId.value));
const idTaken = computed(() => host.plugins.some((p) => p.folder === newId.value));

async function submitCreate() {
  if (!newName.value.trim() || !idValid.value || idTaken.value) return;
  try {
    const dir = await createPlugin(newId.value, newName.value.trim());
    toast(`Плагин создан: ${dir}`);
    creating.value = false;
    newName.value = "";
    newId.value = "";
    idTouched.value = false;
  } catch (e) {
    toast(String(e), "error");
  }
}

const sorted = computed(() =>
  [...host.plugins].sort((a, b) => (a.manifest?.name ?? a.folder).localeCompare(b.manifest?.name ?? b.folder, "ru")),
);
const activeCount = computed(() => host.plugins.filter((p) => p.status === "active").length);

function isEnabled(p: HostPlugin) {
  return !!state.pluginConfig.enabled[p.folder];
}
async function toggle(p: HostPlugin) {
  if (isEnabled(p)) await disablePlugin(p.folder);
  else await enablePlugin(p.folder);
}
async function reload(p: HostPlugin) {
  await reloadPlugin(p.folder);
  const fresh = host.plugins.find((x) => x.folder === p.folder);
  if (fresh?.status === "active") toast(`«${fresh.manifest?.name}» перезагружен`);
  else if (fresh?.error) toast(fresh.error, "error");
}
function openDir(dir: string) {
  openPath(dir).catch((e) => toast(String(e), "error"));
}
function edit(p: HostPlugin) {
  api.openInEditor(state.settings.editor, p.dir).catch((e) => toast(String(e), "error"));
}

const statusLabel: Record<string, [string, string]> = {
  active: ["работает", "green"],
  loading: ["загрузка…", "yellow"],
  error: ["ошибка", "red"],
  invalid: ["некорректный", "red"],
  disabled: ["выключен", ""],
};

function onSetting(p: HostPlugin, key: string, type: string, ev: Event) {
  const t = ev.target as HTMLInputElement;
  const v = type === "boolean" ? t.checked : type === "number" ? Number(t.value) : t.value;
  setSetting(p, key, v);
}

// ---------- bundled plugins (shipped with the app) ----------
const bundled = ref<{ id: string; manifest: Record<string, any> }[]>([]);
function newer(a: string, b: string) {
  const pa = a.split(".").map((x) => parseInt(x, 10) || 0);
  const pb = b.split(".").map((x) => parseInt(x, 10) || 0);
  for (let i = 0; i < 3; i++) if ((pa[i] ?? 0) !== (pb[i] ?? 0)) return (pa[i] ?? 0) > (pb[i] ?? 0);
  return false;
}
const offers = computed(() =>
  bundled.value
    .map((b) => {
      const inst = host.plugins.find((p) => p.folder === b.id);
      return { ...b, update: !!inst?.manifest && newer(String(b.manifest.version), inst.manifest.version), installed: !!inst };
    })
    .filter((x) => !x.installed || x.update),
);
const installing = ref("");
async function installBundled(id: string) {
  installing.value = id;
  try {
    const was = !!state.pluginConfig.enabled[id];
    await api.pluginInstallBundled(id);
    await reloadPlugin(id);
    if (was) toast(`Плагин «${host.plugins.find((p) => p.folder === id)?.manifest?.name ?? id}» обновлён`);
    else await enablePlugin(id);
  } catch (e) {
    toast("Не удалось установить: " + e, "error");
  } finally {
    installing.value = "";
  }
}

onMounted(() => {
  if (!host.plugins.length && !host.busy) loadPlugins();
  api.bundledPlugins().then((b) => (bundled.value = b)).catch(() => {});
});
</script>

<template>
  <div class="plugins">
    <header class="dh">
      <div class="title">
        <Icon name="puzzle" :size="20" />
        <h1>Плагины</h1>
        <span class="muted">Добавляйте в Hangar свои вкладки, кнопки и инструменты</span>
      </div>
      <div class="bar">
        <button class="btn primary" @click="creating = !creating"><Icon name="plus" :size="14" /> Создать плагин</button>
        <button class="btn" @click="installFromFolder"><Icon name="download" :size="14" /> Установить из папки…</button>
        <button class="btn" :disabled="!host.dir" @click="openDir(host.dir)"><Icon name="folder-open" :size="14" /> Папка плагинов</button>
        <button class="btn" :disabled="host.busy" @click="loadPlugins"><Icon name="refresh" :size="14" /> Перезагрузить все</button>
        <span class="spacer" />
        <span class="dim">API v{{ API_VERSION }} · активно {{ activeCount }} из {{ host.plugins.length }}</span>
        <button class="btn ghost" @click="openUrl(DOCS_URL)"><Icon name="note" :size="14" /> Как написать плагин</button>
      </div>

      <form v-if="creating" class="create card" @submit.prevent="submitCreate">
        <label class="field">
          <span>Название</span>
          <input v-model="newName" class="input" placeholder="Например: Мои заметки" autofocus />
        </label>
        <label class="field">
          <span>id (имя папки)</span>
          <input v-model="newId" class="input mono" placeholder="my-notes" @input="idTouched = true" />
        </label>
        <button class="btn primary" type="submit" :disabled="!newName.trim() || !idValid || idTaken">Создать</button>
        <button class="btn ghost" type="button" @click="creating = false">Отмена</button>
        <div class="hint dim">
          <template v-if="newId && !idValid">id: только a-z, 0-9, «-», «_», «.» (2–64 символа)</template>
          <template v-else-if="idTaken">Плагин с таким id уже есть</template>
          <template v-else>Будет создана папка с шаблоном (plugin.json, main.js, типы для автодополнения) и открыта в редакторе. Плагин сразу включится.</template>
        </div>
      </form>
    </header>

    <div class="list">
      <section v-if="offers.length" class="bundled">
        <div class="sec-h">Встроенные — ставятся одной кнопкой</div>
        <div class="offers">
          <div v-for="b in offers" :key="b.id" class="card offer">
            <div class="pic"><Icon :name="b.manifest.icon || 'puzzle'" :size="18" /></div>
            <div class="grow">
              <div class="row"><b>{{ b.manifest.name }}</b> <span class="dim">v{{ b.manifest.version }}</span></div>
              <div class="desc muted small">{{ b.manifest.description }}</div>
            </div>
            <button class="btn sm" :class="{ primary: !b.update }" :disabled="installing === b.id" @click="installBundled(b.id)">
              <Icon name="download" :size="12" /> {{ b.update ? "Обновить" : "Установить" }}
            </button>
          </div>
        </div>
      </section>

      <div v-if="host.busy && !host.plugins.length" class="empty">Загрузка плагинов…</div>
      <div v-else-if="!host.plugins.length" class="empty">
        <Icon name="puzzle" :size="40" />
        <div>Плагинов пока нет</div>
        <div class="muted">
          Нажмите «Создать плагин», чтобы получить готовый шаблон,<br />или установите чужой плагин из папки.
        </div>
        <div class="dim mono selectable">{{ host.dir }}</div>
      </div>

      <div v-for="p in sorted" :key="p.folder" class="card plugin" :class="{ off: !isEnabled(p) }">
        <div class="top">
          <div class="pic"><Icon :name="p.manifest?.icon || 'puzzle'" :size="20" /></div>
          <div class="grow">
            <div class="row">
              <b class="ellipsis">{{ p.manifest?.name ?? p.folder }}</b>
              <span v-if="p.manifest" class="dim">v{{ p.manifest.version }}</span>
              <span class="badge" :class="statusLabel[p.status][1]">{{ statusLabel[p.status][0] }}</span>
            </div>
            <div class="meta dim">
              <span class="mono">{{ p.folder }}</span>
              <template v-if="p.manifest?.author"> · {{ p.manifest.author }}</template>
              <template v-if="p.manifest?.homepage"> · <a @click="openUrl(p.manifest.homepage)">сайт</a></template>
            </div>
            <div v-if="p.manifest?.description" class="desc muted">{{ p.manifest.description }}</div>
          </div>
          <div class="tools">
            <button class="btn sm icon ghost" title="Перезагрузить (после правки кода)" @click="reload(p)"><Icon name="refresh" :size="13" /></button>
            <button class="btn sm icon ghost" title="Открыть в редакторе" @click="edit(p)"><Icon name="code" :size="13" /></button>
            <button class="btn sm icon ghost" title="Открыть папку" @click="openDir(p.dir)"><Icon name="folder-open" :size="13" /></button>
            <button class="btn sm icon ghost" title="Удалить" @click="uninstallPlugin(p.folder)"><Icon name="trash" :size="13" /></button>
            <button
              v-if="p.manifest"
              class="switch"
              :class="{ on: isEnabled(p) }"
              :title="isEnabled(p) ? 'Выключить' : 'Включить'"
              role="switch"
              :aria-checked="isEnabled(p)"
              @click="toggle(p)"
            ><span /></button>
          </div>
        </div>

        <div v-if="p.error" class="error selectable"><Icon name="alert" :size="13" /> {{ p.error }}</div>
        <div v-for="w in p.warnings" :key="w" class="warn"><Icon name="alert" :size="13" /> {{ w }}</div>

        <div v-if="p.manifest?.permissions?.length" class="perms">
          <span v-for="perm in p.manifest.permissions" :key="perm" class="badge" :title="PERMISSIONS[perm]">{{ perm }}</span>
        </div>

        <template v-if="p.manifest?.settings?.length">
          <button class="btn sm ghost settings-toggle" @click="expanded[p.folder] = !expanded[p.folder]">
            <Icon name="settings" :size="12" /> Настройки
            <Icon name="chevron" :size="12" class="chev" :class="{ open: expanded[p.folder] }" />
          </button>
          <div v-if="expanded[p.folder]" class="settings">
            <label v-for="s in p.manifest.settings" :key="s.key" class="field" :class="{ inline: s.type === 'boolean' }">
              <template v-if="s.type === 'boolean'">
                <input type="checkbox" :checked="!!settingValue(p, s.key)" @change="onSetting(p, s.key, s.type, $event)" />
                <span>{{ s.label }}</span>
              </template>
              <template v-else>
                <span>{{ s.label }}</span>
                <select v-if="s.type === 'select'" class="input" :value="settingValue(p, s.key)" @change="onSetting(p, s.key, s.type, $event)">
                  <option v-for="o in s.options ?? []" :key="o.value" :value="o.value">{{ o.label }}</option>
                </select>
                <textarea v-else-if="s.type === 'text'" class="input" rows="4" :placeholder="s.placeholder" :value="String(settingValue(p, s.key) ?? '')" @change="onSetting(p, s.key, s.type, $event)" />
                <input v-else class="input" :type="s.type === 'number' ? 'number' : 'text'" :placeholder="s.placeholder" :value="settingValue(p, s.key) ?? ''" @change="onSetting(p, s.key, s.type, $event)" />
              </template>
              <small v-if="s.description" class="dim">{{ s.description }}</small>
            </label>
          </div>
        </template>
      </div>

      <div v-if="host.plugins.length" class="trust dim">
        <Icon name="info" :size="13" />
        Плагины работают внутри приложения с теми же возможностями, что и сам Hangar. Разрешения ограничивают API плагина,
        но не являются песочницей — устанавливайте плагины только из источников, которым доверяете.
      </div>
    </div>
  </div>
</template>

<style scoped>
.plugins { display: flex; flex-direction: column; height: 100%; }
.dh { padding: 16px 20px 12px; border-bottom: 1px solid var(--border); background: var(--bg-2); }
.title { display: flex; align-items: center; gap: 10px; }
.title svg { color: var(--accent); }
h1 { font-size: 19px; margin: 0; }
.bar { display: flex; align-items: center; gap: 8px; margin-top: 12px; flex-wrap: wrap; }
.create { display: grid; grid-template-columns: 1fr 220px auto auto; gap: 10px; align-items: end; padding: 12px 14px; margin-top: 12px; }
.create .hint { grid-column: 1 / -1; font-size: 12px; }
.field { display: flex; flex-direction: column; gap: 5px; }
.field > span { font-size: 12px; color: var(--text-2); }
.field.inline { flex-direction: row; align-items: center; flex-wrap: wrap; }
.field.inline small { flex-basis: 100%; }
.list { flex: 1; overflow-y: auto; padding: 14px 20px 20px; display: flex; flex-direction: column; gap: 10px; }
.plugin { padding: 12px 14px; }
.plugin.off .pic { opacity: .5; }
.top { display: flex; gap: 12px; align-items: flex-start; }
.pic {
  width: 38px; height: 38px; flex: none; border-radius: 9px; display: flex; align-items: center; justify-content: center;
  background: var(--accent-soft); color: var(--accent);
}
.meta { font-size: 11.5px; margin-top: 2px; }
.desc { margin-top: 4px; }
.tools { display: flex; align-items: center; gap: 2px; }
.switch {
  width: 34px; height: 20px; border-radius: 10px; border: 1px solid var(--border-2); background: var(--panel-2);
  position: relative; cursor: pointer; margin-left: 8px; padding: 0; flex: none;
}
.switch span { position: absolute; top: 2px; left: 2px; width: 14px; height: 14px; border-radius: 50%; background: var(--text-3); transition: transform .15s, background .15s; }
.switch.on { background: var(--accent-soft); border-color: var(--accent-border); }
.switch.on span { transform: translateX(14px); background: var(--accent); }
.error, .warn { display: flex; gap: 6px; align-items: flex-start; margin-top: 8px; font-size: 12px; padding: 6px 8px; border-radius: 6px; }
.error { color: var(--red); background: var(--red-soft); white-space: pre-wrap; }
.warn { color: var(--yellow); background: var(--yellow-soft); }
.error svg, .warn svg { flex: none; margin-top: 2px; }
.perms { display: flex; flex-wrap: wrap; gap: 4px; margin-top: 8px; padding-left: 50px; }
.settings-toggle { margin: 8px 0 0 42px; }
.chev { transition: transform .15s; }
.chev.open { transform: rotate(90deg); }
.settings { display: flex; flex-direction: column; gap: 10px; padding: 10px 0 2px 50px; max-width: 560px; }
.sec-h { font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: .04em; color: var(--text-3); margin-bottom: 8px; }
.offers { display: grid; grid-template-columns: repeat(auto-fill, minmax(330px, 1fr)); gap: 10px; }
.offer { display: flex; gap: 12px; align-items: center; padding: 10px 12px; }
.offer .pic { width: 32px; height: 32px; }
.offer .desc { margin-top: 2px; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
.small { font-size: 12px; }
.bundled { margin-bottom: 8px; }
.trust { display: flex; gap: 6px; align-items: flex-start; font-size: 12px; margin-top: 6px; }
.trust svg { flex: none; margin-top: 2px; }
</style>
