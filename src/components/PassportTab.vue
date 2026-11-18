<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { marked } from "marked";
import DOMPurify from "dompurify";
import { openUrl } from "@tauri-apps/plugin-opener";
import Icon from "./Icon.vue";
import { state, toast, type ProjectEntry } from "../lib/store";
import { api, parentDir } from "../lib/api";
import { plural } from "../lib/format";
import {
  passports, passportHost, loadPassport, passportOps, initPassport, agentFilesOf, linkAgentFile,
  taskPrompt, fillPrompt, taskStats, currentMilestone, isDoneSection, STATUS_LABELS, formatsInOrder,
} from "../lib/passport";
import { projectContextOf } from "../plugins/host";
import type { PassportTask, ProjectStatus } from "../plugins/hangar";

const props = defineProps<{ entry: ProjectEntry }>();
const emit = defineEmits<{ "open-file": [path: string, line?: number] }>();

const path = computed(() => props.entry.path);
const ps = computed(() => passports[path.value]);
const p = computed(() => ps.value?.data ?? null);
const stats = computed(() => taskStats(p.value));
const milestone = computed(() => currentMilestone(p.value));

type View = "overview" | "tasks" | "roadmap" | "docs";
const view = ref<View>("overview");

watch(path, () => {
  loadPassport(path.value);
  loadAgentFiles();
  docSel.value = null;
}, { immediate: true });

// ---------- markdown ----------
function md(text: string) {
  return DOMPurify.sanitize(marked.parse(text || "", { async: false, gfm: true }) as string);
}
/** links in rendered markdown: open http(s) outside, never navigate the app */
function onMdClick(e: MouseEvent) {
  const a = (e.target as HTMLElement).closest("a");
  if (!a) return;
  e.preventDefault();
  const href = a.getAttribute("href") || "";
  if (/^https?:/i.test(href)) openUrl(href);
}

// ---------- create ----------
const agentFiles = ref<{ name: string; exists: boolean; linked: boolean }[]>([]);
const linkSel = reactive<Record<string, boolean>>({});
const initFormat = ref("hangar");
const creating = ref(false);
const initFormats = computed(() => formatsInOrder().filter((f) => f.format.init));

async function loadAgentFiles() {
  agentFiles.value = await agentFilesOf(path.value).catch(() => []);
  for (const f of agentFiles.value) linkSel[f.name] = f.exists && !f.linked;
}

async function create() {
  creating.value = true;
  try {
    const names = Object.keys(linkSel).filter((k) => linkSel[k] && !agentFiles.value.find((f) => f.name === k)?.linked);
    await initPassport(path.value, initFormat.value, initFormat.value === "hangar" ? names : []);
    toast("Паспорт создан. Попросите ИИ заполнить его — промпт в буфере обмена");
    navigator.clipboard.writeText(await fillPrompt(path.value));
    await loadAgentFiles();
  } catch (e) {
    toast("Не удалось создать паспорт: " + (e instanceof Error ? e.message : e), "error");
  } finally {
    creating.value = false;
  }
}

async function link(name: string) {
  try {
    await linkAgentFile(path.value, name);
    toast(`Ссылка на паспорт добавлена в ${name}`);
    await loadAgentFiles();
  } catch (e) {
    toast(String(e), "error");
  }
}

// ---------- header ----------
const STATUSES = Object.keys(STATUS_LABELS) as ProjectStatus[];

async function copyFillPrompt() {
  await navigator.clipboard.writeText(await fillPrompt(path.value));
  toast("Промпт скопирован — вставьте его в Claude Code, Cursor или другой ИИ");
}
function openInEditor(file?: string) {
  if (!file) return;
  api.openInEditor(state.settings.editor, file).catch((e) => toast(String(e), "error"));
}
const folder = computed(() => {
  const f = p.value?.files;
  const any = f?.project ?? f?.tasks ?? f?.roadmap;
  return any ? parentDir(any) : undefined;
});

// ---------- tasks ----------
const hideDone = ref(false);
const drafts = reactive<Record<string, string>>({});
const dragging = ref<PassportTask | null>(null);
const dropOn = ref<string | null>(null);
const openNotes = reactive<Record<string, boolean>>({});

