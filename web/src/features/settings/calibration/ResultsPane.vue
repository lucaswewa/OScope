<script setup lang="ts">
import { computed, shallowRef, useTemplateRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import { setImage, type Colour, type Dark, type Flat, type Geometry } from "@/api/calibration";
import type { Stage } from "@/api/calibration";
import { useProperty } from "@/api/things/use";
import { tableOf } from "@/features/live/colourMaps";
import { mapOf, residualsOf, type Map } from "@/features/settings/calibration/calibration";
import Chart from "@/ui/charts/Chart.vue";
import KeyValue from "@/ui/KeyValue.vue";

/**
 * What the sets in use measured (W8.2), for the camera's mode, gain and
 * exposure, the objective fitted and the lights on: the geometry and its
 * moves' residuals, the stage's backlash, the dark frame's level, the flat
 * field as a map, and the colour matrix.
 */
const { t } = useI18n();
const active = useProperty("/api/v1/calibration/active", { every: 5_000 });
const values = <T,>(kind: "geometry" | "stage" | "dark" | "flat" | "colour") =>
  computed(() => active.value.value?.[kind]?.values as T | undefined);
const geometry = values<Geometry>("geometry");
const stage = values<Stage>("stage");
const dark = values<Dark>("dark");
const flat = values<Flat>("flat");
const colour = values<Colour>("colour");
const fixed = (n: number, digits: number) => n.toFixed(digits);
const percent = (n: number) => (100 * n).toFixed(2);

const residuals = computed(() =>
  residualsOf(geometry.value?.residuals ?? [], t("settings.calibration.moves")),
);
const geometryEntries = computed(() => {
  const g = geometry.value;
  if (!g) return [];
  return [
    { term: t("settings.calibration.pixel"), value: `${fixed(g.pixel_um, 4)} µm` },
    { term: t("settings.calibration.rotation"), value: `${fixed(g.rotation_deg, 2)}°` },
    { term: t("settings.calibration.fitError"), value: `${fixed(g.residual_px, 3)} px` },
  ];
});
const darkEntries = computed(() => {
  const d = dark.value;
  if (!d) return [];
  return [
    { term: t("settings.calibration.level"), value: `${percent(d.mean)} %` },
    { term: t("settings.calibration.spread"), value: `${(100 * d.spread).toPrecision(2)} %` },
    {
      term: t("settings.calibration.takenAt"),
      value: t("settings.calibration.takenSaid", { ms: d.taken_ms, gain: d.gain }),
    },
  ];
});

// The flat field's map, read once for each set.
const flatId = computed(() => (flat.value ? active.value.value?.flat?.id : undefined));
const map = shallowRef<Map & { width: number; height: number }>();
const unread = shallowRef("");
const canvas = useTemplateRef<HTMLCanvasElement>("canvas");
watch(
  flatId,
  async (id) => {
    map.value = undefined;
    unread.value = "";
    if (!id) return;
    try {
      const image = await setImage(id);
      if (id === flatId.value) {
        map.value = { ...mapOf(image, tableOf("viridis")!), ...image };
      }
    } catch (thrown) {
      unread.value = thrown instanceof Error ? thrown.message : String(thrown);
    }
  },
  { immediate: true },
);
watch(
  [canvas, map],
  ([drawn, now]) => {
    const context = drawn?.getContext("2d");
    if (!context || !now) return;
    context.putImageData(new ImageData(now.rgba, now.width, now.height), 0, 0);
  },
  { flush: "post" },
);
</script>

<template>
  <section class="pane" aria-labelledby="results-title" data-said="in-use">
    <h2 id="results-title">{{ t("settings.calibration.resultsTitle") }}</h2>
    <p class="note">{{ t("settings.calibration.resultsNote") }}</p>
    <div class="results">
      <section class="result" data-said="result-geometry">
        <h3>{{ t("settings.calibration.routines.geometry") }}</h3>
        <template v-if="geometry">
          <KeyValue :entries="geometryEntries" />
          <Chart
            v-if="residuals.series[0]!.x.length"
            :title="t('settings.calibration.residuals')"
            :series="residuals.series"
            :x="{ title: t('settings.calibration.dx') }"
            :y="{ title: t('settings.calibration.dy'), range: [-residuals.reach, residuals.reach] }"
            :height="180"
            :level="4"
          />
        </template>
        <p v-else class="note">{{ t("settings.calibration.noneInUse") }}</p>
      </section>
      <section class="result" data-said="result-dark">
        <h3>{{ t("settings.calibration.routines.dark") }}</h3>
        <KeyValue v-if="dark" :entries="darkEntries" />
        <p v-else class="note">{{ t("settings.calibration.noneInUse") }}</p>
      </section>
      <section class="result" data-said="result-flat">
        <h3>{{ t("settings.calibration.routines.flat") }}</h3>
        <template v-if="flat">
          <p class="value">
            {{
              t("settings.calibration.flatSaid", {
                objective: flat.objective,
                light: flat.light,
                uneven: (100 * flat.uneven).toFixed(1),
              })
            }}
          </p>
          <figure v-if="map" class="map">
            <canvas
              ref="canvas"
              :width="map.width"
              :height="map.height"
              role="img"
              :aria-label="t('settings.calibration.mapLabel')"
              :style="{ aspectRatio: `${map.width} / ${map.height}` }"
            />
            <figcaption class="note" data-said="flat-range">
              {{
                t("settings.calibration.mapRange", {
                  low: fixed(map.low, 3),
                  high: fixed(map.high, 3),
                })
              }}
            </figcaption>
          </figure>
          <p v-if="unread" class="note">{{ t("settings.calibration.unread", { why: unread }) }}</p>
        </template>
        <p v-else class="note">{{ t("settings.calibration.noneInUse") }}</p>
      </section>
      <section class="result" data-said="result-colour">
        <h3>{{ t("settings.calibration.routines.colour") }}</h3>
        <template v-if="colour">
          <p class="value">
            {{
              t("settings.calibration.colourSaid", {
                light: colour.light,
                mean: fixed(colour.delta_e_mean, 2),
                before: fixed(colour.delta_e_before, 2),
              })
            }}
          </p>
          <table class="matrix">
            <caption class="note">
              {{
                t("settings.calibration.matrix")
              }}
            </caption>
            <tbody>
              <tr v-for="(row, i) in colour.matrix" :key="i">
                <td v-for="(value, j) in row" :key="j">{{ fixed(value, 3) }}</td>
              </tr>
            </tbody>
          </table>
        </template>
        <p v-else class="note">{{ t("settings.calibration.noneInUse") }}</p>
      </section>
      <section class="result" data-said="result-stage">
        <h3>{{ t("settings.calibration.routines.stage") }}</h3>
        <p v-if="stage" class="value">
          {{
            t("settings.calibration.backlash", {
              x: fixed(stage.backlash_um.x, 2),
              y: fixed(stage.backlash_um.y, 2),
            })
          }}
        </p>
        <p v-else class="note">{{ t("settings.calibration.noneInUse") }}</p>
      </section>
    </div>
  </section>
</template>

<style scoped>
.pane,
.result {
  display: grid;
  gap: var(--space-2);
  align-content: start;
}

h2,
h3 {
  margin: 0;
  font-size: var(--font-size-body);
}

.results {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(18rem, 1fr));
  gap: var(--space-3);
  align-items: start;
}

.result {
  padding: var(--space-2);
  border: 1px solid var(--border);
}

.map {
  display: grid;
  gap: var(--space-1);
  margin: 0;
}

.map canvas {
  inline-size: 100%;
  image-rendering: pixelated;
}

.matrix {
  border-collapse: collapse;
  font-variant-numeric: tabular-nums;
}

.matrix td {
  padding: var(--space-1) var(--space-2);
  border: 1px solid var(--border);
  text-align: end;
}

.matrix caption {
  text-align: start;
}

.value {
  margin: 0;
}

.note {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
