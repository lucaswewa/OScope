<script setup lang="ts">
import { computed, onBeforeUnmount, ref, shallowRef } from "vue";
import { useI18n } from "vue-i18n";

import { usePreview } from "@/api/preview/use";
import { useProperty } from "@/api/things/use";
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
  held = at(event);
  canvas.value?.setPointerCapture(event.pointerId);
}
function move(event: PointerEvent): void {
  const here = at(event);
  pointer.value = here;
  if (!held) return;
  preview.panBy([here[0] - held[0], here[1] - held[1]]);
  held = here;
}
function up(): void {
  held = undefined;
}
function left(): void {
  if (!held) pointer.value = undefined;
}

/** The keys on the image: + and − zoom, 0 fits, 1 shows one to one, F fills the screen. */
function key(event: KeyboardEvent): void {
  if (event.ctrlKey || event.metaKey || event.altKey) return;
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
const bar = computed(() => {
  const view = preview.view.value;
  return view ? scaleBar(side.value.value, view, ratio(), ROOM) : undefined;
});
const one = (value: number) => value.toFixed(1);
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
    {
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
    <div class="stage">
      <canvas
        ref="canvas"
        class="canvas"
        role="img"
        tabindex="0"
        :aria-label="t('live.image')"
        data-said="canvas"
        @wheel.prevent="wheel"
        @pointerdown="down"
        @pointermove="move"
        @pointerup="up"
        @pointercancel="up"
        @pointerleave="left"
        @keydown="key"
      />
      <p
        v-if="state !== 'live'"
        class="said"
        :class="{ top: preview.extent.value !== undefined }"
        data-said="live-said"
      >
        <span class="chip">{{ t(`live.said.${state}`) }}</span>
      </p>
      <figure v-if="bar" class="scale" data-said="scale">
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
  </section>
</template>

<style scoped>
.live {
  display: grid;
  grid-template-rows: auto minmax(0, 1fr) auto;
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
