import { createApp } from "vue";
import App from "./App.vue";
import "./styles.css";

// disable the default webview context menu & reload shortcuts in production
if (!import.meta.env.DEV) {
  window.addEventListener("contextmenu", (e) => {
    const t = e.target as HTMLElement;
    if (!t.closest("input, textarea, .selectable, pre, .term")) e.preventDefault();
  });
}

createApp(App).mount("#app");
