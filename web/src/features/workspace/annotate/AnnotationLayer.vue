<script setup lang="ts">
import { computed, shallowRef } from "vue";

import {
  handlesOf,
  moved,
  outlineOf,
  reshaped,
  type Shape,
} from "@/features/workspace/annotate/shapes";
import type { Placement, Point } from "@/ui/tile-viewer/coordinates";

/**
 * The annotations over the image (W7.5), in its pixels, in the tile viewer's
 * own layer: each shape; the one selected with its handles; the one being
 * drawn; and the keyboard's cross. With no tool chosen, a shape pressed is
 * selected, and dragged, or reshaped by a handle: the change is given back
 * at the drag's end. The pointer is read through the layer's own transform,
 * so a drag is right at any zoom.
 */
const props = defineProps<{
  placement: Placement;
  shapes: readonly Shape[];
  selected: number | undefined;
  /** The selected shape's handle chosen from the keyboard, if one is. */
  handle: number | undefined;
  /** The shape being drawn, through the pointer. */
  draft: Shape | undefined;
  /** Where the keyboard would place a point. */
  cross: Point | undefined;
  /** Screen pixels to a µm, for handles and margins of one size on the screen. */
  scale: number;
  /** Whether a press selects and drags: with no tool chosen. */
  editing: boolean;
}>();
const emit = defineEmits<{
  select: [index: number | undefined];
  change: [index: number, shape: Shape];
}>();

const px = (point: Point) =>
  `${(point.x / props.placement.pixel_um).toFixed(2)},${(point.y / props.placement.pixel_um).toFixed(2)}`;
const pointsOf = (shape: Shape) => outlineOf(shape).map(px).join(" ");
/** A length on the screen, in the image's pixels. */
const screen = (pixels: number) => pixels / Math.max(props.scale * props.placement.pixel_um, 1e-9);

/** The drag under way: what, from where, and the shape as it is now. */
const dragging = shallowRef<{ index: number; handle?: number; from: Point; now: Shape }>();
const shown = computed(() =>
  props.shapes.map((shape, i) => (dragging.value?.index === i ? dragging.value.now : shape)),
);

/** The pointer's place in µm, through the layer's own transform. */
function at(event: PointerEvent): Point | undefined {
  const layer = (event.currentTarget as SVGElement).ownerSVGElement;
  const matrix = layer?.getScreenCTM()?.inverse();
  if (!matrix) return undefined;
  const point = new DOMPoint(event.clientX, event.clientY).matrixTransform(matrix);
  return { x: point.x * props.placement.pixel_um, y: point.y * props.placement.pixel_um };
}
function press(event: PointerEvent, index: number, handle?: number): void {
  if (!props.editing) return;
  // Not the viewer's pan: the shape's drag.
  event.stopPropagation();
  const from = at(event);
  if (!from) return;
  emit("select", index);
  (event.currentTarget as Element).setPointerCapture(event.pointerId);
  dragging.value = {
    index,
    from,
    now: props.shapes[index]!,
    ...(handle === undefined ? {} : { handle }),
  };
}
function move(event: PointerEvent): void {
  const drag = dragging.value;
  const to = drag && at(event);
  if (!drag || !to) return;
  const shape = props.shapes[drag.index]!;
  const by = { x: to.x - drag.from.x, y: to.y - drag.from.y };
  dragging.value = {
    ...drag,
    now: drag.handle === undefined ? moved(shape, by) : reshaped(shape, drag.handle, to),
  };
}
function release(): void {
  const drag = dragging.value;
  dragging.value = undefined;
  if (drag && drag.now !== props.shapes[drag.index]) emit("change", drag.index, drag.now);
}
</script>

<template>
  <g class="annotations" :class="{ editing }" data-said="annotations">
    <template v-for="(shape, i) in shown" :key="shape.id ?? `new-${i}`">
      <circle
        v-if="shape.kind === 'point'"
        class="shape"
        :class="{ selected: i === selected }"
        :cx="shape.at.x / placement.pixel_um"
        :cy="shape.at.y / placement.pixel_um"
        :r="screen(5)"
        :data-shape="shape.id"
        @pointerdown="press($event, i)"
        @pointermove="move"
        @pointerup="release"
      />
      <component
        :is="shape.kind === 'line' ? 'polyline' : 'polygon'"
        v-else
        class="shape"
        :class="{ selected: i === selected }"
        :points="pointsOf(shape)"
        :data-shape="shape.id"
        @pointerdown="press($event, i)"
        @pointermove="move"
        @pointerup="release"
      />
    </template>
    <template v-if="selected !== undefined && shown[selected]">
      <rect
        v-for="(handleAt, h) in handlesOf(shown[selected]!)"
        :key="h"
        class="handle"
        :class="{ chosen: h === handle }"
        :x="handleAt.x / placement.pixel_um - screen(5)"
        :y="handleAt.y / placement.pixel_um - screen(5)"
        :width="screen(10)"
        :height="screen(10)"
        :data-handle="h"
        @pointerdown="press($event, selected!, h)"
        @pointermove="move"
        @pointerup="release"
      />
    </template>
    <template v-if="draft">
      <circle
        v-if="draft.kind === 'point'"
        class="draft"
        :cx="draft.at.x / placement.pixel_um"
        :cy="draft.at.y / placement.pixel_um"
        :r="screen(5)"
      />
      <component
        :is="draft.kind === 'line' ? 'polyline' : 'polygon'"
        v-else
        class="draft"
        :points="pointsOf(draft)"
      />
    </template>
    <g v-if="cross" class="cross" data-said="cross">
      <line
        :x1="cross.x / placement.pixel_um - screen(8)"
        :y1="cross.y / placement.pixel_um"
        :x2="cross.x / placement.pixel_um + screen(8)"
        :y2="cross.y / placement.pixel_um"
      />
      <line
        :x1="cross.x / placement.pixel_um"
        :y1="cross.y / placement.pixel_um - screen(8)"
        :x2="cross.x / placement.pixel_um"
        :y2="cross.y / placement.pixel_um + screen(8)"
      />
    </g>
  </g>
</template>

<style scoped>
.annotations * {
  vector-effect: non-scaling-stroke;
  stroke-width: 2;
}

.shape {
  fill: color-mix(in srgb, var(--chart-3) 12%, transparent);
  stroke: var(--chart-3);
}

polyline.shape,
polyline.draft {
  fill: none;
}

/* Pressed, a shape is selected and dragged: only with no tool chosen. */
.editing .shape,
.editing .handle {
  pointer-events: auto;
  cursor: move;
}

.shape.selected {
  stroke: var(--accent);
  stroke-width: 3;
}

.handle {
  fill: var(--surface-raised);
  stroke: var(--accent);
}

.handle.chosen {
  fill: var(--accent);
}

.draft {
  fill: color-mix(in srgb, var(--accent) 10%, transparent);
  stroke: var(--accent);
  stroke-dasharray: 6 4;
}

.cross line {
  stroke: var(--accent);
}
</style>
