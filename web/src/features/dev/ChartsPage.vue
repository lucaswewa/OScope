<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";

import recorded from "@/features/dev/charts/recorded";
import GalleryHeader from "@/features/dev/GalleryHeader.vue";
import Card from "@/ui/Card.vue";
import Chart from "@/ui/charts/Chart.vue";
import type { Series } from "@/ui/charts/draw";
import Toolbar from "@/ui/Toolbar.vue";
import ToolbarButton from "@/ui/ToolbarButton.vue";

/**
 * Charts drawn by the app (W0.7), from data recorded from the simulated
 * instrument: a histogram of each frame of a burst, shown live, ten frames a
 * second; the share of pixels below each value; a profile across a frame;
 * and a focus run's curve.
 */
const { t } = useI18n();
const { histograms, profile, focus } = recorded;
/** The focus measure by its name, not the server's ID for it. */
const measure = computed(() => t(`charts.focus.measures.${focus.measure}`));

// The live histogram: the recorded frames in turn, ten a second.
const frame = ref(0);
const playing = ref(true);
const log = ref(false);
let timer: ReturnType<typeof setInterval> | undefined;
onMounted(() => {
  timer = setInterval(() => {
    if (playing.value) frame.value = (frame.value + 1) % histograms.counts.length;
  }, 100);
});
onBeforeUnmount(() => clearInterval(timer));

const edges = Array.from(histograms.counts[0]!, (_, n) => n * histograms.binWidth);
const histogram = computed<Series[]>(() => [
  {
    name: t("charts.histogram.series", { frame: frame.value + 1 }),
    kind: "histogram",
    x: edges,
    y: histograms.counts[frame.value]!,
    width: histograms.binWidth,
  },
]);

/** The share of the frame's pixels at or below each value, in percent. */
const cumulative = computed<Series[]>(() => {
  const counts = histograms.counts[frame.value]!;
  const all = counts.reduce((sum, count) => sum + count, 0);
  let below = 0;
  const share = counts.map((count) => ((below += count) / all) * 100);
  return [{ name: t("charts.cumulative.series"), kind: "area", x: edges, y: share }];
});

const profileSeries: Series[] = [
  { name: t("charts.profile.series"), kind: "line", x: profile.um, y: profile.mean },
];
const profileBand = [{ of: 0, x: profile.um, lower: profile.low, upper: profile.high }];

const focusSeries: Series[] = [
  {
    name: t("charts.focus.coarse"),
    kind: "points",
    x: focus.coarse.map((one) => one[0]),
    y: focus.coarse.map((one) => one[1]),
  },
  {
    name: t("charts.focus.fine"),
    kind: "points",
    x: focus.fine.map((one) => one[0]),
    y: focus.fine.map((one) => one[1]),
  },
];
</script>

<template>
  <main class="charts">
    <GalleryHeader :title="t('charts.title')" :intro="t('charts.intro')" />
    <p class="recorded">
      {{
        t("charts.recorded", {
          date: recorded.recorded,
          sample: recorded.sample,
          width: recorded.frame.width,
          height: recorded.frame.height,
        })
      }}
    </p>

    <div class="grid">
      <Card :title="t('charts.histogram.title')" :level="2">
        <template #actions>
          <Toolbar :label="t('charts.histogram.tools')">
            <ToolbarButton
              :icon="playing ? 'pause' : 'play'"
              :pressed="!playing"
              @click="playing = !playing"
            >
              {{ t("charts.histogram.pause") }}
            </ToolbarButton>
            <ToolbarButton :pressed="log" @click="log = !log">
              {{ t("charts.histogram.log") }}
            </ToolbarButton>
          </Toolbar>
        </template>
        <Chart
          :title="t('charts.histogram.chart')"
          :series="histogram"
          :x="{ title: t('charts.value') }"
          :y="{ title: t('charts.histogram.y'), scale: log ? 'log' : 'linear' }"
          :level="3"
        />
      </Card>

      <Card :title="t('charts.cumulative.title')" :level="2">
        <Chart
          :title="t('charts.cumulative.chart')"
          :series="cumulative"
          :x="{ title: t('charts.value') }"
          :y="{ title: t('charts.cumulative.y'), range: [0, 100] }"
          :level="3"
        />
      </Card>

      <Card :title="t('charts.profile.title')" :level="2">
        <Chart
          :title="t('charts.profile.chart')"
          :series="profileSeries"
          :bands="profileBand"
          :x="{ title: t('charts.profile.x') }"
          :y="{ title: t('charts.value') }"
          :level="3"
        />
      </Card>

      <Card :title="t('charts.focus.title')" :level="2">
        <Chart
          :title="t('charts.focus.chart', { measure })"
          :series="focusSeries"
          :x="{ title: t('charts.focus.x') }"
          :y="{ title: t('charts.focus.y', { measure }) }"
          :level="3"
        />
      </Card>
    </div>
  </main>
</template>

<style scoped>
.charts {
  display: grid;
  gap: var(--space-4);
  padding: var(--space-6);
}

.recorded {
  color: var(--text-muted);
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(min(100%, 32rem), 1fr));
  gap: var(--space-4);
}
</style>
