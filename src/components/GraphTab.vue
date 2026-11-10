<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref, shallowRef, watch } from "vue";
import {
  forceCollide, forceLink, forceManyBody, forceSimulation, forceX, forceY,
  type Simulation, type SimulationLinkDatum, type SimulationNodeDatum,
} from "d3-force";
import Icon from "./Icon.vue";
import { api, baseName, type CodeGraph, type GraphEdge, type GraphNode } from "../lib/api";
import { state, toast, type ProjectEntry } from "../lib/store";
import { plural } from "../lib/format";

const props = defineProps<{ entry: ProjectEntry }>();
const emit = defineEmits<{ "open-file": [path: string, line?: number] }>();

interface N extends SimulationNodeDatum {
  id: string; g: GraphNode; name: string; r: number; slot: number; group: string; cyc: boolean;
  // "layers" layout: fixed position, dot radius, label width
  lx: number; ly: number; lr: number; lw: number;
}
interface L extends SimulationLinkDatum<N> { source: N; target: N; kind: string; count: number; cyc: boolean }

// options survive tab switches; graph + layout cached per project for the session
const opts: {
  external: boolean; orphans: boolean; cycles: boolean; colorBy: "folder" | "lang"; sizeBy: "links" | "lines"; depth: number;
} = ((globalThis as any).__devhangarGraphOpts ??= reactive({
  external: false, orphans: true, cycles: true, colorBy: "folder", sizeBy: "links", depth: 0,
}));
type Cached = { graph: CodeGraph; pos: Map<string, { x: number; y: number }>; view: { x: number; y: number; k: number } | null };
const cache: Map<string, Cached> = ((globalThis as any).__devhangarGraph ??= new Map());

const raw = shallowRef<CodeGraph | null>(null);
const loading = ref(false);
const selected = ref<string | null>(null);
const hovered = shallowRef<N | null>(null);
const legendHover = ref<string | null>(null);
const query = ref("");
const tip = reactive({ x: 0, y: 0 });
const visStats = reactive({ nodes: 0, edges: 0 });

const wrap = ref<HTMLDivElement>();
const canvas = ref<HTMLCanvasElement>();
const searchEl = ref<HTMLInputElement>();
let ctx: CanvasRenderingContext2D | null = null;
let W = 0, H = 0;
const view = { x: 0, y: 0, k: 1 };
let autoFit = true;

let sim: Simulation<N, L> | null = null;
const nodeObjs = new Map<string, N>();
let nodes: N[] = [];
let links: L[] = [];
let adj = new Map<string, Set<N>>();

// ---------- data ----------

async function load(force = false) {
  const c = cache.get(props.entry.path);
  if (c && !force) {
    for (const [id, p] of c.pos) nodeObjs.set(id, { id, x: p.x, y: p.y } as N);
    if (c.view) { Object.assign(view, c.view); autoFit = false; }
    raw.value = c.graph;
    return;
  }
  loading.value = true;
  try {
    const g = await api.codeGraph(props.entry.path);
    cache.set(props.entry.path, { graph: g, pos: new Map(), view: null });
    raw.value = g;
  } catch (e) {
    toast(String(e), "error");
  } finally {
    loading.value = false;
  }
}

const isPkg = (id: string) => id.startsWith("pkg:");

const full = computed(() => {
  const g = raw.value;
  if (!g) return null;
  const byId = new Map(g.nodes.map((n) => [n.id, n]));
  const out = new Map<string, GraphEdge[]>();
  const inc = new Map<string, GraphEdge[]>();
  const deg = new Map<string, number>();
  for (const e of g.edges) {
    if (!out.has(e.source)) out.set(e.source, []);
    if (!inc.has(e.target)) inc.set(e.target, []);
    out.get(e.source)!.push(e);
    inc.get(e.target)!.push(e);
    if (!isPkg(e.target)) {
      deg.set(e.source, (deg.get(e.source) ?? 0) + 1);
      deg.set(e.target, (deg.get(e.target) ?? 0) + 1);
    }
  }
  // strongly connected components (Tarjan) → import cycles
  let index = 0;
  const idx = new Map<string, number>();
  const low = new Map<string, number>();
  const onStack = new Set<string>();
  const stack: string[] = [];
  const comp = new Map<string, number>();
  const compSize: number[] = [];
  const strong = (v: string) => {
    idx.set(v, index);
    low.set(v, index++);
    stack.push(v);
    onStack.add(v);
    for (const e of out.get(v) ?? []) {
      if (e.kind !== "import" || isPkg(e.target)) continue;
      const w = e.target;
      if (!idx.has(w)) {
        strong(w);
        low.set(v, Math.min(low.get(v)!, low.get(w)!));
      } else if (onStack.has(w)) {
        low.set(v, Math.min(low.get(v)!, idx.get(w)!));
      }
    }
    if (low.get(v) === idx.get(v)) {
      let w: string, size = 0;
      do {
        w = stack.pop()!;
        onStack.delete(w);
        comp.set(w, compSize.length);
        size++;
      } while (w !== v);
      compSize.push(size);
    }
  };
  for (const n of g.nodes) if (!n.external && !idx.has(n.id)) strong(n.id);
  const inCycle = (id: string) => (compSize[comp.get(id) ?? -1] ?? 0) > 1;
  const sameCycle = (a: string, b: string) => inCycle(a) && comp.get(a) === comp.get(b);
  const cycles = compSize.filter((s) => s > 1).length;
  const cycleFiles = g.nodes.filter((n) => inCycle(n.id)).length;
  const compKey = (id: string) => (inCycle(id) ? `scc:${comp.get(id)}` : id);
  return { byId, out, inc, deg, inCycle, sameCycle, compKey, cycles, cycleFiles, local: g.nodes.filter((n) => !n.external).length };
});

function folderGroup(id: string) {
  const parts = id.split("/");
  parts.pop();
  return parts.length ? parts.slice(0, 2).join("/") : "(корень)";
}
const groupOf = (n: GraphNode) => (n.external ? "внешние пакеты" : opts.colorBy === "lang" ? n.lang : folderGroup(n.id));

// fixed slots from the whole graph, so filters never repaint survivors
const SLOTS = 7;
const groups = computed(() => {
  const counts = new Map<string, number>();
  for (const n of raw.value?.nodes ?? []) if (!n.external) counts.set(groupOf(n), (counts.get(groupOf(n)) ?? 0) + 1);
  const sorted = [...counts].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
  const slot = new Map(sorted.slice(0, SLOTS).map(([g], i) => [g, i]));
  const rest = sorted.slice(SLOTS);
  const legend = sorted.slice(0, SLOTS).map(([name, count], i) => ({ name, count, slot: i, keys: [name] }));
  if (rest.length) legend.push({ name: `другое (${rest.length})`, count: rest.reduce((s, x) => s + x[1], 0), slot: -1, keys: rest.map((x) => x[0]) });
  return { slot, legend };
});

