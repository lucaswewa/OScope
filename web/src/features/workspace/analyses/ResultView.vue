<script setup lang="ts">
import { computed, shallowRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import { fetched } from "@/api/client";
import { fileAddress } from "@/api/media";
import type { Analysed, Summary } from "@/api/workspace";
import type { AnyRecord, Contents, Result } from "@/features/gallery/project/project";
import RecordViewer from "@/features/gallery/viewer/RecordViewer.vue";
import { imageOf, PLAIN } from "@/features/gallery/viewer/viewer";
import {
  gridOf,
  histogramOf,
  outlinesOf,
  perMm2,
  rowsOf,
  visible,
  type Outline,
} from "@/features/workspace/analyses/cells";
import { deconvolved, scoreOf, type Score } from "@/features/workspace/analyses/stains";
import Chart from "@/ui/charts/Chart.vue";
import Field from "@/ui/Field.vue";
import Select from "@/ui/Select.vue";
import type { Rect } from "@/ui/tile-viewer/coordinates";

/**
 * A result seen on the image it was made from (W7.6, D25). A cell count:
 * its outlines over the image, only those the view needs; the table of its
 * cells, a row leading the view to its cell; histograms of their area and
 * circularity; and their count per mm². Stains separated: each channel
 * alone, the vectors used, and DAB's positive fraction and H-score.
 */
const props = defineProps<{ result: Analysed; summary: Summary | undefined; contents: Contents }>();
const { t } = useI18n();
const why = (thrown: unknown) => (thrown instanceof Error ? thrown.message : String(thrown));
const said = shallowRef("");

const analysis = computed(
  () =>
    (props.result.provenance.recipe as Record<string, { recipe?: unknown }> | null)?.[
      "workspace.run"
    ],
);
const read = (name: string) =>
  fileAddress(props.result.project, `results/${props.result.id}/${name}`);
const has = (name: string) => props.result.files.includes(name);

// The image it was made from, and the result's own.
const input = computed<AnyRecord | undefined>(() => {
  const made = props.result.provenance.inputs[0];
  if (made?.kind === "acquisition") {
    const record = props.contents.acquisitions.find((one) => one.id === made.id);
    return record && { kind: "acquisition", record };
  }
  const record = props.contents.results.find((one) => one.id === made?.id);
  return record && { kind: "result", record };
});
const own = computed<AnyRecord>(() => ({
  kind: "result",
  record: props.result as unknown as Result,
}));

// A cell count.
const outlines = shallowRef<Outline[]>([]);
const cells = shallowRef<Record<string, string | number>[]>([]);
const view = shallowRef<Rect>();
const chosen = shallowRef<number>();
const fit = shallowRef<Rect>();
watch(
  () => props.result.id,
  async () => {
    [outlines.value, cells.value, chosen.value, fit.value] = [[], [], undefined, undefined];
    if (!has("outlines.geojson") || !has("objects.csv")) return;
    try {
      const [geojson, csv] = await Promise.all([
        fetched(read("outlines.geojson"), (answer) => answer.json() as Promise<unknown>),
        fetched(read("objects.csv"), (answer) => answer.text()),
      ]);
      const nodes = (
        analysis.value?.recipe as {
          nodes?: { node: string; params?: { pixel_um?: [number, number] } }[];
        }
      )?.nodes;
      const pixelUm = nodes?.find((node) => node.node === "cells.measure")?.params?.pixel_um ?? [
        1, 1,
      ];
      const image = input.value && imageOf(input.value);
      const [x = 0, y = 0] = props.summary?.pixels ?? [];
      outlines.value = outlinesOf(geojson, {
        pixelUm,
        at: [x, y],
        imageUm: image?.pixel_size?.x || 1,
      });
      cells.value = rowsOf(csv);
    } catch (thrown) {
      said.value = t("workspace.result.unread", { why: why(thrown) });
    }
  },
  { immediate: true },
);
const grid = computed(() => gridOf(outlines.value, 100));
/** The outlines drawn: those in view, none past a few thousand, when the view is too wide to see them. */
const MOST = 3000;
const shown = computed(() => {
  // Until the viewer says its view, all of the image.
  const all = { x: 0, y: 0, width: Infinity, height: Infinity };
  const inView = visible(grid.value, view.value ?? all);
  return inView.length > MOST ? [] : inView;
});
const byLabel = computed(() => new Map(outlines.value.map((outline) => [outline.label, outline])));
function choose(label: number): void {
  chosen.value = label;
  const cell = byLabel.value.get(label)?.bounds;
  // The cell, with room about it: four of it across.
  if (cell) {
    const side = Math.max(cell.width, cell.height, 1) * 4;
    fit.value = {
      x: cell.x + cell.width / 2 - side / 2,
      y: cell.y + cell.height / 2 - side / 2,
      width: side,
      height: side,
    };
  }
}
const series = (name: string) => {
  const { edges, counts, width } = histogramOf(
    cells.value.map((cell) => Number(cell[name])),
    20,
  );
  return [{ name, kind: "histogram" as const, x: edges, y: counts, width }];
};
const density = computed(() => {
  const [, , w = 0, h = 0] = props.summary?.pixels ?? [];
  const um = (input.value && imageOf(input.value)?.pixel_size?.x) || 1;
  return perMm2(cells.value.length, w * h * um * um);
});
const px = (outline: Outline, pixel: number) =>
  outline.ring.map((p) => `${(p.x / pixel).toFixed(1)},${(p.y / pixel).toFixed(1)}`).join(" ");

// Stains separated.
const stains = computed(() =>
  props.result.image ? deconvolved(analysis.value?.recipe) : undefined,
);
const channel = shallowRef("0");
const score = shallowRef<Score>();
watch(
  () => props.result.id,
  async () => {
    score.value = undefined;
    if (!has("score.csv")) return;
    const csv = await fetched(read("score.csv"), (answer) => answer.text()).catch(() => "");
    score.value = scoreOf(rowsOf(csv)[0]);
  },
  { immediate: true },
);
const fixed = (value: number, digits = 2) => value.toFixed(digits);
</script>

<template>
  <section class="result-view" aria-labelledby="result-title" data-said="result">
    <h3 id="result-title">{{ t("workspace.result.title", { name: result.name }) }}</h3>
    <template v-if="outlines.length && input">
      <p data-said="count">
        {{
          t("workspace.result.count", { n: cells.length, density: fixed(density, 1) }, cells.length)
        }}
      </p>
      <div class="viewer" data-said="cells-view">
        <RecordViewer
          :contents="contents"
          :one="input"
          :look="PLAIN"
          own
          tell
          :fit="fit"
          @view="view = $event"
        >
          <template #over="{ placement }">
            <g class="outlines" data-said="outlines" :data-shown="shown.length">
              <polygon
                v-for="outline in shown"
                :key="outline.label"
                :class="{ chosen: outline.label === chosen }"
                :points="px(outline, placement.pixel_um)"
              />
            </g>
          </template>
        </RecordViewer>
      </div>
      <p v-if="view && !shown.length" class="said">{{ t("workspace.result.zoomIn") }}</p>
      <div class="charts">
        <Chart
          :title="t('workspace.result.area')"
          :series="series('area')"
          :x="{ title: t('workspace.result.areaAxis') }"
          :y="{ title: t('workspace.result.cells') }"
          :height="160"
          :level="4"
        />
        <Chart
          :title="t('workspace.result.circularity')"
          :series="series('circularity')"
          :x="{ title: t('workspace.result.circularity') }"
          :y="{ title: t('workspace.result.cells') }"
          :height="160"
          :level="4"
        />
      </div>
      <div class="table">
        <table data-said="cells">
          <thead>
            <tr>
              <th scope="col">{{ t("workspace.result.label") }}</th>
              <th scope="col">{{ t("workspace.result.areaAxis") }}</th>
              <th scope="col">{{ t("workspace.result.circularity") }}</th>
              <th scope="col">{{ t("workspace.result.place") }}</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="cell in cells"
              :key="cell.label"
              :class="{ chosen: cell.label === chosen }"
              tabindex="0"
              @click="choose(Number(cell.label))"
              @keydown.enter="choose(Number(cell.label))"
            >
              <th scope="row">{{ cell.label }}</th>
              <td>{{ fixed(Number(cell.area)) }}</td>
              <td>{{ fixed(Number(cell.circularity), 3) }}</td>
              <td>{{ fixed(Number(cell.x)) }}, {{ fixed(Number(cell.y)) }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </template>
    <template v-if="stains">
      <Field :label="t('workspace.result.channel')">
        <Select
          v-model="channel"
          :options="stains.names.map((name, i) => ({ value: String(i), label: name }))"
        />
      </Field>
      <div class="viewer">
        <RecordViewer :contents="contents" :one="own" :look="PLAIN" :channel="Number(channel)" />
      </div>
      <table data-said="vectors">
        <caption>
          {{
            t("workspace.result.vectors")
          }}
        </caption>
        <thead>
          <tr>
            <th scope="col">{{ t("workspace.result.stain") }}</th>
            <th scope="col">R</th>
            <th scope="col">G</th>
            <th scope="col">B</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(vector, i) in stains.vectors" :key="i">
            <th scope="row">{{ stains.names[i] }}</th>
            <td v-for="(value, c) in vector" :key="c">{{ fixed(value, 3) }}</td>
          </tr>
        </tbody>
      </table>
    </template>
    <dl v-if="score" class="score" data-said="score">
      <dt>{{ t("workspace.result.positive") }}</dt>
      <dd data-said="positive">{{ fixed(score.positive) }} %</dd>
      <dd class="said">{{ t("workspace.result.positiveSaid") }}</dd>
      <dt>{{ t("workspace.result.hScore") }}</dt>
      <dd data-said="h-score">{{ fixed(score.hScore) }}</dd>
      <dd class="said">{{ t("workspace.result.hScoreSaid") }}</dd>
    </dl>
    <p class="said" role="status">{{ said }}</p>
  </section>
</template>

<style scoped>
.result-view {
  display: grid;
  gap: var(--space-2);
}

h3 {
  margin: 0;
  font-size: var(--font-size-body);
}

.viewer {
  display: grid;
  min-block-size: 24rem;
}

.outlines polygon {
  fill: none;
  stroke: var(--chart-2);
  stroke-width: 1.5;
  vector-effect: non-scaling-stroke;
}

.outlines polygon.chosen {
  stroke: var(--accent);
  stroke-width: 3;
}

.charts {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(16rem, 1fr));
  gap: var(--space-2);
}

.table {
  max-block-size: 16rem;
  overflow: auto;
}

table {
  border-collapse: collapse;
  font-variant-numeric: tabular-nums;
}

th,
td {
  padding: var(--space-1) var(--space-2);
  border-block-end: 1px solid var(--border);
  text-align: start;
}

caption {
  text-align: start;
}

tbody tr.chosen {
  background: var(--surface-hover);
}

.score {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: var(--space-1) var(--space-3);
  margin: 0;
}

.score .said {
  grid-column: 2;
}

.said {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
