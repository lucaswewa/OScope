<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, useTemplateRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import Button from "../Button.vue";
import Collapsible from "../Collapsible.vue";
import DataTable from "../DataTable.vue";
import type { Column } from "../table";
import {
  DASHES,
  drawAxes,
  drawBand,
  drawCursor,
  drawSeries,
  MARKS,
  type Band,
  type Colours,
  type Frame,
  type Series,
} from "./draw";
import {
  kept,
  loose,
  nearestByX,
  nearestInPlane,
  panned,
  place,
  stepOf,
  ticksOf,
  valueAt,
  widened,
  written,
  zoomed,
  type Scale,
  type ScaleKind,
  type View,
} from "./scale";

export interface Axis {
  /** Its title, with its unit: "Position (µm)". */
  title: string;
  scale?: ScaleKind;
  /** Its range, where it doesn't fit the values in view. */
  range?: [number, number];
}

/**
 * A chart drawn by the app (W0.7): lines, areas, histograms and points, on
 * a canvas, coloured from the tokens. Its axes are marked at round numbers;
 * a cursor reads the values; the wheel, a drag and the keyboard zoom and
 * pan it along x. Behind it, a table holds the same numbers.
 *
 * New numbers are new arrays: the chart draws again when `series` changes.
 */
const props = withDefaults(
  defineProps<{
    title: string;
    series: readonly Series[];
    bands?: readonly Band[];
    x: Axis;
    y: Axis;
    /** The plot's height, in pixels. */
    height?: number;
    /** The level of the numbers' section's title. */
    level?: 2 | 3 | 4;
  }>(),
  { bands: () => [], height: 240, level: 3 },
);

const { t } = useI18n();
const box = useTemplateRef<HTMLElement>("box");
const canvas = useTemplateRef<HTMLCanvasElement>("canvas");
const width = ref(0);
const ratio = ref(1);
const colours = shallowRef<Colours>();
/** What part of x is shown: all of it, unless zoomed. */
const view = ref<View | null>(null);
/** The cursor's x, on a point of the data. */
const cursor = ref<number | null>(null);
/** The scales and the frame last drawn, for the pointer. */
let drawn: { x: Scale; y: Scale; frame: Frame } | undefined;

const extent = (series: Series): [number, number] | undefined =>
  series.x.length === 0
    ? undefined
    : [
        series.x[0]!,
        series.x[series.x.length - 1]! + (series.kind === "histogram" ? (series.width ?? 1) : 0),
      ];

/** The data's x, from the least to the greatest. */
const all = computed<View>(() => {
  const ends = [
    ...props.series.map(extent),
    ...props.bands.map((band) => extent({ name: "", kind: "line", x: band.x, y: [] })),
  ];
  const known = ends.filter((one): one is [number, number] => one !== undefined);
  if (known.length === 0) return { min: 0, max: 1 };
  const [min, max] = widened(
    Math.min(...known.map((one) => one[0])),
    Math.max(...known.map((one) => one[1])),
  );
  // Points at the ends are drawn whole: a little room beyond them.
  const room = props.series.some((series) => series.kind === "points") ? (max - min) * 0.03 : 0;
  return { min: min - room, max: max + room };
});
/** No view narrower than two of the first series' points. */
const least = computed(() => {
  const xs = props.series[0]?.x;
  let step = Number.POSITIVE_INFINITY;
  for (let n = 1; n < (xs?.length ?? 0); n++) step = Math.min(step, xs![n]! - xs![n - 1]!);
  return Number.isFinite(step) && step > 0 ? 2 * step : (all.value.max - all.value.min) / 10;
});
/** The view, kept inside the data, which may have changed under it. */
const shown = computed(() => (view.value ? kept(view.value, all.value, least.value) : all.value));
const yKind = computed(() => props.y.scale ?? "linear");