const editable = computed(() => !!p.value?.editable.tasks);
const columns = computed(() => p.value?.sections ?? []);

function visibleTasks(tasks: PassportTask[], title: string) {
  return hideDone.value && !isDoneSection(title) ? tasks.filter((t) => !t.done) : tasks;
}

function toggle(t: PassportTask) {
  if (!editable.value) return;
  if (t.done) passportOps.setDone(path.value, t, false);
  else passportOps.complete(path.value, t);
}

async function add(section: string) {
  const text = (drafts[section] ?? "").trim();
  if (!text) return;
  drafts[section] = "";
  await passportOps.add(path.value, section, text);
}

function onDragStart(t: PassportTask, e: DragEvent) {
  if (!editable.value) return;
  dragging.value = t;
  e.dataTransfer?.setData("text/plain", t.text);
  if (e.dataTransfer) e.dataTransfer.effectAllowed = "move";
}
function onDrop(section: string) {
  const t = dragging.value;
  dragging.value = null;
  dropOn.value = null;
  if (t && t.section !== section) passportOps.move(path.value, t, section);
}

async function copyTask(t: PassportTask) {
  await navigator.clipboard.writeText(await taskPrompt(path.value, t));
  toast("Промпт задачи скопирован");
}

const taskActions = computed(() => passportHost.taskActions);
function actionsFor(t: PassportTask) {
  const ctx = projectContextOf(props.entry);
  return taskActions.value.filter((a) => {
    try {
      return !a.action.when || a.action.when(t, ctx);
    } catch {
      return false;
    }
  });
}
async function runTaskAction(a: (typeof passportHost.taskActions)[number], t: PassportTask) {
  try {
    await a.action.run(projectContextOf(props.entry), t, JSON.parse(JSON.stringify(p.value)));
  } catch (e) {
    toast(`Плагин «${a.pluginName}»: ${e instanceof Error ? e.message : e}`, "error");
  }
}

// ---------- docs ----------
const docSel = ref<string | null>(null);
const docHtml = ref("");
watch(docSel, async (f) => {
  docHtml.value = "";
  if (!f) return;
  try {
    docHtml.value = md((await api.readText(f)).content);
  } catch (e) {
    docHtml.value = `<p class="err">${String(e)}</p>`;
  }
});
watch(
  () => [view.value, p.value?.docs.length],
  () => {
    if (view.value === "docs" && !docSel.value && p.value?.docs.length) docSel.value = p.value.docs[0].path;
  },
);

const MS = { planned: ["план", ""], active: ["в работе", "green"], done: ["готово", "blue"] } as const;
function pct(done: number, total: number) {
  return total ? Math.round((done / total) * 100) : 0;
}
</script>

