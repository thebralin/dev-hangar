<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { marked } from "marked";
import DOMPurify from "dompurify";
import { openUrl } from "@tauri-apps/plugin-opener";
import Icon from "./Icon.vue";
import StackBadge from "./StackBadge.vue";
import ResumeCard from "./ResumeCard.vue";
import { api, joinPath, type GitSummary, type ProjectInfo, type ProjectStats, type Action, type Module } from "../lib/api";
import { state, runAction, stopProc, procId, toast, type ProjectEntry, type CustomCommand } from "../lib/store";
import { relTime, fullDate, size, plural } from "../lib/format";

const props = defineProps<{ entry: ProjectEntry; info: ProjectInfo; git?: GitSummary }>();
const emit = defineEmits<{ "open-file": [path: string, line?: number]; goto: [tab: string] }>();

const stats = ref<ProjectStats | null>(null);
const statsLoading = ref(false);
const readmeHtml = ref("");
const readmeOpen = ref(false);

async function loadStats() {
  statsLoading.value = true;
  try {
    stats.value = await api.stats(props.entry.path);
  } catch (e) {
    console.warn(e);
  } finally {
    statsLoading.value = false;
  }
}

async function loadReadme() {
  readmeHtml.value = "";
  if (!props.info.readme) return;
  try {
    const f = await api.readText(joinPath(props.entry.path, props.info.readme));
    const html = await marked.parse(f.content.slice(0, 200_000), { gfm: true });
    readmeHtml.value = DOMPurify.sanitize(html);
  } catch {
    /* ignore */
  }
}

onMounted(() => {
  loadStats();
  loadReadme();
});
watch(() => props.info, loadReadme);

const modules = computed(() => [props.info.root, ...props.info.subprojects].filter((m) => m.actions.length || m.scripts.length || (!m.relPath && m.stacks.length)));

function procOf(a: { id: string }) {
  return state.procs[procId(props.entry.path, a.id)];
}
function isRun(a: { id: string }) {
  return procOf(a)?.status === "running";
}
async function toggleAction(a: Action | CustomCommand, prefix = "") {
  if (isRun(a)) await stopProc(procId(props.entry.path, a.id));
  else await runAction(props.entry.path, a, prefix);
}

function scriptAction(m: Module, name: string): Action {
  const pm = m.packageManager || "npm";
  const cmd = pm === "yarn" ? `yarn ${name}` : m.stacks.some((s) => s.id === "deno") && !m.packageManager ? `deno task ${name}` : `${pm} run ${name}`;
  const same = m.actions.find((a) => a.command === cmd);
  if (same) return same;
  return { id: (m.relPath ? m.relPath + ":" : "") + "script:" + name, label: name, command: cmd, kind: "script", cwd: m.relPath };
}

function composerAction(m: Module, name: string): Action {
  const cmd = `composer run ${name}`;
  const same = m.actions.find((a) => a.command === cmd);
  if (same) return same;
  return { id: (m.relPath ? m.relPath + ":" : "") + "composer:" + name, label: name, command: cmd, kind: "script", cwd: m.relPath };
}
function composerInstall(m: Module) {
  return m.actions.find((a) => a.command === "composer install");
}

const kindIcon: Record<string, string> = { dev: "play", build: "hammer", install: "download", test: "zap", start: "play", open: "external", serve: "globe", script: "terminal" };

// ------- custom commands -------
const newCmd = ref({ label: "", command: "", cwd: "" });
const showAdd = ref(false);
function addCustom() {
  const c = newCmd.value;
  if (!c.command.trim()) return;
  if (!props.entry.customCommands) props.entry.customCommands = [];
  props.entry.customCommands.push({
    id: "custom:" + Math.random().toString(36).slice(2, 9),
    label: c.label.trim() || c.command.trim(),
    command: c.command.trim(),
    cwd: c.cwd.trim(),
    kind: "script",
  });
  newCmd.value = { label: "", command: "", cwd: "" };
  showAdd.value = false;
}
function removeCustom(id: string) {
  props.entry.customCommands = props.entry.customCommands?.filter((c) => c.id !== id);
}

