// Hugo — пример плагина, который учит Dev Hangar новому стеку.
// Показывает: stacks.register (detect / actions / dependencies), чтение файлов проекта
// в детекторе, настройки, влияющие на команды.

const CONFIGS = ["hugo.toml", "hugo.yaml", "hugo.yml", "hugo.json"];
const LEGACY = ["config.toml", "config.yaml", "config.yml", "config.json"];

/** @param {import("../../../src/plugins/hangar").Hangar} hangar */
export function activate(hangar) {
  /** Найти конфиг Hugo в папке (или null). */
  async function findConfig(ctx) {
    // ctx.list() читает папку один раз и кэширует результат — это дешевле, чем много ctx.has()
    const names = new Set((await ctx.list()).map((e) => e.name));
    const modern = CONFIGS.find((f) => names.has(f));
    if (modern) return modern;
    // старые сайты: config.* — но так называются конфиги многих инструментов,
    // поэтому дополнительно требуем типичные папки Hugo
    if (names.has("archetypes") || (names.has("content") && names.has("layouts"))) {
      return LEGACY.find((f) => names.has(f)) ?? null;
    }
    return null;
  }

  hangar.stacks.register({
    id: "hugo",
    label: "Hugo",
    color: "#ff4088",

    async detect(ctx) {
      return (await findConfig(ctx)) !== null;
    },

    actions() {
      const drafts = hangar.settings.get("drafts") ? " -D" : "";
      return [
        // primary: true — эта команда окажется на кнопке Dev, даже если в проекте есть package.json
        { id: "server", label: "hugo server", command: `hugo server${drafts}`, kind: "dev", primary: true },
        { id: "build", label: "hugo --minify", command: "hugo --minify", kind: "build", primary: true },
        { id: "new-post", label: "Новый пост", command: "hugo new content posts/new-post.md", kind: "script" },
        { id: "mod-tidy", label: "hugo mod tidy", command: "hugo mod tidy", kind: "install" },
      ];
    },

    async dependencies(ctx) {
      const cfg = await findConfig(ctx);
      const text = cfg ? await ctx.readText(cfg) : null;
      if (!text) return [];
      const deps = [];
      // theme = "ananke"  |  theme: ananke  |  "theme": "ananke"
      const m = text.match(/^\s*"?theme"?\s*[:=]\s*["']?([\w./@-]+)/m);
      if (m) deps.push({ name: m[1], version: "theme" });
      // Hugo Modules: [[module.imports]] path = "github.com/…"
      for (const im of text.matchAll(/path\s*[:=]\s*["']([^"']+)["']/g)) deps.push({ name: im[1], version: "module" });
      return deps;
    },
  });

  // пользователь поменял настройку → пересобрать команды у всех проектов
  hangar.settings.onChange(() => hangar.stacks.refresh());
}