<template>
  <div class="pp">
    <!-- loading -->
    <div v-if="!ps || (ps.loading && !p)" class="empty">Читаю паспорт…</div>

    <!-- no passport yet -->
    <div v-else-if="!p" class="create-wrap">
      <div class="create card">
        <div class="hero">
          <div class="pic"><Icon name="passport" :size="26" /></div>
          <div>
            <h2>Паспорт проекта</h2>
            <p class="muted">
              Описание, статус, задачи, роадмап и документация — в файлах внутри репозитория. Их одинаково читают
              вы, Dev Hangar и ИИ-агенты (Claude Code, Cursor, Codex…), а агенты сами отмечают задачи и пишут, на чём остановились.
            </p>
          </div>
        </div>

        <div class="tree mono">
          <div><Icon name="folder" :size="13" /> .hangar/</div>
          <div class="in">project.md <span class="dim">— описание, статус, «Сейчас»</span></div>
          <div class="in">tasks.md <span class="dim">— задачи по разделам</span></div>
          <div class="in">roadmap.md <span class="dim">— версии и цели</span></div>
          <div class="in">docs/ <span class="dim">— документация</span></div>
          <div class="in">README.md <span class="dim">— описание формата для ИИ</span></div>
        </div>

        <label v-if="initFormats.length > 1" class="field">
          <span>Формат</span>
          <select v-model="initFormat" class="input">
            <option v-for="f in initFormats" :key="f.key" :value="f.key">{{ f.format.label }}</option>
          </select>
        </label>

        <div v-if="initFormat === 'hangar'" class="agents">
          <div class="lbl">Подсказать ИИ-агентам, где лежит паспорт:</div>
          <label v-for="f in agentFiles" :key="f.name" class="chk" :class="{ dim: !f.exists }">
            <input v-model="linkSel[f.name]" type="checkbox" :disabled="f.linked" />
            <span class="mono">{{ f.name }}</span>
            <span class="dim">{{ f.linked ? "уже ссылается" : f.exists ? "добавить блок в конец" : "создать файл" }}</span>
          </label>
        </div>

        <div class="row">
          <button class="btn primary" :disabled="creating" @click="create"><Icon name="plus" :size="14" /> {{ creating ? "Создаю…" : "Создать паспорт" }}</button>
          <span class="dim small">Существующие файлы не перезаписываются. После создания промпт «заполни паспорт по коду» окажется в буфере обмена.</span>
        </div>
      </div>
    </div>

    <!-- passport -->
    <template v-else>
      <header class="ph">
        <div class="line">
          <select
            v-if="p.editable.status"
            class="status-sel"
            :class="STATUS_LABELS[p.status ?? 'active'].cls"
            :value="p.status ?? ''"
            title="Статус проекта"
            @change="passportOps.setStatus(path, ($event.target as HTMLSelectElement).value as ProjectStatus)"
          >
            <option v-if="!p.status" value="" disabled>статус…</option>
            <option v-for="s in STATUSES" :key="s" :value="s">{{ STATUS_LABELS[s].label }}</option>
          </select>
          <span v-else-if="p.status" class="badge" :class="STATUS_LABELS[p.status].cls">{{ STATUS_LABELS[p.status].label }}</span>
          <span class="summary ellipsis" :title="p.summary">{{ p.summary || "Без описания — заполните summary в project.md" }}</span>
          <span class="spacer" />
          <span v-if="ps?.error" class="badge red" :title="ps.error">ошибка чтения</span>
          <span class="dim small" :title="`Формат: ${p.formatLabel}`">{{ p.formatLabel }}</span>
          <button class="btn sm" title="Промпт для ИИ: заполнить и дополнить паспорт по коду" @click="copyFillPrompt"><Icon name="sparkles" :size="12" /> Промпт «заполни»</button>
          <button class="btn sm icon ghost" title="Открыть папку паспорта в редакторе" :disabled="!folder" @click="openInEditor(folder)"><Icon name="code" :size="13" /></button>
          <button class="btn sm icon ghost" title="Перечитать" @click="loadPassport(path)"><Icon name="refresh" :size="13" /></button>
        </div>
        <div v-if="p.tags.length" class="tags">
          <span v-for="t in p.tags" :key="t" class="badge">{{ t }}</span>
        </div>
        <nav class="seg">
          <button :class="{ on: view === 'overview' }" @click="view = 'overview'"><Icon name="info" :size="13" /> Обзор</button>
          <button :class="{ on: view === 'tasks' }" @click="view = 'tasks'">
            <Icon name="check" :size="13" /> Задачи <span class="cnt">{{ stats.open }}</span>
          </button>
          <button :class="{ on: view === 'roadmap' }" @click="view = 'roadmap'">
            <Icon name="flag" :size="13" /> Роадмап <span class="cnt">{{ p.roadmap.length }}</span>
          </button>
          <button :class="{ on: view === 'docs' }" @click="view = 'docs'">
            <Icon name="note" :size="13" /> Документы <span class="cnt">{{ p.docs.length }}</span>
          </button>
        </nav>
      </header>

      <!-- overview -->
      <div v-if="view === 'overview'" class="scroll">
        <div class="ov">
          <div class="main-col">
            <section class="card now">
              <div class="card-h"><Icon name="clock" :size="14" /> Сейчас</div>
              <div v-if="p.now" class="card-b md" @click="onMdClick" v-html="md(p.now)" />
              <div v-else class="card-b dim">Пусто. Раздел «Сейчас» в project.md — ИИ обновляет его в конце сессии.</div>
            </section>
            <section class="card">
              <div class="card-h">
                <Icon name="file" :size="14" /> Описание
                <span class="spacer" />
                <button v-if="p.files.project" class="btn sm ghost" @click="openInEditor(p.files.project)">Править</button>
              </div>
              <div v-if="p.description" class="card-b md" @click="onMdClick" v-html="md(p.description)" />
              <div v-else class="card-b dim">Описания пока нет.</div>
            </section>
          </div>
          <aside class="side-col">
            <section class="card">
              <div class="card-h"><Icon name="check" :size="14" /> Задачи</div>
              <div class="card-b">
                <div class="big">{{ stats.done }} <span class="dim">/ {{ stats.total }}</span></div>
                <div class="bar"><span :style="{ width: pct(stats.done, stats.total) + '%' }" /></div>
                <div class="dim small">
                  открыто {{ stats.open }}<template v-if="stats.doing"> · в работе {{ stats.doing }}</template>
                </div>
                <button class="btn sm link" @click="view = 'tasks'">Открыть доску →</button>
              </div>
            </section>
            <section v-if="milestone" class="card">
              <div class="card-h"><Icon name="flag" :size="14" /> {{ milestone.status === "active" ? "Текущий этап" : "Следующий этап" }}</div>
              <div class="card-b">
                <b>{{ milestone.title }}</b>
                <template v-if="milestone.items.length">
                  <div class="bar"><span :style="{ width: pct(milestone.items.filter((i) => i.done).length, milestone.items.length) + '%' }" /></div>
                  <div class="dim small">{{ milestone.items.filter((i) => i.done).length }} из {{ milestone.items.length }} целей</div>
                </template>
                <button class="btn sm link" @click="view = 'roadmap'">Весь роадмап →</button>
              </div>
            </section>
            <section v-if="columns.some((c) => !isDoneSection(c.title) && c.tasks.some((t) => !t.done))" class="card">
              <div class="card-h"><Icon name="zap" :size="14" /> Ближайшее</div>
              <div class="card-b quick">
                <template v-for="c in columns.filter((c) => !isDoneSection(c.title)).slice(0, 2)" :key="c.title">
                  <div v-for="t in c.tasks.filter((t) => !t.done).slice(0, 4)" :key="t.id" class="qt">
                    <span class="sec">{{ c.title }}</span>
                    <span class="ellipsis" :title="t.text">{{ t.text }}</span>
                  </div>
                </template>
              </div>
            </section>
          </aside>
        </div>
      </div>

      <!-- tasks -->
      <div v-else-if="view === 'tasks'" class="tasks">
        <div class="tbar">
          <label class="chk"><input v-model="hideDone" type="checkbox" /> скрыть выполненные вне «Готово»</label>
          <span class="spacer" />
          <span v-if="!editable" class="dim small">Формат «{{ p.formatLabel }}» только для чтения</span>
          <span v-else class="dim small">Перетаскивайте задачи между колонками</span>
          <button class="btn sm ghost" :disabled="!p.files.tasks" @click="openInEditor(p.files.tasks)"><Icon name="code" :size="12" /> tasks.md</button>
        </div>
        <div v-if="!columns.length" class="empty">
          Задач пока нет.
          <span class="dim">Добавьте раздел «## Очередь» и пункты «- [ ] …» в tasks.md — или попросите ИИ.</span>
          <button v-if="editable" class="btn sm" @click="passportOps.add(path, 'Очередь', 'Первая задача')">Создать первую задачу</button>
        </div>
        <div v-else class="board">
          <section
            v-for="c in columns"
            :key="c.title"
            class="col"
            :class="{ drop: dropOn === c.title && dragging?.section !== c.title, done: isDoneSection(c.title) }"
            @dragover.prevent="dropOn = c.title"
            @dragleave="dropOn === c.title && (dropOn = null)"
            @drop.prevent="onDrop(c.title)"
          >
            <div class="col-h">
              <b class="ellipsis">{{ c.title }}</b>
              <span class="cnt">{{ c.tasks.length }}</span>
            </div>
            <div class="col-b">
              <div
                v-for="t in visibleTasks(c.tasks, c.title)"
                :key="t.id"
                class="task"
                :class="{ done: t.done, high: t.priority === 'high', low: t.priority === 'low', dragging: dragging?.id === t.id }"
                :draggable="editable"
                @dragstart="onDragStart(t, $event)"
                @dragend="dragging = null; dropOn = null"
              >
                <button class="cb" :class="{ on: t.done }" :disabled="!editable" :title="t.done ? 'Снять отметку' : 'Выполнено'" @click="toggle(t)">
                  <Icon v-if="t.done" name="check" :size="11" />
                </button>
                <div class="tb">
                  <div class="tt">{{ t.text }}</div>
                  <div v-if="t.tags.length || t.priority || t.note" class="tm">
                    <span v-if="t.priority === 'high'" class="pri high">важно</span>
                    <span v-else-if="t.priority === 'low'" class="pri">потом</span>
                    <span v-for="tag in t.tags" :key="tag" class="tag">#{{ tag }}</span>
                    <button v-if="t.note" class="note-btn" @click="openNotes[t.id] = !openNotes[t.id]">
                      <Icon name="note" :size="11" /> {{ openNotes[t.id] ? "скрыть" : "подробнее" }}
                    </button>
                  </div>
                  <div v-if="t.note && openNotes[t.id]" class="note">{{ t.note }}</div>
                </div>
                <div class="ta">
                  <button
                    v-for="a in actionsFor(t)"
                    :key="a.key"
                    class="tact"
                    :title="a.action.title || `${a.action.label} (плагин «${a.pluginName}»)`"
                    @click="runTaskAction(a, t)"
                  ><Icon :name="a.action.icon || 'puzzle'" :size="12" /> {{ a.action.label }}</button>
                  <button class="tact icon" title="Скопировать промпт для ИИ: «возьми эту задачу»" @click="copyTask(t)"><Icon name="copy" :size="12" /></button>
                  <button v-if="t.line != null && p.files.tasks" class="tact icon" title="Открыть в tasks.md" @click="emit('open-file', p.files.tasks!, t.line + 1)"><Icon name="file" :size="12" /></button>
                </div>
              </div>
              <div v-if="!c.tasks.length" class="col-empty dim">пусто</div>
            </div>
            <form v-if="editable" class="add" @submit.prevent="add(c.title)">
              <input v-model="drafts[c.title]" class="input" placeholder="+ задача  (#тег !high)" />
            </form>
          </section>
        </div>
      </div>

      <!-- roadmap -->
      <div v-else-if="view === 'roadmap'" class="scroll">
        <div class="rm">
          <div class="rm-bar">
            <span class="dim small">Статус этапа — в конце заголовка: `план`, `в работе`, `готово`</span>
            <span class="spacer" />
            <button class="btn sm ghost" :disabled="!p.files.roadmap" @click="openInEditor(p.files.roadmap)"><Icon name="code" :size="12" /> roadmap.md</button>
          </div>
          <div v-if="!p.roadmap.length" class="empty">Роадмап пуст. Этапы — заголовки «## 1.0.0 `план`» в roadmap.md.</div>
          <div v-for="(m, i) in p.roadmap" :key="i" class="ms" :class="m.status">
            <div class="rail-dot"><span /></div>
            <div class="card ms-card">
              <div class="ms-h">
                <b>{{ m.title }}</b>
                <span class="badge" :class="MS[m.status][1]">{{ MS[m.status][0] }}</span>
                <span class="spacer" />
                <span v-if="m.items.length" class="dim small">{{ m.items.filter((x) => x.done).length }} / {{ m.items.length }}</span>
              </div>
              <div v-if="m.items.length" class="bar"><span :style="{ width: pct(m.items.filter((x) => x.done).length, m.items.length) + '%' }" /></div>
              <div v-if="m.body" class="md body" @click="onMdClick" v-html="md(m.body)" />
              <ul v-if="m.items.length" class="items">
                <li v-for="(it, k) in m.items" :key="k" :class="{ done: it.done }">
                  <span class="cb sm" :class="{ on: it.done }"><Icon v-if="it.done" name="check" :size="9" /></span>
                  {{ it.text }}
                </li>
              </ul>
            </div>
          </div>
        </div>
      </div>

      <!-- docs -->
      <div v-else class="docs">
        <aside class="doc-list">
          <button
            v-for="d in p.docs"
            :key="d.path"
            class="doc"
            :class="{ on: docSel === d.path }"
            :title="d.path"
            @click="docSel = d.path"
          ><Icon name="file" :size="13" /> <span class="ellipsis">{{ d.title }}</span></button>
          <div v-if="!p.docs.length" class="dim small pad">
            Документов нет. Кладите .md-файлы в <span class="mono">.hangar/docs/</span> — архитектура, решения, инструкции.
          </div>
          <div class="agents-mini">
            <div class="lbl">Ссылка на паспорт для агентов</div>
            <div v-for="f in agentFiles.filter((f) => f.exists || f.name === 'CLAUDE.md' || f.name === 'AGENTS.md')" :key="f.name" class="af">
              <span class="mono small ellipsis">{{ f.name }}</span>
              <span v-if="f.linked" class="badge green">есть</span>
              <button v-else class="btn sm ghost" @click="link(f.name)">{{ f.exists ? "добавить" : "создать" }}</button>
            </div>
          </div>
        </aside>
        <div class="doc-view">
          <template v-if="docSel">
            <div class="doc-bar">
              <span class="mono small ellipsis dim">{{ docSel }}</span>
              <span class="spacer" />
              <button class="btn sm ghost" @click="emit('open-file', docSel!)">Во вкладке «Файлы»</button>
              <button class="btn sm ghost" @click="openInEditor(docSel!)"><Icon name="code" :size="12" /> IDE</button>
            </div>
            <div class="md doc-md" @click="onMdClick" v-html="docHtml" />
          </template>
          <div v-else class="empty dim">Выберите документ</div>
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.pp { display: flex; flex-direction: column; height: 100%; min-height: 0; }
.small { font-size: 12px; }
.spacer { flex: 1; }