function radius(n: GraphNode) {
  if (n.external) return 3 + Math.min(10, Math.sqrt(full.value?.inc.get(n.id)?.length ?? 0) * 1.6);
  if (opts.sizeBy === "lines") return Math.min(22, 3 + Math.sqrt(n.lines) / 3.2);
  return Math.min(20, 3.5 + Math.sqrt(full.value?.deg.get(n.id) ?? 0) * 1.9);
}

function applyAttrs() {
  const f = full.value;
  for (const n of nodes) {
    n.group = groupOf(n.g);
    n.slot = n.g.external ? -1 : groups.value.slot.get(n.group) ?? -1;
    n.r = radius(n.g);
    n.cyc = !!f?.inCycle(n.id);
  }
  for (const l of links) l.cyc = !!f?.sameCycle(l.source.id, l.target.id);
  sim?.force("collide", forceCollide<N>((n) => n.r + 2));
  layoutLayers();
  requestDraw();
}

// ---------- layers layout ----------
// Columns by dependency height: the rightmost column holds files that import nothing local,
// every other file sits one column left of its deepest import. Import cycles share a column.

const ROW = 20, FONT = 12, COL_GAP = 150, GROUP_GAP = 10;
let layerCols: { x: number; label: string }[] = [];
let headerY = 0;
const textW = new Map<string, number>();

function measure(s: string) {
  let w = textW.get(s);
  if (w == null) {
    ctx!.font = `${FONT}px ${colors.sans}`;
    w = ctx!.measureText(s).width;
    textW.set(s, w);
  }
  return w;
}

function layoutLayers() {
  const f = full.value;
  if (!f || !ctx) return;
  const key = (n: N) => f.compKey(n.id);
  const succ = new Map<string, Set<string>>();
  for (const l of links) {
    if (l.source.g.external || l.target.g.external) continue;
    const a = key(l.source), b = key(l.target);
    if (a === b) continue;
    if (!succ.has(a)) succ.set(a, new Set());
    succ.get(a)!.add(b);
  }
  const hgt = new Map<string, number>();
  const onPath = new Set<string>();
  const height = (k: string): number => {
    const known = hgt.get(k);
    if (known != null) return known;
    onPath.add(k);
    let h = 0;
    for (const w of succ.get(k) ?? []) if (!onPath.has(w)) h = Math.max(h, height(w) + 1);
    onPath.delete(k);
    hgt.set(k, h);
    return h;
  };

  const linked = nodes.filter((n) => !n.g.external && (adj.get(n.id)?.size ?? 0) > 0);
  const maxH = linked.reduce((m, n) => Math.max(m, height(key(n))), 0);
  const cols: N[][] = Array.from({ length: maxH + 1 }, () => []);
  const labels = cols.map((_, i) => (maxH - i === 0 ? "база" : `слой ${maxH - i}`));
  for (const n of linked) cols[maxH - height(key(n))].push(n);
  const ext = nodes.filter((n) => n.g.external);
  if (ext.length) { cols.push(ext); labels.push("пакеты"); }
  const lone = nodes.filter((n) => !n.g.external && !(adj.get(n.id)?.size));
  if (lone.length) { cols.push(lone); labels.push("без связей"); }

  // order inside a column: folder first (keeps groups together), then barycenter of neighbours
  const rank = (n: N) => (n.slot >= 0 ? n.slot : SLOTS);
  const pos = new Map<N, number>();
  const place = (col: N[]) => col.forEach((n, i) => pos.set(n, col.length > 1 ? i / (col.length - 1) : 0.5));
  for (const col of cols) {
    col.sort((a, b) => rank(a) - rank(b) || a.group.localeCompare(b.group) || a.name.localeCompare(b.name));
    place(col);
  }
  const sweep = (col: N[]) => {
    const bary = new Map<N, number>();
    for (const n of col) {
      let s = 0, c = 0;
      for (const m of adj.get(n.id) ?? []) if (pos.has(m)) { s += pos.get(m)!; c++; }
      bary.set(n, c ? s / c : pos.get(n)!);
    }
    col.sort((a, b) => rank(a) - rank(b) || a.group.localeCompare(b.group) || bary.get(a)! - bary.get(b)!);
    place(col);
  };
  for (let it = 0; it < 4; it++) {
    const order = it % 2 ? [...cols].reverse() : cols;
    for (const col of order) sweep(col);
  }

  let x = 0, top = 0;
  layerCols = [];
  cols.forEach((col, ci) => {
    let y = 0, width = 0;
    const ys: number[] = [];
    col.forEach((n, i) => {
      if (i && n.group !== col[i - 1].group) y += GROUP_GAP;
      ys.push(y);
      y += ROW;
      n.lr = Math.min(6.5, 2.5 + n.r * 0.25);
      n.lw = 6 + measure(n.name);
      width = Math.max(width, n.lr + n.lw);
    });
    const shift = -y / 2 + ROW / 2;
    col.forEach((n, i) => { n.lx = x; n.ly = ys[i] + shift; });
    top = Math.min(top, shift - ROW / 2);
    layerCols.push({ x, label: labels[ci] });
    x += width + COL_GAP;
  });
  headerY = top - 14;
}

// blend: 0 = force layout, 1 = layers; positions are interpolated while switching
let blend = state.settings.graphLayout === "layers" ? 1 : 0;
const X = (n: N) => (blend ? n.x! + (n.lx - n.x!) * blend : n.x!);
const Y = (n: N) => (blend ? n.y! + (n.ly - n.y!) * blend : n.y!);
const R = (n: N) => (blend ? n.r + (n.lr - n.r) * blend : n.r);
const layered = () => blend > 0.5;

