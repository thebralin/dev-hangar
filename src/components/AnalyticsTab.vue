<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import Icon from "./Icon.vue";
import BarChart, { type BarItem } from "./charts/BarChart.vue";
import Heatmap from "./charts/Heatmap.vue";
import { api, joinPath, sameSep, type CodeStats, type GitAnalytics, type GitSummary } from "../lib/api";
import { type ProjectEntry } from "../lib/store";
import { fullDate, plural, relTime } from "../lib/format";

const props = defineProps<{ entry: ProjectEntry; git?: GitSummary }>();
const emit = defineEmits<{ "open-file": [path: string, line?: number]; history: [rel: string] }>();

// cache per project for the session
const cache = ((globalThis as any).__devhangarAnalytics ??= new Map<string, { git: GitAnalytics | null; code: CodeStats | null; at: number }>());

const ga = ref<GitAnalytics | null>(null);
const cs = ref<CodeStats | null>(null);
const loading = ref(false);

async function load(force = false) {
  const c = cache.get(props.entry.path);
  if (c && !force) {
    ga.value = c.git;
    cs.value = c.code;
    return;
  }
  loading.value = true;
  try {
    const [g, code] = await Promise.all([
      api.gitAnalytics(props.entry.path).catch(() => null),
      api.codeStats(props.entry.path).catch(() => null),
    ]);
    ga.value = g;
    cs.value = code;
    cache.set(props.entry.path, { git: g, code, at: Date.now() });
  } finally {
    loading.value = false;
  }
}
onMounted(() => load());

const hasGit = computed(() => !!ga.value?.isRepo && ga.value.totalCommits > 0);
const fmtN = (n: number) => n.toLocaleString("ru");
const DAY = 86_400_000;

