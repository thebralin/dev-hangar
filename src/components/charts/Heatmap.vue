<script setup lang="ts">
import { computed, ref } from "vue";
import type { Day } from "../../lib/api";
import { plural } from "../../lib/format";

const props = defineProps<{ days: Day[]; weeks?: number; end?: Date }>();

const CELL = 12;
const GAP = 3;
const LEFT = 26;
const TOP = 16;

function key(d: Date) {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const dd = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${dd}`;
}

const byDate = computed(() => {
  const m = new Map<string, Day>();
  for (const d of props.days) m.set(d.date, d);
  return m;
});

interface Cell { date: string; x: number; y: number; level: number; day?: Day; future: boolean }

const grid = computed(() => {
  const weeks = props.weeks ?? 53;
  const now = new Date();
  const today = props.end && props.end < now ? new Date(props.end) : now;
  today.setHours(0, 0, 0, 0);
  // start on the Monday `weeks-1` weeks ago
  const start = new Date(today);
  const dow = (today.getDay() + 6) % 7; // Mon=0
  start.setDate(today.getDate() - dow - (weeks - 1) * 7);

  // quantile-based levels so one huge day doesn't flatten the rest
  const counts = props.days.map((d) => d.commits).filter((c) => c > 0).sort((a, b) => a - b);
  const q = (p: number) => counts[Math.min(counts.length - 1, Math.floor(p * counts.length))] ?? 1;
  const t = [q(0.25), q(0.5), q(0.75)];
  const level = (c: number) => (c <= 0 ? 0 : c <= t[0] ? 1 : c <= t[1] ? 2 : c <= t[2] ? 3 : 4);

  const cells: Cell[] = [];
  const months: { x: number; label: string }[] = [];
  let lastMonth = -1;
  for (let w = 0; w < weeks; w++) {
    for (let d = 0; d < 7; d++) {
      const date = new Date(start);
      date.setDate(start.getDate() + w * 7 + d);
      const k = key(date);
      const day = byDate.value.get(k);
      if (d === 0 && date.getMonth() !== lastMonth) {
        lastMonth = date.getMonth();
        months.push({ x: LEFT + w * (CELL + GAP), label: date.toLocaleDateString("ru-RU", { month: "short" }).replace(".", "") });
      }
      cells.push({ date: k, x: LEFT + w * (CELL + GAP), y: TOP + d * (CELL + GAP), level: level(day?.commits ?? 0), day, future: date > today });
    }
  }
  const active = cells.filter((c) => c.day && !c.future).length;
  const commits = cells.reduce((s, c) => s + (c.future ? 0 : c.day?.commits ?? 0), 0);
  return { cells, months: months.filter((m, i) => i === 0 || m.x - months[i - 1].x > 30), width: LEFT + weeks * (CELL + GAP), active, commits };
});

const height = TOP + 7 * (CELL + GAP);
const hover = ref<Cell | null>(null);
const tip = ref({ x: 0, y: 0 });
const box = ref<HTMLElement | null>(null);

function onEnter(e: MouseEvent, c: Cell) {
  hover.value = c;
  const r = box.value!.getBoundingClientRect();
  tip.value = { x: e.clientX - r.left, y: e.clientY - r.top };
}

function fmtDate(k: string) {
  const [y, m, d] = k.split("-").map(Number);
  return new Date(y, m - 1, d).toLocaleDateString("ru-RU", { weekday: "short", day: "numeric", month: "long", year: "numeric" });
}
</script>

<template>
  <div ref="box" class="hm">
    <div class="scroll">
      <svg :width="grid.width" :height="height" role="img" aria-label="Коммиты по дням">
        <text v-for="m in grid.months" :key="m.x" :x="m.x" :y="10" class="lbl">{{ m.label }}</text>
        <text v-for="(d, i) in ['Пн', '', 'Ср', '', 'Пт', '', '']" :key="i" :x="0" :y="TOP + i * (CELL + GAP) + 10" class="lbl">{{ d }}</text>
        <rect
          v-for="c in grid.cells.filter((c) => !c.future)"
          :key="c.date"
          :x="c.x" :y="c.y" :width="CELL" :height="CELL" rx="2.5"
          :class="['cell', 'l' + c.level, { on: hover?.date === c.date }]"
          @mouseenter="onEnter($event, c)"
          @mouseleave="hover = null"
        />
      </svg>
    </div>
    <div class="foot">
      <span class="dim">{{ grid.commits }} {{ plural(grid.commits, 'коммит', 'коммита', 'коммитов') }} за период, активных дней: {{ grid.active }}</span>
      <span class="spacer" />
      <span class="dim">меньше</span>
      <i v-for="l in 5" :key="l" :class="['sw', 'l' + (l - 1)]" />
      <span class="dim">больше</span>
    </div>
    <div v-if="hover" class="tip" :style="{ left: Math.min(tip.x + 12, grid.width - 200) + 'px', top: tip.y - 44 + 'px' }">
      <div class="th">{{ fmtDate(hover.date) }}</div>
      <div v-if="hover.day">{{ hover.day.commits }} комм. · <span class="plus">+{{ hover.day.adds }}</span> <span class="minus">−{{ hover.day.dels }}</span></div>
      <div v-else class="dim">нет коммитов</div>
    </div>
  </div>
</template>

<style scoped>
.hm { position: relative; }
.scroll { overflow-x: auto; padding-bottom: 2px; }
svg { display: block; }
.lbl { fill: var(--text-3); font-size: 10px; font-family: var(--sans); }
.cell { stroke: transparent; stroke-width: 1.5; }
.cell.on { stroke: var(--text); }
.l0 { fill: var(--heat-0); background: var(--heat-0); }
.l1 { fill: var(--heat-1); background: var(--heat-1); }
.l2 { fill: var(--heat-2); background: var(--heat-2); }
.l3 { fill: var(--heat-3); background: var(--heat-3); }
.l4 { fill: var(--heat-4); background: var(--heat-4); }
.foot { display: flex; align-items: center; gap: 4px; font-size: 11.5px; margin-top: 6px; }
.sw { display: inline-block; width: 11px; height: 11px; border-radius: 2.5px; }
.tip {
  position: absolute; pointer-events: none; z-index: 5; white-space: nowrap;
  background: var(--panel); border: 1px solid var(--border-2); border-radius: 6px; padding: 6px 9px;
  box-shadow: 0 6px 18px rgba(0,0,0,.3); font-size: 12px;
}
.th { font-weight: 600; }
.plus { color: var(--text-2); }
.minus { color: var(--text-2); }
</style>