let blendAnim = 0;
function setLayout(mode: "force" | "layers") {
  if (state.settings.graphLayout === mode) return;
  state.settings.graphLayout = mode;
  cancelAnimationFrame(blendAnim);
  cancelAnimationFrame(viewAnim);
  autoFit = true;
  const from = blend, to = mode === "layers" ? 1 : 0;
  if (!nodes.length || !W) { blend = to; requestDraw(); return; }
  const v0 = { ...view };
  blend = to;
  const v1 = fitView(nodes);
  blend = from;
  let t0 = -1;
  const step = (t: number) => {
    if (t0 < 0) t0 = t;
    const p = Math.min(1, Math.max(0, (t - t0) / 500));
    const e = p < 0.5 ? 4 * p * p * p : 1 - Math.pow(-2 * p + 2, 3) / 2;
    blend = from + (to - from) * e;
    view.x = v0.x + (v1.x - v0.x) * e;
    view.y = v0.y + (v1.y - v0.y) * e;
    view.k = v0.k * Math.pow(v1.k / v0.k, e);
    draw();
    if (p < 1) blendAnim = requestAnimationFrame(step);
    else {
      const n = selected.value ? nodeObjs.get(selected.value) : null;
      if (n && nodes.includes(n)) centerOn(n);
    }
  };
  blendAnim = requestAnimationFrame(step);
}

function visibleIds(): Set<string> {
  const f = full.value!;
  const allowed = (id: string) => opts.external || !isPkg(id);
  let ids = new Set([...f.byId.keys()].filter(allowed));
  if (opts.depth > 0 && selected.value && ids.has(selected.value)) {
    const keep = new Set([selected.value]);
    let frontier = [selected.value];
    for (let d = 0; d < opts.depth; d++) {
      const next: string[] = [];
      for (const id of frontier) {
        for (const e of f.out.get(id) ?? []) if (allowed(e.target) && !keep.has(e.target)) { keep.add(e.target); next.push(e.target); }
        for (const e of f.inc.get(id) ?? []) if (!keep.has(e.source)) { keep.add(e.source); next.push(e.source); }
      }
      frontier = next;
    }
    return keep;
  }
  if (!opts.orphans) {
    const linked = new Set<string>();
    for (const e of raw.value!.edges) if (ids.has(e.source) && ids.has(e.target)) { linked.add(e.source); linked.add(e.target); }
    ids = linked;
  }
  return ids;
}

function rebuild() {
  const f = full.value;
  if (!f) return;
  const ids = visibleIds();
  const fresh = ![...ids].some((id) => nodeObjs.get(id)?.x != null);
  nodes = [];
  for (const id of ids) {
    let n = nodeObjs.get(id);
    if (!n || !n.g) {
      n = { ...(n ?? {}), id, g: f.byId.get(id)!, name: isPkg(id) ? id.slice(4) : baseName(id), r: 4, slot: -1, group: "", cyc: false, lx: 0, ly: 0, lr: 4, lw: 0 } as N;
      nodeObjs.set(id, n);
    }
    n.g = f.byId.get(id)!;
    nodes.push(n);
  }
  links = raw.value!.edges
    .filter((e) => ids.has(e.source) && ids.has(e.target))
    .map((e) => ({ source: nodeObjs.get(e.source)!, target: nodeObjs.get(e.target)!, kind: e.kind, count: e.count, cyc: false }));
  adj = new Map(nodes.map((n) => [n.id, new Set<N>()]));
  for (const l of links) {
    adj.get(l.source.id)!.add(l.target);
    adj.get(l.target.id)!.add(l.source);
  }
  // new nodes start next to an already placed neighbour (or on a spiral) so the layout doesn't explode
  const spread = Math.sqrt(nodes.length) * 14;
  nodes.forEach((n, i) => {
    if (n.x != null && n.y != null) return;
    const nb = [...(adj.get(n.id) ?? [])].find((m) => m.x != null);
    const a = i * 2.39996;
    if (nb) { n.x = nb.x! + Math.cos(a) * 20; n.y = nb.y! + Math.sin(a) * 20; }
    else { const r = spread * Math.sqrt((i + 0.5) / nodes.length); n.x = Math.cos(a) * r; n.y = Math.sin(a) * r; }
    n.vx = n.vy = 0;
  });
  visStats.nodes = nodes.length;
  visStats.edges = links.length;

  sim?.stop();
  const big = nodes.length > 800;
  const small = nodes.length < 300;
  let ticks = 0;
  sim = forceSimulation<N, L>(nodes)
    .force("link", forceLink<N, L>(links).distance((l) => (small ? 45 : 28) + l.source.r + l.target.r))
    .force("charge", forceManyBody<N>().strength(big ? -60 : small ? -260 : -120).distanceMax(big ? 350 : 700).theta(big ? 1.1 : 0.9))
    .force("x", forceX<N>(0).strength(0.045))
    .force("y", forceY<N>(0).strength(0.045))
    .alphaDecay(big ? 0.035 : 0.025)
    .on("tick", () => {
      // the layers view doesn't move with the simulation; it only feeds the force view
      if (blend === 1) return;
      // keep the camera on the graph while it unfolds, until the user takes over
      if (autoFit && !selected.value && ++ticks % 4 === 0) fit(false);
      requestDraw();
    })
    .on("end", () => { if (autoFit && !selected.value && blend === 0) fit(true); saveLayout(); });
  applyAttrs();
  if (blend === 1 && autoFit) fit(false);
  if (fresh) {
    sim.stop();
    const pre = Math.min(120, Math.floor(60000 / Math.max(1, nodes.length)));
    for (let i = 0; i < pre; i++) sim.tick();
    if (autoFit) fit(false);
    sim.alpha(0.6).restart();
  } else {
    sim.alpha(0.35).restart();
  }
}

function saveLayout() {
  const c = cache.get(props.entry.path);
  if (!c) return;
  for (const n of nodeObjs.values()) if (n.x != null) c.pos.set(n.id, { x: n.x, y: n.y! });
  c.view = autoFit ? null : { ...view };
}

// ---------- view ----------

function bounds(list: N[]) {
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const n of list) {
    const x = X(n), y = Y(n), r = R(n);
    x0 = Math.min(x0, x - r); y0 = Math.min(y0, y - r);
    x1 = Math.max(x1, x + r + n.lw * blend); y1 = Math.max(y1, y + r);
  }
  if (blend > 0 && list === nodes) y0 = Math.min(y0, headerY - FONT);
  return { x0, y0, x1, y1 };
}

function fit(animate = true, list = nodes) {
  if (!list.length || !W) return;
  const target = fitView(list);
  animate ? animateView(target) : Object.assign(view, target);
  requestDraw();
}

