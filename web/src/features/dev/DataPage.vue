<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";

import { logLines, type LogLine } from "@/features/dev/lines";
import GalleryHeader from "@/features/dev/GalleryHeader.vue";
import { kept } from "@/host/kept";
import Button from "@/ui/Button.vue";
import Card from "@/ui/Card.vue";
import Collapsible from "@/ui/Collapsible.vue";
import DataTable from "@/ui/DataTable.vue";
import EmptyState from "@/ui/EmptyState.vue";
import KeyValue from "@/ui/KeyValue.vue";
import Pagination from "@/ui/Pagination.vue";
import ScrollArea from "@/ui/ScrollArea.vue";
import Splitter from "@/ui/Splitter.vue";
import Tabs from "@/ui/Tabs.vue";
import Toolbar from "@/ui/Toolbar.vue";
import ToolbarButton from "@/ui/ToolbarButton.vue";
import Tree from "@/ui/Tree.vue";
import type { Column, Key, Sort } from "@/ui/table";
import type { Branch } from "@/ui/tree";

/**
 * Tabs, panes, a table of 100,000 rows, a tree, and the rest (ADR-0042).
 * The panes' sizes and the columns' widths are kept between visits.
 */
const { t } = useI18n();

const LINES = 100_000;
/** A count as English writes it: 100,000. */
const count = (n: number): string => n.toLocaleString("en");
const all = logLines(LINES);
const tab = ref("log");
const tabs = computed(() => [
  { value: "log", label: t("data.tabs.log"), icon: "menu" as const },
  { value: "files", label: t("data.tabs.files"), icon: "folder" as const },
  { value: "pieces", label: t("data.tabs.pieces"), icon: "settings" as const },
]);

// The log.
const errorsOnly = ref(false);
const sort = ref<Sort | null>(null);
const picked = ref<Key[]>([]);
const isPair = (value: unknown): value is [number, number] =>
  Array.isArray(value) && value.length === 2 && value.every((one) => typeof one === "number");
const isWidths = (value: unknown): value is Record<string, number> =>
  typeof value === "object" &&
  value !== null &&
  Object.values(value).every((one) => typeof one === "number");
const sizes = kept("oscope.gallery.panes", [70, 30] as [number, number], isPair);
const widths = kept("oscope.gallery.widths", {} as Record<string, number>, isWidths);

const rows = computed(() =>
  errorsOnly.value ? all.filter((line) => line.level === "error") : all,
);
const columns = computed<Column<LogLine>[]>(() => [
  { key: "n", label: t("data.log.n"), width: 80, align: "end", sortable: true },
  { key: "time", label: t("data.log.time"), width: 120, sortable: true },
  { key: "level", label: t("data.log.level"), width: 100, sortable: true },
  { key: "source", label: t("data.log.source"), width: 110, sortable: true },
  { key: "message", label: t("data.log.message"), width: 300, sortable: true },
]);
const line = computed(() => {
  const n = picked.value.at(-1);
  return typeof n === "number" ? all[n - 1] : undefined;
});
const lineEntries = computed(() =>
  line.value
    ? [
        { term: t("data.log.n"), value: String(line.value.n) },
        { term: t("data.log.time"), value: line.value.time },
        { term: t("data.log.level"), value: line.value.level },
        { term: t("data.log.source"), value: line.value.source },
        { term: t("data.log.message"), value: line.value.message },
      ]
    : [],
);

// The files.
const files: Branch[] = [
  {
    id: "scans",
    label: "scans",
    icon: "folder",
    children: [
      {
        id: "scans/slide-4",
        label: "slide-4",
        icon: "folder",
        children: [
          { id: "scans/slide-4/mosaic.ome.tiff", label: "mosaic.ome.tiff", icon: "file" },
          { id: "scans/slide-4/tiles.json", label: "tiles.json", icon: "file" },
          { id: "scans/slide-4/focus-map.json", label: "focus-map.json", icon: "file" },
        ],
      },
      {
        id: "scans/slide-5",
        label: "slide-5",
        icon: "folder",
        children: [{ id: "scans/slide-5/mosaic.ome.tiff", label: "mosaic.ome.tiff", icon: "file" }],
      },
    ],
  },
  {
    id: "captures",
    label: "captures",
    icon: "folder",
    children: [
      { id: "captures/capture-0001.png", label: "capture-0001.png", icon: "file" },
      { id: "captures/capture-0002.png", label: "capture-0002.png", icon: "file" },
    ],
  },
  { id: "project.json", label: "project.json", icon: "file" },
];
const file = ref<string | undefined>();
const open = ref<string[]>(["scans"]);

// The rest.
const scanOpen = ref(true);
const stitchOpen = ref(false);
const page = ref(1);
const CAPTURES = 240;
const PER_PAGE = 20;
const camera = computed(() => [
  { term: t("data.pieces.model"), value: "MIChrome 20" },
  { term: t("data.pieces.pixels"), value: "5440 × 3648" },
  { term: t("data.pieces.pixelSize"), value: "2.4 µm" },
]);
const stage = computed(() => [
  { term: t("data.pieces.travel"), value: "100 × 75 × 10 mm" },
  { term: t("data.pieces.step"), value: "0.1 µm" },
]);
</script>