function dkey(d: Date) {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

// ---------- tiles ----------
const streaks = computed(() => {
  const set = new Set(ga.value?.days.map((d) => d.date) ?? []);
  // current: consecutive days ending today (or yesterday)
  let cur = 0;
  const d = new Date();
  d.setHours(12, 0, 0, 0);
  if (!set.has(dkey(d))) d.setTime(d.getTime() - DAY);
  while (set.has(dkey(d))) {
    cur++;
    d.setTime(d.getTime() - DAY);
  }
  // longest
  let best = 0;
  let run = 0;
  let prev: number | null = null;
  for (const day of ga.value?.days ?? []) {
    const [y, m, dd] = day.date.split("-").map(Number);
    const t = Date.UTC(y, m - 1, dd);
    run = prev != null && t - prev === DAY ? run + 1 : 1;
    best = Math.max(best, run);
    prev = t;
  }
  return { cur, best };
});

const last30 = computed(() => {
  const from = dkey(new Date(Date.now() - 30 * DAY));
  return (ga.value?.days ?? []).filter((d) => d.date >= from).reduce((s, d) => s + d.commits, 0);
});

const age = computed(() => {
  if (!ga.value?.firstDate) return null;
  const ms = Date.now() - Date.parse(ga.value.firstDate);
  const months = Math.floor(ms / (30.44 * DAY));
  if (months < 1) {
    const d = Math.max(1, Math.floor(ms / DAY));
    return `${d} ${plural(d, "день", "дня", "дней")}`;
  }
  if (months < 12) return `${months} мес.`;
  const y = Math.floor(months / 12);
  const m = months % 12;
  return m ? `${y} г. ${m} мес.` : `${y} ${plural(y, "год", "года", "лет")}`;
});

// ---------- heatmap range ----------
const years = computed(() => {
  const s = new Set((ga.value?.days ?? []).map((d) => d.date.slice(0, 4)));
  return [...s].sort().reverse();
});
const heatRange = ref<string>("last");
const heatEnd = computed(() => (heatRange.value === "last" ? undefined : new Date(Number(heatRange.value), 11, 31)));

// ---------- commits over time ----------
const commitMode = ref<"weeks" | "months">("weeks");
const MONTHS = ["янв", "фев", "мар", "апр", "май", "июн", "июл", "авг", "сен", "окт", "ноя", "дек"];

function mondayOf(d: Date) {
  const x = new Date(d);
  x.setHours(12, 0, 0, 0);
  x.setDate(x.getDate() - ((x.getDay() + 6) % 7));
  return x;
}

const commitItems = computed<BarItem[]>(() => {
  const days = ga.value?.days ?? [];
  if (commitMode.value === "weeks") {
    const weeks = 52;
    const start = mondayOf(new Date(Date.now() - (weeks - 1) * 7 * DAY));
    const buckets = Array.from({ length: weeks }, (_, i) => ({ start: new Date(start.getTime() + i * 7 * DAY), c: 0, a: 0, d: 0 }));
    for (const d of days) {
      const [y, m, dd] = d.date.split("-").map(Number);
      const t = new Date(y, m - 1, dd, 12).getTime();
      const idx = Math.floor((t - start.getTime()) / (7 * DAY));
      if (idx >= 0 && idx < weeks) {
        buckets[idx].c += d.commits;
        buckets[idx].a += d.adds;
        buckets[idx].d += d.dels;
      }
    }
    return buckets.map((b) => {
      const end = new Date(b.start.getTime() + 6 * DAY);
      const label = b.start.getDate() <= 7 ? MONTHS[b.start.getMonth()] : "";
      return {
        label,
        value: b.c,
        tip: `${b.start.toLocaleDateString("ru-RU", { day: "numeric", month: "short" })} – ${end.toLocaleDateString("ru-RU", { day: "numeric", month: "short", year: "numeric" })}\n${b.c} ${plural(b.c, "коммит", "коммита", "коммитов")}\n+${fmtN(b.a)} / −${fmtN(b.d)} строк`,
      };
    });
  }
  return monthBuckets.value.map((b) => ({
    label: b.m === 0 || monthBuckets.value.length <= 14 ? (b.m === 0 ? String(b.y) : MONTHS[b.m]) : "",
    value: b.c,
    tip: `${MONTHS[b.m]} ${b.y}\n${b.c} ${plural(b.c, "коммит", "коммита", "коммитов")}\n+${fmtN(b.a)} / −${fmtN(b.d)} строк`,
  }));
});

const monthBuckets = computed(() => {
  const days = ga.value?.days ?? [];
  if (!days.length) return [];
  const [fy, fm] = days[0].date.split("-").map(Number);
  const now = new Date();
  const out: { y: number; m: number; c: number; a: number; d: number }[] = [];
  let y = fy, m = fm - 1;
  while (y < now.getFullYear() || (y === now.getFullYear() && m <= now.getMonth())) {
    out.push({ y, m, c: 0, a: 0, d: 0 });
    m++;
    if (m > 11) { m = 0; y++; }
    if (out.length > 240) break;
  }
  const idx = new Map(out.map((b, i) => [`${b.y}-${b.m}`, i]));
  for (const d of days) {
    const [yy, mm] = d.date.split("-").map(Number);
    const i = idx.get(`${yy}-${mm - 1}`);
    if (i != null) {
      out[i].c += d.commits;
      out[i].a += d.adds;
      out[i].d += d.dels;
    }
  }
  return out;
});

const churnItems = computed<BarItem[]>(() => {
  const b = monthBuckets.value.slice(-24);
  return b.map((x) => ({
    label: x.m === 0 || b.length <= 14 ? (x.m === 0 ? String(x.y) : MONTHS[x.m]) : x === b[0] ? MONTHS[x.m] : "",
    value: x.a,
    neg: x.d,
    tip: `${MONTHS[x.m]} ${x.y}\nдобавлено: ${fmtN(x.a)}\nудалено: ${fmtN(x.d)}\nитого: ${x.a - x.d >= 0 ? "+" : ""}${fmtN(x.a - x.d)}`,
  }));
});

// ---------- when do I work ----------
const WD = ["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Вс"];
const WD_FULL = ["понедельник", "вторник", "среда", "четверг", "пятница", "суббота", "воскресенье"];
const weekdayItems = computed<BarItem[]>(() =>
  (ga.value?.punch ?? []).map((row, i) => {
    const v = row.reduce((s, x) => s + x, 0);
    return { label: WD[i], value: v, tip: `${WD_FULL[i]}\n${v} ${plural(v, "коммит", "коммита", "коммитов")}` };
  }),
);
const hourItems = computed<BarItem[]>(() =>
  Array.from({ length: 24 }, (_, h) => {
    const v = (ga.value?.punch ?? []).reduce((s, row) => s + (row[h] ?? 0), 0);
    return { label: String(h), value: v, tip: `${String(h).padStart(2, "0")}:00–${String(h).padStart(2, "0")}:59\n${v} ${plural(v, "коммит", "коммита", "коммитов")}` };
  }),
);
const peak = computed(() => {
  const wd = weekdayItems.value.reduce((b, x, i, arr) => (x.value > arr[b].value ? i : b), 0);
  const h = hourItems.value.reduce((b, x, i, arr) => (x.value > arr[b].value ? i : b), 0);
  const night = hourItems.value.filter((_, i) => i >= 22 || i < 5).reduce((s, x) => s + x.value, 0);
  const weekend = weekdayItems.value.slice(5).reduce((s, x) => s + x.value, 0);
  const total = ga.value?.totalCommits || 1;
  return { wd: WD_FULL[wd], h, nightPct: Math.round((night / total) * 100), weekendPct: Math.round((weekend / total) * 100) };
});

// ---------- languages ----------
const langs = computed(() => {
  const l = cs.value?.languages ?? [];
  const max = Math.max(1, ...l.map((x) => x.code));
  const total = cs.value?.totalCode || 1;
  return l.slice(0, 12).map((x) => ({ ...x, w: (x.code / max) * 100, pct: Math.round((x.code / total) * 1000) / 10 }));
});

// ---------- authors ----------
const authorMax = computed(() => Math.max(1, ...(ga.value?.authors ?? []).map((a) => a.commits)));

function repoRoot() {
  return sameSep(props.git?.root || props.entry.path, props.entry.path);
}
function openRepoFile(rel: string) {
  emit("open-file", joinPath(repoRoot(), rel));
}
</script>

<template>
  <div class="an">
    <div v-if="loading && !ga && !cs" class="empty">Считаю статистику…</div>
    <template v-else>
      <div class="toolbar">
        <span class="muted">Статистика по git-истории и файлам проекта</span>
        <span v-if="ga?.truncated" class="badge yellow">учтены последние 20 000 коммитов</span>
        <span class="spacer" />
        <button class="btn sm ghost" :disabled="loading" @click="load(true)"><Icon name="refresh" :size="12" /> {{ loading ? "Считаю…" : "Пересчитать" }}</button>
      </div>

      <!-- TILES -->
      <div class="tiles">
        <div v-if="hasGit" class="tile">
          <div class="k">Коммитов</div>
          <div class="v">{{ fmtN(ga!.totalCommits) }}</div>
          <div class="s">за последние 30 дней: {{ last30 }}</div>
        </div>
        <div v-if="hasGit" class="tile">
          <div class="k">Активных дней</div>
          <div class="v">{{ fmtN(ga!.days.length) }}</div>
          <div class="s">≈ {{ (ga!.totalCommits / Math.max(1, ga!.days.length)).toFixed(1) }} коммита в активный день</div>
        </div>
        <div v-if="hasGit" class="tile">
          <div class="k">Серия</div>
          <div class="v">{{ streaks.cur }} <small>{{ plural(streaks.cur, "день", "дня", "дней") }}</small></div>
          <div class="s">рекорд: {{ streaks.best }} {{ plural(streaks.best, "день", "дня", "дней") }} подряд</div>
        </div>
        <div v-if="hasGit" class="tile">
          <div class="k">Возраст проекта</div>
          <div class="v">{{ age }}</div>
          <div class="s" :title="fullDate(ga!.lastDate)">последний коммит {{ relTime(ga!.lastDate) }}</div>
        </div>
        <div v-if="cs" class="tile">
          <div class="k">Строк кода</div>
          <div class="v">{{ fmtN(cs.totalCode) }}</div>
          <div class="s">{{ fmtN(cs.totalFiles) }} {{ plural(cs.totalFiles, "файл", "файла", "файлов") }}, без пустых строк</div>
        </div>
        <div v-if="hasGit" class="tile">
          <div class="k">Изменено строк</div>
          <div class="v">{{ fmtN(ga!.totalAdds + ga!.totalDels) }}</div>
          <div class="s">+{{ fmtN(ga!.totalAdds) }} / −{{ fmtN(ga!.totalDels) }}</div>
        </div>
      </div>

      <div v-if="!hasGit" class="card nogit">
        <Icon name="git" :size="16" />
        {{ ga?.isRepo ? "В репозитории пока нет коммитов, поэтому графиков активности нет." : "Проект не в git, поэтому показываю только статистику кода." }}
      </div>

      <template v-if="hasGit">
        <!-- HEATMAP -->
        <section class="card">
          <div class="card-h">
            Активность по дням
            <span class="spacer" />
            <select v-model="heatRange" class="input sel">
              <option value="last">последние 12 месяцев</option>
              <option v-for="y in years" :key="y" :value="y">{{ y }} год</option>
            </select>
          </div>
          <div class="card-b"><Heatmap :days="ga!.days" :end="heatEnd" /></div>
        </section>

        <div class="two">
          <section class="card">
            <div class="card-h">
              Коммиты
              <span class="spacer" />
              <div class="seg">
                <button :class="{ on: commitMode === 'weeks' }" @click="commitMode = 'weeks'">по неделям, год</button>
                <button :class="{ on: commitMode === 'months' }" @click="commitMode = 'months'">по месяцам, всё время</button>
              </div>
            </div>
            <div class="card-b"><BarChart :items="commitItems" :height="180" /></div>
          </section>
          <section class="card">
            <div class="card-h">Строки кода по месяцам <span class="muted">последние 24 мес.</span></div>
            <div class="card-b"><BarChart :items="churnItems" :height="180" pos-label="добавлено строк" neg-label="удалено строк" /></div>
          </section>
        </div>

        <div class="two">
          <section class="card">
            <div class="card-h">По дням недели</div>
            <div class="card-b">
              <BarChart :items="weekdayItems" :height="150" />
              <div class="cap">Чаще всего коммиты в <b>{{ peak.wd }}</b>. На выходные приходится {{ peak.weekendPct }}%.</div>
            </div>
          </section>
          <section class="card">
            <div class="card-h">По часам <span class="muted">время автора коммита</span></div>
            <div class="card-b">
              <BarChart :items="hourItems" :height="150" :label-every="3" />
              <div class="cap">Пик активности около <b>{{ String(peak.h).padStart(2, "0") }}:00</b>. Ночью (22:00–05:00) сделано {{ peak.nightPct }}% коммитов.</div>
            </div>
          </section>
        </div>
      </template>

      <div class="two">
        <section v-if="cs && langs.length" class="card">
          <div class="card-h">Языки <span class="muted">строк кода без пустых</span></div>
          <div class="card-b">
            <div v-for="l in langs" :key="l.lang" class="lrow" :title="`${l.lang}: ${fmtN(l.code)} строк, ${l.files} ${plural(l.files, 'файл', 'файла', 'файлов')}`">
              <span class="ln">{{ l.lang }}</span>
              <span class="track"><span class="fill" :style="{ width: l.w + '%' }" /></span>
              <span class="lv mono">{{ fmtN(l.code) }}</span>
              <span class="lp dim">{{ l.pct }}%</span>
            </div>
          </div>
        </section>
        <section v-if="cs && cs.biggest.length" class="card">
          <div class="card-h">Самые большие файлы <span class="muted">кандидаты на разбиение</span></div>
          <div class="card-b tbl">
            <div v-for="b in cs.biggest.slice(0, 12)" :key="b.path" class="tr" @click="emit('open-file', b.path)">
              <span class="ellipsis grow mono">{{ b.rel }}</span>
              <span class="badge">{{ b.lang }}</span>
              <span class="num mono">{{ fmtN(b.lines) }}</span>
            </div>
          </div>
        </section>
      </div>

      <div v-if="hasGit" class="two">
        <section class="card">
          <div class="card-h">Горячие файлы <span class="muted">чаще всего менялись</span></div>
          <div class="card-b tbl">
            <div class="tr th"><span class="grow">Файл</span><span class="num">коммитов</span><span class="num2">строк ±</span><span class="num2">изменён</span></div>
            <div v-for="h in ga!.hotFiles.slice(0, 15)" :key="h.path" class="tr" :title="h.path" @click="openRepoFile(h.path)">
              <span class="ellipsis grow mono">{{ h.path }}</span>
              <span class="num mono">{{ h.commits }}</span>
              <span class="num2 mono dim">+{{ fmtN(h.adds) }} −{{ fmtN(h.dels) }}</span>
              <span class="num2 dim">{{ relTime(h.last) }}</span>
            </div>
          </div>
        </section>
        <section class="card">
          <div class="card-h">Авторы <span class="muted">{{ ga!.authors.length }}</span></div>
          <div class="card-b tbl">
            <div v-for="a in ga!.authors.slice(0, 12)" :key="a.email" class="tr nohov" :title="`${a.email}\n${fullDate(a.first)} — ${fullDate(a.last)}`">
              <span class="an-name ellipsis">{{ a.name }}</span>
              <span class="track"><span class="fill" :style="{ width: (a.commits / authorMax) * 100 + '%' }" /></span>
              <span class="num mono">{{ fmtN(a.commits) }}</span>
              <span class="num2 dim">{{ Math.round((a.commits / ga!.totalCommits) * 100) }}%</span>
            </div>
            <div v-if="ga!.authors.length === 1" class="dim cap">Вы единственный автор. С {{ fullDate(ga!.authors[0].first).split(",")[0] }} добавлено {{ fmtN(ga!.authors[0].adds) }} строк.</div>
          </div>
        </section>
      </div>
    </template>
  </div>
</template>

<style scoped>
.an { height: 100%; overflow-y: auto; padding: 14px 20px 24px; display: flex; flex-direction: column; gap: 14px; }
.toolbar { display: flex; align-items: center; gap: 10px; }
.tiles { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 10px; }
.tile { background: var(--panel); border: 1px solid var(--border); border-radius: 8px; padding: 12px 14px; }
.tile .k { font-size: 11.5px; color: var(--text-2); }
.tile .v { font-size: 22px; white-space: nowrap; font-weight: 650; margin: 2px 0; font-variant-numeric: tabular-nums; }
.tile .v small { font-size: 13px; font-weight: 500; color: var(--text-2); }
.tile .s { font-size: 11.5px; color: var(--text-3); }
.nogit { display: flex; align-items: center; gap: 8px; padding: 12px 14px; color: var(--text-2); }
.two { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; }
@media (max-width: 1150px) { .two { grid-template-columns: 1fr; } }
.two > .card { min-width: 0; }
.sel { width: 190px; height: 24px; font-size: 12px; font-weight: 400; }
.seg { display: flex; background: var(--bg); border: 1px solid var(--border); border-radius: 6px; padding: 2px; font-weight: 400; }
.seg button { border: 0; background: none; padding: 2px 9px; border-radius: 4px; cursor: pointer; color: var(--text-2); font-size: 11.5px; }
.seg button.on { background: var(--active); color: var(--text); }
.cap { font-size: 12px; color: var(--text-2); margin-top: 8px; }
.lrow { display: flex; align-items: center; gap: 10px; padding: 3px 0; font-size: 12.5px; }
.ln { width: 96px; flex: none; }
.track { flex: 1; height: 8px; background: var(--heat-0); border-radius: 4px; overflow: hidden; min-width: 40px; }
.fill { display: block; height: 100%; background: var(--viz-1); border-radius: 0 4px 4px 0; }
.lv { width: 70px; text-align: right; font-size: 12px; flex: none; }
.lp { width: 44px; text-align: right; font-size: 11.5px; flex: none; }
.tbl { padding: 6px 8px 10px; }
.tr { display: flex; align-items: center; gap: 10px; padding: 4px 6px; border-radius: 4px; cursor: pointer; font-size: 12.5px; }
.tr:hover:not(.th):not(.nohov) { background: var(--hover); }
.tr.th { cursor: default; font-size: 11px; color: var(--text-3); text-transform: uppercase; letter-spacing: .03em; }
.tr.nohov { cursor: default; }
.tr .mono { font-size: 12px; }
.num { width: 64px; text-align: right; flex: none; }
.num2 { width: 118px; text-align: right; flex: none; font-size: 11.5px; white-space: nowrap; }
.an-name { width: 150px; flex: none; }
</style>
