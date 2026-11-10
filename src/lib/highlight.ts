import { createHighlighter, bundledLanguages, type Highlighter } from "shiki";

let hl: Promise<Highlighter> | null = null;
const loaded = new Set<string>();

function getHighlighter() {
  if (!hl) {
    hl = createHighlighter({ themes: ["github-dark-default", "github-light-default"], langs: [] });
  }
  return hl;
}

const NAME_MAP: Record<string, string> = {
  dockerfile: "docker",
  makefile: "make",
  "cmakelists.txt": "cmake",
  ".gitignore": "text",
  ".npmrc": "ini",
  ".editorconfig": "ini",
  ".prettierrc": "json",
  ".babelrc": "json",
  ".eslintrc": "json",
  "tsconfig.json": "jsonc",
  "jsconfig.json": "jsonc",
  "deno.jsonc": "jsonc",
  "bun.lock": "jsonc",
  "cargo.lock": "toml",
  "go.mod": "go",
  "gemfile": "ruby",
  "procfile": "shell",
};

const EXT_MAP: Record<string, string> = {
  mjs: "javascript", cjs: "javascript", mts: "typescript", cts: "typescript",
  yml: "yaml", htm: "html", svg: "xml", xaml: "xml", csproj: "xml", props: "xml",
  env: "dotenv", conf: "ini", cfg: "ini", lock: "text", txt: "text", log: "log",
  ps1: "powershell", psm1: "powershell", bat: "bat", cmd: "bat", sh: "shellscript", bash: "shellscript",
  h: "c", hpp: "cpp", cc: "cpp", kt: "kotlin", kts: "kotlin", rs: "rust", py: "python",
  rb: "ruby", cs: "csharp", fs: "fsharp", md: "markdown", mdx: "mdx", gql: "graphql",
  webmanifest: "json", map: "json", code_workspace: "jsonc", phtml: "php", neon: "yaml",
};

export function langFor(fileName: string): string {
  const name = fileName.split(/[\\/]/).pop()!.toLowerCase();
  if (NAME_MAP[name]) return NAME_MAP[name];
  if (name.startsWith(".env")) return "dotenv";
  if (name.startsWith("dockerfile")) return "docker";
  if (name.endsWith(".blade.php") && "blade" in bundledLanguages) return "blade";
  if (name === "artisan" || name === "spark") return "php";
  const ext = name.includes(".") ? name.split(".").pop()! : "";
  const cand = EXT_MAP[ext] || ext;
  if (cand && cand in bundledLanguages) return cand;
  return "text";
}

export const IMAGE_EXT = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "svg", "avif"];
export function isImage(fileName: string) {
  const ext = fileName.split(".").pop()!.toLowerCase();
  return IMAGE_EXT.includes(ext);
}

export async function highlight(code: string, lang: string, dark: boolean): Promise<string | null> {
  if (lang === "text" || code.length > 400_000) return null;
  try {
    const h = await getHighlighter();
    if (!loaded.has(lang)) {
      await h.loadLanguage(lang as any);
      loaded.add(lang);
    }
    return h.codeToHtml(code, { lang, theme: dark ? "github-dark-default" : "github-light-default" });
  } catch (e) {
    console.warn("highlight failed", lang, e);
    return null;
  }
}