function fitView(list: N[]) {
  const b = bounds(list);
  const kAll = Math.min(W / (b.x1 - b.x0 + 120), H / (b.y1 - b.y0 + 120));
  let k = kAll;
  // tall layer columns: rather fit the width at a readable scale and start from the top
  if (layered() && kAll < 0.6) k = Math.max(kAll, Math.min(1, W / (b.x1 - b.x0 + 120)));
  k = Math.min(2.2, Math.max(0.05, k));
  const target = { k, x: W / 2 - ((b.x0 + b.x1) / 2) * k, y: H / 2 - ((b.y0 + b.y1) / 2) * k };
  if (k > kAll * 1.01) target.y = 40 - b.y0 * k;
  return target;
}

function refit() {
  autoFit = true;
  fit(true);
}

function centerOn(n: N, k = Math.max(view.k, 1.1)) {
  animateView({ k, x: W / 2 - (X(n) + (n.lw / 2) * blend) * k, y: H / 2 - Y(n) * k });
}

let viewAnim = 0;
function animateView(to: { x: number; y: number; k: number }) {
  cancelAnimationFrame(viewAnim);
  const from = { ...view };
  let t0 = -1;
  const step = (t: number) => {
    // rAF timestamps can be earlier than performance.now(), so start the clock on the first frame
    if (t0 < 0) t0 = t;
    const p = Math.min(1, Math.max(0, (t - t0) / 350));
    const e = 1 - Math.pow(1 - p, 3);
    view.x = from.x + (to.x - from.x) * e;
    view.y = from.y + (to.y - from.y) * e;
    view.k = from.k + (to.k - from.k) * e;
    draw();
    if (p < 1) viewAnim = requestAnimationFrame(step);
  };
  viewAnim = requestAnimationFrame(step);
}

// ---------- drawing ----------

let colors = {
  bg: "#121316", text: "#ece8e2", text2: "#a8a39b", edge: "#383940", accent: "#ff8a3d", green: "#3fb950",
  red: "#f85149", yellow: "#d29922", other: "#6b7785", cats: [] as string[], sans: "sans-serif",
};
function readColors() {
  const s = getComputedStyle(document.documentElement);
  const v = (n: string) => s.getPropertyValue(n).trim();
  colors = {
    bg: v("--bg"), text: v("--text"), text2: v("--text-2"), edge: v("--border-2"), accent: v("--accent"),
    green: v("--green"), red: v("--red"), yellow: v("--yellow"), other: v("--cat-other"),
    cats: Array.from({ length: SLOTS }, (_, i) => v(`--cat-${i + 1}`)), sans: v("--sans") || "sans-serif",
  };
  requestDraw();
}

let raf = 0;
function requestDraw() {
  if (!raf) raf = requestAnimationFrame(() => { raf = 0; draw(); });
}

const matches = computed(() => {
  const q = query.value.trim().toLowerCase();
  if (!q || !raw.value) return null;
  return new Set(raw.value.nodes.filter((n) => (opts.external || !n.external) && n.id.toLowerCase().includes(q)).map((n) => n.id));
});

function arrowPath(c: CanvasRenderingContext2D, l: L, size: number) {
  const s = l.source, t = l.target;
  const dx = X(t) - X(s), dy = Y(t) - Y(s);
  const d = Math.hypot(dx, dy) || 1;
  const ux = dx / d, uy = dy / d;
  const ex = X(t) - ux * (R(t) + 1.5), ey = Y(t) - uy * (R(t) + 1.5);
  c.moveTo(ex, ey);
  c.lineTo(ex - ux * size - uy * size * 0.5, ey - uy * size + ux * size * 0.5);
  c.lineTo(ex - ux * size + uy * size * 0.5, ey - uy * size - ux * size * 0.5);
  c.closePath();
}

// straight line in the force view; in layers it bends into a curve from the end of the
// importer's label to the imported file's dot, or loops on the left inside a cycle column
function edgePath(c: CanvasRenderingContext2D, l: L) {
  const s = l.source, t = l.target, b = blend;
  const sy = Y(s), ty = Y(t);
  if (b < 0.01) { c.moveTo(X(s), sy); c.lineTo(X(t), ty); return; }
  if (t.lx > s.lx) {
    const x1 = X(s) + (R(s) + s.lw + 4) * b, x2 = X(t) - (R(t) + 2) * b;
    const d = Math.max(30, (x2 - x1) / 2) * b;
    c.moveTo(x1, sy);
    c.bezierCurveTo(x1 + d, sy, x2 - d, ty, x2, ty);
  } else {
    const x1 = X(s) - (R(s) + 2) * b, x2 = X(t) - (R(t) + 2) * b;
    const d = (24 + Math.abs(ty - sy) * 0.2) * b;
    c.moveTo(x1, sy);
    c.bezierCurveTo(x1 - d, sy, x2 - d, ty, x2, ty);
  }
}

