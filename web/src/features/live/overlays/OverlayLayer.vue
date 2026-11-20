<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";

import type { Line, PixelRect } from "@/api/preview/message";
import { placed, sensorAt, type Size, type View } from "@/api/preview/view";
import {
  CORNERS,
  cornerOf,
  gridLines,
  gridStep,
  moved,
  nudge,
  resized,
  spanned,
  type Corner,
  type Point,
} from "@/features/live/overlays/geometry";
import { useOverlays } from "@/features/live/overlays/overlays";
import { onCanvas } from "@/features/live/viewer";

/**
 * The layers over the live image (W3.5, ADR-0049): SVG in the canvas's
 * device pixels, the shapes in the sensor's, placed by the view, so they
 * hold their place under zoom and pan. The crosshair and the grid are only
 * shown; the ROI and the line are drawn with the pointer, moved and
 * resized by it or by their handles' keys. Where nothing is drawn, the
 * pointer goes through to the image.
 */
const props = defineProps<{ view: View; extent: PixelRect; size: Size }>();
const { t } = useI18n();
const overlays = useOverlays();
const svg = ref<SVGSVGElement>();

/** The sensor's point under the pointer. */
function sensorOf(event: PointerEvent | MouseEvent): Point {
  const box = svg.value!.getBoundingClientRect();
  const at = onCanvas([event.clientX - box.left, event.clientY - box.top], box, props.size);
  return sensorAt(props.view, at, props.size);
}
/** A sensor's point on the canvas. */
function canvasOf(point: Point): Point {
  const spot = placed(props.view, { x: point[0], y: point[1], width: 0, height: 0 }, props.size);
  return [spot.x, spot.y];
}

const sensorToCanvas = computed(
  () =>
    `translate(${props.size.width / 2} ${props.size.height / 2}) scale(${props.view.scale}) ` +
    `translate(${-props.view.centre[0]} ${-props.view.centre[1]})`,
);
const middle = computed<Point>(() => [
  props.extent.x + props.extent.width / 2,
  props.extent.y + props.extent.height / 2,
]);
const grid = computed(() => gridLines(props.extent, gridStep(props.view.scale)));

// A drag: drawing a shape, or moving one, or one of its handles.
type Drag =
  | { kind: "draw"; from: Point }
  | { kind: "roi"; from: Point; was: PixelRect }
  | { kind: "corner"; corner: Corner; was: PixelRect }
  | { kind: "end"; end: 0 | 1 };
let drag: Drag | undefined;

function grab(event: PointerEvent, what: Drag): void {
  // While a tool is chosen, a press on a shape draws, as anywhere else on the image.
  if (event.button !== 0 || (overlays.tool !== "none" && what.kind !== "draw")) return;
  event.stopPropagation();
  // On what was pressed, which takes the pointer: its moves come up to the layer.
  (event.currentTarget as Element).setPointerCapture(event.pointerId);
  drag = what;
}
function start(event: PointerEvent): void {
  if (overlays.tool === "none") return;
  grab(event, { kind: "draw", from: sensorOf(event) });
}
function follow(event: PointerEvent): void {
  if (!drag) return;
  const here = sensorOf(event);
  if (drag.kind === "draw" && overlays.tool === "roi") {
    overlays.roi = spanned(drag.from, here, props.extent);
  } else if (drag.kind === "draw") {
    overlays.line = { from: drag.from, to: here };
  } else if (drag.kind === "roi") {
    const by: Point = [here[0] - drag.from[0], here[1] - drag.from[1]];
    overlays.roi = moved(drag.was, by, props.extent);
  } else if (drag.kind === "corner") {
    overlays.roi = resized(drag.was, drag.corner, here, props.extent);
  } else if (overlays.line) {
    overlays.line = endAt(overlays.line, drag.end, here);
  }
}
function end(): void {
  // A shape drawn, the pointer goes back to the image.
  if (drag?.kind === "draw") overlays.tool = "none";
  drag = undefined;
}

/** The line with its end `end` at `to`. */
function endAt(line: Line, end: 0 | 1, to: Point): Line {
  return end === 0 ? { from: to, to: line.to } : { from: line.from, to };
}

// The handles from the keyboard: the arrows move them a pixel, ten with Shift.
function keyRoi(event: KeyboardEvent): void {
  const by = nudge(event.key, event.shiftKey);
  if (!by || !overlays.roi) return;
  event.preventDefault();
  overlays.roi = moved(overlays.roi, by, props.extent);
}
function keyCorner(event: KeyboardEvent, corner: Corner): void {
  const by = nudge(event.key, event.shiftKey);
  if (!by || !overlays.roi) return;
  event.preventDefault();
  const [x, y] = cornerOf(overlays.roi, corner);
  overlays.roi = resized(overlays.roi, corner, [x + by[0], y + by[1]], props.extent);
}
function keyEnd(event: KeyboardEvent, end: 0 | 1): void {
  const by = nudge(event.key, event.shiftKey);
  const line = overlays.line;
  if (!by || !line) return;
  event.preventDefault();
  const at = end === 0 ? line.from : line.to;
  overlays.line = endAt(line, end, [at[0] + by[0], at[1] + by[1]]);
}