/** The y range: the axis's own, or the values in view, from 0 for histograms and areas. */
const yRange = computed<[number, number]>(() => {
  if (props.y.range) return props.y.range;
  let min = Number.POSITIVE_INFINITY;
  let max = Number.NEGATIVE_INFINITY;
  const take = (value: number): void => {
    if (!Number.isFinite(value) || (yKind.value === "log" && value <= 0)) return;
    min = Math.min(min, value);
    max = Math.max(max, value);
  };
  const { min: from, max: to } = shown.value;
  for (const series of props.series) {
    const before = series.kind === "histogram" ? (series.width ?? 1) : 0;
    for (let n = 0; n < series.x.length; n++) {
      if (series.x[n]! >= from - before && series.x[n]! <= to) take(series.y[n]!);
    }
  }
  for (const band of props.bands) {
    for (let n = 0; n < band.x.length; n++) {
      if (band.x[n]! >= from && band.x[n]! <= to) {
        take(band.lower[n]!);
        take(band.upper[n]!);
      }
    }
  }
  if (!Number.isFinite(min)) return yKind.value === "log" ? [1, 10] : [0, 1];
  if (yKind.value === "log")
    return [10 ** Math.floor(Math.log10(min)), 10 ** Math.ceil(Math.log10(max))];
  if (props.series.some((series) => series.kind === "histogram" || series.kind === "area"))
    min = Math.min(0, min);
  return loose(min, max, Math.max(2, Math.floor(props.height / 50)));
});

/** The tokens' colours, as the canvas takes them: `light-dark()` resolved for the scheme in use. */
function readColours(): void {
  const element = box.value;
  if (!element) return;
  const probe = document.createElement("span");
  probe.style.display = "none";
  probe.style.fontFamily = "var(--font-ui)";
  probe.style.fontSize = "var(--font-size-small)";
  element.append(probe);
  const read = (token: string): string => {
    probe.style.color = `var(--${token})`;
    return getComputedStyle(probe).color;
  };
  const style = getComputedStyle(probe);
  colours.value = {
    series: [1, 2, 3, 4, 5, 6].map((n) => read(`chart-${n}`)),
    grid: read("chart-grid"),
    axis: read("chart-axis"),
    text: read("text-muted"),
    font: `${style.fontSize} ${style.fontFamily}`,
  };
  probe.remove();
}

/** Draws it all, once the next frame comes: changes in between are drawn together. */
let pending = 0;
function redraw(): void {
  if (pending === 0) pending = requestAnimationFrame(paint);
}

function paint(): void {
  pending = 0;
  const element = canvas.value;
  const ink = colours.value;
  if (!element || !ink || width.value <= 0) return;
  const started = performance.now();
  const [w, h, r] = [width.value, props.height, ratio.value];
  if (element.width !== Math.round(w * r) || element.height !== Math.round(h * r)) {
    element.width = Math.round(w * r);
    element.height = Math.round(h * r);
  }
  const pen = element.getContext("2d");
  if (!pen) return;
  pen.setTransform(r, 0, 0, r, 0, 0);
  pen.clearRect(0, 0, w, h);
  pen.font = ink.font;
  const line = parseFloat(ink.font) * 1.4 || 18;
  const bottom = h - 2 * line - 8;
  const y: Scale = {
    kind: yKind.value,
    min: yRange.value[0],
    max: yRange.value[1],
    from: bottom,
    to: 8,
  };
  const yTicks = ticksOf(y, 50);
  const left = line + 10 + Math.max(0, ...yTicks.map((tick) => pen.measureText(tick.label).width));
  const frame: Frame = { left, top: 8, right: w - 16, bottom, height: h };
  const x: Scale = {
    kind: props.x.scale ?? "linear",
    min: shown.value.min,
    max: shown.value.max,
    from: left,
    to: frame.right,
  };
  drawAxes(
    pen,
    frame,
    { scale: x, ticks: ticksOf(x, 80), title: props.x.title },
    { scale: y, ticks: yTicks, title: props.y.title },
    ink,
  );
  pen.save();
  pen.beginPath();
  pen.rect(frame.left, frame.top, frame.right - frame.left, frame.bottom - frame.top);
  pen.clip();
  for (const band of props.bands) drawBand(pen, band, x, y, ink);
  props.series.forEach((series, n) => drawSeries(pen, series, n, x, y, ink));
  if (cursor.value !== null) {
    const at = cursor.value;
    const points = props.series.map((series, n) => {
      const k = nearestByX(series.x, at);
      const middle = series.kind === "histogram" ? (series.width ?? 1) / 2 : 0;
      return { x: place(x, series.x[k]! + middle), y: place(y, series.y[k]!), n };
    });
    drawCursor(
      pen,
      frame,
      place(x, at + (props.series[0]?.kind === "histogram" ? (props.series[0].width ?? 1) / 2 : 0)),
      points,
      ink,
    );
  }
  pen.restore();
  drawn = { x, y, frame };
  // How long it took, for the page's own timeline: a test reads it.
  performance.measure("oscope.chart.draw", {
    start: started,
    end: performance.now(),
    detail: props.title,
  });
}