function draw() {
  const c = ctx;
  if (!c || !W) return;
  const dpr = window.devicePixelRatio || 1;
  const k = view.k;
  const b = blend, lay = layered();
  c.setTransform(dpr, 0, 0, dpr, 0, 0);
  c.fillStyle = colors.bg;
  c.fillRect(0, 0, W, H);
  c.setTransform(dpr * k, 0, 0, dpr * k, dpr * view.x, dpr * view.y);

  // world-space viewport for culling
  const vx0 = -view.x / k, vy0 = -view.y / k, vx1 = (W - view.x) / k, vy1 = (H - view.y) / k;
  const onScreen = (n: N) => {
    const x = X(n), y = Y(n), r = R(n), h = Math.max(r, (ROW / 2) * b);
    return x + r + n.lw * b > vx0 && x - r < vx1 && y + h > vy0 && y - h < vy1;
  };

  const focus = hovered.value ?? (selected.value ? nodeObjs.get(selected.value) ?? null : null);
  const near = focus ? adj.get(focus.id) : null;
  const m = matches.value;
  const lg = legendHover.value ? new Set(groups.value.legend.find((x) => x.name === legendHover.value)?.keys ?? [legendHover.value]) : null;
  const lit = (n: N) =>
    (!focus || n === focus || !!near?.has(n)) && (!m || m.has(n.id)) && (!lg || lg.has(n.group));
  const dimming = !!(focus || m || lg);

  // layer column headers
  if (b > 0.02 && FONT * k > 4) {
    c.globalAlpha = b * 0.8;
    c.font = `600 ${FONT - 1}px ${colors.sans}`;
    c.textAlign = "left";
    c.textBaseline = "alphabetic";
    c.fillStyle = colors.text2;
    for (const col of layerCols) c.fillText(col.label.toUpperCase(), col.x - 6, headerY);
  }

  // edges, batched by style
  const px = 1 / Math.max(k, 0.45);
  const passive: L[] = [], cyc: L[] = [], ipc: L[] = [], outL: L[] = [], inL: L[] = [];
  for (const l of links) {
    if (focus && l.source === focus) outL.push(l);
    else if (focus && l.target === focus) inL.push(l);
    else if (l.kind === "ipc") ipc.push(l);
    else if (opts.cycles && l.cyc) cyc.push(l);
    else passive.push(l);
  }
  // in layers the direction is the reading order (left imports right), so no arrowheads
  const showArrows = k > 0.7 && !lay;
  const stroke = (list: L[], color: string, alpha: number, width: number, dash?: number[], arrows = showArrows) => {
    if (!list.length) return;
    c.globalAlpha = alpha;
    c.strokeStyle = color;
    c.fillStyle = color;
    c.lineWidth = width * px;
    c.setLineDash(dash ? dash.map((d) => d * px) : []);
    c.beginPath();
    for (const l of list) edgePath(c, l);
    c.stroke();
    c.setLineDash([]);
    if (arrows) {
      c.beginPath();
      for (const l of list) arrowPath(c, l, Math.min(7, 3.2 + width) * Math.max(1, px * 0.8));
      c.fill();
    }
  };
  // many curves overlap in layers, so the idle ones are fainter there
  const bgAlpha = dimming ? (lay ? 0.04 : 0.07) : lay ? 0.35 : 0.55;
  stroke(passive, colors.edge, bgAlpha, 1);
  stroke(cyc, colors.red, dimming ? 0.1 : 0.7, 1.2);
  stroke(ipc, colors.yellow, dimming ? 0.08 : 0.6, 1.1, [4, 3]);
  stroke(outL, colors.accent, 0.95, 1.8, undefined, !lay);
  stroke(inL, colors.green, 0.95, 1.8, undefined, !lay);

  // nodes
  const sel = selected.value;
  for (const n of nodes) {
    if (!onScreen(n)) continue;
    const on = lit(n);
    const x = X(n), y = Y(n), r = R(n);
    const picked = n.id === sel || n === hovered.value;
    if (lay && picked) {
      c.globalAlpha = 0.18;
      c.fillStyle = colors.accent;
      c.beginPath();
      c.roundRect(x - r - 5, y - ROW / 2 + 1, r * 2 + n.lw + 9, ROW - 2, 4);
      c.fill();
    }
    c.globalAlpha = dimming && !on ? 0.15 : 1;
    const col = n.slot >= 0 ? colors.cats[n.slot] : colors.other;
    c.beginPath();
    c.arc(x, y, r, 0, Math.PI * 2);
    if (n.g.external) {
      c.fillStyle = colors.bg;
      c.fill();
      c.lineWidth = 1.6 * px;
      c.strokeStyle = col;
      c.stroke();
    } else {
      c.fillStyle = col;
      c.fill();
    }
    if (opts.cycles && n.cyc) {
      c.lineWidth = 1.6 * px;
      c.strokeStyle = colors.red;
      c.stroke();
    }
    if (picked && !lay) {
      c.beginPath();
      c.arc(x, y, r + 3.5 * px, 0, Math.PI * 2);
      c.lineWidth = 2 * px;
      c.strokeStyle = colors.text;
      c.stroke();
    }
  }

  c.lineJoin = "round";
  if (lay) {
    // layers: every row is labelled, text scales with zoom like a document
    if (FONT * k >= 4.5) {
      c.font = `${FONT}px ${colors.sans}`;
      c.textAlign = "left";
      c.textBaseline = "middle";
      const fade = Math.min(1, (b - 0.5) * 2.5);
      for (const n of nodes) {
        if (!onScreen(n)) continue;
        const on = lit(n);
        c.globalAlpha = (dimming && !on ? 0.2 : 1) * fade;
        const x = X(n) + R(n) + 6 * b, y = Y(n);
        c.lineWidth = 3;
        c.strokeStyle = colors.bg;
        c.strokeText(n.name, x, y);
        c.fillStyle = (dimming && on) || n.id === sel ? colors.text : colors.text2;
        c.fillText(n.name, x, y);
      }
    }
  } else {
    // force: labels fade in with zoom, big nodes first; always for the focus neighbourhood and search hits
    const fs = 11.5 / k;
    c.font = `${fs}px ${colors.sans}`;
    c.textAlign = "center";
    c.textBaseline = "top";
    for (const n of nodes) {
      if (!onScreen(n)) continue;
      const on = lit(n);
      const forced = dimming && on && (focus ? true : nodes.length < 400 || (m?.size ?? 99) < 60);
      let a = forced ? 1 : Math.max(0, Math.min(1, (k - (1.25 - n.r / 16)) / 0.35));
      if (dimming && !on) a = Math.min(a, 0.12);
      a *= 1 - b * 2;
      if (a <= 0.02) continue;
      c.globalAlpha = a;
      const y = Y(n) + R(n) + 3 / k;
      c.lineWidth = 3 / k;
      c.strokeStyle = colors.bg;
      c.strokeText(n.name, X(n), y);
      c.fillStyle = n === focus || n.id === sel ? colors.text : colors.text2;
      c.fillText(n.name, X(n), y);
    }
  }
  c.globalAlpha = 1;
}

// ---------- interaction ----------

function toWorld(ev: { clientX: number; clientY: number }) {
  const r = canvas.value!.getBoundingClientRect();
  const sx = ev.clientX - r.left, sy = ev.clientY - r.top;
  return { sx, sy, x: (sx - view.x) / view.k, y: (sy - view.y) / view.k };
}

function hit(x: number, y: number): N | null {
  if (layered()) {
    // the whole row (dot + label) is the target
    for (const n of nodes) {
      const nx = X(n), r = R(n);
      if (Math.abs(y - Y(n)) <= ROW / 2 && x >= nx - r - 5 && x <= nx + r + n.lw * blend + 4) return n;
    }
    return null;
  }
  const slack = 4 / view.k;
  let best: N | null = null, bd = Infinity;
  for (let i = nodes.length - 1; i >= 0; i--) {
    const n = nodes[i];
    const d = Math.hypot(X(n) - x, Y(n) - y);
    if (d <= n.r + slack && d < bd) { best = n; bd = d; }
  }
  return best;
}

let drag: { node: N | null; sx: number; sy: number; vx: number; vy: number; moved: boolean; id: number } | null = null;

function onDown(ev: PointerEvent) {
  if (ev.button !== 0) return;
  wrap.value?.focus();
  const w = toWorld(ev);
  const n = hit(w.x, w.y);
  drag = { node: n, sx: w.sx, sy: w.sy, vx: view.x, vy: view.y, moved: false, id: ev.pointerId };
  canvas.value!.setPointerCapture(ev.pointerId);
  cancelAnimationFrame(viewAnim);
}