<template>
  <main class="data">
    <GalleryHeader :title="t('data.title')" :intro="t('data.intro')" />

    <Tabs v-model="tab" class="tabs" :tabs="tabs" :label="t('data.tabs.label')">
      <template #log>
        <div class="log">
          <div class="bar">
            <Toolbar :label="t('data.log.tools')">
              <ToolbarButton icon="danger" :pressed="errorsOnly" @click="errorsOnly = !errorsOnly">
                {{ t("data.log.errorsOnly") }}
              </ToolbarButton>
              <ToolbarButton icon="down" @click="sort = { key: 'time', descending: true }">
                {{ t("data.log.newest") }}
              </ToolbarButton>
              <ToolbarButton icon="close" :disabled="picked.length === 0" @click="picked = []">
                {{ t("data.log.pickNone") }}
              </ToolbarButton>
            </Toolbar>
            <output data-said="log">
              {{
                t("data.log.count", {
                  shown: count(rows.length),
                  all: count(LINES),
                  picked: count(picked.length),
                })
              }}
            </output>
          </div>
          <Splitter v-model:sizes="sizes" class="split" :label="t('data.log.split')">
            <template #first>
              <DataTable
                v-model:sort="sort"
                v-model:selected="picked"
                v-model:widths="widths"
                class="table"
                :columns="columns"
                :rows="rows"
                :row-key="(row: LogLine) => row.n"
                :label="t('data.log.label')"
                picking="multiple"
              />
            </template>
            <template #second>
              <Card :level="2" v-if="line" class="line" :title="t('data.log.line', { n: line.n })">
                <KeyValue :entries="lineEntries" />
              </Card>
              <EmptyState v-else :title="t('data.log.none')">{{
                t("data.log.noneMore")
              }}</EmptyState>
            </template>
          </Splitter>
          <output data-said="panes">{{ sizes.map((size) => Math.round(size)).join(" : ") }}</output>
        </div>
      </template>

      <template #files>
        <div class="files">
          <Tree
            v-model="file"
            v-model:expanded="open"
            :items="files"
            :label="t('data.files.label')"
          />
          <Card :level="2" :title="t('data.files.picked')">
            <p data-said="file">{{ file ?? t("data.files.none") }}</p>
          </Card>
        </div>
      </template>

      <template #pieces>
        <div class="pieces">
          <div class="cards">
            <Card :level="2" :title="t('data.pieces.camera')">
              <KeyValue :entries="camera" />
              <template #actions>
                <Button compact>{{ t("data.pieces.calibrate") }}</Button>
              </template>
            </Card>
            <Card :level="2" :title="t('data.pieces.stage')">
              <KeyValue :entries="stage" />
            </Card>
          </div>
          <div class="sections">
            <Collapsible :level="2" v-model:open="scanOpen" :title="t('data.pieces.scan')">
              <p>{{ t("data.pieces.scanText") }}</p>
            </Collapsible>
            <Collapsible :level="2" v-model:open="stitchOpen" :title="t('data.pieces.stitch')">
              <p>{{ t("data.pieces.stitchText") }}</p>
            </Collapsible>
          </div>
          <div class="paging">
            <Pagination v-model="page" :total="CAPTURES" :per-page="PER_PAGE" />
            <output data-said="page">
              {{ t("data.pieces.page", { page, pages: CAPTURES / PER_PAGE }) }}
            </output>
          </div>
          <ScrollArea class="notes" :label="t('data.pieces.notes')">
            <p v-for="n in 12" :key="n">{{ t("data.pieces.note", { n }) }}</p>
          </ScrollArea>
        </div>
      </template>
    </Tabs>
  </main>
</template>

<style scoped>
.data {
  display: grid;
  gap: var(--space-6);
  max-inline-size: 80rem;
  padding: var(--space-6);
}

.log {
  display: grid;
  gap: var(--space-3);
}

.bar {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-4);
  align-items: center;
  justify-content: space-between;
}

.split {
  block-size: 28rem;
}

.table {
  block-size: 100%;
}

.line {
  margin-inline-start: var(--space-2);
}

.files {
  display: grid;
  grid-template-columns: 18rem minmax(0, 24rem);
  gap: var(--space-6);
  align-items: start;
}

.pieces {
  display: grid;
  gap: var(--space-6);
}

.cards {
  display: grid;
  grid-template-columns: repeat(auto-fill, 22rem);
  gap: var(--space-4);
}

.sections {
  max-inline-size: 30rem;
}

.paging {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-4);
  align-items: center;
}

.notes {
  max-inline-size: 30rem;
  block-size: 10rem;
  padding: var(--space-2);
  border: 1px solid var(--border);
}

output {
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
