<script setup lang="ts">
import { onMounted, ref } from "vue";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import Icon from "./Icon.vue";
import { state } from "../lib/store";
import { api } from "../lib/api";

const emit = defineEmits<{ close: [] }>();
const cfgPath = ref("");

const editorNames: Record<string, string> = {
  code: "VS Code", cursor: "Cursor", windsurf: "Windsurf", antigravity: "Antigravity", zed: "Zed", trae: "Trae",
  kiro: "Kiro", codium: "VSCodium", webstorm: "WebStorm", idea: "IntelliJ IDEA", subl: "Sublime Text", "notepad++": "Notepad++",
};

onMounted(async () => {
  cfgPath.value = await api.configLocation().catch(() => "");
  if (!state.detectedEditors.length) state.detectedEditors = await api.detectEditors().catch(() => []);
});
</script>

<template>
  <div class="backdrop" @mousedown.self="emit('close')">
    <div class="modal">
      <div class="mh">
        <Icon name="settings" :size="16" /> <b>Настройки</b>
        <span class="spacer" />
        <button class="btn sm icon ghost" @click="emit('close')"><Icon name="x" :size="14" /></button>
      </div>
      <div class="mb">
        <label class="field">
          <span>Редактор / IDE для кнопки «В IDE»</span>
          <div class="chips">
            <button
              v-for="e in state.detectedEditors"
              :key="e"
              class="btn sm"
              :class="{ primary: state.settings.editor === e }"
              @click="state.settings.editor = e"
            >{{ editorNames[e] || e }}</button>
            <span v-if="!state.detectedEditors.length" class="dim">Редакторы в PATH не найдены — укажите команду вручную</span>
          </div>
          <input v-model="state.settings.editor" class="input mono" placeholder='code  или  "C:\Program Files\...\app.exe"' />
          <small class="dim">Команда, которой открывается папка/файл. Для VS Code-подобных редакторов (Cursor, Windsurf…) поддерживается переход к строке.</small>
        </label>

        <label class="field">
          <span>Терминал</span>
          <select v-model="state.settings.terminal" class="input">
            <option value="wt">Windows Terminal</option>
            <option value="powershell">PowerShell</option>
            <option value="pwsh">PowerShell 7 (pwsh)</option>
            <option value="cmd">cmd</option>
            <option value="git-bash">Git Bash</option>
          </select>
        </label>

        <label class="field">
          <span>Тема</span>
          <select v-model="state.settings.theme" class="input">
            <option value="dark">Тёмная</option>
            <option value="light">Светлая</option>
            <option value="system">Как в системе</option>
          </select>
        </label>

        <div class="field">
          <span>Список проектов</span>
          <label class="chk"><input v-model="state.settings.groupByFolder" type="checkbox" /> Группировать по родительской папке</label>
          <div class="row">
            <span class="dim">Сортировка:</span>
            <select v-model="state.settings.sortBy" class="input" style="width: 220px">
              <option value="activity">по последней активности</option>
              <option value="name">по имени</option>
              <option value="added">по дате добавления</option>
            </select>
          </div>
        </div>

        <div v-if="cfgPath" class="field">
          <span>Файл настроек</span>
          <div class="row">
            <code class="dim ellipsis grow">{{ cfgPath }}</code>
            <button class="btn sm" @click="revealItemInDir(cfgPath)">Показать</button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.backdrop { position: fixed; inset: 0; background: rgba(0,0,0,.45); z-index: 90; display: flex; align-items: center; justify-content: center; }
.modal { width: 600px; max-height: 85vh; display: flex; flex-direction: column; background: var(--panel); border: 1px solid var(--border-2); border-radius: 12px; box-shadow: 0 24px 60px rgba(0,0,0,.5); }
.mh { display: flex; align-items: center; gap: 8px; padding: 12px 16px; border-bottom: 1px solid var(--border); }
.mb { padding: 16px; overflow-y: auto; display: flex; flex-direction: column; gap: 18px; }
.field { display: flex; flex-direction: column; gap: 7px; }
.field > span { font-weight: 600; }
.chips { display: flex; flex-wrap: wrap; gap: 6px; }
.chk { display: flex; gap: 6px; align-items: center; cursor: pointer; }
</style>
