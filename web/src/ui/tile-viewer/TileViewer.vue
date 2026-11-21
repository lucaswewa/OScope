<script setup lang="ts">
import OpenSeadragon from "openseadragon";
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import { roundLength } from "../lengths";
import Toolbar from "../Toolbar.vue";
import ToolbarButton from "../ToolbarButton.vue";
import {
  stageRectToViewport,
  viewportToStage,
  type Placement,
  type Point,
  type Rect,
} from "./coordinates";
import { between, pointsIn, rectShape, type Shape } from "./shapes";
import { sourceOf, type TiledImage } from "./source";

/**
 * A deep-zoom image of tiles, on the stage (W3.8, ADR-0050): the one place
 * OpenSeadragon is used. The image's tiles come from the caller; its places
 * are the stage's µm, by its placement; its controls are the app's own, with
 * a scale bar, the pointer's place and a navigator. A double click says
 * where, on the stage. A new key for the image fetches its tiles in view
 * again, drawn over the old until they have come, so that nothing blinks.
 *
 * Shapes on the stage are drawn over the image, zooming with it (W5.1).
 * While drawing, a drag draws a rectangle of the stage instead of panning,
 * and says it when let go.
 */
const props = defineProps<{
  label: string;
  image: TiledImage | undefined;
  placement: Placement | undefined;
  /** A rectangle of the stage marked over the image: the field of view. */
  marker?: Rect | undefined;
  /** Where to look when an image first opens, on the stage: all of it, without. */
  start?: Rect | undefined;
  /** Where to look, on the stage, each time it changes. */
  fit?: Rect | undefined;
  /** Shapes of the stage, drawn over the image. */
  shapes?: readonly Shape[] | undefined;
  /** Whether a drag draws a rectangle, which `draw` then says, instead of panning. */
  drawing?: boolean;
}>();
const emit = defineEmits<{ pick: [at: Point]; draw: [rect: Rect] }>();
const { t } = useI18n();

const host = ref<HTMLDivElement>();
const field = ref<HTMLDivElement>();
const layer = ref<SVGSVGElement>();
let viewer: OpenSeadragon.Viewer | undefined;
let tracker: OpenSeadragon.MouseTracker | undefined;
const scale = shallowRef<{ length: number; width: number }>();
const pointer = shallowRef<Point>();
/** A zoom button's step. */
const STEP = 1.5;
/** The longest the scale bar may be, in CSS pixels. */
const ROOM = 120;

/** The stage's place under a point of the viewer, in its pixels. */
function stageAt(position: OpenSeadragon.Point): Point | undefined {
  if (!viewer || !props.placement) return undefined;
  const point = viewer.viewport.pointFromPixel(position);
  return viewportToStage(props.placement, point);
}

/** The scale bar, from the µm a CSS pixel spans now. */
function measure(): void {
  if (!viewer || !props.placement) return void (scale.value = undefined);
  const across = viewer.viewport.deltaPixelsFromPoints(new OpenSeadragon.Point(1, 0), true).x;
  if (!(across > 0)) return;
  const perPixel = (props.placement.pixel_um * props.placement.width) / across;
  const length = roundLength(ROOM * perPixel);
  scale.value = { length, width: length / perPixel };
}
const scaleSaid = computed(() => {
  const length = scale.value?.length ?? 0;
  return length >= 1000
    ? t("ui.tileViewer.mm", { length: length / 1000 })
    : t("ui.tileViewer.microns", { length });
});

/** Looks where `start` says, or at all of the image. */
function look(): void {
  if (!viewer || !props.placement) return;
  if (!props.start) return void viewer.viewport.goHome(true);
  const { x, y, width, height } = stageRectToViewport(props.placement, props.start);
  viewer.viewport.fitBounds(new OpenSeadragon.Rect(x, y, width, height), true);
}

/** Looks at `rect` of the stage, as `fit` asks. */
function fitTo(rect: Rect | undefined): void {
  if (!viewer || !props.placement || !rect) return;
  const { x, y, width, height } = stageRectToViewport(props.placement, rect);
  viewer.viewport.fitBounds(new OpenSeadragon.Rect(x, y, width, height));
}

/** The field of view, marked over the image. */
let marked = false;
function mark(): void {
  const element = field.value;
  if (!viewer || !element) return;
  const rect =
    props.marker && props.placement
      ? stageRectToViewport(props.placement, props.marker)
      : undefined;
  if (!rect) {
    if (marked) viewer.removeOverlay(element);
    marked = false;
    return;
  }
  const location = new OpenSeadragon.Rect(rect.x, rect.y, rect.width, rect.height);
  if (marked) viewer.updateOverlay(element, location);
  else viewer.addOverlay({ element, location });
  marked = true;
}