watch(
  [() => props.series, () => props.bands, () => props.height, view, cursor, width, ratio, colours],
  redraw,
);

let resizing: ResizeObserver | undefined;
let theming: MutationObserver | undefined;
const scheme = matchMedia("(prefers-color-scheme: dark)");
onMounted(() => {
  const element = box.value!;
  const measure = (): void => {
    width.value = element.clientWidth;
    ratio.value = window.devicePixelRatio || 1;
  };
  measure();
  resizing = new ResizeObserver(measure);
  resizing.observe(element);
  readColours();
  // The theme and the accent are on the page's root; the computer's scheme is the system's.
  theming = new MutationObserver(readColours);
  theming.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ["data-theme", "data-accent", "style", "class"],
  });
  scheme.addEventListener("change", readColours);
  redraw();
});
onBeforeUnmount(() => {
  resizing?.disconnect();
  theming?.disconnect();
  scheme.removeEventListener("change", readColours);
  if (pending) cancelAnimationFrame(pending);
});

// Reading, zooming and panning.
const first = computed(() => props.series[0]);
const pointsOnly = computed(
  () => props.series.length > 0 && props.series.every((series) => series.kind === "points"),
);
const zoomedIn = computed(() => view.value !== null);
const showAll = (): void => {
  view.value = null;
};
/** Shows a view: one as wide as the data is everything again. */
function show(next: View): void {
  view.value = next.max - next.min >= all.value.max - all.value.min - 1e-9 ? null : next;
}

/** The pointer's place over the plot, in the page's pixels. */
function spot(event: MouseEvent): { px: number; py: number } {
  const rect = box.value!.getBoundingClientRect();
  return { px: event.clientX - rect.left, py: event.clientY - rect.top };
}

/** The x of the data point nearest a pixel. */
function snapped(px: number, py: number): number | null {
  if (!drawn || !first.value) return null;
  if (pointsOnly.value) {
    let best: { at: number; distance: number } | null = null;
    for (const series of props.series) {
      const xs = Array.from(series.x, (one) => place(drawn!.x, one));
      const ys = Array.from(series.y, (one) => place(drawn!.y, one));
      const k = nearestInPlane(xs, ys, px, py);
      const distance = k < 0 ? Number.POSITIVE_INFINITY : (xs[k]! - px) ** 2 + (ys[k]! - py) ** 2;
      if (k >= 0 && (!best || distance < best.distance)) best = { at: series.x[k]!, distance };
    }
    return best?.at ?? null;
  }
  const middle = first.value.kind === "histogram" ? (first.value.width ?? 1) / 2 : 0;
  const k = nearestByX(first.value.x, valueAt(drawn.x, px) - middle);
  return k < 0 ? null : first.value.x[k]!;
}

let drag: { from: number; view: View } | undefined;
function pressed(event: PointerEvent): void {
  if (event.button !== 0) return;
  drag = { from: spot(event).px, view: shown.value };
  box.value?.setPointerCapture?.(event.pointerId);
}
function moved(event: PointerEvent): void {
  const { px, py } = spot(event);
  if (drag && drawn) {
    const by = valueAt(drawn.x, drag.from) - valueAt(drawn.x, px);
    if (Math.abs(px - drag.from) > 2) show(panned(drag.view, by, all.value, least.value));
    return;
  }
  cursor.value = snapped(px, py);
}
const released = (): void => {
  drag = undefined;
};
function wheeled(event: WheelEvent): void {
  if (!drawn || event.deltaY === 0) return;
  event.preventDefault();
  const about = valueAt(drawn.x, spot(event).px);
  show(zoomed(shown.value, event.deltaY < 0 ? 1.25 : 1 / 1.25, about, all.value, least.value));
}

