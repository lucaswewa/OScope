<script setup lang="ts">
import { computed, shallowRef } from "vue";
import { useI18n } from "vue-i18n";

import { tile, type StoredImage } from "@/api/media";
import type { AnyRecord, Contents } from "@/features/gallery/project/project";
import {
  dziLevels,
  imageOf,
  lookKey,
  lookOf,
  mapPixels,
  placementOf,
  type ViewLook,
} from "@/features/gallery/viewer/viewer";
import { tableOf } from "@/features/live/colourMaps";
import type { Point, Rect } from "@/ui/tile-viewer/coordinates";
import type { TiledImage } from "@/ui/tile-viewer/source";
import TileViewer from "@/ui/tile-viewer/TileViewer.vue";

/**
 * A record's image, deep-zoomed (W6.4): its Deep Zoom tiles, made on
 * demand by the server in `look` (ADR-0028), a one-channel image's through
 * its colour map here; placed on the stage, so that the scale, the pointer's
 * place and another view's are in µm. Moved by its user, it says its view;
 * given `fit`, it follows another's.
 */
const props = defineProps<{
  contents: Contents;
  one: AnyRecord;
  look: ViewLook;
  fit?: Rect | undefined;
  /** The image at its own origin, not on the stage: its µm from its top-left corner (W7.5). */
  own?: boolean;
  /** Whether presses and drags draw, for the annotations (W7.5). */
  drawing?: boolean;
  /** One channel alone, as grey: a stain's amount (W7.6). */
  channel?: number | undefined;
  /** Whether every view is said, the page's own fits too (W7.6). */
  tell?: boolean;
}>();
const emit = defineEmits<{
  view: [rect: Rect];
  press: [at: Point];
  draw: [rect: Rect];
  drag: [rect: Rect];
  hover: [at: Point | undefined];
}>();
const { t } = useI18n();

const stored = computed<StoredImage>(() => ({
  project: props.contents.project.id,
  kind: props.one.kind === "acquisition" ? "acquisitions" : "results",
  id: props.one.record.id,
}));
const info = computed(() => imageOf(props.one));
const query = computed(() => ({
  ...lookOf(props.look, info.value?.planes ?? 1),
  ...(props.channel === undefined ? {} : { channel: props.channel }),
}));

/** A tile through a colour map's table, as a PNG. */
async function mapped(picture: Blob, table: Uint8Array): Promise<Blob> {
  const bitmap = await createImageBitmap(picture);
  const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
  const context = canvas.getContext("2d")!;
  context.drawImage(bitmap, 0, 0);
  bitmap.close();
  const pixels = context.getImageData(0, 0, canvas.width, canvas.height);
  mapPixels(pixels.data, table);
  context.putImageData(pixels, 0, 0);
  return canvas.convertToBlob({ type: "image/png" });
}

const image = computed<TiledImage | undefined>(() => {
  const size = info.value;
  if (!size) return undefined;
  const look = query.value;
  const map = size.channels === 1 ? props.look.map : "none";
  const table = tableOf(map);
  const at = stored.value;
  return {
    width: size.width,
    height: size.height,
    tileSize: 256,
    levels: dziLevels(size.width, size.height),
    key: `${at.id}:${lookKey(look, map)}`,
    load: async (level, col, row, signal) => {
      const picture = await tile(at, { level, col, row }, { look, signal });
      return table ? mapped(picture, table) : picture;
    },
  };
});
const placement = computed(() => {
  const at = placementOf(props.one, props.contents, query.value.plane ?? 0);
  return at && props.own ? { ...at, x_um: 0, y_um: 0 } : at;
});
const tileViewer = shallowRef<InstanceType<typeof TileViewer>>();
/** The view as it is drawn, a PNG. */
defineExpose({ snapshot: () => tileViewer.value?.snapshot() ?? Promise.resolve(undefined) });
</script>

<template>
  <div class="record-viewer" :data-said="`viewer-${one.record.id}`">
    <TileViewer
      ref="tileViewer"
      :label="t('viewer.label', { name: one.record.name })"
      :image="image"
      :placement="placement"
      :fit="fit"
      :drawing="drawing"
      :tell="tell"
      @view="(rect) => emit('view', rect)"
      @press="(at) => emit('press', at)"
      @draw="(rect) => emit('draw', rect)"
      @drag="(rect) => emit('drag', rect)"
      @hover="(at) => emit('hover', at)"
    >
      <template #over="scope"><slot name="over" v-bind="scope" /></template>
    </TileViewer>
  </div>
</template>

<style scoped>
.record-viewer {
  display: grid;
  min-block-size: 0;
}
</style>