/** The shapes' layer, laid over the whole image. */
let layered = false;
function lay(): void {
  const element = layer.value;
  if (!viewer || !element) return;
  const at = props.placement;
  if (!at) {
    if (layered) viewer.removeOverlay(element);
    layered = false;
    return;
  }
  const location = new OpenSeadragon.Rect(0, 0, 1, at.height / at.width);
  if (layered) viewer.updateOverlay(element, location);
  else viewer.addOverlay({ element, location });
  layered = true;
}

/** The rectangle being drawn: where the drag began, and where it is. */
const dragged = shallowRef<{ from: Point; to: Point }>();
const drawn = computed(() =>
  dragged.value ? rectShape("region", between(dragged.value.from, dragged.value.to)) : undefined,
);
const allShapes = computed(() => [...(props.shapes ?? []), ...(drawn.value ? [drawn.value] : [])]);
/** The viewer's own panning and zooming, unless a drag draws. */
function dragging(): void {
  viewer?.setMouseNavEnabled(!props.drawing);
}

/** The image's key shown, and whether a newer is being drawn over it. */
let shown: string | undefined;
let swapping = false;
function show(): void {
  if (!viewer) return;
  const image = props.image;
  if (!image) {
    viewer.world.removeAll();
    shown = undefined;
    return;
  }
  if (image.key === shown || swapping) return;
  const world = viewer.world;
  const older = Array.from({ length: world.getItemCount() }, (_, i) => world.getItemAt(i));
  const first = older.length === 0;
  shown = image.key;
  swapping = !first;
  viewer.addTiledImage({
    tileSource: sourceOf(image),
    index: older.length,
    success: (event) => {
      const item = (event as unknown as { item: OpenSeadragon.TiledImage }).item;
      if (first) {
        look();
        measure();
        return;
      }
      // Over the older, which goes once the tiles in view have come; or after a
      // moment, whatever came. A key that changed meanwhile is shown next.
      const drop = () => {
        if (!swapping) return;
        swapping = false;
        for (const one of older) world.removeItem(one);
        if (props.image && props.image.key !== shown) show();
      };
      if (item.getFullyLoaded()) drop();
      else {
        item.addOnceHandler("fully-loaded-change", drop);
        setTimeout(drop, 2_000);
      }
    },
    error: () => {
      swapping = false;
      shown = undefined;
    },
  });
}

function zoom(by: number): void {
  viewer?.viewport.zoomBy(by);
  viewer?.viewport.applyConstraints();
}

onMounted(() => {
  const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
  viewer = OpenSeadragon({
    element: host.value!,
    // The 2D canvas: pixels past one to one drawn as squares, and every browser alike.
    drawer: "canvas",
    imageSmoothingEnabled: false,
    showNavigationControl: false,
    // The navigator stays: OpenSeadragon would fade it, as a control, once the pointer leaves.
    autoHideControls: false,
    showNavigator: true,
    navigatorPosition: "TOP_RIGHT",
    navigatorSizeRatio: 0.2,
    navigatorBackground: "var(--surface-image)",
    navigatorBorderColor: "var(--border-strong)",
    navigatorDisplayRegionColor: "var(--accent)",
    gestureSettingsMouse: { clickToZoom: false, dblClickToZoom: false },
    gestureSettingsTouch: { dblClickToZoom: false },
    zoomPerScroll: 1.25,
    maxZoomPixelRatio: 4,
    visibilityRatio: 0.1,
    animationTime: still ? 0 : 0.4,
    preserveImageSizeOnResize: true,
  });
  viewer.addHandler("canvas-double-click", (event) => {
    event.preventDefaultAction = true;
    const at = stageAt(event.position);
    if (at) emit("pick", at);
  });
  viewer.addHandler("animation", measure);
  viewer.addHandler("resize", measure);
  tracker = new OpenSeadragon.MouseTracker({
    element: viewer.canvas,
    moveHandler: (event) => (pointer.value = stageAt(event.position)),
    leaveHandler: () => (pointer.value = undefined),
    pressHandler: (event) => {
      const at = props.drawing ? stageAt(event.position) : undefined;
      dragged.value = at ? { from: at, to: at } : undefined;
    },
    dragHandler: (event) => {
      const at = dragged.value && stageAt(event.position);
      if (dragged.value && at) dragged.value = { from: dragged.value.from, to: at };
    },
    releaseHandler: () => {
      const rect = dragged.value && between(dragged.value.from, dragged.value.to);
      dragged.value = undefined;
      if (rect && rect.width > 0 && rect.height > 0) emit("draw", rect);
    },
  });
  dragging();
  show();
  mark();
  lay();
});
onBeforeUnmount(() => {
  tracker?.destroy();
  viewer?.destroy();
  viewer = undefined;
});

