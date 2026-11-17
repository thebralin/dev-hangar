// Hello World — минимальный плагин Dev Hangar.
// Показывает: вкладку, команду в меню «Все команды», toast и hangar.storage.

/** @param {import("../../../src/plugins/hangar").Hangar} hangar */
export function activate(hangar) {
  const { escapeHtml } = hangar.util;

  hangar.tabs.register({
    id: "hello",
    label: "Hello",
    mount(el, { project }) {
      const render = () => {
        const clicks = hangar.storage.get("clicks", 0);
        el.innerHTML = `
          <div style="padding: 24px; max-width: 640px">
            <h2 style="margin: 0 0 8px">👋 Привет, ${escapeHtml(project.name)}!</h2>
            <p class="muted">Стек: ${project.stacks.map(escapeHtml).join(", ") || "не определён"}</p>
            <button class="btn primary" data-act="click">Нажми меня</button>
            <span class="dim" style="margin-left: 8px">нажатий за всё время: ${clicks}</span>
          </div>`;
        el.querySelector("[data-act=click]").addEventListener("click", () => {
          hangar.storage.set("clicks", clicks + 1);
          render();
        });
      };
      render();
    },
  });

  hangar.actions.register({
    id: "greet",
    label: "Поздороваться",
    run(project) {
      hangar.ui.toast(`Привет из плагина! Проект: ${project.name}`);
    },
  });
}
