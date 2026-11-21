<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { thingStore } from "@/api/things/store";
import { useProperty } from "@/api/things/use";
import { useJobAction } from "@/features/control/jobAction";
import { fieldOf } from "@/features/live/mosaic/mosaic";
import { COVERSLIP, overviewFrames, regionOf } from "@/features/scan/plan";
import RegionFields from "@/features/scan/RegionFields.vue";
import { kept } from "@/host/kept";
import Button from "@/ui/Button.vue";
import Progress from "@/ui/Progress.vue";
import type { Rect } from "@/ui/tile-viewer/coordinates";

/**
 * The overview (W5.1, P37): a picture of the slide, frames a field apart
 * over a region, made as a job. Its region is typed or taken from the
 * stage's corners, a standard slide's coverslip at first, and kept. The
 * frames it will take are said first: an objective of low magnification
 * makes few.
 */
const { t } = useI18n();
const overview = useProperty("/api/v1/scan/last_overview", { every: 10_000 });
const streaming = useProperty("/api/v1/camera/streaming", { every: 1_000 });
const roi = useProperty("/api/v1/camera/roi", { every: 10_000 });
const camera = useProperty("/api/v1/camera/capabilities", { every: 60_000 });
const pixel = useProperty("/api/v1/objective/pixel_at_sample", { every: 10_000 });
const making = useJobAction("/api/v1/scan/overview");

const isRect = (value: unknown): value is Rect =>
  typeof value === "object" &&
  value !== null &&
  ["x", "y", "width", "height"].every(
    (key) => typeof (value as Record<string, unknown>)[key] === "number",
  );
const region = kept("oscope.scan.overview.region", COVERSLIP, isRect);

/** The frames the overview takes, through the objective fitted now. */
const frames = computed(() => {
  const sensor = camera.value.value;
  const size =
    roi.value.value ?? (sensor && { width: sensor.sensor_width, height: sensor.sensor_height });
  const field = fieldOf({ x: 0, y: 0 }, size ?? undefined, pixel.value.value);
  return region.value && field ? overviewFrames(region.value, field) : undefined;
});
/** Past this, a lower magnification is suggested. */
const MANY = 300;

async function make(): Promise<void> {
  if (!region.value) return;
  await making.run({ region: regionOf(region.value) });
  await thingStore("scan").read("last_overview");
}

const last = computed(() => {
  const view = overview.value.value;
  if (!view) return undefined;
  return t("scan.overview.last", {
    width: ((view.width * view.pixel_um) / 1000).toFixed(1),
    height: ((view.height * view.pixel_um) / 1000).toFixed(1),
    frames: view.frames,
    pixel: view.pixel_um.toFixed(0),
  });
});
</script>

<template>
  <section class="overview" aria-labelledby="overview-title" data-said="overview">
    <h2 id="overview-title">{{ t("scan.overview.title") }}</h2>
    <RegionFields v-model="region" :legend="t('scan.overview.region')" said="overview-region" />
    <p v-if="frames !== undefined" class="said" data-said="overview-frames">
      {{ t("scan.overview.frames", { frames }) }}
      <template v-if="frames > MANY">{{ t("scan.overview.many") }}</template>
    </p>
    <Button
      :busy="making.running.value"
      :disabled="!region || streaming.value.value !== true"
      @click="make"
    >
      {{ t("scan.overview.make") }}
    </Button>
    <p v-if="streaming.value.value !== true" class="said">{{ t("scan.overview.needsStream") }}</p>
    <Progress
      v-if="making.running.value"
      :value="making.job.value?.progress.fraction ?? null"
      :max="1"
      :label="making.job.value?.progress.stage || t('scan.overview.making')"
    />
    <p v-if="making.failed.value" class="said failed" data-said="overview-said">
      {{ making.failed.value }}
    </p>
    <p v-else-if="last" class="said" data-said="overview-said">{{ last }}</p>
  </section>
</template>

<style scoped>
.overview {
  display: grid;
  gap: var(--space-2);
}

.overview h2,
.said {
  margin: 0;
}

.said {
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

.said.failed {
  color: var(--danger);
}
</style>
