<script setup lang="ts">
import { computed, shallowRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import { useAction, useProperty } from "@/api/things/use";
import ProjectFields from "@/features/projects/ProjectFields.vue";
import { useProjectChoice } from "@/features/projects/projectChoice";
import type { Focus } from "@/features/scan/plan";
import { usePlanning } from "@/features/scan/planning";
import { mosaicRegionOf, runInput } from "@/features/scan/running";
import type { Scan } from "@/features/scan/scanning";
import Button from "@/ui/Button.vue";
import Field from "@/ui/Field.vue";
import NumberField from "@/ui/NumberField.vue";
import Select from "@/ui/Select.vue";
import TextField from "@/ui/TextField.vue";

/**
 * The run (W5.2): the plan run as a scan into a project, named; each tile's
 * z found as chosen, at the stage's z now, by autofocus every so many
 * tiles, or by a focus map measured first; and the settling before each
 * frame. The live mosaic is begun over the plan first, to draw the scan's
 * frames alone, as they are taken.
 */
const props = withDefaults(
  defineProps<{
    scan: Scan;
    /** Whether the focus is chosen here: the histo scan chooses its map before (W5.3). */
    chooseFocus?: boolean;
  }>(),
  { chooseFocus: true },
);
const { t } = useI18n();
const plan = usePlanning();
const choice = useProjectChoice("oscope.scan.project");
const streaming = useProperty("/api/v1/camera/streaming", { every: 1_000 });
const begin = useAction("/api/v1/live_mosaic/begin");
const run = useAction("/api/v1/scan/run");

const name = shallowRef("");
const settleMs = shallowRef<number | null>(50);

/** The tissue found has no z of its own: a focus map or autofocus finds it. */
const tissue = computed(() => plan.pattern.value === "tissue");
const kinds = computed(() =>
  (tissue.value ? ["map", "autofocus"] : ["here", "autofocus", "map"]).map((value) => ({
    value,
    label: t(`scan.run.focuses.${value}`),
  })),
);
const MAP: Focus = { kind: "map", columns: 3, rows: 3, surface: "plane" };
function choose(kind: string): void {
  if (kind === "autofocus") plan.focus.value = { kind, every: 5 };
  else if (kind === "map") plan.focus.value = MAP;
  else plan.focus.value = { kind: "here" };
}
watch(
  tissue,
  (now) => {
    if (now && plan.focus.value.kind === "here") plan.focus.value = MAP;
  },
  { immediate: true },
);
/** A number of the focus's, as a field writes it: cleared, it is 1. */
const focusField = (key: "every" | "columns" | "rows") =>
  computed<number | null>({
    get: () => (plan.focus.value as Record<string, unknown>)[key] as number,
    set: (value) => (plan.focus.value = { ...plan.focus.value, [key]: value ?? 1 } as Focus),
  });
const every = focusField("every");
const columns = focusField("columns");
const rows = focusField("rows");
const surfaces = computed(() =>
  (["plane", "thin_plate"] as const).map((value) => ({
    value,
    label: t(`scan.run.surfaces.${value}`),
  })),
);
function surfaceIs(value: string): void {
  if (plan.focus.value.kind === "map")
    plan.focus.value = { ...plan.focus.value, surface: value as "plane" | "thin_plate" };
}

/** What the run waits for, if anything: a plan, a project, the stream, the last scan's end. */
const waits = computed(() => {
  if (!plan.preview.value || !plan.request.value) return t("scan.run.needsPlan");
  if (!choice.chosen.value) return t("scan.run.needsProject");
  if (streaming.value.value !== true) return t("scan.run.needsStream");
  if (props.scan.goes.value) return t("scan.run.needsEnd");
  return undefined;
});

async function runIt(): Promise<void> {
  const preview = plan.preview.value;
  const request = plan.request.value;
  if (!preview || !request || waits.value) return;
  // The live mosaic, over the plan, draws the scan's frames and no others.
  // Its other parts are the server's defaults, which the generated type asks for.
  const region = mosaicRegionOf(preview.plan);
  const mosaic = region && {
    region: {
      origin: { x: region.x, y: region.y },
      size: { width: region.width, height: region.height },
    },
    roam: false,
  };
  if (mosaic) await begin.run({ request: mosaic } as Parameters<typeof begin.run>[0]);
  const asked = { project: choice.project.value, name: name.value, settleMs: settleMs.value ?? 50 };
  const started = await run.run(runInput(request, asked));
  if (started) props.scan.show();
}
</script>

<template>
  <section class="run" aria-labelledby="run-title" data-said="run">
    <h2 id="run-title">{{ t("scan.run.title") }}</h2>
    <ProjectFields :choice="choice" />
    <Field :label="t('scan.run.name')">
      <TextField v-model="name" :placeholder="t('scan.run.namePlaceholder')" />
    </Field>
    <Field v-if="chooseFocus" :label="t('scan.run.focus')">
      <Select
        :model-value="plan.focus.value.kind"
        :options="kinds"
        @update:model-value="(value) => choose(value as string)"
      />
    </Field>
    <Field v-if="chooseFocus && plan.focus.value.kind === 'autofocus'" :label="t('scan.run.every')">
      <NumberField v-model="every" :min="1" :max="1000" :step="1" :digits="0" />
    </Field>
    <div v-else-if="chooseFocus && plan.focus.value.kind === 'map'" class="row" data-said="map">
      <Field class="grow" :label="t('scan.run.columns')">
        <NumberField v-model="columns" :min="1" :max="10" :step="1" :digits="0" />
      </Field>
      <Field class="grow" :label="t('scan.run.rows')">
        <NumberField v-model="rows" :min="1" :max="10" :step="1" :digits="0" />
      </Field>
      <Field class="grow" :label="t('scan.run.surface')">
        <Select
          :model-value="plan.focus.value.surface"
          :options="surfaces"
          @update:model-value="(value) => surfaceIs(value as string)"
        />
      </Field>
    </div>
    <Field :label="t('scan.run.settle')">
      <NumberField v-model="settleMs" :min="0" :max="10000" :step="10" :digits="0" unit="ms" />
    </Field>
    <Button
      :busy="begin.running.value || run.running.value"
      :disabled="waits !== undefined"
      data-said="run-scan"
      @click="runIt"
    >
      {{ t("scan.run.go") }}
    </Button>
    <p v-if="waits" class="said" data-said="run-said">{{ waits }}</p>
  </section>
</template>

<style scoped>
.run {
  display: grid;
  gap: var(--space-2);
}

.run h2,
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
</style>