function onMove(ev: PointerEvent) {
  const w = toWorld(ev);
  if (drag) {
    if (!drag.moved && Math.hypot(w.sx - drag.sx, w.sy - drag.sy) > 3) drag.moved = true;
    if (!drag.moved) return;
    // nodes are draggable only in the force view; in layers dragging anywhere pans
    if (drag.node && blend === 0) {
      drag.node.fx = w.x;
      drag.node.fy = w.y;
      sim?.alphaTarget(0.25).restart();
    } else {
      autoFit = false;
      view.x = drag.vx + (w.sx - drag.sx);
      view.y = drag.vy + (w.sy - drag.sy);
      requestDraw();
    }
    tip.x = w.sx; tip.y = w.sy;
    return;
  }
  const n = hit(w.x, w.y);
  tip.x = w.sx; tip.y = w.sy;
  if (n !== hovered.value) { hovered.value = n; requestDraw(); }
}

function onUp(ev: PointerEvent) {
  if (!drag) return;
  const d = drag;
  drag = null;
  canvas.value?.releasePointerCapture(ev.pointerId);
  if (d.moved) {
    if (d.node && d.node.fx != null) {
      d.node.fx = d.node.fy = null;
      sim?.alphaTarget(0);
      saveLayout();
    }
    return;
  }
  if (!d.moved) select(d.node?.id ?? null, false);
}

function onLeave() {
  if (!drag && hovered.value) { hovered.value = null; requestDraw(); }
}

function onDbl(ev: MouseEvent) {
  const w = toWorld(ev);
  const n = hit(w.x, w.y);
  if (n && !n.g.external) emit("open-file", n.g.path);
}

function onWheel(ev: WheelEvent) {
  ev.preventDefault();
  cancelAnimationFrame(viewAnim);
  autoFit = false;
  // layers read like a document: wheel scrolls, Ctrl+wheel (or pinch) zooms
  if (layered() && !ev.ctrlKey) {
    const dx = ev.shiftKey ? ev.deltaY : ev.deltaX, dy = ev.shiftKey ? 0 : ev.deltaY;
    view.x -= dx;
    view.y -= dy;
    requestDraw();
    return;
  }
  const w = toWorld(ev);
  const k = Math.min(8, Math.max(0.04, view.k * Math.exp(-ev.deltaY * (ev.ctrlKey ? 0.01 : 0.0015))));
  view.x = w.sx - w.x * k;
  view.y = w.sy - w.y * k;
  view.k = k;
  requestDraw();
}

function onKey(ev: KeyboardEvent) {
  if (ev.key === "Escape") {
    if (query.value) query.value = "";
    else select(null);
  } else if ((ev.ctrlKey && ev.key === "f") || (ev.key === "/" && ev.target === wrap.value)) {
    ev.preventDefault();
    searchEl.value?.focus();
  }
}

function select(id: string | null, center = true) {
  selected.value = id;
  if (id && center) {
    const n = nodeObjs.get(id);
    if (n?.x != null && nodes.includes(n)) centerOn(n);
  }
  requestDraw();
}

function onSearchEnter() {
  const m = matches.value;
  if (!m?.size) return;
  const hits = nodes.filter((n) => m.has(n.id));
  if (hits.length === 1) select(hits[0].id);
  else if (hits.length) { autoFit = false; fit(true, hits); }
}

// ---------- side panel ----------

const panel = computed(() => {
  const f = full.value;
  const id = selected.value;
  if (!f || !id) return null;
  const node = f.byId.get(id);
  if (!node) return null;
  const item = (other: string, e: GraphEdge) => ({ id: other, name: isPkg(other) ? other.slice(4) : other, kind: e.kind, count: e.count });
  const sort = (a: { name: string }, b: { name: string }) => a.name.localeCompare(b.name);
  const imports = (f.out.get(id) ?? []).map((e) => item(e.target, e)).sort(sort);
  const usedBy = (f.inc.get(id) ?? []).map((e) => item(e.source, e)).sort(sort);
  return { node, imports, usedBy, cyc: f.inCycle(id) };
});

async function openIde() {
  const p = panel.value?.node.path;
  if (!p) return;
  try { await api.openInEditor(state.settings.editor, p); } catch (e) { toast(String(e), "error"); }
}

function setDepth(d: number) {
  opts.depth = d;
}

// ---------- lifecycle ----------

let ro: ResizeObserver | null = null;
let mo: MutationObserver | null = null;
const mq = window.matchMedia("(prefers-color-scheme: dark)");
const onScheme = () => setTimeout(readColors);

function resize() {
  const el = wrap.value, cv = canvas.value;
  if (!el || !cv) return;
  const dpr = window.devicePixelRatio || 1;
  const first = !W;
  W = el.clientWidth;
  H = el.clientHeight;
  cv.width = Math.round(W * dpr);
  cv.height = Math.round(H * dpr);
  cv.style.width = W + "px";
  cv.style.height = H + "px";
  if (first && autoFit && nodes.length) fit(false);
  draw();
}

onMounted(() => {
  ctx = canvas.value!.getContext("2d");
  readColors();
  ro = new ResizeObserver(resize);
  ro.observe(wrap.value!);
  mo = new MutationObserver(onScheme);
  mo.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme", "class", "style"] });
  mq.addEventListener("change", onScheme);
  load();
});

onBeforeUnmount(() => {
  saveLayout();
  sim?.stop();
  ro?.disconnect();
  mo?.disconnect();
  mq.removeEventListener("change", onScheme);
  cancelAnimationFrame(raf);
  cancelAnimationFrame(viewAnim);
});

watch([raw, () => opts.external, () => opts.orphans, () => opts.depth], () => rebuild());
watch(selected, (id, prev) => {
  if (opts.depth > 0 && id !== prev) {
    rebuild();
    if (id) setTimeout(() => { const n = nodeObjs.get(id); if (n) centerOn(n); }, 30);
  }
});
watch([() => opts.colorBy, () => opts.sizeBy], () => { applyAttrs(); sim?.alpha(0.25).restart(); });
watch([matches, legendHover, () => opts.cycles], requestDraw);

const tipNode = computed(() => (hovered.value && !drag ? hovered.value : null));
const tipIn = computed(() => (tipNode.value ? full.value?.inc.get(tipNode.value.id)?.length ?? 0 : 0));
const tipOut = computed(() => (tipNode.value ? full.value?.out.get(tipNode.value.id)?.length ?? 0 : 0));
const noEdges = computed(() => !!raw.value && !raw.value.edges.some((e) => !isPkg(e.target)));
const legendColor = (slot: number) => (slot >= 0 ? `var(--cat-${slot + 1})` : "var(--cat-other)");
</script>

