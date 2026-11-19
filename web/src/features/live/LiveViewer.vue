<script setup lang="ts">
import { computed, onBeforeUnmount, ref, shallowRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import { usePreview } from "@/api/preview/use";
import { sensorAt } from "@/api/preview/view";
import { useProperty } from "@/api/things/use";
import { keyStep, written } from "@/features/control/stage/stage";
import { useChoices } from "@/features/control/stage/choices";
import { usePointing } from "@/features/control/stage/usePointing";
import OverlayMeasures from "@/features/live/overlays/OverlayMeasures.vue";
import OverlayLayer from "@/features/live/overlays/OverlayLayer.vue";
import { useOverlays } from "@/features/live/overlays/overlays";
import { useStreaming } from "@/features/live/streaming";
import {
  fromCentre,
  onCanvas,
  percent,
  pixelAt,
  scaleBar,
  STEP,
  wheelFactor,
} from "@/features/live/viewer";
import { host } from "@/host/host";
import { useDisabled } from "@/ui/control";
import ControlScope from "@/ui/ControlScope.vue";
import Toolbar from "@/ui/Toolbar.vue";
import ToolbarButton from "@/ui/ToolbarButton.vue";

/**
 * The live image (W3.1, ADR-0049): the camera's preview on W1.5's client,
 * fitted to the view. The wheel zooms about the pointer, and dragging pans;
 * zoomed in, the region shown is asked for at the sensor's resolution, its
 * pixels squares past one to one. Streaming starts and stops here. Its
 * state, rate and latency, the zoom and the place under the pointer are
 * read out, with a scale bar. Full screen, and the view saved as a PNG.
 *
 * The stage (W3.3): on the focused image, the arrows step or jog it along
 * x and y, and Page Up and Page Down along z, by the pane's step. With the
 * active calibration, a double click brings the place to the centre, a
 * drag pans the stage when the stage's pane says so, and the place under
 * the pointer is read out in the stage's coordinates.
 */
const { t } = useI18n();
const root = ref<HTMLElement>();
const canvas = ref<HTMLCanvasElement>();
const preview = usePreview(canvas);
const streaming = useStreaming(computed(() => preview.numbers.value.drawn));
const capabilities = useProperty("/api/v1/camera/capabilities", { every: 60_000 });
const side = useProperty("/api/v1/objective/pixel_at_sample", { every: 5_000 });

const full = shallowRef(host().window.isFullScreen());
onBeforeUnmount(host().window.onFullScreen((now) => (full.value = now)));

/** The longest the scale bar may be, in CSS pixels. */
const ROOM = 160;
const ratio = () => globalThis.devicePixelRatio || 1;

/** The pointer's place on the canvas, in device pixels, while it is over it. */
const pointer = shallowRef<[number, number]>();
/** Where a drag last was, in device pixels. */
let held: [number, number] | undefined;

const pointing = usePointing();
const overlays = useOverlays();
// The statistics follow what is drawn: the ROI's, and the line's profile (W3.5).
watch(
  () => [overlays.roi, overlays.line] as const,
  ([roi, line]) => preview.measure(roi || line ? { roi, profile: line } : undefined),
  { immediate: true },
);
/** Switches a drawing tool on, or off again. */
function draw(tool: "roi" | "line"): void {
  overlays.tool = overlays.tool === tool ? "none" : tool;
}
const choices = useChoices();
const locked = useDisabled({});
/** A drag of the stage: where it began, in the sensor's pixels and on the page. */
let dragged: { from: [number, number]; client: [number, number] } | undefined;
/** How far the image is drawn aside while the stage is dragged, in CSS pixels. */
const aside = shallowRef<[number, number]>();
/** How far a drag must go to move the stage, in CSS pixels: a click doesn't. */
const DRAG = 4;

function at(event: MouseEvent): [number, number] {
  const box = canvas.value!.getBoundingClientRect();
  return onCanvas([event.clientX - box.left, event.clientY - box.top], box, preview.size());
}
const middle = (): [number, number] => {
  const size = preview.size();
  return [size.width / 2, size.height / 2];
};

function wheel(event: WheelEvent): void {
  preview.zoomBy(wheelFactor(event.deltaY), at(event));
}
function down(event: PointerEvent): void {
  if (event.button !== 0) return;
  canvas.value?.setPointerCapture(event.pointerId);
  const view = preview.view.value;
  if (choices.drags && view && !locked.value) {
    dragged = {
      from: sensorAt(view, at(event), preview.size()),
      client: [event.clientX, event.clientY],
    };
  } else {
    held = at(event);
  }
}
function move(event: PointerEvent): void {
  const here = at(event);
  pointer.value = here;
  if (dragged) aside.value = [event.clientX - dragged.client[0], event.clientY - dragged.client[1]];
  if (!held) return;
  preview.panBy([here[0] - held[0], here[1] - held[1]]);
  held = here;
}
function up(): void {
  const drag = dragged;
  const by = aside.value;
  cancel();
  const view = preview.view.value;
  if (!drag || !by || !view || Math.hypot(...by) < DRAG) return;
  // From the drag's own length: the image drawn aside has moved the canvas's box.
  const [dx, dy] = onCanvas(by, canvas.value!.getBoundingClientRect(), preview.size());
  pointing.bring(drag.from, [drag.from[0] + dx / view.scale, drag.from[1] + dy / view.scale]);
}
function cancel(): void {
  held = undefined;
  dragged = undefined;
  aside.value = undefined;
}
function left(): void {
  if (!held && !dragged) pointer.value = undefined;
}
/** A double click brings the place to the view's centre. */
function centre(event: MouseEvent): void {
  const view = preview.view.value;
  if (locked.value || pointing.lacking() || !view) return;
  pointing.bring(sensorAt(view, at(event), preview.size()), view.centre);
}

/**
 * The keys on the image: + and − zoom, 0 fits, 1 shows one to one, F fills
 * the screen; the arrows, Page Up and Page Down move the stage (W3.3).
 */
function key(event: KeyboardEvent): void {
  if (event.ctrlKey || event.metaKey || event.altKey) return;
  const towards = keyStep(event.key);
  if (towards) {
    // The arrows and the page keys are the stage's here, not the page's.
    event.preventDefault();
    const size = choices.size(towards.axis);
    if (locked.value) return;
    if (size !== undefined) void pointing.stage.step(towards, size);
    else if (!event.repeat) pointing.stage.jog(towards);
    return;
  }
  const keys: Record<string, () => void> = {
    "+": () => preview.zoomBy(STEP, middle()),
    "=": () => preview.zoomBy(STEP, middle()),
    "-": () => preview.zoomBy(1 / STEP, middle()),
    "0": () => preview.whole(),
    "1": () => preview.oneToOne(middle()),
    f: () => void toggleFull(),
  };
  const run = keys[event.key];
  if (!run) return;
  // Handled here, the shell's shortcuts leave it alone.
  event.preventDefault();
  run();
}

/** A jog lasts as long as its key is held. */
function keyUp(event: KeyboardEvent): void {
  if (keyStep(event.key)) void pointing.stage.endJog();
}

async function toggleFull(): Promise<void> {
  if (full.value) await host().window.leaveFullScreen();
  else if (root.value) await host().window.enterFullScreen(root.value);
}

async function save(): Promise<void> {
  const png = await preview.snapshot();
  const stamp = new Date().toISOString().replace(/[:.]/g, "-");
  if (png) await host().files.save(`live-${stamp}.png`, png);
}

const state = streaming.state;
const pixel = computed(() => {
  const view = preview.view.value;
  const extent = preview.extent.value;
  if (!view || !extent || !pointer.value) return undefined;
  return pixelAt(view, pointer.value, preview.size(), extent);
});
const atSample = computed(() => {
  const caps = capabilities.value.value;
  const um = side.value.value;
  if (!pixel.value || !caps || !um) return undefined;
  return fromCentre(pixel.value, { width: caps.sensor_width, height: caps.sensor_height }, um);
});
/** With a calibration, the place under the pointer on the stage: the stage is where the sensor's centre is. */
const onStage = computed(() => {
  const caps = capabilities.value.value;
  if (!pixel.value || !caps) return undefined;
  const middle: [number, number] = [caps.sensor_width / 2, caps.sensor_height / 2];
  return pointing.placeOf([pixel.value[0] + 0.5, pixel.value[1] + 0.5], middle);
});
const bar = computed(() => {
  const view = preview.view.value;
  return view ? scaleBar(side.value.value, view, ratio(), ROOM) : undefined;
});
const one = (value: number) => value.toFixed(1);
const stageXy = (x: number, y: number) => {
  const unit = pointing.unit.value;
  return t("live.stageXy", {
    x: written(x, unit),
    y: written(y, unit),
    unit: t(`stage.units.${unit}`),
  });
};
const readouts = computed(() => {
  const numbers = preview.numbers.value;
  const view = preview.view.value;
  const none = t("live.none");
  const zoom = view ? percent(view) : undefined;
  return [
    { term: t("live.state"), value: t(`live.states.${state.value}`), said: "state" },
    { term: t("live.rate"), value: t("live.fps", { fps: one(numbers.fps) }), said: "rate" },
    {
      term: t("live.latency"),
      value: t("live.ms", { ms: numbers.latency.toFixed(0) }),
      said: "latency",
    },
    {
      term: t("live.zoom"),
      value:
        zoom === undefined
          ? none
          : t("live.percent", { percent: zoom < 100 ? one(zoom) : zoom.toFixed(0) }),
      said: "zoom",
    },
    {
      term: t("live.frame"),
      value: numbers.last ? t("live.region", { ...numbers.last.roi }) : none,
      said: "frame",
    },
    {
      term: t("live.pixel"),
      value: pixel.value ? t("live.xy", { x: pixel.value[0], y: pixel.value[1] }) : none,
      said: "pixel",
    },
    pointing.geometry.value
      ? {
          term: t("live.onStage"),
          value: onStage.value ? stageXy(onStage.value.x, onStage.value.y) : none,
          said: "sample",
        }
      : {
          term: t("live.sample"),
          value: atSample.value
            ? t("live.um", { x: one(atSample.value[0]), y: one(atSample.value[1]) })
            : none,
          said: "sample",
        },
  ];
});
const scaleLabel = computed(() => {
  if (!bar.value) return "";
  const { length } = bar.value;
  return length >= 1000 ? t("live.mm", { length: length / 1000 }) : t("live.microns", { length });
});
</script>

<template>
  <section ref="root" class="live" :aria-label="t('live.label')" :data-state="state">
    <Toolbar class="tools" :label="t('live.tools')">
      <ToolbarButton
        v-if="state === 'off'"
        icon="play"
        :disabled="streaming.busy.value"
        @click="streaming.start"
      >
        {{ t("live.start") }}
      </ToolbarButton>
      <ToolbarButton v-else icon="stop" :disabled="streaming.busy.value" @click="streaming.stop">
        {{ t("live.stop") }}
      </ToolbarButton>
      <!-- These change nothing on the instrument: free in the read-only mode. -->
      <ControlScope :locked="false">
        <ToolbarButton icon="zoomOut" @click="preview.zoomBy(1 / STEP, middle())">
          {{ t("live.zoomOut") }}
        </ToolbarButton>
        <ToolbarButton icon="zoomIn" @click="preview.zoomBy(STEP, middle())">
          {{ t("live.zoomIn") }}
        </ToolbarButton>
        <ToolbarButton icon="fit" @click="preview.whole()">{{ t("live.fit") }}</ToolbarButton>
        <ToolbarButton icon="actual" @click="preview.oneToOne(middle())">
          {{ t("live.actual") }}
        </ToolbarButton>
        <ToolbarButton :icon="full ? 'leaveFullScreen' : 'fullScreen'" @click="toggleFull">
          {{ full ? t("live.leaveFullScreen") : t("live.fullScreen") }}
        </ToolbarButton>
        <ToolbarButton icon="download" @click="save">{{ t("live.save") }}</ToolbarButton>
      </ControlScope>
    </Toolbar>
    <!-- The overlays change nothing on the instrument: free in the read-only mode. -->
    <ControlScope :locked="false">
      <Toolbar class="tools" :label="t('live.overlays.tools')">
        <ToolbarButton
          v-for="layer in ['crosshair', 'grid', 'scale'] as const"
          :key="layer"
          :pressed="overlays.layers[layer]"
          @click="overlays.layers[layer] = !overlays.layers[layer]"
        >
          {{ t(`live.overlays.layers.${layer}`) }}
        </ToolbarButton>
        <ToolbarButton :pressed="overlays.tool === 'roi'" @click="draw('roi')">
          {{ t("live.overlays.drawRoi") }}
        </ToolbarButton>
        <ToolbarButton :pressed="overlays.tool === 'line'" @click="draw('line')">
          {{ t("live.overlays.drawLine") }}
        </ToolbarButton>
      </Toolbar>
    </ControlScope>
    <div class="stage">
      <canvas
        ref="canvas"
        class="canvas"
        role="img"
        tabindex="0"
        :aria-label="t('live.image')"
        data-said="canvas"
        :class="{ drags: choices.drags }"
        :style="aside ? { translate: `${aside[0]}px ${aside[1]}px` } : undefined"
        @wheel.prevent="wheel"
        @pointerdown="down"
        @pointermove="move"
        @pointerup="up"
        @pointercancel="cancel"
        @pointerleave="left"
        @keydown="key"
        @keyup="keyUp"
        @dblclick="centre"
      />
      <p
        v-if="state !== 'live'"
        class="said"
        :class="{ top: preview.extent.value !== undefined }"
        data-said="live-said"
      >
        <span class="chip">{{ t(`live.said.${state}`) }}</span>
      </p>
      <OverlayLayer
        v-if="preview.view.value && preview.extent.value"
        :view="preview.view.value"
        :extent="preview.extent.value"
        :size="preview.size()"
      />
      <figure v-if="bar && overlays.layers.scale" class="scale" data-said="scale">
        <span class="line" :style="{ inlineSize: `${bar.width}px` }" />
        <figcaption>{{ scaleLabel }}</figcaption>
      </figure>
    </div>
    <dl class="readouts">
      <div v-for="one in readouts" :key="one.said" :data-said="one.said">
        <dt>{{ one.term }}</dt>
        <dd>{{ one.value }}</dd>
      </div>
    </dl>
    <OverlayMeasures :stats="preview.stats.value" />
  </section>
</template>

<style scoped>
.live {
  display: grid;
  grid-template-rows: auto auto minmax(0, 1fr) auto auto;
  block-size: 100%;
  background: var(--surface-page);
}

.tools {
  padding: var(--space-1) var(--space-2);
  border-block-end: 1px solid var(--border);
}

.stage {
  position: relative;
  min-block-size: 0;
  background: var(--surface-image);
}

.canvas {
  display: block;
  inline-size: 100%;
  block-size: 100%;
  cursor: grab;
  touch-action: none;
}

.canvas:active {
  cursor: grabbing;
}

/* Dragging moves the stage: the pointer says so. */
.canvas.drags {
  cursor: move;
}

.canvas:focus-visible {
  outline: var(--focus-width) solid var(--focus-color);
  outline-offset: calc(-1 * var(--focus-width));
}

.said {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  margin: 0;
  padding: var(--space-3);
  text-align: center;
  pointer-events: none;
}

/* A frame shown, stopped or stale, is still worth seeing: the word goes to the top. */
.said.top {
  place-items: start center;
}

/* On a chip, as the scale bar is: legible on a light image and on the black. */
.chip {
  padding: var(--space-1) var(--space-2);
  border-radius: var(--radius-surface);
  background: var(--surface-inverse);
  color: var(--text-inverse);
}

.scale {
  position: absolute;
  inset-block-end: var(--space-3);
  inset-inline-end: var(--space-3);
  display: grid;
  gap: var(--space-1);
  justify-items: center;
  margin: 0;
  padding: var(--space-1) var(--space-2);
  border-radius: var(--radius-surface);
  background: var(--surface-inverse);
  color: var(--text-inverse);
  font-size: var(--font-size-small);
  pointer-events: none;
}

.line {
  block-size: 3px;
  background: currentColor;
}

/* Cells as wide whatever they say: a value growing never moves the image. */
.readouts {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(14rem, 1fr));
  gap: var(--space-1) var(--space-4);
  margin: 0;
  padding: var(--space-1) var(--space-2);
  border-block-start: 1px solid var(--border);
  font-size: var(--font-size-small);
}

.readouts div {
  display: flex;
  gap: var(--space-1);
  min-inline-size: 0;
  white-space: nowrap;
}

.readouts dt {
  color: var(--text-muted);
}

.readouts dd {
  overflow: hidden;
  margin: 0;
  font-variant-numeric: tabular-nums;
  text-overflow: ellipsis;
}
</style>
