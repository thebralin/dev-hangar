<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { convertFileSrc } from "@tauri-apps/api/core";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import { marked } from "marked";
import DOMPurify from "dompurify";
import Icon from "./Icon.vue";
import { api, relPath, type FileContent } from "../lib/api";
import { highlight, langFor, isImage } from "../lib/highlight";
import { state, toast, escapeHtml } from "../lib/store";
import { useDark } from "../lib/theme";
import { size, relTime, fullDate } from "../lib/format";

const props = defineProps<{
  /** absolute path on disk */
  path?: string | null;
  /** project root, for relative display */
  root: string;
  /** show given content instead of reading from disk (e.g. git version) */
  content?: string | null;
  /** label shown instead of path (e.g. "file.ts @ abc123") */
  label?: string;
  line?: number;
  hideHistory?: boolean;
}>();
const emit = defineEmits<{ history: [rel: string] }>();

const dark = useDark();
const file = ref<FileContent | null>(null);
const html = ref("");
const error = ref("");
const loading = ref(false);
const mode = ref<"code" | "preview">("code");
const scroller = ref<HTMLElement | null>(null);

const name = computed(() => (props.path || (props.label || "").split(" @ ")[0]).split(/[\\/]/).pop() || "");
const lang = computed(() => langFor(name.value));
const image = computed(() => props.path && props.content == null && isImage(name.value));
const isMd = computed(() => lang.value === "markdown" || lang.value === "mdx");
const text = computed(() => (props.content != null ? props.content : file.value?.content ?? ""));
const lineCount = computed(() => (text.value ? text.value.split("\n").length : 0));
const rel = computed(() => (props.path ? relPath(props.root, props.path) : ""));

async function load() {
  error.value = "";
  html.value = "";
  file.value = null;
  if (!props.path && props.content == null) return;
  if (image.value) {
    mode.value = "preview";
    return;
  }
  mode.value = "code";
  loading.value = true;
  try {
    if (props.content == null && props.path) {
      file.value = await api.readText(props.path);
      if (file.value.binary) {
        loading.value = false;
        return;
      }
    }
    await render();
  } catch (e) {
    error.value = String(e);
  } finally {
    loading.value = false;
  }
}

async function render() {
  const code = text.value;
  const h = code.length > 250_000 || lineCount.value > 12_000 ? null : await highlight(code, lang.value, dark.value);
  if (h) {
    html.value = h;
  } else {
    html.value =
      '<pre class="plain"><code>' +
      code.split("\n").map((l) => `<span class="line">${escapeHtml(l)}</span>`).join("\n") +
      "</code></pre>";
  }
  await nextTick();
  scrollToLine();
}

function scrollToLine() {
  if (!props.line || !scroller.value) return;
  const lines = scroller.value.querySelectorAll(".line");
  const el = lines[props.line - 1] as HTMLElement | undefined;
  if (el) {
    el.classList.add("hl");
    el.scrollIntoView({ block: "center" });
  }
}

const mdHtml = computed(() => (isMd.value && mode.value === "preview" ? DOMPurify.sanitize(marked.parse(text.value, { async: false }) as string) : ""));

watch(() => [props.path, props.content], load, { immediate: true });
watch(dark, () => text.value && render());
watch(() => props.line, () => nextTick(scrollToLine));

async function toggleSvg() {
  if (mode.value === "preview") {
    mode.value = "code";
    if (!file.value && props.path) {
      try {
        file.value = await api.readText(props.path);
        await render();
      } catch (e) {
        error.value = String(e);
      }
    }
  } else {
    mode.value = "preview";
  }
}

async function openDefault() {
  if (!props.path) return;
  try {
    await openPath(props.path);
  } catch (e) {
    toast("Не удалось открыть: " + e, "error");
  }
}
async function openEditor() {
  if (!props.path) return;
  try {
    await api.openInEditor(state.settings.editor, props.path, props.line);
  } catch (e) {
    toast(String(e), "error");
  }
}
function copyPath() {
  if (props.path) navigator.clipboard.writeText(props.path);
  toast("Путь скопирован");
}
function copyContent() {
  navigator.clipboard.writeText(text.value);
  toast("Содержимое скопировано");
}
</script>

