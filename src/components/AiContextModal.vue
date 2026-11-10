<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import Icon from "./Icon.vue";
import { api, type GitSummary, type ProjectInfo } from "../lib/api";
import { buildAiContext, PRESET_FILE, PRESET_PROMPT, type AiOptions } from "../lib/aicontext";
import { toast, refreshBrief, type ProjectEntry } from "../lib/store";

const props = defineProps<{ entry: ProjectEntry; info: ProjectInfo; git?: GitSummary }>();
const emit = defineEmits<{ close: []; "open-file": [path: string] }>();

const preset = ref<"prompt" | "file">("prompt");
const opts = ref<AiOptions>({ ...PRESET_PROMPT });
const text = ref("");
const loading = ref(false);
const edited = ref(false);
const confirmOverwrite = ref<string | null>(null);

const existing = computed(() =>
  ["CLAUDE.md", "AGENTS.md"].filter((n) => props.info.notableFiles.some((f) => f.toLowerCase() === n.toLowerCase())),
);

async function regen() {
  loading.value = true;
  try {
    text.value = await buildAiContext(props.entry, props.info, props.git, opts.value);
    edited.value = false;
  } finally {
    loading.value = false;
  }
}

function setPreset(p: "prompt" | "file") {
  preset.value = p;
  opts.value = { ...(p === "prompt" ? PRESET_PROMPT : PRESET_FILE) };
}

watch(opts, regen, { deep: true });
onMounted(regen);

const tokens = computed(() => Math.round(text.value.length / 3.5));

function copy() {
  navigator.clipboard.writeText(text.value);
  toast("Контекст скопирован, вставьте его в чат с ИИ");
}

async function save(name: string, overwrite = false) {
  try {
    const p = await api.writeAiFile(props.entry.path, name, text.value, overwrite);
    confirmOverwrite.value = null;
    toast(`Сохранено: ${name}`);
    refreshBrief(props.entry.path);
    emit("open-file", p);
    emit("close");
  } catch (e) {
    if (String(e) === "exists") confirmOverwrite.value = name;
    else toast(String(e), "error");
  }
}

const checks: [keyof AiOptions, string][] = [
  ["run", "Как запускать и скрипты"],
  ["tree", "Дерево файлов"],
  ["deps", "Зависимости"],
  ["devDeps", "…включая dev"],
  ["env", "Имена переменных .env"],
  ["commits", "Последние коммиты"],
  ["changes", "Незакоммиченные файлы"],
  ["notes", "Мои заметки"],
  ["readme", "Начало README"],
];
</script>

<template>
  <div class="backdrop" @mousedown.self="emit('close')">
    <div class="modal">
      <div class="mh">
        <Icon name="sparkles" :size="16" /> <b>Контекст проекта для ИИ</b>
        <span class="spacer" />
        <button class="btn sm icon ghost" @click="emit('close')"><Icon name="x" :size="14" /></button>
      </div>
      <div class="mb">
        <div class="side">
          <div class="seg">
            <button :class="{ on: preset === 'prompt' }" @click="setPreset('prompt')">Для чата</button>
            <button :class="{ on: preset === 'file' }" @click="setPreset('file')">Для файла</button>
          </div>
          <div class="hint">
            {{ preset === "prompt"
              ? "Вставьте в начало разговора с ИИ, чтобы он сразу понимал проект, включая текущие изменения."
              : "Постоянное описание проекта. Claude Code читает CLAUDE.md, Codex и Cursor читают AGENTS.md." }}
          </div>
          <label v-for="[k, label] in checks" :key="k" class="chk" :class="{ sub: k === 'devDeps' }">
            <input v-model="(opts as any)[k]" type="checkbox" :disabled="k === 'devDeps' && !opts.deps" /> {{ label }}
          </label>
          <label v-if="opts.tree" class="chk sub">
            глубина дерева
            <select v-model.number="opts.treeDepth" class="input depth">
              <option v-for="d in [2, 3, 4, 5, 6]" :key="d" :value="d">{{ d }}</option>
            </select>
          </label>
          <div v-if="existing.length" class="hint ex">
            В проекте уже есть: <b>{{ existing.join(", ") }}</b>
          </div>
        </div>
        <div class="main">
          <textarea v-model="text" class="input ta mono" spellcheck="false" @input="edited = true" />
          <div class="foot">
            <span class="dim">{{ loading ? "собираю…" : `≈ ${tokens.toLocaleString("ru")} токенов · ${text.length.toLocaleString("ru")} символов` }}</span>
            <span v-if="edited" class="dim">· изменено вручную</span>
            <span class="spacer" />
            <template v-if="confirmOverwrite">
              <span class="warn">{{ confirmOverwrite }} уже существует.</span>
              <button class="btn sm danger" @click="save(confirmOverwrite!, true)">Перезаписать</button>
              <button class="btn sm ghost" @click="confirmOverwrite = null">Отмена</button>
            </template>
            <template v-else>
              <button class="btn" @click="save('AGENTS.md')">Сохранить AGENTS.md</button>
              <button class="btn" @click="save('CLAUDE.md')">Сохранить CLAUDE.md</button>
              <button class="btn primary" @click="copy"><Icon name="copy" :size="13" /> Скопировать</button>
            </template>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.backdrop { position: fixed; inset: 0; background: rgba(0,0,0,.45); z-index: 90; display: flex; align-items: center; justify-content: center; }
.modal { width: min(1100px, 94vw); height: min(760px, 90vh); display: flex; flex-direction: column; background: var(--panel); border: 1px solid var(--border-2); border-radius: 12px; box-shadow: 0 24px 60px rgba(0,0,0,.5); }
.mh { display: flex; align-items: center; gap: 8px; padding: 12px 16px; border-bottom: 1px solid var(--border); }
.mb { flex: 1; display: flex; min-height: 0; }
.side { width: 250px; flex: none; padding: 14px; border-right: 1px solid var(--border); display: flex; flex-direction: column; gap: 8px; overflow-y: auto; }
.seg { display: flex; background: var(--bg); border: 1px solid var(--border); border-radius: 6px; padding: 2px; }
.seg button { flex: 1; border: 0; background: none; padding: 4px; border-radius: 4px; cursor: pointer; color: var(--text-2); font-size: 12px; }
.seg button.on { background: var(--active); color: var(--text); }
.hint { font-size: 12px; color: var(--text-2); line-height: 1.45; margin-bottom: 4px; }
.hint.ex { margin-top: 8px; }
.chk { display: flex; align-items: center; gap: 7px; cursor: pointer; font-size: 12.5px; }
.chk.sub { padding-left: 22px; color: var(--text-2); }
.depth { width: 60px; height: 24px; }
.main { flex: 1; min-width: 0; display: flex; flex-direction: column; padding: 12px; gap: 10px; }
.ta { flex: 1; resize: none; font-size: 12px; line-height: 1.5; background: var(--bg); }
.foot { display: flex; align-items: center; gap: 8px; font-size: 12px; }
.warn { color: var(--yellow); }
</style>
