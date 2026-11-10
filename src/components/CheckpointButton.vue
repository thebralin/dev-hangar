<script setup lang="ts">
import { computed, nextTick, ref } from "vue";
import Icon from "./Icon.vue";
import { api, type GitSummary } from "../lib/api";
import { state, createCheckpoint, loadCheckpoints, refreshBrief, toast, type ProjectEntry } from "../lib/store";
import { relTime } from "../lib/format";

const props = defineProps<{ entry: ProjectEntry; git?: GitSummary }>();
const emit = defineEmits<{ goto: [tab: string] }>();

const open = ref(false);
const msg = ref("");
const busy = ref(false);
const input = ref<HTMLInputElement | null>(null);
const list = computed(() => state.checkpoints[props.entry.path] ?? []);

async function toggle() {
  open.value = !open.value;
  if (open.value) {
    if (props.git?.isRepo) loadCheckpoints(props.entry.path);
    await nextTick();
    input.value?.focus();
  }
}

async function create() {
  busy.value = true;
  try {
    const cp = await createCheckpoint(props.entry.path, msg.value);
    if (cp) {
      msg.value = "";
      open.value = false;
    }
  } finally {
    busy.value = false;
  }
}

async function init() {
  busy.value = true;
  try {
    await api.gitInit(props.entry.path);
    await refreshBrief(props.entry.path);
    toast("git-репозиторий создан");
  } catch (e) {
    toast(String(e), "error");
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="cpw">
    <button class="btn" title="Чекпоинт: снимок текущего состояния, к которому можно откатиться" @click="toggle">
      <Icon name="bookmark" :size="14" /> Чекпоинт
    </button>
    <div v-if="open" class="pop" @keydown.esc="open = false">
      <template v-if="git?.isRepo">
        <div class="t">Снимок всех файлов проекта (включая незакоммиченные). История веток не меняется. Удобно делать перед тем, как дать ИИ-агенту поправить код.</div>
        <div class="row">
          <input ref="input" v-model="msg" class="input" placeholder="Описание, напр. «до рефакторинга авторизации»" @keydown.enter="create" />
          <button class="btn primary" :disabled="busy" @click="create">{{ busy ? "…" : "Создать" }}</button>
        </div>
        <template v-if="list.length">
          <div class="lh">Последние</div>
          <div v-for="c in list.slice(0, 4)" :key="c.id" class="li">
            <span class="ellipsis grow">{{ c.message }}</span>
            <span class="dim">{{ relTime(c.date) }}</span>
          </div>
          <a class="all" @click="open = false; emit('goto', 'git')">Все чекпоинты, сравнение и откат →</a>
        </template>
      </template>
      <template v-else>
        <div class="t">Чекпоинты хранятся в git. Этот проект ещё не git-репозиторий. Создать? Файлы не изменятся, появится только папка <code>.git</code>.</div>
        <button class="btn primary" :disabled="busy" @click="init">git init</button>
      </template>
    </div>
    <div v-if="open" class="shade" @click="open = false" />
  </div>
</template>

<style scoped>
.cpw { position: relative; }
.pop {
  position: absolute; right: 0; top: 36px; z-index: 61; width: 420px;
  background: var(--panel); border: 1px solid var(--border-2); border-radius: 10px; padding: 12px;
  box-shadow: 0 12px 34px rgba(0,0,0,.4); display: flex; flex-direction: column; gap: 10px;
}
.shade { position: fixed; inset: 0; z-index: 60; }
.t { color: var(--text-2); font-size: 12.5px; line-height: 1.5; }
.lh { font-size: 11px; text-transform: uppercase; color: var(--text-3); font-weight: 600; letter-spacing: .04em; }
.li { display: flex; gap: 8px; font-size: 12.5px; }
.all { font-size: 12.5px; }
</style>
