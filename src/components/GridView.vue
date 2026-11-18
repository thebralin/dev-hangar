<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref } from "vue";
import { convertFileSrc } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import Icon from "./Icon.vue";
import StackBadge from "./StackBadge.vue";
import {
  state, filterProjects, selectProject, displayName, activityOf, isRunning,
  captureCover, importCover, removeCover, pickActions, toast, type ProjectEntry,
} from "../lib/store";
import { relTime, fullDate, hueOf, initials, plural } from "../lib/format";
import { passports, loadPassports, taskStats, currentMilestone, STATUS_LABELS } from "../lib/passport";

onMounted(() => loadPassports(state.projects.map((p) => p.path)));

function pass(p: ProjectEntry) {
  return passports[p.path]?.data ?? null;
}
function passLine(p: ProjectEntry) {
  const ps = pass(p);
  if (!ps) return null;
  const s = taskStats(ps);
  const m = currentMilestone(ps);
  return { ...s, pct: s.total ? Math.round((s.done / s.total) * 100) : 0, milestone: m?.title };
}

const q = ref("");
const list = computed(() => filterProjects(q.value));
const broken = reactive<Record<string, boolean>>({});

const BUSY_TEXT = { starting: "запускаю dev…", waiting: "жду сервер…", shooting: "снимаю…" } as const;

function info(p: ProjectEntry) {
  return state.briefs[p.path]?.info;
}
function version(p: ProjectEntry) {
  const i = info(p);
  return i?.root.version || i?.subprojects.find((m) => m.version)?.version || null;
}
function stacks(p: ProjectEntry) {
  return (info(p)?.stacks ?? []).filter((s) => !["ts", "node", "vite"].includes(s.id)).slice(0, 3);
}
function coverSrc(p: ProjectEntry) {
  return p.cover && !broken[p.cover] ? convertFileSrc(p.cover) : "";
}
function canShoot(p: ProjectEntry) {
  return pickActions(info(p), "dev").length > 0;
}
function dirty(p: ProjectEntry) {
  return state.briefs[p.path]?.git?.status.length ?? 0;
}

async function pickImage(p: ProjectEntry) {
  const res = await open({
    multiple: false,
    title: `Обложка для «${displayName(p)}»`,
    filters: [{ name: "Картинки", extensions: ["png", "jpg", "jpeg", "webp", "gif", "svg", "bmp", "avif"] }],
  });
  if (!res || Array.isArray(res)) return;
  await importCover(p.path, res);
}

// ---------- batch: screenshot every project without a cover ----------
const batch = reactive({ running: false, done: 0, total: 0, ok: 0, cancel: false, current: "" });
const missing = computed(() => list.value.filter((p) => !p.cover && canShoot(p) && info(p)?.exists !== false));

async function shootAll() {
  if (batch.running) {
    batch.cancel = true;
    return;
  }
  const todo = [...missing.value];
  if (!todo.length) return;
  Object.assign(batch, { running: true, done: 0, total: todo.length, ok: 0, cancel: false, current: "" });
  for (const p of todo) {
    if (batch.cancel) break;
    batch.current = displayName(p);
    if (await captureCover(p.path, { quiet: true })) batch.ok++;
    batch.done++;
  }
  const failed = batch.done - batch.ok;
  toast(
    `Обложки: готово ${batch.ok}` + (failed ? `, не вышло ${failed} — запустите их по одной, чтобы увидеть причину` : ""),
    failed && !batch.ok ? "error" : "info",
  );
  batch.running = false;
}
onBeforeUnmount(() => (batch.cancel = true));
</script>