<template>
  <div class="gt">
    <div class="toolbar">
      <div class="seg" title="Вид">
        <button
          :class="{ on: state.settings.graphLayout === 'layers' }"
          title="Файлы по колонкам: слева — то, что импортирует, справа — то, от чего зависят"
          @click="setLayout('layers')"
        >слои</button>
        <button
          :class="{ on: state.settings.graphLayout !== 'layers' }"
          title="Свободный граф, как в Obsidian"
          @click="setLayout('force')"
        >граф</button>
      </div>
      <div class="search">
        <Icon name="search" :size="13" />
        <input
          ref="searchEl"
          v-model="query"
          class="input"
          placeholder="Найти файл…"
          spellcheck="false"
          @keydown.enter="onSearchEnter"
          @keydown.esc="query = ''"
        />
        <span v-if="matches" class="dim cnt">{{ matches.size }}</span>
      </div>
      <div class="seg" title="Цвет узлов">
        <button :class="{ on: opts.colorBy === 'folder' }" @click="opts.colorBy = 'folder'">папки</button>
        <button :class="{ on: opts.colorBy === 'lang' }" @click="opts.colorBy = 'lang'">языки</button>
      </div>
      <div class="seg" title="Размер узлов">
        <button :class="{ on: opts.sizeBy === 'links' }" @click="opts.sizeBy = 'links'">по связям</button>
        <button :class="{ on: opts.sizeBy === 'lines' }" @click="opts.sizeBy = 'lines'">по строкам</button>
      </div>
      <label class="chk"><input v-model="opts.external" type="checkbox" /> внешние пакеты</label>
      <label class="chk"><input v-model="opts.orphans" type="checkbox" /> файлы без связей</label>
      <label v-if="full?.cycles" class="chk cyc" title="Файлы, которые импортируют друг друга по кругу">
        <input v-model="opts.cycles" type="checkbox" /> циклы: {{ full.cycles }}
      </label>
      <span class="spacer" />
      <span v-if="raw" class="dim stat">
        {{ visStats.nodes }} {{ plural(visStats.nodes, "узел", "узла", "узлов") }} · {{ visStats.edges }} {{ plural(visStats.edges, "связь", "связи", "связей") }}
      </span>
      <span v-if="raw?.truncated" class="badge yellow" title="Учтены первые 4000 файлов">не все файлы</span>
      <button class="btn sm ghost icon" title="Вписать в экран" @click="refit"><Icon name="focus" :size="13" /></button>
      <button class="btn sm ghost" :disabled="loading" title="Заново просканировать импорты" @click="load(true)">
        <Icon name="refresh" :size="12" /> {{ loading ? "Сканирую…" : "Обновить" }}
      </button>
    </div>

    <div class="main">
      <div ref="wrap" class="stage" tabindex="0" @keydown="onKey">
        <canvas
          ref="canvas"
          :class="{ grab: !!hovered }"
          @pointerdown="onDown"
          @pointermove="onMove"
          @pointerup="onUp"
          @pointercancel="onUp"
          @pointerleave="onLeave"
          @dblclick="onDbl"
          @wheel="onWheel"
        />
        <div v-if="loading && !raw" class="overlay empty">Разбираю импорты…</div>
        <div v-else-if="noEdges" class="overlay empty">
          <Icon name="graph" :size="36" />
          <div>Связей между файлами не найдено.</div>
          <div class="dim">Поддерживаются JS/TS, Vue, Svelte, Astro, CSS/SCSS, HTML, Python, Rust, Go и PHP (Blade, Twig).</div>
        </div>

        <div v-if="tipNode" class="tip" :style="{ left: tip.x + 14 + 'px', top: tip.y + 14 + 'px' }">
          <div class="mono">{{ tipNode.g.external ? tipNode.name : tipNode.id }}</div>
          <div class="dim">
            <template v-if="!tipNode.g.external">{{ tipNode.g.lang }} · {{ tipNode.g.lines }} {{ plural(tipNode.g.lines, "строка", "строки", "строк") }} · </template>
            импортирует {{ tipOut }} · используется в {{ tipIn }}
          </div>
        </div>

        <div class="legend">
          <div
            v-for="g in groups.legend"
            :key="g.name"
            class="lg"
            @mouseenter="legendHover = g.name"
            @mouseleave="legendHover = null"
          >
            <span class="dot" :style="{ background: legendColor(g.slot) }" />
            <span class="ellipsis">{{ g.name }}</span>
            <span class="dim">{{ g.count }}</span>
          </div>
          <div class="lg-sep" />
          <div class="lg"><span class="ln" style="background: var(--accent)" /> импортирует</div>
          <div class="lg"><span class="ln" style="background: var(--green)" /> используется в</div>
          <div v-if="raw?.edges.some((e) => e.kind === 'ipc')" class="lg"><span class="ln dash" /> вызов invoke → Rust</div>
          <div v-if="opts.cycles && full?.cycles" class="lg"><span class="ln" style="background: var(--red)" /> циклический импорт</div>
        </div>
        <div v-if="state.settings.graphLayout === 'layers'" class="hint dim">
          слева направо: кто импортирует → что импортируется · колесо — прокрутка · Ctrl+колесо — зум · клик — выбрать · двойной клик — открыть файл
        </div>
        <div v-else class="hint dim">колесо — зум · перетаскивание — сдвиг · клик — выбрать · двойной клик — открыть файл</div>
      </div>

      <aside v-if="panel" class="side">
        <div class="side-h">
          <div class="grow">
            <div class="fname ellipsis" :title="panel.node.id">{{ panel.node.external ? panel.node.id.slice(4) : baseName(panel.node.id) }}</div>
            <div class="dim mono ellipsis small" :title="panel.node.id">{{ panel.node.external ? "внешний пакет" : panel.node.id }}</div>
          </div>
          <button class="btn sm ghost icon" title="Закрыть (Esc)" @click="select(null)"><Icon name="x" :size="13" /></button>
        </div>
        <div v-if="!panel.node.external" class="side-meta">
          <span class="badge">{{ panel.node.lang }}</span>
          <span class="badge">{{ panel.node.lines }} {{ plural(panel.node.lines, "строка", "строки", "строк") }}</span>
          <span v-if="panel.cyc" class="badge red" title="Этот файл входит в цикл импортов">в цикле</span>
        </div>
        <div v-if="!panel.node.external" class="side-actions">
          <button class="btn sm" @click="emit('open-file', panel.node.path)"><Icon name="file" :size="12" /> Открыть</button>
          <button class="btn sm" @click="openIde"><Icon name="code" :size="12" /> В IDE</button>
        </div>
        <div class="side-local">
          <span class="muted">Окружение:</span>
          <div class="seg">
            <button :class="{ on: opts.depth === 0 }" @click="setDepth(0)">весь граф</button>
            <button :class="{ on: opts.depth === 1 }" @click="setDepth(1)">1 шаг</button>
            <button :class="{ on: opts.depth === 2 }" @click="setDepth(2)">2 шага</button>
          </div>
        </div>
        <div class="side-b">
          <div v-if="!panel.node.external" class="sec">
            <div class="sec-h"><span class="ln" style="background: var(--accent)" /> Импортирует <span class="dim">{{ panel.imports.length }}</span></div>
            <div v-if="!panel.imports.length" class="dim none">ничего</div>
            <button v-for="x in panel.imports" :key="x.id" class="it" :title="x.id" @click="select(x.id)">
              <span class="ellipsis grow mono">{{ x.name }}</span>
              <span v-if="x.kind === 'ipc'" class="badge yellow">invoke ×{{ x.count }}</span>
              <span v-if="isPkg(x.id)" class="dim">пакет</span>
            </button>
          </div>
          <div class="sec">
            <div class="sec-h"><span class="ln" style="background: var(--green)" /> Используется в <span class="dim">{{ panel.usedBy.length }}</span></div>
            <div v-if="!panel.usedBy.length" class="dim none">нигде: возможно, это точка входа или мёртвый код</div>
            <button v-for="x in panel.usedBy" :key="x.id" class="it" :title="x.id" @click="select(x.id)">
              <span class="ellipsis grow mono">{{ x.name }}</span>
              <span v-if="x.kind === 'ipc'" class="badge yellow">invoke ×{{ x.count }}</span>
            </button>
          </div>
        </div>
      </aside>
    </div>
  </div>
