// Claude Code для Dev Hangar.
// Показывает: shell.openTerminal, passport.registerTaskAction, passport.taskPrompt / fillPrompt, настройки плагина.

/** @param {import("../../../src/plugins/hangar").Hangar} hangar */
export function activate(hangar) {
  const isWin = /Windows/i.test(navigator.userAgent);
  const setting = (k) => String(hangar.settings.get(k) ?? "").trim();

  /** Prompt → one safely quoted shell argument. */
  const quote = (text) => {
    const s = text.replace(/\s*\r?\n\s*/g, " ").trim();
    if (isWin) return `"${s.replace(/"/g, "'").replace(/%/g, "％")}"`;
    return `"${s.replace(/(["$`\\])/g, "\\$1")}"`;
  };

  const base = () => [setting("command") || "claude", setting("extraArgs")].filter(Boolean).join(" ");

  /** Run Claude with a prompt — interactively in a terminal or headless in the Hangar console. */
  async function runClaude(project, prompt, label) {
    if (setting("mode") === "console") {
      const perm = setting("permissionMode") || "acceptEdits";
      // stdin from nul: otherwise `claude -p` waits for piped input
      const cmd = `${base()} -p ${quote(prompt)} --permission-mode ${perm} ${isWin ? "< nul" : "< /dev/null"}`;
      await hangar.shell.run(project.path, cmd, { label });
      hangar.ui.toast("Claude работает в консоли — ответ появится, когда он закончит");
    } else {
      await hangar.shell.openTerminal(project.path, `${base()} ${quote(prompt)}`);
    }
  }

  hangar.actions.register({
    id: "open",
    label: "Claude",
    icon: "zap",
    toolbar: true,
    title: "Открыть Claude Code в терминале в папке проекта",
    async run(project) {
      await hangar.shell.openTerminal(project.path, base());
    },
  });

  hangar.actions.register({
    id: "fill-passport",
    label: "Claude: заполнить паспорт проекта",
    async run(project) {
      const prompt = await hangar.passport.fillPrompt(project.path);
      await runClaude(project, prompt, "Claude · паспорт");
    },
  });

  hangar.passport.registerTaskAction({
    id: "do",
    label: "Claude",
    icon: "zap",
    title: "Отдать задачу Claude Code",
    when: (task) => !task.done,
    async run(project, task) {
      const prompt = await hangar.passport.taskPrompt(project.path, task);
      await runClaude(project, prompt, `Claude · ${task.text.slice(0, 28)}`);
    },
  });
}
