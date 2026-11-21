<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { useProperty } from "@/api/things/use";
import { fieldOf } from "@/features/live/mosaic/mosaic";
import { blankOf, imageOf, placementOf } from "@/features/scan/overview";
import { usePlanning } from "@/features/scan/planning";
import { shapesOf } from "@/features/scan/plan";
import type { Rect } from "@/ui/tile-viewer/coordinates";
import TileViewer from "@/ui/tile-viewer/TileViewer.vue";

/**
 * The plan over the overview (W5.1): the last overview, or the stage's
 * travel blank before one, in the tile viewer; the plan's frames over it in
 * their order, the path from each to the next, and its region; and the
 * camera's field of view where the stage is. While the region is being
 * drawn, a drag on it draws one.
 */
const { t } = useI18n();
const plan = usePlanning();
const overview = useProperty("/api/v1/scan/last_overview", { every: 10_000 });
const capabilities = useProperty("/api/v1/stage/capabilities", { every: 60_000 });
const position = useProperty("/api/v1/stage/position", { every: 1_000 });
const roi = useProperty("/api/v1/camera/roi", { every: 10_000 });
const camera = useProperty("/api/v1/camera/capabilities", { every: 60_000 });
const pixel = useProperty("/api/v1/objective/pixel_at_sample", { every: 10_000 });

const blank = computed(() => {
  const travel = capabilities.value.value;
  return travel ? blankOf(travel) : undefined;
});
const shown = computed(() => {
  const view = overview.value.value;
  if (view) return { image: imageOf(view), placement: placementOf(view) };
  return blank.value;
});
const shapes = computed(() => {
  const preview = plan.preview.value;
  return preview ? shapesOf(preview.plan, plan.region.value) : [];
});
const field = computed(() => {
  const sensor = camera.value.value;
  const size =
    roi.value.value ?? (sensor && { width: sensor.sensor_width, height: sensor.sensor_height });
  return fieldOf(position.value.value?.position, size ?? undefined, pixel.value.value);
});

/** Where to look: the region, with as much again about it, each time it changes. */
const fit = computed(() => {
  const r = plan.region.value;
  return (
    r && { x: r.x - r.width / 2, y: r.y - r.height / 2, width: r.width * 2, height: r.height * 2 }
  );
});

function drew(rect: Rect): void {
  plan.region.value = rect;
  plan.drawing.value = false;
}
</script>

<template>
  <div class="scan-view">
    <TileViewer
      :label="t('scan.viewer')"
      :image="shown?.image"
      :placement="shown?.placement"
      :marker="field"
      :fit="fit"
      :shapes="shapes"
      :drawing="plan.drawing.value"
      @draw="drew"
    />
    <p class="said" data-said="view-said" role="status">
      <template v-if="plan.drawing.value">{{ t("scan.drawHint") }}</template>
      <template v-else-if="!overview.value.value">{{ t("scan.noOverview") }}</template>
    </p>
  </div>
</template>

<style scoped>
.scan-view {
  display: grid;
  grid-template-rows: minmax(0, 1fr) auto;
  block-size: 100%;
}

.said {
  margin: 0;
  padding: var(--space-1) var(--space-3);
  color: var(--text-muted);
  font-size: var(--font-size-small);
  min-block-size: 1.5em;
}
</style>