<template>
  <div class="viewer">
    <div v-if="path || label" class="bar">
      <Icon name="file" :size="14" class="dim" />
      <span class="fname ellipsis mono" :title="path || label">{{ label || rel }}</span>
      <span class="dim small" v-if="file">{{ size(file.size) }} · {{ lineCount }} строк · <span :title="fullDate(file.modified)">изм. {{ relTime(file.modified) }}</span></span>
      <span class="badge" v-if="lang !== 'text'">{{ lang }}</span>
      <span class="spacer" />
      <button v-if="isMd" class="btn sm" @click="mode = mode === 'code' ? 'preview' : 'code'">
        <Icon :name="mode === 'code' ? 'eye' : 'code'" :size="12" /> {{ mode === "code" ? "Превью" : "Код" }}
      </button>
      <button v-if="image && name.toLowerCase().endsWith('.svg')" class="btn sm" @click="toggleSvg">
        {{ mode === "code" ? "Картинка" : "Код" }}
      </button>
      <button v-if="!hideHistory && path" class="btn sm ghost" title="История изменений файла в git" @click="emit('history', rel)"><Icon name="history" :size="13" /> История</button>
      <button class="btn sm icon ghost" title="Скопировать содержимое" @click="copyContent"><Icon name="copy" :size="13" /></button>
      <template v-if="path">
        <button class="btn sm icon ghost" title="Скопировать путь" @click="copyPath"><Icon name="list" :size="13" /></button>
        <button class="btn sm icon ghost" title="Показать в проводнике" @click="revealItemInDir(path!)"><Icon name="folder-open" :size="13" /></button>
        <button class="btn sm" title="Открыть в приложении по умолчанию" @click="openDefault"><Icon name="external" :size="12" /> Открыть</button>
        <button class="btn sm primary" :title="`Открыть в ${state.settings.editor}`" @click="openEditor"><Icon name="code" :size="12" /> В IDE</button>
      </template>
    </div>
    <div ref="scroller" class="scroll">
      <div v-if="loading" class="empty">Загрузка…</div>
      <div v-else-if="error" class="empty err">{{ error }}</div>
      <div v-else-if="image && mode === 'preview'" class="img-wrap">
        <img :src="convertFileSrc(path!)" :alt="name" />
      </div>
      <div v-else-if="file?.binary" class="empty">
        <Icon name="file" :size="36" />
        Бинарный файл ({{ size(file.size) }})
        <button class="btn" @click="openDefault"><Icon name="external" :size="13" /> Открыть в приложении по умолчанию</button>
      </div>
      <div v-else-if="isMd && mode === 'preview'" class="md md-wrap" v-html="mdHtml" />
      <template v-else>
        <div v-if="file?.truncated" class="trunc">Файл больше 3 МБ — показано начало.</div>
        <div class="code-view selectable" v-html="html" />
      </template>
    </div>
  </div>
</template>

<style scoped>
.viewer { display: flex; flex-direction: column; height: 100%; min-width: 0; background: var(--bg); }
.bar { display: flex; align-items: center; gap: 8px; padding: 6px 10px 6px 14px; border-bottom: 1px solid var(--border); background: var(--bg-2); min-height: 40px; }
.fname { font-size: 12.5px; font-weight: 600; flex: 0 1 auto; min-width: 80px; }
.small { font-size: 11.5px; white-space: nowrap; }
.scroll { flex: 1; overflow: auto; min-height: 0; }
.err { color: var(--red); }
.img-wrap { padding: 24px; display: flex; justify-content: center; background: repeating-conic-gradient(var(--panel-2) 0 25%, transparent 0 50%) 50% / 20px 20px; min-height: 100%; }
.img-wrap img { max-width: 100%; object-fit: contain; }
.md-wrap { padding: 20px 28px; max-width: 900px; }
.trunc { padding: 6px 14px; background: var(--yellow-soft); color: var(--yellow); font-size: 12px; }
</style>
