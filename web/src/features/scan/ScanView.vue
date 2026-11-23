<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { useProperty } from "@/api/things/use";
import {
  fieldOf,
  mosaicImage,
  placementOf as mosaicPlacement,
} from "@/features/live/mosaic/mosaic";
import { useHisto } from "@/features/scan/histo/flow";
import { mapPointsOf } from "@/features/scan/histo/histo";
import { blankOf, imageOf, placementOf } from "@/features/scan/overview";
import { usePlanning } from "@/features/scan/planning";
import { shapesOf } from "@/features/scan/plan";
import { mosaicRegionOf, runShapes } from "@/features/scan/running";
import type { Scan } from "@/features/scan/scanning";
import { fieldAbout, type Point, type Rect } from "@/ui/tile-viewer/coordinates";
import { rectShape, type Shape, type ShapeKind } from "@/ui/tile-viewer/shapes";
import TileViewer from "@/ui/tile-viewer/TileViewer.vue";

/**
 * The plan over the overview (W5.1): the last overview, or the stage's
 * travel blank before one, in the tile viewer; the plan's frames over it in
 * their order, the path from each to the next, and its region; and the
 * camera's field of view where the stage is. While the region is being
 * drawn, a drag on it draws one.
 *
 * A scan, while it is shown (W5.2): the live mosaic its frames draw, and
 * its plan over it, each tile marked as it is taken.
 */
const props = defineProps<{ scan: Scan; histo?: boolean }>();
const flow = useHisto();
const { t } = useI18n();
const plan = usePlanning();
const overview = useProperty("/api/v1/scan/last_overview", { every: 10_000 });
const capabilities = useProperty("/api/v1/stage/capabilities", { every: 60_000 });
const position = useProperty("/api/v1/stage/position", { every: 1_000 });
const roi = useProperty("/api/v1/camera/roi", { every: 10_000 });
const camera = useProperty("/api/v1/camera/capabilities", { every: 60_000 });
const pixel = useProperty("/api/v1/objective/pixel_at_sample", { every: 10_000 });
const mosaic = useProperty("/api/v1/live_mosaic/state", { every: 1_000 });

const blank = computed(() => {
  const travel = capabilities.value.value;
  return travel ? blankOf(travel) : undefined;
});
const shown = computed(() => {
  const image = props.scan.shown.value ? mosaicImage(mosaic.value.value) : undefined;
  if (image) return { image, placement: mosaicPlacement(mosaic.value.value) };
  const view = overview.value.value;
  if (view) return { image: imageOf(view), placement: placementOf(view) };
  return blank.value;
});
/** A focus map's point, a small square about it, a quarter of a frame across. */
function pointShape(kind: ShapeKind, at: Point, side: number): Shape {
  return rectShape(kind, fieldAbout(at, side, side));
}
/**
 * The histo scan's (W5.3): its regions, and the focus map's points, where
 * the run will measure them, or, once it has, those used and those rejected.
 */
const histoShapes = computed<Shape[]>(() => {
  if (!props.histo) return [];
  const preview = plan.preview.value;
  const z = props.scan.progress.value?.plan.z;
  const side = (preview?.plan.field.cell_um.width ?? 0) / 4;
  const measured =
    props.scan.shown.value && z?.kind === "map"
      ? [
          ...z.map.used.map((p) => pointShape("point", { x: p.x_um, y: p.y_um }, side)),
          ...z.map.rejected.map((p) => pointShape("fault", { x: p.x_um, y: p.y_um }, side)),
        ]
      : undefined;
  const choosing = flow.step.value === "focus" || flow.step.value === "run";
  const planned =
    preview && choosing
      ? mapPointsOf(preview.plan, flow.columns.value, flow.rows.value).map((p) =>
          pointShape("point", p, side),
        )
      : [];
  return [...flow.shapes.value, ...(measured ?? planned)];
});
const shapes = computed(() => {
  const progress = props.scan.progress.value;
  const scanned =
    props.scan.shown.value && progress ? runShapes(progress, props.scan.job.value) : undefined;
  const preview = plan.preview.value;
  const planned = preview
    ? shapesOf(preview.plan, props.histo ? undefined : plan.region.value)
    : [];
  return [...(scanned ?? planned), ...histoShapes.value];
});
const field = computed(() => {
  const sensor = camera.value.value;
  const size =
    roi.value.value ?? (sensor && { width: sensor.sensor_width, height: sensor.sensor_height });
  return fieldOf(position.value.value?.position, size ?? undefined, pixel.value.value);
});

/** The histo scan's regions kept, about them all. */
const keptBounds = computed<Rect | undefined>(() => {
  const kept = flow.regions.value.filter((one) => one.kept).map((one) => one.rect);
  if (kept.length === 0) return undefined;
  const x = Math.min(...kept.map((r) => r.x));
  const y = Math.min(...kept.map((r) => r.y));
  const width = Math.max(...kept.map((r) => r.x + r.width)) - x;
  const height = Math.max(...kept.map((r) => r.y + r.height)) - y;
  return { x, y, width, height };
});

/**
 * Where to look: the plan's region, or the scan's frames while it is shown,
 * with as much again about it, each time it changes.
 */
const fit = computed(() => {
  const progress = props.scan.progress.value;
  const r =
    props.scan.shown.value && progress
      ? mosaicRegionOf(progress.plan)
      : props.histo
        ? keptBounds.value
        : plan.region.value;
  return (
    r && { x: r.x - r.width / 2, y: r.y - r.height / 2, width: r.width * 2, height: r.height * 2 }
  );
});

function drew(rect: Rect): void {
  if (props.histo) flow.drew(rect);
  else plan.region.value = rect;
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
      <template v-else-if="!overview.value.value && !scan.shown.value">{{
        t("scan.noOverview")
      }}</template>
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