<template>
  <div class="grid-view">
    <header class="gh">
      <div class="title">
        <Icon name="grid" :size="20" />
        <h1>Все проекты</h1>
        <span class="muted">{{ list.length }} {{ plural(list.length, "проект", "проекта", "проектов") }}</span>
      </div>
      <div class="bar">
        <div class="search">
          <Icon name="search" :size="14" class="si" />
          <input v-model="q" class="input" placeholder="Поиск проекта или стека…" />
        </div>
        <select v-model="state.settings.sortBy" class="input sel" title="Сортировка">
          <option value="activity">по активности</option>
          <option value="name">по имени</option>
          <option value="added">по дате добавления</option>
        </select>
        <span class="spacer" />
        <span v-if="batch.running" class="dim small">{{ batch.done + 1 }} / {{ batch.total }} · {{ batch.current }}</span>
        <button
          class="btn sm"
          :class="{ danger: batch.running }"
          :disabled="!batch.running && !missing.length"
          :title="batch.running ? 'Остановить после текущего' : 'Запустить dev каждого проекта без обложки и сделать скриншот. Проекты без dev-команды пропускаются.'"
          @click="shootAll"
        >
          <Icon :name="batch.running ? 'stop' : 'camera'" :size="13" />
          {{ batch.running ? "Остановить" : `Обложки для всех без картинки (${missing.length})` }}
        </button>
      </div>
    </header>

    <div class="cards">
      <article
        v-for="p in list"
        :key="p.path"
        class="card-p"
        :class="{ sel: state.selected === p.path }"
        :title="p.path"
        @click="selectProject(p.path)"
      >
        <div class="cover">
          <img v-if="coverSrc(p)" :src="coverSrc(p)" alt="" draggable="false" @error="broken[p.cover!] = true" />
          <div v-else class="ph" :style="{ '--h': hueOf(displayName(p)) }">
            <span class="ini">{{ initials(displayName(p)) }}</span>
          </div>

          <div v-if="state.coverBusy[p.path]" class="busy">
            <span class="spinner" /> {{ BUSY_TEXT[state.coverBusy[p.path]] }}
          </div>
          <div v-else class="ctools" @click.stop>
            <button
              class="cbtn"
              :disabled="!canShoot(p)"
              :title="canShoot(p) ? 'Сделать скриншот: запустить dev и снять страницу' : 'Нет dev-команды — выберите картинку'"
              @click="captureCover(p.path)"
            ><Icon name="camera" :size="14" /></button>
            <button class="cbtn" title="Выбрать свою картинку" @click="pickImage(p)"><Icon name="image" :size="14" /></button>
            <button v-if="p.cover" class="cbtn" title="Убрать обложку" @click="removeCover(p.path)"><Icon name="trash" :size="14" /></button>
          </div>

          <div class="flags">
            <span v-if="p.pinned" class="flag" title="Закреплён"><Icon name="pin" :size="11" /></span>
            <span v-if="isRunning(p.path)" class="flag live" title="Запущен процесс"><span class="dot" /> работает</span>
          </div>
        </div>

        <div class="meta">
          <div class="row1">
            <span class="name ellipsis">{{ displayName(p) }}</span>
            <span v-if="pass(p)?.status" class="badge st" :class="STATUS_LABELS[pass(p)!.status!].cls">{{ STATUS_LABELS[pass(p)!.status!].label }}</span>
            <span v-if="version(p)" class="badge ver">v{{ version(p) }}</span>
          </div>
          <div v-if="pass(p)?.summary" class="sum ellipsis" :title="pass(p)!.summary">{{ pass(p)!.summary }}</div>
          <div class="row2">
            <template v-if="info(p)?.exists === false"><span class="badge red">папка не найдена</span></template>
            <template v-else>
              <StackBadge v-for="s in stacks(p)" :key="s.id" :stack="s" mini />
            </template>
            <span class="spacer" />
            <span v-if="dirty(p)" class="dirty" :title="`Незакоммиченных изменений: ${dirty(p)}`">●{{ dirty(p) }}</span>
            <span class="when dim" :title="`Последнее обновление: ${fullDate(activityOf(p.path))}`">
              <Icon name="clock" :size="11" /> {{ relTime(activityOf(p.path)) }}
            </span>
          </div>
          <div
            v-if="passLine(p)?.total"
            class="row3"
            :title="`Задачи: ${passLine(p)!.done} из ${passLine(p)!.total} выполнено` + (passLine(p)!.milestone ? ` · этап ${passLine(p)!.milestone}` : '')"
          >
            <div class="pbar"><span :style="{ width: passLine(p)!.pct + '%' }" /></div>
            <span class="dim">{{ passLine(p)!.done }}/{{ passLine(p)!.total }}</span>
            <span v-if="passLine(p)!.milestone" class="ms ellipsis"><Icon name="flag" :size="10" /> {{ passLine(p)!.milestone }}</span>
          </div>
        </div>
      </article>

      <div v-if="!list.length" class="empty">{{ state.projects.length ? "Ничего не найдено" : "Проектов пока нет — перетащите папку в окно" }}</div>
    </div>
  </div>
</template>

<style scoped>
.grid-view { display: flex; flex-direction: column; height: 100%; }
.gh { padding: 16px 20px 12px; border-bottom: 1px solid var(--border); background: var(--bg-2); }
.title { display: flex; align-items: center; gap: 10px; }
.title svg { color: var(--accent); }
h1 { font-size: 19px; margin: 0; }
.bar { display: flex; align-items: center; gap: 8px; margin-top: 12px; flex-wrap: wrap; }
.search { position: relative; width: 280px; }
.search .input { padding-left: 30px; height: 28px; }
.search .si { position: absolute; left: 10px; top: 7px; color: var(--text-3); }
.sel { width: 170px; height: 28px; font-size: 12px; }
.small { font-size: 12px; }