watch(() => props.image, show);
watch(() => [props.marker, props.placement], mark);
watch(() => props.placement, measure);
watch(() => props.placement, lay);
watch(() => props.drawing, dragging);
// Where to look, again when the image beneath is another, whose own
// coordinates the view is in (W5.2): by value, as each is made anew.
const valueOf = (rect: Rect | Placement | undefined) => rect && JSON.stringify(rect);
watch(
  () => [valueOf(props.fit), valueOf(props.placement)],
  () => fitTo(props.fit),
);
</script>

<template>
  <section class="tile-viewer" :aria-label="label">
    <Toolbar :label="t('ui.tileViewer.controls')">
      <ToolbarButton icon="zoomOut" @click="zoom(1 / STEP)">
        {{ t("ui.tileViewer.zoomOut") }}
      </ToolbarButton>
      <ToolbarButton icon="zoomIn" @click="zoom(STEP)">
        {{ t("ui.tileViewer.zoomIn") }}
      </ToolbarButton>
      <ToolbarButton icon="fit" @click="viewer?.viewport.goHome()">
        {{ t("ui.tileViewer.fit") }}
      </ToolbarButton>
    </Toolbar>
    <div class="frame" :class="{ drawing }">
      <div ref="host" class="viewer" data-said="tiles" />
      <div ref="field" class="field" data-said="field" />
      <svg
        ref="layer"
        class="layer"
        data-said="shapes"
        aria-hidden="true"
        preserveAspectRatio="none"
        :viewBox="placement ? `0 0 ${placement.width} ${placement.height}` : undefined"
      >
        <template v-if="placement">
          <template v-for="(shape, i) in allShapes" :key="i">
            <polyline
              v-if="shape.kind === 'path'"
              :class="shape.kind"
              :points="pointsIn(placement, shape)"
            />
            <polygon v-else :class="shape.kind" :points="pointsIn(placement, shape)" />
          </template>
        </template>
      </svg>
      <figure v-if="scale" class="scale" data-said="tile-scale">
        <span class="line" :style="{ inlineSize: `${scale.width}px` }" />
        <figcaption>{{ scaleSaid }}</figcaption>
      </figure>
      <p v-if="pointer" class="at" data-said="at">
        {{ t("ui.tileViewer.at", { x: pointer.x.toFixed(1), y: pointer.y.toFixed(1) }) }}
      </p>
    </div>
  </section>
</template>

<style scoped>
.tile-viewer {
  display: grid;
  grid-template-rows: auto minmax(0, 1fr);
  block-size: 100%;
  min-block-size: 0;
}

.frame {
  position: relative;
  min-block-size: 0;
  overflow: hidden;
  background: var(--surface-image);
}

.viewer {
  position: absolute;
  inset: 0;
}

.drawing {
  cursor: crosshair;
}

/* The shapes: thin whatever the zoom, and never in the pointer's way. */
.layer {
  pointer-events: none;
  overflow: visible;
}

.layer polygon,
.layer polyline {
  fill: none;
  stroke: var(--accent);
  stroke-width: 1px;
  vector-effect: non-scaling-stroke;
}

.layer .frame {
  fill: color-mix(in srgb, var(--accent) 12%, transparent);
}

.layer .first {
  fill: color-mix(in srgb, var(--accent) 40%, transparent);
  stroke-width: 2px;
}

/* The path in the accent, as the live view's overlays: the image is black in either scheme. */
.layer .path {
  stroke-dasharray: 3 3;
}

.layer .region {
  stroke-width: 2px;
  stroke-dasharray: 6 4;
}

/* A scan's tiles (W5.2): taken, the next, and the one at fault. */
.layer .done {
  fill: color-mix(in srgb, var(--success) 30%, transparent);
  stroke: var(--success);
}

.layer .next {
  fill: color-mix(in srgb, var(--accent) 40%, transparent);
  stroke-width: 2px;
}

.layer .fault {
  fill: color-mix(in srgb, var(--danger) 40%, transparent);
  stroke: var(--danger);
  stroke-width: 2px;
}

/* The field of view: where the camera looks now. */
.field {
  border: 2px solid var(--accent);
  pointer-events: none;
}

/* On chips, as the live view's scale bar is: legible on the image and on the dark. */
.scale,
.at {
  position: absolute;
  inset-block-end: var(--space-3);
  margin: 0;
  padding: var(--space-1) var(--space-2);
  border-radius: var(--radius-surface);
  background: var(--surface-inverse);
  color: var(--text-inverse);
  font-size: var(--font-size-small);
  pointer-events: none;
}

.scale {
  inset-inline-start: var(--space-3);
  display: grid;
  gap: var(--space-1);
  justify-items: center;
}

.line {
  block-size: 3px;
  background: currentColor;
}

.at {
  inset-inline-end: var(--space-3);
  font-variant-numeric: tabular-nums;
}
</style>