</template>

<style scoped>
.gt { height: 100%; display: flex; flex-direction: column; min-height: 0; }
.toolbar { display: flex; align-items: center; gap: 10px; padding: 8px 14px; border-bottom: 1px solid var(--border); flex-wrap: wrap; }
.search { position: relative; display: flex; align-items: center; width: 220px; color: var(--text-3); }
.search svg { position: absolute; left: 9px; pointer-events: none; }
.search .input { height: 26px; padding-left: 28px; padding-right: 30px; font-size: 12.5px; }
.search .cnt { position: absolute; right: 9px; font-size: 11px; }
.seg { display: flex; background: var(--bg); border: 1px solid var(--border); border-radius: 6px; padding: 2px; }
.seg button { border: 0; background: none; padding: 2px 9px; border-radius: 4px; cursor: pointer; color: var(--text-2); font-size: 11.5px; }
.seg button.on { background: var(--active); color: var(--text); }
.chk { display: flex; align-items: center; gap: 5px; font-size: 12px; color: var(--text-2); cursor: pointer; white-space: nowrap; }
.chk input { margin: 0; accent-color: var(--accent); }
.chk.cyc { color: var(--red); }
.stat { font-size: 12px; white-space: nowrap; }
.main { flex: 1; min-height: 0; display: flex; }
.stage { position: relative; flex: 1; min-width: 0; overflow: hidden; outline: none; background: var(--bg); }
canvas { display: block; cursor: default; touch-action: none; }
canvas.grab { cursor: pointer; }
.overlay { position: absolute; inset: 0; pointer-events: none; }
.tip {
  position: absolute; pointer-events: none; z-index: 5; max-width: 420px;
  background: var(--panel); border: 1px solid var(--border-2); border-radius: 6px; padding: 6px 9px;
  font-size: 12px; box-shadow: 0 8px 24px rgba(0,0,0,.3);
}
.tip .mono { font-size: 11.5px; word-break: break-all; }
.legend {
  position: absolute; left: 12px; bottom: 30px; max-width: 240px; padding: 8px 10px;
  background: color-mix(in srgb, var(--panel) 88%, transparent); border: 1px solid var(--border); border-radius: 8px;
  font-size: 11.5px; display: flex; flex-direction: column; gap: 2px;
}
.lg { display: flex; align-items: center; gap: 7px; padding: 1px 2px; border-radius: 4px; color: var(--text-2); }
.lg:hover { background: var(--hover); color: var(--text); }
.lg .ellipsis { flex: 1; min-width: 0; }
.lg-sep { height: 1px; background: var(--border); margin: 4px 0; }
.dot { width: 9px; height: 9px; border-radius: 50%; flex: none; }
.ln { width: 14px; height: 2px; border-radius: 1px; flex: none; display: inline-block; }
.ln.dash { background: repeating-linear-gradient(90deg, var(--yellow) 0 4px, transparent 4px 7px); }
.hint { position: absolute; left: 12px; bottom: 8px; font-size: 11px; pointer-events: none; }
.side { width: 300px; flex: none; border-left: 1px solid var(--border); background: var(--bg-2); display: flex; flex-direction: column; min-height: 0; }
.side-h { display: flex; align-items: flex-start; gap: 8px; padding: 12px 12px 8px 14px; }
.fname { font-weight: 650; font-size: 14px; }
.small { font-size: 11px; margin-top: 2px; }
.side-meta { display: flex; gap: 6px; flex-wrap: wrap; padding: 0 14px 8px; }
.side-actions { display: flex; gap: 6px; padding: 0 14px 10px; }
.side-local { display: flex; align-items: center; gap: 8px; padding: 8px 14px; border-top: 1px solid var(--border); border-bottom: 1px solid var(--border); font-size: 12px; }
.side-b { flex: 1; overflow-y: auto; padding: 6px 8px 14px; }
.sec { margin-top: 8px; }
.sec-h { display: flex; align-items: center; gap: 7px; font-size: 11px; text-transform: uppercase; letter-spacing: .03em; color: var(--text-2); padding: 4px 6px; font-weight: 600; }
.none { font-size: 12px; padding: 2px 6px 4px; }
.it { display: flex; align-items: center; gap: 6px; width: 100%; text-align: left; background: none; border: 0; padding: 4px 6px; border-radius: 4px; cursor: pointer; font-size: 12px; }
.it:hover { background: var(--hover); }
.it .mono { font-size: 11.5px; }
</style>