.cards {
  flex: 1; overflow-y: auto; padding: 18px 20px 28px;
  display: grid; grid-template-columns: repeat(auto-fill, minmax(250px, 1fr)); gap: 16px; align-content: start;
}
.card-p {
  background: var(--panel); border: 1px solid var(--border); border-radius: 10px; overflow: hidden; cursor: pointer;
  transition: border-color .12s, transform .12s, box-shadow .12s;
}
.card-p:hover { border-color: var(--border-2); transform: translateY(-2px); box-shadow: 0 10px 26px rgba(0,0,0,.25); }
.card-p.sel { border-color: var(--accent-border); }

.cover { position: relative; aspect-ratio: 16 / 10; background: var(--panel-2); overflow: hidden; border-bottom: 1px solid var(--border); }
.cover img { width: 100%; height: 100%; object-fit: cover; object-position: top center; display: block; }
.ph {
  width: 100%; height: 100%; display: flex; align-items: center; justify-content: center;
  background:
    radial-gradient(circle at 20% 15%, hsl(var(--h) 70% 60% / .45), transparent 55%),
    linear-gradient(135deg, hsl(var(--h) 45% 30%), hsl(calc(var(--h) + 50) 50% 20%));
}
.ini { font-size: 42px; font-weight: 750; color: rgba(255,255,255,.88); letter-spacing: .03em; text-shadow: 0 2px 12px rgba(0,0,0,.25); }

.ctools { position: absolute; right: 8px; top: 8px; display: flex; gap: 4px; opacity: 0; transition: opacity .12s; }
.card-p:hover .ctools { opacity: 1; }
.cbtn {
  width: 30px; height: 28px; display: flex; align-items: center; justify-content: center; cursor: pointer;
  border: 1px solid rgba(255,255,255,.14); border-radius: 7px; background: rgba(12,16,22,.72); color: #fff; backdrop-filter: blur(4px);
}
.cbtn:hover:not(:disabled) { background: rgba(12,16,22,.92); border-color: rgba(255,255,255,.3); }
.cbtn:disabled { opacity: .4; cursor: default; }
.busy {
  position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; gap: 8px;
  background: rgba(10,14,20,.6); color: #fff; font-size: 12.5px; backdrop-filter: blur(2px);
}
.flags { position: absolute; left: 8px; top: 8px; display: flex; gap: 4px; }
.flag {
  display: flex; align-items: center; gap: 5px; height: 22px; padding: 0 7px; border-radius: 6px; font-size: 11px;
  background: rgba(12,16,22,.72); color: #fff; backdrop-filter: blur(4px);
}
.flag.live .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--green); }

.meta { padding: 10px 12px 11px; }
.row1 { display: flex; align-items: center; gap: 8px; }
.name { font-weight: 650; font-size: 14px; flex: 1; min-width: 0; }
.ver { flex: none; font-family: var(--mono); font-size: 11px; }
.row2 { display: flex; align-items: center; gap: 4px; margin-top: 7px; min-height: 18px; overflow: hidden; }
.row2 :deep(.badge) { flex-shrink: 1; min-width: 0; overflow: hidden; }
.dirty { color: var(--yellow); font-size: 11px; font-weight: 600; flex: none; margin-right: 4px; }
.when { display: flex; align-items: center; gap: 4px; font-size: 11.5px; white-space: nowrap; flex: none; }
.empty { grid-column: 1 / -1; }
.st { flex: none; font-size: 10.5px; }
.sum { font-size: 12px; color: var(--text-2); margin-top: 4px; }
.row3 { display: flex; align-items: center; gap: 8px; margin-top: 8px; font-size: 11px; min-width: 0; }
.pbar { width: 70px; flex: none; height: 4px; border-radius: 2px; background: var(--panel-2); overflow: hidden; }
.pbar span { display: block; height: 100%; background: var(--green); }
.ms { display: flex; align-items: center; gap: 3px; color: var(--text-3); min-width: 0; }
.spinner { width: 13px; height: 13px; border: 2px solid rgba(255,255,255,.3); border-top-color: #fff; border-radius: 50%; animation: spin .8s linear infinite; display: inline-block; }
@keyframes spin { to { transform: rotate(360deg); } }
</style>
