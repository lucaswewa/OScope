<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

import { saturatedPixels } from "@/api/preview/message";
import { usePreview } from "@/api/preview/use";
import { useAction, useProperty } from "@/api/things/use";
import GalleryHeader from "@/features/dev/GalleryHeader.vue";
import Button from "@/ui/Button.vue";
import Card from "@/ui/Card.vue";
import type { Series } from "@/ui/charts/draw";
import Chart from "@/ui/charts/Chart.vue";
import Field from "@/ui/Field.vue";
import KeyValue from "@/ui/KeyValue.vue";
import Slider from "@/ui/Slider.vue";
import Switch from "@/ui/Switch.vue";

/**
 * The live preview (W1.5): the camera's frames, decoded in a worker and
 * drawn at the display's refresh. The wheel or + and - zoom, dragging or the
 * arrow keys pan, and 0 shows the whole frame again; P18's statistics come
 * when asked for.
 */
const { t } = useI18n();
const canvas = ref<HTMLCanvasElement>();
const quality = ref(80);
const measuring = ref(false);
const preview = usePreview(canvas, { quality: quality.value });
const streaming = useProperty("/api/v1/camera/streaming", { every: 1000 });
const start = useAction("/api/v1/camera/start_streaming");
const stop = useAction("/api/v1/camera/stop_streaming");

watch(quality, (now) => preview.choose({ quality: now }));
watch(measuring, (on) => preview.choose({ stats: on ? { bins: 64, mask: true } : undefined }));

const ratio = () => globalThis.devicePixelRatio || 1;
const middle = (): [number, number] => {
  const box = canvas.value?.getBoundingClientRect();
  return [((box?.width ?? 0) * ratio()) / 2, ((box?.height ?? 0) * ratio()) / 2];
};

function wheel(event: WheelEvent): void {
  event.preventDefault();
  preview.zoomBy(event.deltaY < 0 ? 1.25 : 0.8, [event.offsetX * ratio(), event.offsetY * ratio()]);
}

let held: [number, number] | undefined;
function press(event: PointerEvent): void {
  held = [event.clientX, event.clientY];
  canvas.value?.setPointerCapture(event.pointerId);
}
function drag(event: PointerEvent): void {
  if (!held) return;
  preview.panBy([(event.clientX - held[0]) * ratio(), (event.clientY - held[1]) * ratio()]);
  held = [event.clientX, event.clientY];
}

/** The keys: + and - zoom about the middle, the arrows pan a tenth, 0 shows the whole frame. */
function key(event: KeyboardEvent): void {
  const [x, y] = middle();
  const step = Math.min(x, y) / 5;
  const pans: Record<string, [number, number]> = {
    ArrowLeft: [step, 0],
    ArrowRight: [-step, 0],
    ArrowUp: [0, step],
    ArrowDown: [0, -step],
  };
  if (event.key === "+" || event.key === "=") preview.zoomBy(1.25, [x, y]);
  else if (event.key === "-") preview.zoomBy(0.8, [x, y]);
  else if (event.key === "0") preview.whole();
  else if (pans[event.key]) preview.panBy(pans[event.key]!);
  else return;
  event.preventDefault();
}

const readout = computed(() => {
  const numbers = preview.numbers.value;
  const last = numbers.last;
  const none = t("preview.none");
  return [
    { term: t("preview.state"), value: t(`preview.states.${preview.state.value}`) },
    { term: t("preview.rate"), value: t("preview.fps", { fps: numbers.fps }) },
    {
      term: t("preview.latency"),
      value: t("preview.ms", {
        total: numbers.latency.toFixed(0),
        server: numbers.serverLatency.toFixed(0),
        page: numbers.pageLatency.toFixed(0),
      }),
    },
    {
      term: t("preview.frames"),
      value: t("preview.counts", { drawn: numbers.drawn, replaced: numbers.replaced }),
    },
    {
      term: t("preview.size"),
      value: last
        ? t("preview.sized", { width: last.width, height: last.height, quality: last.quality })
        : none,
    },
    {
      term: t("preview.region"),
      value: last ? t("preview.at", { ...last.roi }) : none,
    },
    {
      term: t("preview.renderer"),
      value: preview.kind.value ? t(`preview.renderers.${preview.kind.value}`) : none,
    },
  ];
});