/* create */
.create-wrap { flex: 1; overflow-y: auto; padding: 28px 20px; display: flex; justify-content: center; }
.create { max-width: 640px; width: 100%; padding: 22px 24px; display: flex; flex-direction: column; gap: 16px; height: fit-content; }
.hero { display: flex; gap: 14px; }
.hero h2 { margin: 2px 0 6px; font-size: 17px; }
.hero p { margin: 0; line-height: 1.55; }
.pic { width: 48px; height: 48px; border-radius: 12px; flex: none; display: flex; align-items: center; justify-content: center; background: var(--accent-soft); color: var(--accent); }
.tree { font-size: 12.5px; line-height: 1.8; background: var(--bg); border: 1px solid var(--border); border-radius: 8px; padding: 10px 14px; }
.tree div { display: flex; align-items: center; gap: 6px; }
.tree .in { padding-left: 20px; }
.field { display: flex; flex-direction: column; gap: 5px; }
.field > span, .lbl { font-size: 12px; color: var(--text-2); }
.agents { display: flex; flex-direction: column; gap: 6px; }
.chk { display: flex; align-items: center; gap: 8px; cursor: pointer; font-size: 12.5px; }
.row { display: flex; align-items: center; gap: 12px; }

/* header */
.ph { padding: 10px 20px 0; border-bottom: 1px solid var(--border); }
.line { display: flex; align-items: center; gap: 10px; min-width: 0; }
.summary { color: var(--text-2); min-width: 0; }
.status-sel {
  height: 24px; border-radius: 12px; padding: 0 8px; font-size: 12px; font-weight: 600; cursor: pointer;
  border: 1px solid var(--border-2); background: var(--panel); color: var(--text);
}
.status-sel.green { color: var(--green); background: var(--green-soft); border-color: transparent; }
.status-sel.yellow { color: var(--yellow); background: var(--yellow-soft); border-color: transparent; }
.status-sel.blue { color: var(--accent); background: var(--accent-soft); border-color: transparent; }
.status-sel.purple { color: var(--purple); border-color: var(--border-2); }
.status-sel option { color: var(--text); background: var(--panel); }
.tags { display: flex; gap: 4px; flex-wrap: wrap; margin-top: 8px; }
.seg { display: flex; gap: 2px; margin-top: 8px; }
.seg button {
  display: flex; align-items: center; gap: 6px; padding: 7px 12px; border: 0; background: none; cursor: pointer;
  color: var(--text-2); border-bottom: 2px solid transparent; font-weight: 500;
}
.seg button:hover { color: var(--text); }
.seg button.on { color: var(--text); border-bottom-color: var(--accent); }
.cnt { font-size: 10.5px; background: var(--panel-2); border: 1px solid var(--border); padding: 0 6px; border-radius: 9px; color: var(--text-2); }

