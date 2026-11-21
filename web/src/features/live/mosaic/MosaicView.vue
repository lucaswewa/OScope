<script setup lang="ts">
import { computed, shallowRef } from "vue";
import { useI18n } from "vue-i18n";

import { useAction, useProperty } from "@/api/things/use";
import { fieldOf, mosaicImage, placementOf } from "@/features/live/mosaic/mosaic";
import Button from "@/ui/Button.vue";
import Confirm from "@/ui/Confirm.vue";
import { fieldAbout, type Point } from "@/ui/tile-viewer/coordinates";
import TileViewer from "@/ui/tile-viewer/TileViewer.vue";

/**
 * The live mosaic (W3.8, D13): drawn by the server from the frames as the
 * stage roams (P38), and shown in the tile viewer beside the live view.
 * Begun, halted and cleared here. Its tiles are fetched again as its
 * generation moves on, which its state, read each second, says; the field
 * of view is marked where the stage is; and a double click sends the stage
 * there.
 */
const { t } = useI18n();
const state = useProperty("/api/v1/live_mosaic/state", { every: 1_000 });
const position = useProperty("/api/v1/stage/position", { every: 1_000 });
const roi = useProperty("/api/v1/camera/roi", { every: 10_000 });
const capabilities = useProperty("/api/v1/camera/capabilities", { every: 60_000 });
const pixel = useProperty("/api/v1/objective/pixel_at_sample", { every: 10_000 });
const begin = useAction("/api/v1/live_mosaic/begin");
const halt = useAction("/api/v1/live_mosaic/halt");
const clear = useAction("/api/v1/live_mosaic/clear");
const move = useAction("/api/v1/stage/move_to");

const placement = computed(() => placementOf(state.value.value));
const image = computed(() => mosaicImage(state.value.value));

/** Where the stage is, and the field of view about it. */
const here = computed(() => position.value.value?.position);
const field = computed(() => {
  const camera = capabilities.value.value;
  const size =
    roi.value.value ?? (camera && { width: camera.sensor_width, height: camera.sensor_height });
  return fieldOf(here.value, size ?? undefined, pixel.value.value);
});
/** Where to look first: about the field, a few fields across. */
const firstLook = computed(() => {
  const now = field.value;
  if (!now || !here.value) return undefined;
  return fieldAbout(here.value, now.width * 8, now.height * 8);
});

const drawing = computed(() => state.value.value?.drawing === true);
const said = computed(() => {
  const now = state.value.value;
  if (!now?.canvas) return t("mosaic.none");
  const { drawn, blurred, same } = now.counts;
  return t(drawing.value ? "mosaic.drawing" : "mosaic.halted", {
    drawn,
    tiles: now.tiles,
    blurred,
    same,
  });
});

/** A new mosaic: the one before goes, so it is asked first when it has anything. */
const starting = shallowRef(false);
const clearing = shallowRef(false);
function beginMosaic(): void {
  if (state.value.value?.canvas && state.value.value.counts.drawn > 0) starting.value = true;
  else void begin.run({});
}
async function goTo(at: Point): Promise<void> {
  const z = here.value?.z;
  if (z === undefined) return;
  await move.run({ x: at.x, y: at.y, z });
}
</script>

<template>
  <section class="mosaic" :aria-label="t('mosaic.label')" data-said="mosaic">
    <div class="bar">
      <Button v-if="!drawing" compact :busy="begin.running.value" @click="beginMosaic">
        {{ t("mosaic.begin") }}
      </Button>
      <Button v-else compact :busy="halt.running.value" @click="halt.run({})">
        {{ t("mosaic.halt") }}
      </Button>
      <Button
        compact
        variant="bare"
        :disabled="!state.value.value?.canvas"
        @click="clearing = true"
      >
        {{ t("mosaic.clear") }}
      </Button>
      <p class="said" data-said="mosaic-said">{{ said }}</p>
    </div>
    <TileViewer
      :label="t('mosaic.viewer')"
      :image="image"
      :placement="placement"
      :marker="field"
      :start="firstLook"
      @pick="goTo"
    />
    <Confirm
      v-model:open="starting"
      :title="t('mosaic.again.title')"
      :text="t('mosaic.again.text')"
      :confirm="t('mosaic.again.confirm')"
      tone="danger"
      @confirm="begin.run({})"
    />
    <Confirm
      v-model:open="clearing"
      :title="t('mosaic.clearing.title')"
      :text="t('mosaic.clearing.text')"
      :confirm="t('mosaic.clearing.confirm')"
      tone="danger"
      @confirm="clear.run({})"
    />
  </section>
</template>

<style scoped>
.mosaic {
  display: grid;
  grid-template-rows: auto minmax(0, 1fr);
  block-size: 100%;
  min-block-size: 0;
}

.bar {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  align-items: center;
  padding: var(--space-1) var(--space-2);
  border-block-end: 1px solid var(--border);
}

.said {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