/** The keys: the cursor along the points, zoom about it, the view moved by half its width, and everything again. */
function key(event: KeyboardEvent): void {
  const xs = first.value?.x;
  if (!xs || xs.length === 0) return;
  const here = cursor.value === null ? -1 : nearestByX(xs, cursor.value);
  const at = (k: number): void => {
    cursor.value = xs[Math.min(Math.max(k, 0), xs.length - 1)]!;
    const { min, max } = shown.value;
    if (cursor.value < min || cursor.value > max) {
      show(panned(shown.value, cursor.value - (min + max) / 2, all.value, least.value));
    }
  };
  const middle = (shown.value.min + shown.value.max) / 2;
  const width = shown.value.max - shown.value.min;
  switch (event.key) {
    case "ArrowRight":
      at(here < 0 ? nearestByX(xs, middle) : here + 1);
      break;
    case "ArrowLeft":
      at(here < 0 ? nearestByX(xs, middle) : here - 1);
      break;
    case "Home":
      at(0);
      break;
    case "End":
      at(xs.length - 1);
      break;
    case "+":
    case "=":
      show(zoomed(shown.value, 2, cursor.value ?? middle, all.value, least.value));
      break;
    case "-":
      show(zoomed(shown.value, 0.5, cursor.value ?? middle, all.value, least.value));
      break;
    case "PageDown":
      show(panned(shown.value, width / 2, all.value, least.value));
      break;
    case "PageUp":
      show(panned(shown.value, -width / 2, all.value, least.value));
      break;
    case "0":
      showAll();
      break;
    default:
      return;
  }
  event.preventDefault();
}

/** A value written for the readout: a decimal more than the axis's ticks. */
function said(value: number, kind: ScaleKind, min: number, max: number): string {
  if (kind === "log")
    return written(
      Number(value.toPrecision(3)),
      Math.max(0, 2 - Math.floor(Math.log10(Math.abs(value) || 1))),
    );
  const step = stepOf(min, max, 5);
  return written(value, Math.max(0, -Math.floor(Math.log10(step) + 1e-9)) + 1);
}

/** What the cursor reads: x, and each series' value there. */
const readout = computed(() => {
  const at = cursor.value;
  if (at === null || !first.value) return "";
  const parts: string[] = [];
  const x = (value: number): string =>
    said(value, props.x.scale ?? "linear", shown.value.min, shown.value.max);
  const y = (value: number): string => said(value, yKind.value, yRange.value[0], yRange.value[1]);
  parts.push(
    first.value.kind === "histogram"
      ? t("ui.chart.bin", {
          axis: props.x.title,
          from: x(at),
          to: x(at + (first.value.width ?? 1)),
        })
      : t("ui.chart.at", { axis: props.x.title, value: x(at) }),
  );
  for (const series of props.series) {
    const k = nearestByX(series.x, at);
    const value = series.y[k]!;
    parts.push(
      yKind.value === "log" && !(value > 0)
        ? t("ui.chart.offScale", { name: series.name })
        : t("ui.chart.value", { name: series.name, value: y(value) }),
    );
  }
  return parts.join("; ");
});

/** The image's name: the title, and a sentence of what the chart shows. */
const named = computed(() => {
  const kinds = [...new Set(props.series.map((series) => t(`ui.chart.kinds.${series.kind}`)))].join(
    ", ",
  );
  return `${props.title}. ${t("ui.chart.shows", {
    kinds,
    series: props.series.map((series) => series.name).join(", "),
    x: props.x.title,
    xFrom: written(all.value.min, 2),
    xTo: written(all.value.max, 2),
    y: props.y.title,
    yFrom: written(yRange.value[0], 2),
    yTo: written(yRange.value[1], 2),
  })}`;
});

// The numbers: a row for each x where the series share theirs, or for each point where they don't.
type Row = Record<string, number | string> & { key: number };
const shared = computed(() => {
  const xs = first.value?.x;
  return (
    xs !== undefined &&
    [...props.series, ...props.bands.map((band) => ({ x: band.x }))].every(
      (one) =>
        one.x.length === xs.length &&
        Array.prototype.every.call(one.x, (value: number, n: number) => value === xs[n]),
    )
  );
});
const columns = computed<Column<Row>[]>(() => {
  const number = (key: string, label: string): Column<Row> => ({
    key,
    label,
    align: "end",
    sortable: true,
  });
  if (!shared.value) {
    return [
      { key: "series", label: t("ui.chart.series"), sortable: true },
      number("x", props.x.title),
      number("y", props.y.title),
    ];
  }
  const histogram = first.value?.kind === "histogram";
  return [
    ...(histogram
      ? [number("x", t("ui.chart.from")), number("to", t("ui.chart.to"))]
      : [number("x", props.x.title)]),
    ...props.series.map((series, n) => number(`s${n}`, series.name)),
    ...props.bands.flatMap((band, n) => {
      const name = props.series[band.of]?.name ?? "";
      return [
        number(`low${n}`, t("ui.chart.low", { name })),
        number(`high${n}`, t("ui.chart.high", { name })),
      ];
    }),
  ];
});
const rows = computed<Row[]>(() => {
  if (!first.value) return [];
  if (!shared.value) {
    return props.series.flatMap((series, s) =>
      Array.from(series.x, (x, n) => ({
        key: s * 1e9 + n,
        series: series.name,
        x,
        y: series.y[n]!,
      })),
    );
  }
  const width = first.value.width ?? 1;
  return Array.from(first.value.x, (x, n) => {
    const row: Row = { key: n, x, to: x + width };
    props.series.forEach((series, s) => (row[`s${s}`] = series.y[n]!));
    props.bands.forEach((band, b) => {
      row[`low${b}`] = band.lower[n]!;
      row[`high${b}`] = band.upper[n]!;
    });
    return row;
  });
});
const rowKey = (row: Row): number => row.key;