const camera = computed(() => {
  const on = streaming.value.value;
  return on === undefined ? t("preview.unread") : on ? t("preview.streaming") : t("preview.still");
});

const histogram = computed<Series[]>(() => {
  const stats = preview.stats.value;
  if (!stats) return [];
  const width = (stats.full_scale + 1) / (stats.histogram[0]?.length ?? 1);
  return stats.channels.map((name, channel) => {
    const bins = stats.histogram[channel] ?? [];
    return { name, kind: "histogram", width, x: bins.map((_, bin) => bin * width), y: bins };
  });
});

const measured = computed(() => {
  const stats = preview.stats.value;
  if (!stats) return [];
  const each = (values: number[], show: (value: number) => string) =>
    values.map((value, channel) => `${stats.channels[channel]} ${show(value)}`).join(", ");
  return [
    { term: t("preview.focus"), value: stats.focus.toFixed(1) },
    {
      term: t("preview.median"),
      value: each(
        stats.percentiles.map((p) => p[2] ?? 0),
        (v) => v.toFixed(0),
      ),
    },
    {
      term: t("preview.saturated"),
      value: each(stats.saturated, (v) => `${(v * 100).toFixed(2)} %`),
    },
    {
      term: t("preview.masked"),
      value: String(preview.mask.value ? saturatedPixels(preview.mask.value) : 0),
    },
  ];
});
</script>

<template>
  <main class="preview">
    <GalleryHeader :title="t('preview.title')" :intro="t('preview.intro')" />
    <div class="layout">
      <section class="live" :aria-label="t('preview.live')">
        <canvas
          ref="canvas"
          class="frame"
          role="img"
          tabindex="0"
          :aria-label="t('preview.canvas')"
          @wheel="wheel"
          @pointerdown="press"
          @pointermove="drag"
          @pointerup="held = undefined"
          @pointercancel="held = undefined"
          @keydown="key"
        />
        <KeyValue data-said="readout" :entries="readout" />
      </section>

      <div class="side">
        <Card :title="t('preview.camera')" :level="2">
          <p class="note" data-said="camera">{{ camera }}</p>
          <div class="actions">
            <Button :busy="start.running.value" @click="start.run({})">{{
              t("preview.start")
            }}</Button>
            <Button :busy="stop.running.value" @click="stop.run({})">{{
              t("preview.stop")
            }}</Button>
            <Button @click="preview.whole()">{{ t("preview.whole") }}</Button>
          </div>
        </Card>

        <Card :title="t('preview.asked')" :level="2">
          <Field :label="t('preview.quality')">
            <Slider v-model="quality" :min="30" :max="95" :step="5" />
          </Field>
          <Switch v-model="measuring">{{ t("preview.measure") }}</Switch>
        </Card>

        <Card v-if="preview.stats.value" :title="t('preview.stats')" :level="2">
          <KeyValue data-said="stats" :entries="measured" />
          <Chart
            :title="t('preview.histogram')"
            :series="histogram"
            :x="{ title: t('preview.level') }"
            :y="{ title: t('preview.count'), scale: 'log' }"
          />
        </Card>
      </div>
    </div>
  </main>
</template>

<style scoped>
.preview {
  display: grid;
  gap: var(--space-4);
  padding: var(--space-6);
}

.layout {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 22rem;
  gap: var(--space-6);
  align-items: start;
}

.live {
  display: grid;
  gap: var(--space-3);
}

.frame {
  display: block;
  width: 100%;
  aspect-ratio: 3 / 2;
  border-radius: var(--radius-surface);
  background: var(--surface-image);
  touch-action: none;
}

.frame:focus-visible {
  outline: var(--focus-width) solid var(--focus-color);
  outline-offset: var(--focus-offset);
}

.side {
  display: grid;
  gap: var(--space-6);
}

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3);
}

.note {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

@media (max-width: 60rem) {
  .layout {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
