import { computed, ref } from "vue";
import { state } from "./store";

const mq = window.matchMedia("(prefers-color-scheme: dark)");
const systemDark = ref(mq.matches);
mq.addEventListener("change", (e) => (systemDark.value = e.matches));

const dark = computed(() =>
  state.settings.theme === "system" ? systemDark.value : state.settings.theme === "dark",
);

export function useDark() {
  return dark;
}
