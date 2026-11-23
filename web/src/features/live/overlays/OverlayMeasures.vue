<script setup lang="ts">
import { computed, shallowRef } from "vue";
import { useI18n } from "vue-i18n";

import type { Line, LiveStats } from "@/api/preview/message";
import { useAction, useProperty } from "@/api/things/use";
import { onGrid } from "@/features/live/overlays/geometry";
import { useOverlays } from "@/features/live/overlays/overlays";
import Button from "@/ui/Button.vue";
import Chart from "@/ui/charts/Chart.vue";
import type { Series } from "@/ui/charts/draw";

/**
 * What the overlays measure (W3.5), from the frames' statistics (P18,
 * ADR-0035): the ROI's mean, deviation, least and greatest, a channel each,
 * with the region the server measured; the ROI made the camera's region,
 * on its grid; and the line's profile, charted as it changes.
 */
const props = defineProps<{ stats: LiveStats | undefined }>();
const { t } = useI18n();
const overlays = useOverlays();
const capabilities = useProperty("/api/v1/camera/capabilities", { every: 60_000 });
const roi = useProperty("/api/v1/camera/roi", { every: 5_000 });
const streaming = useProperty("/api/v1/camera/streaming", { every: 1_000 });
const stop = useAction("/api/v1/camera/stop_streaming");
const start = useAction("/api/v1/camera/start_streaming");
const reading = shallowRef(false);

const measured = computed(() => props.stats?.roi ?? undefined);
const rows = computed(() => {
  const now = measured.value;
  if (!now) return [];
  const one = (value: number | undefined) => (value === undefined ? "" : value.toFixed(1));
  return (props.stats?.channels ?? []).map((channel, i) => ({
    channel,
    mean: one(now.mean[i]),
    std: one(now.std[i]),
    min: one(now.min[i]),
    max: one(now.max[i]),
  }));
});

/**
 * The ROI as the camera's region: on its grid, at least its least side. The
 * camera takes a region only while it doesn't stream, so streaming stops for
 * it, and starts again.
 */
async function readOut(): Promise<void> {
  const grid = capabilities.value.value?.roi;
  if (!overlays.roi || !grid) return;
  const region = onGrid(overlays.roi, { x: grid.x_step, y: grid.y_step, least: grid.min_size });
  reading.value = true;
  try {
    const was = streaming.value.value === true;
    if (was) await stop.run({});
    await roi.write(region);
    if (was) await start.run({});
  } finally {
    reading.value = false;
  }
}

/** A line's ends, to the pixel, as they are said. */
const ends = (line: Line) => ({
  x1: line.from[0].toFixed(0),
  y1: line.from[1].toFixed(0),
  x2: line.to[0].toFixed(0),
  y2: line.to[1].toFixed(0),
});

const profile = computed<Series[]>(() => {
  const line = props.stats?.profile;
  if (!line) return [];
  return (props.stats?.channels ?? []).map((channel, i) => {
    const x: number[] = [];
    const y: number[] = [];
    line.samples.forEach((sample, at) => {
      if (sample?.[i] === undefined) return;
      x.push(at * line.step);
      y.push(sample[i]);
    });
    return { name: t(`live.overlays.channels.${channel}`), kind: "line", x, y };
  });
});
</script>

<template>
  <section
    v-if="overlays.roi || overlays.line"
    class="measured"
    :aria-label="t('live.overlays.measuredLabel')"
  >
    <div v-if="overlays.roi" class="roi" data-said="roi-stats">
      <h2>{{ t("live.overlays.roiTitle") }}</h2>
      <p v-if="measured" class="said" data-said="roi-measured">
        {{ t("live.overlays.region", { ...measured.rect }) }}
      </p>
      <table v-if="rows.length">
        <thead>
          <tr>
            <th scope="col">{{ t("live.overlays.channel") }}</th>
            <th scope="col">{{ t("live.overlays.mean") }}</th>
            <th scope="col">{{ t("live.overlays.std") }}</th>
            <th scope="col">{{ t("live.overlays.min") }}</th>
            <th scope="col">{{ t("live.overlays.max") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in rows" :key="row.channel">
            <th scope="row">{{ t(`live.overlays.channels.${row.channel}`) }}</th>
            <td>{{ row.mean }}</td>
            <td>{{ row.std }}</td>
            <td>{{ row.min }}</td>
            <td>{{ row.max }}</td>
          </tr>
        </tbody>
      </table>
      <p v-if="roi.refused.value" class="said failed" data-said="read-out">
        {{ roi.refused.value.message }}
      </p>
      <div class="row">
        <Button compact :busy="reading" @click="readOut">{{ t("live.overlays.readOut") }}</Button>
        <Button compact variant="bare" @click="overlays.roi = undefined">
          {{ t("live.overlays.clearRoi") }}
        </Button>
      </div>
    </div>
    <div v-if="overlays.line" class="line" data-said="profile">
      <p v-if="stats?.profile" class="said" data-said="line-measured">
        {{ t("live.overlays.lineMeasured", ends(stats.profile.line)) }}
      </p>
      <Chart
        :title="t('live.overlays.profileTitle')"
        :series="profile"
        :x="{ title: t('live.overlays.along') }"
        :y="{ title: t('live.overlays.value') }"
        :height="120"
      />
      <Button compact variant="bare" @click="overlays.line = undefined">
        {{ t("live.overlays.clearLine") }}
      </Button>
    </div>
  </section>
</template>

<style scoped>
/* Never more than some of the window: the image keeps its room. */
.measured {
  display: grid;
  max-block-size: 40vh;
  overflow: auto;
  grid-template-columns: repeat(auto-fit, minmax(18rem, 1fr));
  gap: var(--space-3);
  padding: var(--space-2);
  border-block-start: 1px solid var(--border);
}

.roi,
.line {
  display: grid;
  gap: var(--space-1);
  align-content: start;
  justify-items: start;
}

h2,
.said {
  margin: 0;
}

h2 {
  font-size: var(--font-size-body);
}

.said {
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

.said.failed {
  color: var(--danger);
}

table {
  border-collapse: collapse;
  font-size: var(--font-size-small);
  font-variant-numeric: tabular-nums;
}

th,
td {
  padding: 0 var(--space-2);
  text-align: end;
}

th[scope="row"],
thead th:first-child {
  text-align: start;
}

.row {
  display: flex;
  gap: var(--space-2);
}

.line {
  justify-items: stretch;
}
</style>