function openRel(rel: string) {
  emit("open-file", joinPath(props.entry.path, rel));
}

function copy(text: string) {
  navigator.clipboard.writeText(text);
  toast("Скопировано");
}

const gitDirty = computed(() => props.git?.status.length ?? 0);
</script>

<template>
  <div class="ov">
    <ResumeCard :entry="entry" :info="info" :git="git" :stats="stats" @open-file="(p: string, l?: number) => emit('open-file', p, l)" @goto="(t: string) => emit('goto', t)" />
    <div class="col main-col">
      <!-- RUN -->
      <section v-for="m in modules" :key="m.relPath" class="card">
        <div class="card-h">
          <Icon name="play" :size="14" />
          <span>{{ m.relPath ? m.relPath : "Запуск" }}</span>
          <span v-if="m.relPath && m.name !== m.relPath" class="muted">{{ m.name }}</span>
          <span class="spacer" />
          <span v-if="m.packageManager" class="badge">{{ m.packageManager }}</span>
          <span v-if="m.nodeVersion" class="badge" title="Версия Node (.nvmrc / engines)">node {{ m.nodeVersion }}</span>
          <span v-if="m.phpVersion" class="badge" title="Версия PHP (.php-version / composer.json)">php {{ m.phpVersion }}</span>
          <template v-if="m.relPath"><StackBadge v-for="s in m.stacks" :key="s.id" :stack="s" mini /></template>
        </div>
        <div class="card-b">
          <div v-if="m.needsInstall && m.packageManager" class="warn">
            <Icon name="info" :size="14" /> node_modules не найден — зависимости не установлены
            <span class="spacer" />
            <button class="btn sm" @click="runAction(entry.path, m.actions.find(a => a.kind === 'install')!, m.relPath)">
              <Icon name="download" :size="12" /> {{ m.packageManager }} install
            </button>
          </div>
          <div v-if="m.vendorMissing && composerInstall(m)" class="warn">
            <Icon name="info" :size="14" /> vendor не найден — composer-зависимости не установлены
            <span class="spacer" />
            <button class="btn sm" @click="runAction(entry.path, composerInstall(m)!, m.relPath)">
              <Icon name="download" :size="12" /> composer install
            </button>
          </div>
          <div v-if="m.missingTools?.length" class="warn">
            <Icon name="info" :size="14" /> Не найдено в PATH: {{ m.missingTools.join(", ") }}. Команды запуска работать не будут, пока не установите.
          </div>
          <div v-if="m.description" class="muted desc">{{ m.description }}</div>
          <div class="acts">
            <div v-for="a in m.actions" :key="a.id" class="act" :class="{ running: isRun(a) }">
              <button class="btn sm" :class="isRun(a) ? 'danger' : a.kind === 'dev' ? 'success' : ''" @click="toggleAction(a, m.relPath)">
                <Icon :name="isRun(a) ? 'stop' : kindIcon[a.kind] || 'play'" :size="12" />
                {{ a.label }}
              </button>
              <code class="cmd ellipsis" :title="a.command" @click="copy(a.command)">{{ a.command }}</code>
              <span v-if="procOf(a)?.url" class="url"><a @click="openUrl(procOf(a)!.url!)">{{ procOf(a)!.url }}</a></span>
            </div>
          </div>
          <details v-if="m.scripts.length" class="scripts" :open="m.scripts.length <= 8">
            <summary>Все скрипты ({{ m.scripts.length }})</summary>
            <div v-for="[name, cmd] in m.scripts" :key="name" class="script">
              <button class="btn sm icon ghost" :class="{ danger: isRun(scriptAction(m, name)) }" :title="isRun(scriptAction(m, name)) ? 'Остановить' : 'Запустить'" @click="toggleAction(scriptAction(m, name), m.relPath)">
                <Icon :name="isRun(scriptAction(m, name)) ? 'stop' : 'play'" :size="12" />
              </button>
              <span class="sname">{{ name }}</span>
              <code class="dim ellipsis" :title="cmd">{{ cmd }}</code>
            </div>
          </details>
          <details v-if="m.composerScripts?.length" class="scripts" :open="m.composerScripts.length <= 8">
            <summary>Скрипты composer ({{ m.composerScripts.length }})</summary>
            <div v-for="[name, cmd] in m.composerScripts" :key="name" class="script">
              <button class="btn sm icon ghost" :class="{ danger: isRun(composerAction(m, name)) }" :title="isRun(composerAction(m, name)) ? 'Остановить' : 'Запустить'" @click="toggleAction(composerAction(m, name), m.relPath)">
                <Icon :name="isRun(composerAction(m, name)) ? 'stop' : 'play'" :size="12" />
              </button>
              <span class="sname">{{ name }}</span>
              <code class="dim ellipsis" :title="cmd">{{ cmd }}</code>
            </div>
          </details>
        </div>
      </section>

      <section v-if="!modules.length" class="card">
        <div class="card-h"><Icon name="info" :size="14" /> Тип проекта не определён</div>
        <div class="card-b muted">
          Не нашёл package.json, Cargo.toml, go.mod, composer.json, index.html, index.php и т.п. Добавьте свои команды запуска ниже — они сохранятся для этого проекта.
        </div>
      </section>

      <!-- CUSTOM COMMANDS -->
      <section class="card">
        <div class="card-h">
          <Icon name="terminal" :size="14" /> Свои команды
          <span class="spacer" />
          <button class="btn sm ghost" @click="showAdd = !showAdd"><Icon name="plus" :size="12" /> Добавить</button>
        </div>
        <div class="card-b">
          <div v-if="showAdd" class="add-form">
            <input v-model="newCmd.label" class="input" placeholder="Название (напр. Dev + tunnel)" />
            <input v-model="newCmd.command" class="input mono" placeholder="Команда, напр. bun run dev --host" @keydown.enter="addCustom" />
            <input v-model="newCmd.cwd" class="input mono" placeholder="Подпапка (необязательно)" />
            <button class="btn primary" @click="addCustom">Сохранить</button>
          </div>
          <div v-for="c in entry.customCommands ?? []" :key="c.id" class="act">
            <button class="btn sm" :class="isRun(c) ? 'danger' : ''" @click="toggleAction(c)">
              <Icon :name="isRun(c) ? 'stop' : 'play'" :size="12" /> {{ c.label }}
            </button>
            <code class="cmd ellipsis">{{ c.cwd ? c.cwd + " › " : "" }}{{ c.command }}</code>
            <button class="btn sm icon ghost" title="Удалить" @click="removeCustom(c.id)"><Icon name="trash" :size="12" /></button>
          </div>
          <div v-if="!entry.customCommands?.length && !showAdd" class="dim">Например: запуск с флагами, docker, деплой, миграции.</div>
        </div>
      </section>

      <!-- NOTES -->
      <section class="card">
        <div class="card-h"><Icon name="note" :size="14" /> Заметки <span class="muted">— как запускать, пароли от тестовых аккаунтов, нюансы</span></div>
        <div class="card-b">
          <textarea v-model="entry.notes" class="input notes" rows="4" placeholder="Ваши заметки по проекту…" />
        </div>
      </section>

      <!-- README -->
      <section v-if="readmeHtml" class="card">
        <div class="card-h">
          <Icon name="file" :size="14" /> {{ info.readme }}
          <span class="spacer" />
          <button class="btn sm ghost" @click="openRel(info.readme!)">Открыть файл</button>
        </div>
        <div class="card-b readme" :class="{ open: readmeOpen }">
          <div class="md" v-html="readmeHtml" />
          <button v-if="!readmeOpen" class="more btn sm" @click="readmeOpen = true">Показать полностью</button>
        </div>
      </section>
    </div>

    <div class="col side-col">
      <!-- GIT -->
      <section class="card">
        <div class="card-h">
          <Icon name="git" :size="14" /> Git
          <span class="spacer" />
          <button v-if="git?.isRepo" class="btn sm ghost" @click="emit('goto', 'git')">История →</button>
        </div>
        <div class="card-b kv">
          <template v-if="!git">
            <div class="dim">Загрузка…</div>
          </template>
          <template v-else-if="!git.gitAvailable">
            <div class="dim">git не найден в PATH</div>
          </template>
          <template v-else-if="!git.isRepo">
            <div class="dim">Не git-репозиторий</div>
          </template>
          <template v-else>
            <div class="k">Ветка</div>
            <div class="v"><b>{{ git.branch }}</b> <span v-if="git.upstream" class="dim">→ {{ git.upstream }}</span></div>
            <template v-if="git.ahead || git.behind">
              <div class="k">Синхр.</div>
              <div class="v"><span class="badge yellow">↑{{ git.ahead }} не запушено</span> <span v-if="git.behind" class="badge yellow">↓{{ git.behind }} не подтянуто</span></div>
            </template>
            <div class="k">Remote</div>
            <div class="v">
              <div v-for="r in git.remotes" :key="r.name" class="ellipsis">
                <span class="dim">{{ r.name }}</span>
                <a v-if="r.webUrl" class="rlink" :title="r.url" @click="openUrl(r.webUrl!)">{{ r.webUrl.replace(/^https?:\/\//, "") }}</a>
                <span v-else class="mono rlink">{{ r.url }}</span>
              </div>
              <div v-if="!git.remotes.length" class="dim">нет (локальный репозиторий)</div>
            </div>
            <div class="k">Последний</div>
            <div class="v" v-if="git.lastCommit">
              <div class="ellipsis" :title="git.lastCommit.subject">{{ git.lastCommit.subject }}</div>
              <div class="dim small">{{ git.lastCommit.author }} · <span :title="fullDate(git.lastCommit.date)">{{ relTime(git.lastCommit.date) }}</span> · {{ git.lastCommit.short }}</div>
            </div>
            <div class="v dim" v-else>коммитов нет</div>
            <div class="k">Изменения</div>
            <div class="v">
              <a v-if="gitDirty" @click="emit('goto', 'git')"><span class="badge yellow">{{ gitDirty }} {{ plural(gitDirty, "файл", "файла", "файлов") }} не закоммичено</span></a>
              <span v-else class="badge green">чисто</span>
            </div>
            <div class="k">Коммитов</div>
            <div class="v">{{ git.commitCount }} <span class="dim small" v-if="git.firstCommitDate">с {{ fullDate(git.firstCommitDate).split(",")[0] }}</span></div>
            <template v-if="git.tags.length">
              <div class="k">Теги</div>
              <div class="v ellipsis">{{ git.tags.slice(0, 5).join(", ") }}</div>
            </template>
            <template v-if="git.stashCount">
              <div class="k">Stash</div>
              <div class="v">{{ git.stashCount }}</div>
            </template>
            <div class="k">Автор</div>
            <div class="v small">{{ git.userName || "—" }} <span class="dim">{{ git.userEmail }}</span></div>
          </template>
        </div>
      </section>

      <!-- ACTIVITY -->
      <section class="card">
        <div class="card-h">
          <Icon name="clock" :size="14" /> Размер и активность
          <span class="spacer" />
          <button class="btn sm icon ghost" title="Пересчитать" @click="loadStats"><Icon name="refresh" :size="12" /></button>
        </div>
        <div class="card-b">
          <div v-if="statsLoading && !stats" class="dim">Сканирую…</div>
          <template v-if="stats">
            <div class="stat-row">
              <div><b>{{ stats.files.toLocaleString("ru") }}</b><span class="dim"> файлов{{ stats.truncated ? "+" : "" }}</span></div>
              <div><b>{{ size(stats.totalSize) }}</b><span class="dim"> без node_modules</span></div>
            </div>
            <div class="dim small" style="margin: 4px 0 8px">Последнее изменение: <b class="muted">{{ relTime(stats.lastModified) }}</b> ({{ fullDate(stats.lastModified) }})</div>
            <div class="langs">
              <span v-for="[ext, n] in stats.languages" :key="ext" class="badge">.{{ ext }} <span class="dim">{{ n }}</span></span>
            </div>
          </template>
        </div>
      </section>

      <!-- ENV & CONFIGS -->
      <section v-if="info.envFiles.length || info.notableFiles.length" class="card">
        <div class="card-h"><Icon name="settings" :size="14" /> Конфиги и окружение</div>
        <div class="card-b files">
          <button v-for="f in info.envFiles" :key="f" class="btn sm" @click="openRel(f)">🔑 {{ f }}</button>
          <button v-for="f in info.notableFiles" :key="f" class="btn sm" @click="openRel(f)">{{ f }}</button>
        </div>
      </section>
    </div>
  </div>
</template>

<style scoped>
.ov { display: grid; grid-template-columns: minmax(0, 1fr) 380px; gap: 14px; padding: 16px 20px; height: 100%; overflow-y: auto; align-content: start; }
@media (max-width: 1150px) { .ov { grid-template-columns: 1fr; } }
.col { display: flex; flex-direction: column; gap: 14px; min-width: 0; }
.acts { display: flex; flex-direction: column; gap: 6px; }
.act { display: flex; align-items: center; gap: 10px; min-width: 0; margin-bottom: 4px; }
.act .btn { min-width: 150px; justify-content: flex-start; }
.cmd { color: var(--text-2); font-size: 12px; cursor: copy; flex: 1; min-width: 0; }
.url a { font-size: 12px; }
.desc { margin-bottom: 10px; }
.warn { display: flex; align-items: center; gap: 8px; padding: 8px 10px; margin-bottom: 10px; border-radius: 6px; background: var(--yellow-soft); color: var(--yellow); }
.scripts { margin-top: 10px; border-top: 1px dashed var(--border); padding-top: 8px; }
.scripts summary { cursor: pointer; color: var(--text-2); font-size: 12px; margin-bottom: 4px; }
.script { display: flex; align-items: center; gap: 8px; padding: 1px 0; }
.sname { font-weight: 600; min-width: 110px; font-family: var(--mono); font-size: 12px; }
.script code { font-size: 12px; flex: 1; }
.add-form { display: grid; grid-template-columns: 1fr 2fr 1fr auto; gap: 6px; margin-bottom: 10px; }
.notes { font-family: var(--sans); min-height: 80px; }
.readme { max-height: 360px; overflow: hidden; position: relative; }
.readme.open { max-height: none; }
.readme .more { position: absolute; bottom: 10px; left: 50%; transform: translateX(-50%); box-shadow: 0 -20px 30px 10px var(--panel); }
.kv { display: grid; grid-template-columns: 82px minmax(0, 1fr); gap: 7px 10px; }
.kv .k { color: var(--text-3); font-size: 12px; padding-top: 1px; }
.kv .v { min-width: 0; }
.kv > .dim { grid-column: 1 / -1; }
.small { font-size: 11.5px; }
.stat-row { display: flex; gap: 18px; }
.recent { display: flex; gap: 10px; padding: 3px 6px; margin: 0 -6px; border-radius: 4px; cursor: pointer; font-size: 12px; }
.recent:hover { background: var(--hover); }
.recent .mono { font-size: 11.5px; }
.langs { display: flex; flex-wrap: wrap; gap: 4px; margin-top: 10px; }
.files { display: flex; flex-wrap: wrap; gap: 6px; }
.rlink { margin-left: 6px; }
</style>
