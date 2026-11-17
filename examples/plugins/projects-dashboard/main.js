// Сводка по проектам — пример своего экрана (pages.register).
// Показывает: экран в боковой панели, hangar.projects, переход к проекту, AbortSignal.

/** @param {import("../../../src/plugins/hangar").Hangar} hangar */
export function activate(hangar) {
  const { escapeHtml } = hangar.util;

  hangar.pages.register({
    id: "dashboard",
    label: "Сводка",
    icon: "layers",
    async mount(el, { signal }) {
      const projects = hangar.projects.list();
      el.innerHTML = `
        <div style="padding: 20px 24px">
          <h1 style="font-size: 19px; margin: 0 0 4px">Сводка по проектам</h1>
          <p class="muted" style="margin: 0 0 16px">Проектов: ${projects.length}. Нажмите на строку, чтобы открыть проект.</p>
          <div class="card"><table style="width: 100%; border-collapse: collapse" data-rows></table></div>
        </div>`;
      const table = el.querySelector("[data-rows]");

      // git-статус собираем параллельно, но не больше 4 запросов одновременно
      const rows = projects.map((p) => ({ p, git: null }));
      const render = () => {
        table.innerHTML = rows
          .map(({ p, git }) => {
            const dirty = git?.status?.length ?? 0;
            return `<tr data-path="${escapeHtml(p.path)}" style="cursor: pointer; border-bottom: 1px solid var(--border)">
              <td style="padding: 8px 14px"><b>${escapeHtml(p.name)}</b>
                <div class="dim mono" style="font-size: 11px">${escapeHtml(p.path)}</div></td>
              <td style="padding: 8px 14px">${p.stacks.slice(0, 3).map((s) => `<span class="badge">${escapeHtml(s)}</span>`).join(" ")}</td>
              <td style="padding: 8px 14px" class="mono">${git?.branch ? escapeHtml(git.branch) : '<span class="dim">—</span>'}</td>
              <td style="padding: 8px 14px; text-align: right">${
                git == null ? '<span class="dim">…</span>'
                : !git.isRepo ? '<span class="dim">не git</span>'
                : dirty ? `<span class="badge yellow">●${dirty} изменений</span>`
                : '<span class="badge green">чисто</span>'
              }</td>
            </tr>`;
          })
          .join("");
      };
      render();

      table.addEventListener("click", (e) => {
        const tr = e.target.closest("tr[data-path]");
        if (tr) hangar.projects.select(tr.dataset.path);
      });

      const queue = [...rows];
      const worker = async () => {
        while (queue.length && !signal.aborted) {
          const row = queue.shift();
          row.git = await hangar.projects.git(row.p.path).catch(() => ({ isRepo: false }));
          if (!signal.aborted) render();
        }
      };
      await Promise.all([worker(), worker(), worker(), worker()]);
    },
  });
}
