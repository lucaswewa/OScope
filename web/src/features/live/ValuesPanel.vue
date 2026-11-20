<script setup lang="ts">
import { computed, ref, shallowRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import { call, server } from "@/api/client";
import type { LiveStats } from "@/api/preview/message";
import { thingStore } from "@/api/things/store";
import { useProperty } from "@/api/things/use";
import { MAPS, tableOf, type MapName } from "@/features/live/colourMaps";
import { histogramSeries, percentilesOf, toLevel, toSensor } from "@/features/live/histogram";
import { useOverlays } from "@/features/live/overlays/overlays";
import Button from "@/ui/Button.vue";
import Chart from "@/ui/charts/Chart.vue";
import Field from "@/ui/Field.vue";
import NumberField from "@/ui/NumberField.vue";
import Select from "@/ui/Select.vue";
import Slider from "@/ui/Slider.vue";
import Switch from "@/ui/Switch.vue";

/**
 * The frame's values and how the preview shows them (W3.6): the histogram,
 * a channel each, linear or logarithmic, its percentiles said; the look's
 * black and white points on it, its gamma and its white balance, with auto
 * white balance's found gains offered, and reset; the colour map; and the
 * focus value with its trace, to focus by hand.
 */
const props = defineProps<{ stats: LiveStats | undefined }>();
const { t } = useI18n();
const overlays = useOverlays();
const look = useProperty("/api/v1/preview/look", { every: 5_000 });
const balance = useProperty("/api/v1/exposure/last_white_balance", { every: 5_000 });

const log = shallowRef(true);
const channel = (name: string) => t(`live.overlays.channels.${name}`);
const series = computed(() =>
  props.stats ? histogramSeries(props.stats, log.value, channel) : [],
);
const fullScale = computed(() => props.stats?.full_scale ?? 4095);
const percentiles = computed(() =>
  (props.stats?.channels ?? []).map((name, i) => ({
    name: channel(name),
    values: Object.entries(percentilesOf(props.stats!, i))
      .map(([p, value]) => `${p} ${value.toFixed(0)}`)
      .join(" · "),
  })),
);

// The black and white points, in the sensor's units on the histogram's axis.
const black = computed(() => toSensor(look.value.value?.black ?? 0, fullScale.value));
const white = computed(() => toSensor(look.value.value?.white ?? 1, fullScale.value));
// A value as it was is not written again: a field gives its value once more as it
// loses the focus, which, sent after "Put the look back", would undo it.
const same = (a: number | null | undefined, b: number | null | undefined) =>
  a === b || (a != null && b != null && Math.abs(a - b) < 1e-9);
function level(which: "black" | "white", value: number): void {
  const now = look.value.value;
  const to = toLevel(value, fullScale.value);
  if (now && !same(now[which], to)) void look.write({ ...now, [which]: to });
}
function gamma(value: number | null): void {
  const now = look.value.value;
  if (now && !same(now.gamma, value)) void look.write({ ...now, gamma: value });
}
function gain(index: number, value: number | null): void {
  const now = look.value.value;
  if (!now || value === null || same(now.gains[index], value)) return;
  const gains = [...now.gains];
  gains[index] = value;
  void look.write({ ...now, gains });
}
/** Auto white balance's gains, when they differ from the look's. */
const found = computed(() => {
  const gains = balance.value.value?.gains;
  const now = look.value.value?.gains;
  return gains && now && gains.some((g, i) => Math.abs(g - (now[i] ?? 0)) > 1e-3)
    ? gains
    : undefined;
});
async function reset(): Promise<void> {
  await call((signal) => server.api.POST("/api/v1/preview/look/reset", { signal }));
  await thingStore("preview").read("look");
}

const maps = computed(() =>
  MAPS.map((name) => ({ value: name, label: t(`live.values.maps.${name}`) })),
);
/** The chosen map as a bar, dark to light: its 256 colours, drawn from its table. */
const bar = ref<HTMLCanvasElement>();
const table = computed(() => tableOf(overlays.map));
watch(
  [bar, table],
  ([canvas, colours]) => {
    const drawn = canvas?.getContext("2d");
    if (!drawn || !colours) return;
    const image = drawn.createImageData(256, 1);
    for (let i = 0; i < 256; i++) {
      image.data.set([colours[i * 3]!, colours[i * 3 + 1]!, colours[i * 3 + 2]!, 255], i * 4);
    }
    drawn.putImageData(image, 0, 0);
  },
  { flush: "post" },
);

// The focus value, and the last minute or so of it.
const focus = shallowRef<number[]>([]);
watch(
  () => props.stats?.focus,
  (now) => {
    if (now !== undefined) focus.value = [...focus.value, now].slice(-120);
  },
);
const trace = computed(() => [
  {
    name: t("live.values.focus"),
    kind: "line" as const,
    x: focus.value.map((_, i) => i),
    y: focus.value,
  },
]);
</script>

<template>
  <section class="values" :aria-label="t('live.values.label')" data-said="values">
    <div class="histogram">
      <Chart
        :title="t('live.values.histogram')"
        :series="series"
        :x="{ title: t('live.values.counts'), range: [0, fullScale + 1] }"
        :y="{ title: t('live.values.pixels'), scale: log ? 'log' : 'linear' }"
        :height="140"
      />
      <Switch v-model="log">{{ t("live.values.log") }}</Switch>
      <p v-for="one in percentiles" :key="one.name" class="said" data-said="percentiles">
        {{ one.name }}: {{ one.values }}
      </p>
      <Slider
        :label="t('live.values.black')"
        :model-value="black"
        :min="0"
        :max="fullScale"
        @commit="(value) => level('black', value)"
      />
      <Slider
        :label="t('live.values.white')"
        :model-value="white"
        :min="0"
        :max="fullScale"
        @commit="(value) => level('white', value)"
      />
    </div>
    <div class="look">
      <Field :label="t('live.values.map')">
        <Select
          :model-value="overlays.map"
          :options="maps"
          @update:model-value="(name) => (overlays.map = name as MapName)"
        />
      </Field>
      <canvas v-if="table" ref="bar" class="bar" width="256" height="1" data-said="map-bar" />
      <Field :label="t('live.values.gamma')">
        <NumberField
          :model-value="look.value.value?.gamma ?? null"
          optional
          :min="0.1"
          :max="5"
          :step="0.1"
          :digits="2"
          :placeholder="t('live.values.srgb')"
          @update:model-value="gamma"
        />
      </Field>
      <div class="gains">
        <Field v-for="(name, i) in ['red', 'green', 'blue']" :key="name" :label="channel(name)">
          <NumberField
            :model-value="look.value.value?.gains[i] ?? null"
            :min="0"
            :max="16"
            :step="0.01"
            :digits="2"
            compact
            @update:model-value="(value) => gain(i, value)"
          />
        </Field>
      </div>
      <Button
        v-if="found && look.value.value"
        compact
        data-said="balance-offer"
        @click="look.write({ ...look.value.value, gains: found })"
      >
        {{ t("live.values.useBalance", { gains: found.map((g) => g.toFixed(2)).join(", ") }) }}
      </Button>
      <Button compact variant="bare" @click="reset">{{ t("live.values.reset") }}</Button>
      <p class="said" data-said="focus">
        {{ t("live.values.focusIs", { focus: stats ? stats.focus.toFixed(1) : "—" }) }}
      </p>
      <Chart
        v-if="focus.length > 1"
        :title="t('live.values.focus')"
        :series="trace"
        :x="{ title: t('live.values.frames') }"
        :y="{ title: t('live.values.focus') }"
        :height="80"
      />
    </div>
  </section>
</template>

<style scoped>
/* Never more than some of the window: the image keeps its room. */
.values {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(18rem, 1fr));
  gap: var(--space-3);
  max-block-size: 40vh;
  padding: var(--space-2);
  overflow: auto;
  border-block-start: 1px solid var(--border);
}

.histogram,
.look {
  display: grid;
  gap: var(--space-2);
  align-content: start;
}

.bar {
  inline-size: 100%;
  block-size: var(--space-3);
  border-radius: var(--radius-control);
}

.gains {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: var(--space-2);
}

.said {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
