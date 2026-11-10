const rtf = new Intl.RelativeTimeFormat("ru", { numeric: "auto" });

export function relTime(input?: number | string | null): string {
  if (input == null || input === "" || input === 0) return "—";
  const t = typeof input === "number" ? input : Date.parse(input);
  if (!t || isNaN(t)) return "—";
  const diff = (t - Date.now()) / 1000;
  const abs = Math.abs(diff);
  if (abs < 45) return "только что";
  if (abs < 3600) return rtf.format(Math.round(diff / 60), "minute");
  if (abs < 86400) return rtf.format(Math.round(diff / 3600), "hour");
  if (abs < 86400 * 30) return rtf.format(Math.round(diff / 86400), "day");
  if (abs < 86400 * 365) return rtf.format(Math.round(diff / (86400 * 30)), "month");
  return rtf.format(Math.round(diff / (86400 * 365)), "year");
}

export function fullDate(input?: number | string | null): string {
  if (input == null || input === "" || input === 0) return "—";
  const t = typeof input === "number" ? input : Date.parse(input);
  if (!t || isNaN(t)) return "—";
  return new Date(t).toLocaleString("ru-RU", {
    day: "2-digit", month: "2-digit", year: "numeric", hour: "2-digit", minute: "2-digit",
  });
}

export function size(bytes?: number | null): string {
  if (bytes == null) return "—";
  const u = ["Б", "КБ", "МБ", "ГБ"];
  let i = 0;
  let n = bytes;
  while (n >= 1024 && i < u.length - 1) { n /= 1024; i++; }
  return `${n < 10 && i > 0 ? n.toFixed(1) : Math.round(n)} ${u[i]}`;
}

export function plural(n: number, one: string, few: string, many: string) {
  const m10 = n % 10, m100 = n % 100;
  if (m10 === 1 && m100 !== 11) return one;
  if (m10 >= 2 && m10 <= 4 && (m100 < 12 || m100 > 14)) return few;
  return many;
}

export const STACK_COLORS: Record<string, string> = {
  vue: "#42b883", nuxt: "#00dc82", astro: "#ff5d01", react: "#61dafb", next: "#a3a3a3",
  svelte: "#ff3e00", sveltekit: "#ff3e00", solid: "#4f88c6", angular: "#dd0031",
  electron: "#9feaf9", tauri: "#ffc131", express: "#b0b0b0", hono: "#ff7a1a", fastify: "#cfcfcf",
  koa: "#aaaaaa", elysia: "#c084fc", nest: "#e0234e", node: "#5fa04e", bun: "#e8c07d", deno: "#70ffaf",
  vite: "#a78bfa", webpack: "#8dd6f9", tailwind: "#38bdf8", prisma: "#5a67d8", drizzle: "#c5f74f",
  ts: "#3178c6", rust: "#dea584", go: "#00add8", python: "#ffd43b", django: "#44b78b",
  php: "#8892bf", laravel: "#ff2d20", dotnet: "#9b4f96", java: "#f89820", gradle: "#02303a",
  flutter: "#02569b", cmake: "#064f8c", make: "#6d8086", docker: "#2496ed", html: "#e34c26",
  socketio: "#dddddd", unknown: "#777777",
  symfony: "#cfcfcf", yii: "#40b3d8", codeigniter: "#ee4323", cakephp: "#d33c43", slim: "#74a045",
  laminas: "#68b604", drupal: "#0678be", wordpress: "#21759b", livewire: "#fb70a9", inertia: "#9553e9",
  filament: "#fdae4b", doctrine: "#fc6a31",
};
