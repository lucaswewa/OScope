<script setup lang="ts">
import { computed, onBeforeUnmount, watch } from "vue";
import { useI18n } from "vue-i18n";

import { useProperty } from "@/api/things/use";
import MapSaid from "@/features/scan/histo/MapSaid.vue";
import { useHisto } from "@/features/scan/histo/flow";
import { maskOf, reachable, STEPS, type Step } from "@/features/scan/histo/histo";
import OverviewPane from "@/features/scan/OverviewPane.vue";
import { usePlanning } from "@/features/scan/planning";
import RegionFields from "@/features/scan/RegionFields.vue";
import RunningPane from "@/features/scan/RunningPane.vue";
import RunPane from "@/features/scan/RunPane.vue";
import type { Scan } from "@/features/scan/scanning";
import Button from "@/ui/Button.vue";
import Checkbox from "@/ui/Checkbox.vue";
import Field from "@/ui/Field.vue";
import NumberField from "@/ui/NumberField.vue";
import Select from "@/ui/Select.vue";
import type { Rect } from "@/ui/tile-viewer/coordinates";
import ToggleButton from "@/ui/ToggleButton.vue";

/**
 * The histo scan (W5.3), a slide scanner's flow, step by step, each kept
 * when going back: the overview; the tissue found in it (P37); its islands
 * proposed as regions, each kept, dropped, edited or drawn again; the focus
 * map's grid, its points on the overview; and the run, which measures the
 * map before its first tile and shows it, each point's focus and the
 * surface fitted through them.
 */
const props = defineProps<{ scan: Scan }>();
const { t } = useI18n();
const plan = usePlanning();
const histo = useHisto();
const { step, regions, redrawing, columns, rows, surface } = histo;
const overview = useProperty("/api/v1/scan/last_overview", { every: 10_000 });

const flowNow = computed(() => ({
  overview: overview.value.value ?? undefined,
  regions: regions.value,
}));
const can = (one: Step) => reachable(one, flowNow.value);
const at = computed(() => STEPS.indexOf(step.value));
const next = computed(() => STEPS[at.value + 1]);

// A new overview's tissue proposes its regions.
watch(
  () => overview.value.value,
  (view) => {
    if (view) histo.propose(view);
  },
  { immediate: true },
);

// The plan: a snake over the regions kept, its z by the focus map the run measures.
watch(
  [() => overview.value.value, regions, columns, rows, surface],
  () => {
    const view = overview.value.value;
    plan.pattern.value = "snake";
    plan.mask.value = view ? maskOf(view, regions.value) : undefined;
    plan.focus.value = {
      kind: "map",
      columns: columns.value,
      rows: rows.value,
      surface: surface.value,
    };
  },
  { immediate: true, deep: true },
);
onBeforeUnmount(() => {
  plan.mask.value = undefined;
  plan.drawing.value = false;
  redrawing.value = undefined;
});

const tissueSaid = computed(() => {
  const view = overview.value.value;
  if (!view) return "";
  return t(
    "scan.histo.tissue",
    {
      kept: view.kept,
      area: view.area_mm2.toFixed(1),
      dropped: view.dropped,
      threshold: view.threshold.toFixed(2),
    },
    view.kept,
  );
});

function edit(index: number, rect: Rect | undefined): void {
  if (!rect) return;
  regions.value = regions.value.map((one, i) => (i === index ? { ...one, rect } : one));
}
function keep(index: number, kept: boolean): void {
  regions.value = regions.value.map((one, i) => (i === index ? { ...one, kept } : one));
}
/** Draws region `index` again on the view, or a new one past the last. */
function redraw(index: number, on: boolean): void {
  redrawing.value = on ? index : undefined;
  plan.drawing.value = on;
}

const grid = (key: "columns" | "rows") =>
  computed<number | null>({
    get: () => histo[key].value,
    set: (value) => (histo[key].value = value ?? 1),
  });
