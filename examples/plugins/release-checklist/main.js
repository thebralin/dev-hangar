// Чеклист релиза.
// Показывает: UI на Vue через hangar.vue (без своего бандла), реактивность,
// хранение данных по проектам, настройки и подписку на события.

/** @param {import("../../../src/plugins/hangar").Hangar} hangar */
export function activate(hangar) {
  const { createApp, h, reactive, computed } = hangar.vue;

  const BUILD_ITEM = "Сборка проходит";
  const items = () =>
    String(hangar.settings.get("items") || "")
      .split("\n")
      .map((s) => s.trim())
      .filter(Boolean);

  // { [projectPath]: { [item]: true } } — общий реактивный объект для всех открытых вкладок
  const done = reactive(hangar.storage.get("done", {}));
  const save = () => hangar.storage.set("done", done);
  const toggle = (path, item, value) => {
    done[path] ??= {};
    done[path][item] = value ?? !done[path][item];
    save();
  };

  // Успешный Build → отмечаем пункт автоматически
  hangar.events.on("process:exit", (e) => {
    if (e.kind === "build" && e.code === 0 && items().includes(BUILD_ITEM)) {
      toggle(e.projectPath, BUILD_ITEM, true);
      hangar.ui.toast(`Чеклист: «${BUILD_ITEM}» ✓`);
    }
  });

  // настройки меняются на экране «Плагины» — вкладки перерисуются через этот счётчик
  const rev = reactive({ n: 0 });
  hangar.settings.onChange(() => rev.n++);

  hangar.tabs.register({
    id: "checklist",
    label: "Релиз",
    icon: "check",
    mount(el, { project }) {
      const app = createApp({
        setup() {
          const list = computed(() => (rev.n, items()));
          const state = computed(() => done[project.path] ?? {});
          const progress = computed(() => list.value.filter((i) => state.value[i]).length);
          return () =>
            h("div", { style: "padding: 20px; max-width: 560px" }, [
              h("h2", { style: "margin: 0 0 4px" }, "Чеклист релиза"),
              h("p", { class: "muted", style: "margin: 0 0 14px" }, `${project.name} · готово ${progress.value} из ${list.value.length}`),
              h(
                "div",
                { class: "card" },
                list.value.map((item) =>
                  h("label", { class: "row", style: "padding: 9px 14px; border-bottom: 1px solid var(--border); cursor: pointer" }, [
                    h("input", {
                      type: "checkbox",
                      checked: !!state.value[item],
                      onChange: (ev) => toggle(project.path, item, ev.target.checked),
                    }),
                    h("span", { style: state.value[item] ? "text-decoration: line-through; color: var(--text-3)" : "" }, item),
                  ]),
                ),
              ),
              h("div", { class: "row", style: "margin-top: 12px" }, [
                h(
                  "button",
                  {
                    class: "btn",
                    onClick: () => {
                      delete done[project.path];
                      save();
                    },
                  },
                  "Сбросить",
                ),
                progress.value === list.value.length && list.value.length
                  ? h("span", { class: "badge green" }, "Можно релизить 🚀")
                  : null,
              ]),
            ]);
        },
      });
      app.mount(el);
      return () => app.unmount();
    },
  });
}
