<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { useProperty } from "@/api/things/use";
import { upFor } from "@/features/about/about";
import { sizeOf } from "@/features/logging/logs";
import { warningsOf, type Warning } from "@/features/scan/plan";
import { usePlanning } from "@/features/scan/planning";
import RegionFields from "@/features/scan/RegionFields.vue";
import Banner from "@/ui/Banner.vue";
import Button from "@/ui/Button.vue";
import Field from "@/ui/Field.vue";
import IconButton from "@/ui/IconButton.vue";
import KeyValue from "@/ui/KeyValue.vue";
import NumberField from "@/ui/NumberField.vue";
import SchemaForm from "@/ui/schema-form/SchemaForm.vue";
import Select from "@/ui/Select.vue";
import ToggleButton from "@/ui/ToggleButton.vue";

/**
 * The plan (W5.1): a pattern, the built-in planners' and the extensions'
 * workflows', each with the form of what it needs: a region, drawn on the
 * view, typed or from the stage's corners; a stack; a place; positions; or
 * a workflow's parameters, by its schema. The server's plan of it follows
 * each change, with its estimates and its warnings. Nothing moves.
 */
const { t } = useI18n();
const plan = usePlanning();
const patterns = useProperty("/api/v1/scan/patterns", { every: 60_000 });
const position = useProperty("/api/v1/stage/position", { every: 1_000 });
const capabilities = useProperty("/api/v1/stage/capabilities", { every: 60_000 });
const active = useProperty("/api/v1/calibration/active", { every: 10_000 });
const scratch = useProperty("/api/v1/system/scratch_root", { every: 30_000 });

const BUILT_IN = new Set(["raster", "snake", "tiled_stack", "z_scan", "positions", "tissue"]);
const options = computed(() =>
  (patterns.value.value ?? []).map((one) => ({
    value: one.name,
    label: BUILT_IN.has(one.name) ? t(`scan.patterns.${one.name}`) : one.name,
  })),
);
const chosen = computed(() =>
  (patterns.value.value ?? []).find((one) => one.name === plan.pattern.value),
);
const needs = computed(() => {
  const name = plan.pattern.value;
  return {
    region: ["raster", "snake", "tiled_stack"].includes(name),
    stack: ["tiled_stack", "z_scan"].includes(name),
    at: name === "z_scan",
    positions: name === "positions",
    schema: chosen.value?.kind === "workflow" ? chosen.value.schema : undefined,
  };
});

const overlapPercent = computed<number | null>({
  get: () => Math.round(plan.overlap.value * 1000) / 10,
  set: (value) => (plan.overlap.value = (value ?? 10) / 100),
});
/** A stack's bound, as a field writes it: cleared, it is 0. */
const stackField = (key: "below_um" | "above_um" | "step_um") =>
  computed<number | null>({
    get: () => plan.stack.value[key],
    set: (value) => (plan.stack.value = { ...plan.stack.value, [key]: value ?? 0 }),
  });
const below = stackField("below_um");
const above = stackField("above_um");
const step = stackField("step_um");
const here = computed(() => position.value.value?.position);
function atHere(): void {
  if (here.value) plan.at.value = { x: here.value.x, y: here.value.y };
}
function addHere(): void {
  if (here.value)
    plan.positions.value = [...plan.positions.value, { x: here.value.x, y: here.value.y }];
}
function drop(index: number): void {
  plan.positions.value = plan.positions.value.filter((_, i) => i !== index);
}
const mm = (um: number) => (um / 1000).toFixed(3);

const entries = computed(() => {
  const preview = plan.preview.value;
  if (!preview) return [];
  const { plan: planned, estimate } = preview;
  const data = sizeOf(estimate.bytes);
  return [
    { term: t("scan.result.tiles"), value: String(estimate.tiles) },
    {
      term: t("scan.result.grid"),
      value: t("scan.result.gridIs", { rows: planned.rows, columns: planned.columns }),
    },
    { term: t("scan.result.overlap"), value: `${Math.round(planned.field.overlap * 100)} %` },
    { term: t("scan.result.time"), value: upFor(estimate.seconds) },
    { term: t("scan.result.data"), value: t("logging.files.size", data) },
  ];
});
const warnings = computed<Warning[]>(() => {
  const preview = plan.preview.value;
  if (!preview) return [];
  const travel = capabilities.value.value;
  return warningsOf(preview, {
    travel: travel && { x: travel.x, y: travel.y },
    calibrated: active.value.value ? active.value.value.geometry != null : undefined,
    available: scratch.value.value?.available_bytes,
  });
});
function said(warning: Warning): string {
  switch (warning.kind) {
    case "limits":
      return t("scan.warnings.limits", { tiles: warning.tiles });
    case "calibration":
      return t("scan.warnings.calibration");
    case "room":
      return t("scan.warnings.room", {
        needed: t("logging.files.size", sizeOf(warning.needed)),
        available: t("logging.files.size", sizeOf(warning.available)),
      });
  }
}
</script>

