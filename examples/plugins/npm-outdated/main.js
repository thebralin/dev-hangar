// Устаревшие npm-пакеты.
// Показывает: условие `when`, shell.exec с захватом вывода, shell.run в консоли,
// настройки из plugin.json, отдельный CSS-файл, отмену через AbortSignal.

/** @param {import("../../../src/plugins/hangar").Hangar} hangar */
export function activate(hangar) {
  const { escapeHtml } = hangar.util;
  const isNode = (p) => p.stacks.includes("node") || p.stacks.includes("bun");

  async function check(project) {
    const { stdout, stderr, timedOut } = await hangar.shell.exec(project.path, "npm outdated --json", {
      timeoutMs: Number(hangar.settings.get("timeout") || 60) * 1000,
    });
    if (timedOut) throw new Error("npm outdated не уложился в таймаут");
    // npm outdated выходит с кодом 1, когда есть устаревшие пакеты, — это не ошибка
    if (!stdout.trim()) {
      if (stderr.trim()) throw new Error(stderr.trim());
      return [];
    }
    const data = JSON.parse(stdout);
    let rows = Object.entries(data).map(([name, v]) => ({ name, ...(Array.isArray(v) ? v[0] : v) }));
    if (!hangar.settings.get("includeDev")) rows = rows.filter((r) => r.type !== "devDependencies");
    return rows;
  }

  function kind(r) {
    const a = String(r.current ?? "").split(".");
    const b = String(r.latest ?? "").split(".");
    if (a[0] !== b[0]) return "major";
    if (a[1] !== b[1]) return "minor";
    return "patch";
  }

  hangar.tabs.register({
    id: "outdated",
    label: "Обновления",
    icon: "package",
    when: isNode,
    async mount(el, { project, signal }) {
      el.innerHTML = `<div class="npo"><div class="npo-status dim">Проверяю пакеты… (npm outdated)</div></div>`;
      let rows;
      try {
        rows = await check(project);
      } catch (e) {
        if (signal.aborted) return;
        el.innerHTML = `<div class="npo"><div class="npo-error">${escapeHtml(e.message || String(e))}</div></div>`;
        return;
      }
      if (signal.aborted) return; // пользователь уже ушёл с вкладки

      if (!rows.length) {
        el.innerHTML = `<div class="npo"><div class="npo-ok">${hangar.ui.icon("check", 18)} Все пакеты актуальны</div></div>`;
        return;
      }
      el.innerHTML = `
        <div class="npo">
          <div class="npo-bar">
            <b>Устарело: ${rows.length}</b>
            <span class="spacer"></span>
            <button class="btn" data-act="update">${hangar.ui.icon("download", 14)} npm update (в пределах semver)</button>
          </div>
          <table class="npo-table">
            <thead><tr><th>Пакет</th><th>Текущая</th><th>Подходящая</th><th>Последняя</th><th></th></tr></thead>
            <tbody>
              ${rows
                .map(
                  (r) => `<tr>
                    <td><a data-url="https://www.npmjs.com/package/${encodeURIComponent(r.name)}">${escapeHtml(r.name)}</a>
                      ${r.type === "devDependencies" ? '<span class="badge">dev</span>' : ""}</td>
                    <td class="mono">${escapeHtml(r.current ?? "—")}</td>
                    <td class="mono">${escapeHtml(r.wanted ?? "—")}</td>
                    <td class="mono">${escapeHtml(r.latest ?? "—")}</td>
                    <td><span class="npo-kind ${kind(r)}">${kind(r)}</span></td>
                  </tr>`,
                )
                .join("")}
            </tbody>
          </table>
        </div>`;
      el.querySelector("[data-act=update]").addEventListener("click", () =>
        hangar.shell.run(project.path, "npm update", { label: "npm update" }),
      );
      el.querySelectorAll("[data-url]").forEach((a) =>
        a.addEventListener("click", () => hangar.ui.openUrl(a.dataset.url)),
      );
    },
  });
}