/* overview */
.scroll { flex: 1; overflow-y: auto; min-height: 0; }
.ov { display: grid; grid-template-columns: minmax(0, 1fr) 300px; gap: 14px; padding: 16px 20px 24px; }
.main-col, .side-col { display: flex; flex-direction: column; gap: 14px; min-width: 0; }
.card-h { display: flex; align-items: center; gap: 8px; }
.now { border-color: var(--accent-border); }
.now .card-h { color: var(--accent); }
.big { font-size: 26px; font-weight: 700; }
.bar { height: 6px; border-radius: 3px; background: var(--panel-2); overflow: hidden; margin: 8px 0 6px; }
.bar span { display: block; height: 100%; background: var(--green); border-radius: 3px; transition: width .2s; }
.link { margin-top: 8px; padding-left: 0; background: none; border: 0; color: var(--accent); }
.quick { display: flex; flex-direction: column; gap: 6px; }
.qt { display: flex; gap: 8px; align-items: baseline; font-size: 12.5px; min-width: 0; }
.qt .sec { font-size: 10.5px; color: var(--text-3); flex: none; width: 64px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

/* tasks */
.tasks { flex: 1; display: flex; flex-direction: column; min-height: 0; }
.tbar { display: flex; align-items: center; gap: 10px; padding: 8px 20px; border-bottom: 1px solid var(--border); }
.board { flex: 1; display: flex; gap: 12px; padding: 14px 20px 18px; overflow-x: auto; min-height: 0; align-items: flex-start; }
.col {
  width: 290px; flex: none; max-height: 100%; display: flex; flex-direction: column;
  background: var(--bg-2); border: 1px solid var(--border); border-radius: 10px; transition: border-color .1s, background .1s;
}
.col.drop { border-color: var(--accent); background: var(--accent-soft); }
.col-h { display: flex; align-items: center; gap: 8px; padding: 10px 12px 6px; }
.col-b { flex: 1; overflow-y: auto; padding: 4px 8px; display: flex; flex-direction: column; gap: 6px; min-height: 40px; }
.col-empty { text-align: center; font-size: 12px; padding: 10px; }
.task {
  display: flex; gap: 8px; align-items: flex-start; padding: 8px 9px; background: var(--panel); border: 1px solid var(--border);
  border-radius: 8px; position: relative; cursor: grab;
}
.task:hover { border-color: var(--border-2); }
.task.high { border-left: 3px solid var(--red); }
.task.low { opacity: .85; }
.task.dragging { opacity: .4; }
.task.done .tt { color: var(--text-3); text-decoration: line-through; }
.cb {
  width: 16px; height: 16px; flex: none; margin-top: 1px; border-radius: 4px; border: 1.5px solid var(--border-2);
  background: none; display: flex; align-items: center; justify-content: center; cursor: pointer; padding: 0; color: #fff;
}
.cb.on { background: var(--green); border-color: var(--green); }
.cb:disabled { cursor: default; }
.cb.sm { width: 13px; height: 13px; display: inline-flex; vertical-align: -2px; margin-right: 6px; }
.tb { flex: 1; min-width: 0; }
.tt { font-size: 13px; line-height: 1.4; word-break: break-word; }
.tm { display: flex; flex-wrap: wrap; gap: 4px; margin-top: 5px; align-items: center; }
.tag { font-size: 11px; color: var(--accent); }
.pri { font-size: 10.5px; padding: 0 5px; border-radius: 4px; background: var(--panel-2); color: var(--text-2); }
.pri.high { background: var(--red-soft); color: var(--red); }
.note-btn { display: inline-flex; align-items: center; gap: 3px; border: 0; background: none; color: var(--text-3); font-size: 11px; cursor: pointer; padding: 0; }
.note-btn:hover { color: var(--text); }
.note { margin-top: 6px; font-size: 12px; color: var(--text-2); white-space: pre-wrap; border-left: 2px solid var(--border-2); padding-left: 8px; }
.ta { position: absolute; right: 6px; top: 5px; display: none; gap: 3px; background: var(--panel); padding-left: 4px; }
.task:hover .ta { display: flex; }
.tact {
  display: flex; align-items: center; gap: 4px; height: 22px; padding: 0 7px; border-radius: 5px; cursor: pointer;
  border: 1px solid var(--border-2); background: var(--panel-2); color: var(--text-2); font-size: 11.5px;
}
.tact.icon { padding: 0 5px; }
.tact:hover { color: var(--text); border-color: var(--accent-border); }
.add { padding: 6px 8px 8px; }
.add .input { height: 28px; font-size: 12.5px; background: transparent; }

/* roadmap */
.rm { padding: 12px 20px 24px; max-width: 860px; }
.rm-bar { display: flex; align-items: center; margin-bottom: 10px; }
.ms { display: flex; gap: 12px; }
.rail-dot { width: 18px; flex: none; display: flex; flex-direction: column; align-items: center; position: relative; }
.rail-dot::before { content: ""; position: absolute; top: 0; bottom: -12px; width: 2px; background: var(--border); }
.ms:last-child .rail-dot::before { bottom: auto; height: 18px; }
.rail-dot span { position: relative; margin-top: 14px; width: 12px; height: 12px; border-radius: 50%; background: var(--panel); border: 2px solid var(--border-2); }
.ms.active .rail-dot span { border-color: var(--green); background: var(--green); box-shadow: 0 0 0 4px var(--green-soft); }
.ms.done .rail-dot span { border-color: var(--accent); background: var(--accent); }
.ms-card { flex: 1; padding: 12px 14px; margin-bottom: 12px; min-width: 0; }
.ms.done .ms-card { opacity: .78; }
.ms-h { display: flex; align-items: center; gap: 8px; }
.body { margin-top: 8px; font-size: 13px; }
.items { list-style: none; margin: 8px 0 0; padding: 0; display: flex; flex-direction: column; gap: 4px; font-size: 13px; }
.items li.done { color: var(--text-3); }

/* docs */
.docs { flex: 1; display: flex; min-height: 0; }
.doc-list { width: 250px; flex: none; border-right: 1px solid var(--border); overflow-y: auto; padding: 8px; display: flex; flex-direction: column; gap: 1px; }
.doc { display: flex; align-items: center; gap: 7px; padding: 6px 8px; border: 0; border-radius: 6px; background: none; color: var(--text-2); cursor: pointer; text-align: left; }
.doc:hover { background: var(--hover); color: var(--text); }
.doc.on { background: var(--active); color: var(--text); }
.pad { padding: 8px; line-height: 1.5; }
.agents-mini { margin-top: auto; padding: 12px 6px 4px; border-top: 1px solid var(--border); display: flex; flex-direction: column; gap: 4px; }
.af { display: flex; align-items: center; gap: 6px; justify-content: space-between; }
.doc-view { flex: 1; min-width: 0; display: flex; flex-direction: column; }
.doc-bar { display: flex; align-items: center; gap: 6px; padding: 8px 16px; border-bottom: 1px solid var(--border); }
.doc-md { flex: 1; overflow-y: auto; padding: 16px 24px 30px; max-width: 900px; }
</style>