<template>
  <section class="plan" aria-labelledby="plan-title" data-said="plan">
    <h2 id="plan-title">{{ t("scan.plan.title") }}</h2>
    <Field :label="t('scan.plan.pattern')">
      <Select
        :model-value="plan.pattern.value"
        :options="options"
        @update:model-value="(value) => (plan.pattern.value = value as string)"
      />
    </Field>
    <RegionFields
      v-if="needs.region"
      v-model="plan.region.value"
      :legend="t('scan.plan.region')"
      said="plan-region"
    >
      <ToggleButton v-model="plan.drawing.value" compact icon="edit">{{
        t("scan.plan.draw")
      }}</ToggleButton>
    </RegionFields>
    <div v-if="needs.stack" class="row" data-said="stack">
      <Field class="grow" :label="t('scan.plan.below')">
        <NumberField v-model="below" :min="0" :step="1" unit="µm" />
      </Field>
      <Field class="grow" :label="t('scan.plan.above')">
        <NumberField v-model="above" :min="0" :step="1" unit="µm" />
      </Field>
      <Field class="grow" :label="t('scan.plan.step')">
        <NumberField v-model="step" :min="0.1" :step="0.5" unit="µm" />
      </Field>
    </div>
    <div v-if="needs.at" class="row" data-said="at">
      <Button compact variant="bare" :disabled="!here" @click="atHere">{{
        t("scan.plan.atHere")
      }}</Button>
      <span v-if="plan.at.value" class="said">
        {{ t("scan.plan.at", { x: mm(plan.at.value.x), y: mm(plan.at.value.y) }) }}
      </span>
    </div>
    <div v-if="needs.positions" class="positions" data-said="positions">
      <Button compact variant="bare" :disabled="!here" @click="addHere">{{
        t("scan.plan.addHere")
      }}</Button>
      <ol v-if="plan.positions.value.length">
        <li v-for="(one, i) in plan.positions.value" :key="i">
          {{ t("scan.plan.at", { x: mm(one.x), y: mm(one.y) }) }}
          <IconButton icon="close" :label="t('scan.plan.drop', { n: i + 1 })" @click="drop(i)" />
        </li>
      </ol>
    </div>
    <SchemaForm
      v-if="needs.schema"
      :key="plan.pattern.value"
      :model-value="plan.params.value as never"
      :schema="needs.schema"
      :label="t('scan.plan.params', { name: plan.pattern.value })"
      @update:model-value="(value) => (plan.params.value = value)"
    />
    <Field
      v-if="plan.pattern.value !== 'positions' && plan.pattern.value !== 'z_scan'"
      :label="t('scan.plan.overlap')"
    >
      <NumberField v-model="overlapPercent" :min="0" :max="50" :step="1" unit="%" />
    </Field>

    <p v-if="!plan.request.value" class="said" data-said="plan-said">{{ t("scan.plan.needs") }}</p>
    <p v-else-if="plan.refused.value" class="said failed" data-said="plan-said">
      {{ plan.refused.value }}
    </p>
    <KeyValue v-else-if="plan.preview.value" :entries="entries" data-said="estimate" />
    <ul v-if="warnings.length" class="warnings" data-said="warnings">
      <li v-for="warning in warnings" :key="warning.kind">
        <Banner level="warning">{{ said(warning) }}</Banner>
      </li>
    </ul>
  </section>
</template>

<style scoped>
.plan {
  display: grid;
  gap: var(--space-2);
}

.plan h2,
.said {
  margin: 0;
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

.said.failed {
  color: var(--danger);
}

.positions ol,
.warnings {
  display: grid;
  gap: var(--space-1);
  margin: 0;
  padding: 0;
  list-style-position: inside;
}

.warnings {
  list-style: none;
}

.positions li {
  display: flex;
  gap: var(--space-2);
  align-items: center;
}
</style>