/** A series' look, for the legend: its colour, and its dash or mark. */
const swatches = computed(() =>
  props.series.map((series, n) => ({
    name: series.name,
    colour: `var(--chart-${(n % 6) + 1})`,
    dash: DASHES[n % DASHES.length]!.join(" "),
    mark: series.kind === "points" ? MARKS[n % MARKS.length]! : null,
  })),
);
</script>

<template>
  <figure class="chart">
    <figcaption class="title">{{ title }}</figcaption>
    <div
      ref="box"
      class="plot"
      role="img"
      tabindex="0"
      :aria-label="named"
      :style="{ blockSize: `${height}px` }"
      @keydown="key"
      @pointerdown="pressed"
      @pointermove="moved"
      @pointerup="released"
      @pointercancel="released"
      @pointerleave="cursor = drag ? cursor : null"
      @wheel="wheeled"
      @dblclick="showAll"
    >
      <canvas ref="canvas" class="canvas" />
    </div>
    <div class="under">
      <output class="readout" aria-live="polite">{{ readout }}</output>
      <Button v-if="zoomedIn" compact variant="outline" @click="showAll">{{
        t("ui.chart.showAll")
      }}</Button>
    </div>
    <ul v-if="series.length > 1" class="legend">
      <li v-for="swatch in swatches" :key="swatch.name">
        <svg class="swatch" viewBox="0 0 24 12" aria-hidden="true">
          <circle
            v-if="swatch.mark === 'circle'"
            cx="12"
            cy="6"
            r="4"
            :style="{ fill: swatch.colour }"
          />
          <rect
            v-else-if="swatch.mark === 'square'"
            x="8"
            y="2"
            width="8"
            height="8"
            :style="{ fill: swatch.colour }"
          />
          <polygon
            v-else-if="swatch.mark === 'triangle'"
            points="12,1 17,10 7,10"
            :style="{ fill: swatch.colour }"
          />
          <polygon
            v-else-if="swatch.mark === 'diamond'"
            points="12,0 18,6 12,12 6,6"
            :style="{ fill: swatch.colour }"
          />
          <line
            v-else
            x1="0"
            y1="6"
            x2="24"
            y2="6"
            :style="{ stroke: swatch.colour, strokeDasharray: swatch.dash }"
            stroke-width="2"
          />
        </svg>
        {{ swatch.name }}
      </li>
    </ul>
    <Collapsible :title="t('ui.chart.numbers')" :level="level">
      <DataTable
        class="numbers"
        :columns="columns"
        :rows="rows"
        :row-key="rowKey"
        :label="t('ui.chart.table', { title })"
      />
    </Collapsible>
  </figure>
</template>

<style scoped>
.chart {
  display: grid;
  gap: var(--space-2);
  min-inline-size: 0;
  margin: 0;
}

.title {
  font-weight: var(--font-weight-strong);
}

.plot {
  position: relative;
  min-inline-size: 0;
  touch-action: none;
  cursor: crosshair;
}

.plot:focus-visible {
  outline: var(--focus-width) solid var(--focus-color);
  outline-offset: var(--focus-offset);
}

.canvas {
  position: absolute;
  inset: 0;
  inline-size: 100%;
  block-size: 100%;
}

.under {
  display: flex;
  gap: var(--space-3);
  align-items: center;
  justify-content: space-between;
  min-block-size: var(--size-control-compact);
}

.readout {
  color: var(--text-muted);
  font-size: var(--font-size-small);
  font-variant-numeric: tabular-nums;
}

.legend {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-1) var(--space-4);
  margin: 0;
  padding: 0;
  list-style: none;
  font-size: var(--font-size-small);
}

.legend li {
  display: flex;
  gap: var(--space-1);
  align-items: center;
}

.swatch {
  inline-size: 1.5rem;
  block-size: 0.75rem;
}

.numbers {
  max-block-size: 20rem;
}
</style>