const corners = computed(() =>
  overlays.roi
    ? CORNERS.map((corner) => ({ corner, at: canvasOf(cornerOf(overlays.roi!, corner)) }))
    : [],
);
const ends = computed(() =>
  overlays.line ? [canvasOf(overlays.line.from), canvasOf(overlays.line.to)] : [],
);
</script>

<template>
  <svg
    ref="svg"
    class="overlays"
    :class="{ drawing: overlays.tool !== 'none' }"
    :viewBox="`0 0 ${size.width} ${size.height}`"
    preserveAspectRatio="none"
    data-said="overlays"
    @pointerdown="start"
    @pointermove="follow"
    @pointerup="end"
    @pointercancel="end"
  >
    <g :transform="sensorToCanvas">
      <g v-if="overlays.layers.grid" class="guide" data-layer="grid">
        <line
          v-for="x in grid.xs"
          :key="`x${x}`"
          :x1="x"
          :x2="x"
          :y1="extent.y"
          :y2="extent.y + extent.height"
          vector-effect="non-scaling-stroke"
        />
        <line
          v-for="y in grid.ys"
          :key="`y${y}`"
          :x1="extent.x"
          :x2="extent.x + extent.width"
          :y1="y"
          :y2="y"
          vector-effect="non-scaling-stroke"
        />
      </g>
      <g v-if="overlays.layers.crosshair" class="guide cross" data-layer="crosshair">
        <line
          :x1="extent.x"
          :x2="extent.x + extent.width"
          :y1="middle[1]"
          :y2="middle[1]"
          vector-effect="non-scaling-stroke"
        />
        <line
          :x1="middle[0]"
          :x2="middle[0]"
          :y1="extent.y"
          :y2="extent.y + extent.height"
          vector-effect="non-scaling-stroke"
        />
      </g>
      <rect
        v-if="overlays.roi"
        class="shape roi"
        :x="overlays.roi.x"
        :y="overlays.roi.y"
        :width="overlays.roi.width"
        :height="overlays.roi.height"
        vector-effect="non-scaling-stroke"
        tabindex="0"
        role="button"
        :aria-label="t('live.overlays.roi', { ...overlays.roi })"
        data-said="roi"
        @pointerdown="grab($event, { kind: 'roi', from: sensorOf($event), was: overlays.roi })"
        @keydown="keyRoi"
      />
      <line
        v-if="overlays.line"
        class="shape profile"
        :x1="overlays.line.from[0]"
        :y1="overlays.line.from[1]"
        :x2="overlays.line.to[0]"
        :y2="overlays.line.to[1]"
        vector-effect="non-scaling-stroke"
        data-said="line"
      />
    </g>
    <circle
      v-for="one in corners"
      :key="one.corner"
      class="handle"
      :cx="one.at[0]"
      :cy="one.at[1]"
      r="7"
      tabindex="0"
      role="button"
      :aria-label="t(`live.overlays.corners.${one.corner}`)"
      @pointerdown="grab($event, { kind: 'corner', corner: one.corner, was: overlays.roi! })"
      @keydown="keyCorner($event, one.corner)"
    />
    <circle
      v-for="(at, index) in ends"
      :key="`end${index}`"
      class="handle"
      :cx="at[0]"
      :cy="at[1]"
      r="7"
      tabindex="0"
      role="button"
      :aria-label="t(index === 0 ? 'live.overlays.lineFrom' : 'live.overlays.lineTo')"
      @pointerdown="grab($event, { kind: 'end', end: index === 0 ? 0 : 1 })"
      @keydown="keyEnd($event, index === 0 ? 0 : 1)"
    />
  </svg>
</template>

<style scoped>
/* Over the canvas, letting the pointer through but to what is drawn. */
.overlays {
  position: absolute;
  inset: 0;
  inline-size: 100%;
  block-size: 100%;
  pointer-events: none;
}

.overlays.drawing {
  cursor: crosshair;
  pointer-events: all;
}

/* The guides in the accent, faint: the image shows through. */
.guide line {
  stroke: var(--accent);
  stroke-opacity: 0.6;
  stroke-width: 1;
}

.cross line {
  stroke-dasharray: 6 4;
}

.shape {
  fill: none;
  stroke: var(--accent);
  stroke-width: 2;
  pointer-events: stroke;
  cursor: move;
}

.roi {
  pointer-events: all;
  fill: transparent;
}

.handle {
  fill: var(--surface-page);
  stroke: var(--accent);
  stroke-width: 2;
  pointer-events: all;
  cursor: grab;
}

.shape:focus-visible,
.handle:focus-visible {
  outline: none;
  stroke: var(--focus-color);
  stroke-width: 3;
}
</style>
