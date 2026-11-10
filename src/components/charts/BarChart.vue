<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";

export interface BarItem {
  label: string;
  value: number;
  /** optional downward value (diverging chart) */
  neg?: number;
  tip: string;
}

const props = withDefaults(
  defineProps<{ items: BarItem[]; height?: number; labelEvery?: number; posLabel?: string; negLabel?: string }>(),
  { height: 170, labelEvery: 1 },
);

const box = ref<HTMLElement | null>(null);
const width = ref(600);
let ro: ResizeObserver | null = null;
onMounted(() => {
  ro = new ResizeObserver(([e]) => (width.value = Math.max(200, e.contentRect.width)));
  if (box.value) ro.observe(box.value);
});
onBeforeUnmount(() => ro?.disconnect());

const PAD_L = 38;
const PAD_B = 20;
const PAD_T = 8;

const diverging = computed(() => props.items.some((i) => (i.neg ?? 0) > 0));
const maxPos = computed(() => Math.max(1, ...props.items.map((i) => i.value)));
const maxNeg = computed(() => (diverging.value ? Math.max(1, ...props.items.map((i) => i.neg ?? 0)) : 0));

function nice(v: number) {
  const p = Math.pow(10, Math.floor(Math.log10(v)));
  const n = v / p;
  return (n <= 1 ? 1 : n <= 2 ? 2 : n <= 5 ? 5 : 10) * p;
}
const topMax = computed(() => nice(maxPos.value));
const botMax = computed(() => (diverging.value ? nice(maxNeg.value) : 0));

const plotH = computed(() => props.height - PAD_B - PAD_T);
const baseY = computed(() => PAD_T + plotH.value * (topMax.value / (topMax.value + botMax.value)));
const plotW = computed(() => width.value - PAD_L - 4);
const slot = computed(() => plotW.value / Math.max(1, props.items.length));
const barW = computed(() => Math.max(1, Math.min(28, slot.value - Math.max(2, slot.value * 0.25))));

function yUp(v: number) {
  return (v / topMax.value) * (baseY.value - PAD_T);
}
function yDown(v: number) {
  return botMax.value ? (v / botMax.value) * (PAD_T + plotH.value - baseY.value) : 0;
}

/** bar with rounded data-end (top for up bars, bottom for down bars) */
function barPath(x: number, w: number, h: number, up: boolean) {
  if (h <= 0) return "";
  const r = Math.min(3, w / 2, h);
  const b = baseY.value;
  if (up) {
    const t = b - h;
    return `M${x},${b}V${t + r}Q${x},${t} ${x + r},${t}H${x + w - r}Q${x + w},${t} ${x + w},${t + r}V${b}Z`;
  }
  const t = b + h;
  return `M${x},${b}V${t - r}Q${x},${t} ${x + r},${t}H${x + w - r}Q${x + w},${t} ${x + w},${t - r}V${b}Z`;
}

function fmt(n: number) {
  if (n >= 1000000) return (n / 1000000).toFixed(n >= 10000000 ? 0 : 1) + "M";
  if (n >= 1000) return (n / 1000).toFixed(n >= 10000 ? 0 : 1) + "k";
  return String(n);
}

const grid = computed(() => {
  const g: { y: number; label: string }[] = [];
  for (const f of [0.5, 1]) g.push({ y: baseY.value - yUp(topMax.value * f), label: fmt(topMax.value * f) });
  if (diverging.value) g.push({ y: baseY.value + yDown(botMax.value), label: "−" + fmt(botMax.value) });
  return g;
});

const hover = ref<number | null>(null);
const tipPos = ref({ x: 0, y: 0 });
function onMove(e: MouseEvent, i: number) {
  hover.value = i;
  const r = box.value!.getBoundingClientRect();
  tipPos.value = { x: e.clientX - r.left, y: e.clientY - r.top };
}
</script>

<template>
  <div ref="box" class="bc">
    <div v-if="diverging && (posLabel || negLabel)" class="legend">
      <span><i class="sw pos" /> {{ posLabel }}</span>
      <span><i class="sw neg" /> {{ negLabel }}</span>
    </div>
    <svg :width="width" :height="height" role="img">
      <g class="grid">
        <line v-for="g in grid" :key="g.label" :x1="PAD_L" :x2="width - 4" :y1="g.y" :y2="g.y" />
        <text v-for="g in grid" :key="'t' + g.label" :x="PAD_L - 6" :y="g.y + 3" text-anchor="end">{{ g.label }}</text>
      </g>
      <line class="base" :x1="PAD_L" :x2="width - 4" :y1="baseY" :y2="baseY" />
      <g v-for="(it, i) in items" :key="i">
        <path class="bar pos" :class="{ dim: hover != null && hover !== i }" :d="barPath(PAD_L + i * slot + (slot - barW) / 2, barW, yUp(it.value), true)" />
        <path v-if="it.neg" class="bar neg" :class="{ dim: hover != null && hover !== i }" :d="barPath(PAD_L + i * slot + (slot - barW) / 2, barW, yDown(it.neg), false)" />
        <text v-if="i % labelEvery === 0" class="xl" :x="PAD_L + i * slot + slot / 2" :y="height - 5" text-anchor="middle">{{ it.label }}</text>
        <rect class="hit" :x="PAD_L + i * slot" :y="0" :width="slot" :height="height - PAD_B" @mousemove="onMove($event, i)" @mouseleave="hover = null" />
      </g>
    </svg>
    <div v-if="hover != null && items[hover]" class="tip" :style="{ left: Math.min(tipPos.x + 12, width - 190) + 'px', top: Math.max(0, tipPos.y - 40) + 'px' }">
      <div v-for="(l, k) in items[hover].tip.split('\n')" :key="k" :class="{ th: k === 0 }">{{ l }}</div>
    </div>
  </div>
</template>

<style scoped>
.bc { position: relative; width: 100%; }
.legend { display: flex; gap: 14px; font-size: 11.5px; color: var(--text-2); margin-bottom: 4px; }
.sw { display: inline-block; width: 10px; height: 10px; border-radius: 3px; vertical-align: -1px; margin-right: 4px; }
.sw.pos { background: var(--viz-1); }
.sw.neg { background: var(--viz-neg); }
svg { display: block; overflow: visible; }
.grid line { stroke: var(--border); stroke-dasharray: 2 3; }
.grid text, .xl { fill: var(--text-3); font-size: 10.5px; font-family: var(--sans); }
.base { stroke: var(--border-2); }
.bar { transition: opacity .1s; }
.bar.pos { fill: var(--viz-1); }
.bar.neg { fill: var(--viz-neg); }
.bar.dim { opacity: .45; }
.hit { fill: transparent; cursor: default; }
.tip {
  position: absolute; pointer-events: none; z-index: 5; min-width: 150px;
  background: var(--panel); border: 1px solid var(--border-2); border-radius: 6px; padding: 6px 9px;
  box-shadow: 0 6px 18px rgba(0,0,0,.3); font-size: 12px; color: var(--text); white-space: nowrap;
}
.tip .th { font-weight: 600; margin-bottom: 2px; }
</style>