const columnsField = grid("columns");
const rowsField = grid("rows");
const surfaces = computed(() =>
  (["plane", "thin_plate"] as const).map((value) => ({
    value,
    label: t(`scan.run.surfaces.${value}`),
  })),
);
</script>

<template>
  <section class="histo" aria-labelledby="histo-title" data-said="histo">
    <h2 id="histo-title">{{ t("scan.histo.title") }}</h2>
    <ol class="steps" :aria-label="t('scan.histo.stepsLabel')">
      <li v-for="(one, i) in STEPS" :key="one">
        <Button
          compact
          :variant="one === step ? 'primary' : 'bare'"
          :disabled="!can(one)"
          :aria-current="one === step ? 'step' : undefined"
          @click="step = one"
        >
          {{ i + 1 }}. {{ t(`scan.histo.steps.${one}`) }}
        </Button>
      </li>
    </ol>

    <OverviewPane v-if="step === 'overview'" />
    <div v-else-if="step === 'tissue'" class="step">
      <p class="said" data-said="tissue">{{ tissueSaid }}</p>
      <Button
        compact
        variant="bare"
        @click="overview.value.value && histo.propose(overview.value.value, true)"
      >
        {{ t("scan.histo.proposeAgain") }}
      </Button>
    </div>
    <div v-else-if="step === 'regions'" class="step" data-said="regions">
      <p class="said">{{ t("scan.histo.regionsSaid", { n: regions.length }, regions.length) }}</p>
      <RegionFields
        v-for="(one, i) in regions"
        :key="i"
        :model-value="one.rect"
        :legend="t('scan.histo.region', { n: i + 1 })"
        :said="`region-${i + 1}`"
        @update:model-value="(rect) => edit(i, rect)"
      >
        <Checkbox :model-value="one.kept" @update:model-value="(kept) => keep(i, kept)">
          {{ t("scan.histo.keep") }}
        </Checkbox>
        <ToggleButton
          compact
          icon="edit"
          :model-value="redrawing === i"
          @update:model-value="(on) => redraw(i, on)"
        >
          {{ t("scan.histo.redraw") }}
        </ToggleButton>
      </RegionFields>
      <ToggleButton
        compact
        icon="edit"
        :model-value="redrawing === regions.length"
        @update:model-value="(on) => redraw(regions.length, on)"
      >
        {{ t("scan.histo.drawNew") }}
      </ToggleButton>
    </div>
    <div v-else-if="step === 'focus'" class="step" data-said="focus">
      <p class="said">{{ t("scan.histo.focusSaid") }}</p>
      <div class="row">
        <Field class="grow" :label="t('scan.run.columns')">
          <NumberField v-model="columnsField" :min="1" :max="10" :step="1" :digits="0" />
        </Field>
        <Field class="grow" :label="t('scan.run.rows')">
          <NumberField v-model="rowsField" :min="1" :max="10" :step="1" :digits="0" />
        </Field>
        <Field class="grow" :label="t('scan.run.surface')">
          <Select v-model="surface" :options="surfaces" />
        </Field>
      </div>
    </div>
    <template v-else>
      <RunPane :scan="props.scan" :choose-focus="false" />
      <RunningPane :scan="props.scan" />
      <MapSaid :scan="props.scan" />
    </template>

    <div class="row">
      <Button v-if="at > 0" compact variant="bare" @click="step = STEPS[at - 1]!">
        {{ t("scan.histo.back") }}
      </Button>
      <Button v-if="next" compact :disabled="!can(next)" @click="step = next">
        {{ t("scan.histo.next") }}
      </Button>
    </div>
  </section>
</template>

<style scoped>
.histo,
.step {
  display: grid;
  gap: var(--space-2);
}

.histo h2,
.said {
  margin: 0;
}

.steps {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-1);
  margin: 0;
  padding: 0;
  list-style: none;
}

.row {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  align-items: end;
}

.grow {
  flex: 1 1 5rem;
}

.said {
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
