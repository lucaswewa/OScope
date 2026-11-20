<script setup lang="ts">
import { computed, shallowRef } from "vue";
import { useI18n } from "vue-i18n";

import { call, server } from "@/api/client";
import { ApiError } from "@/api/errors";
import { thingStore } from "@/api/things/store";
import { useProperty, type ValueOf } from "@/api/things/use";
import { curveOf, type FocusReport } from "@/features/control/focus/curve";
import { useJobAction } from "@/features/control/jobAction";
import Button from "@/ui/Button.vue";
import Chart from "@/ui/charts/Chart.vue";
import Field from "@/ui/Field.vue";
import NumberField from "@/ui/NumberField.vue";
import Progress from "@/ui/Progress.vue";
import Select from "@/ui/Select.vue";

/**
 * Autofocus (W3.7): a sweep in z for where the frame is sharpest (P34), run
 * as a job and followed. Its measure and its reach start as its defaults,
 * which follow the objective and the light. The last run's curve is drawn,
 * with where it left the stage: P34's follow-up. The frame's measures are
 * read when asked: each read waits for a frame and measures it.
 */
const { t } = useI18n();
const defaults = useProperty("/api/v1/autofocus/defaults", { every: 10_000 });
const last = useProperty("/api/v1/autofocus/last", { every: 10_000 });
const streaming = useProperty("/api/v1/camera/streaming", { every: 1_000 });
const focus = useJobAction("/api/v1/autofocus/run");

type Measure = FocusReport["measure"];
type Sweep = FocusReport["sweep"];
const chosen = shallowRef<Measure>();
const measure = computed(() => chosen.value ?? defaults.value.value?.measure ?? "brenner");
const sweep = shallowRef<Sweep>("stepped");
const reach = shallowRef<number | null>(null);
const measures = computed(() =>
  (["brenner", "tenengrad", "laplacian"] as const).map((value) => ({
    value,
    label: t(`focus.measures.${value}`),
  })),
);
const sweeps = computed(() =>
  (["stepped", "continuous"] as const).map((value) => ({
    value,
    label: t(`focus.sweeps.${value}`),
  })),
);

async function run(): Promise<void> {
  const reachOf = reach.value === null ? {} : { range_um: reach.value };
  await focus.run({ measure: measure.value, sweep: sweep.value, ...reachOf });
  await thingStore("autofocus").read("last");
}

/** What the last run found, or why it found nothing. */
const said = computed(() => {
  const report = last.value.value;
  if (!report) return undefined;
  if (!report.found) return t("focus.notFound", { why: report.refused ?? "—" });
  return t("focus.found", {
    z: report.z_um.toFixed(1),
    moved: (report.z_um - report.start_um).toFixed(1),
    frames: report.frames,
    took: (report.took_ms / 1000).toFixed(1),
  });
});
const curve = computed(() => {
  const report = last.value.value;
  return report
    ? curveOf(report, {
        coarse: t("focus.coarse"),
        fine: t("focus.fine"),
        focus: t("focus.focus"),
      })
    : [];
});

/** The frame's three measures, as read when asked. */
const now = shallowRef<ValueOf<"/api/v1/autofocus/measures">>();
const reading = shallowRef(false);
const unread = shallowRef<string>();
async function measureNow(): Promise<void> {
  reading.value = true;
  unread.value = undefined;
  try {
    now.value = await call((signal) => server.api.GET("/api/v1/autofocus/measures", { signal }));
  } catch (thrown) {
    now.value = undefined;
    unread.value = thrown instanceof ApiError ? thrown.message : String(thrown);
  } finally {
    reading.value = false;
  }
}
const nowSaid = computed(() => {
  const read = now.value;
  if (!read) return unread.value;
  const number = (value: number) => value.toPrecision(4);
  return t("focus.now", {
    brenner: number(read.brenner),
    tenengrad: number(read.tenengrad),
    laplacian: number(read.laplacian),
    z: read.z_um.toFixed(1),
  });
});
const about = computed(() => {
  const now = defaults.value.value;
  return now
    ? t("focus.defaults", {
        depth: now.depth_of_field_um.toFixed(1),
        reach: now.range_um.toFixed(0),
        coarse: now.coarse_um.toFixed(1),
        fine: now.fine_um.toFixed(1),
      })
    : undefined;
});
</script>

<template>
  <section class="focus" aria-labelledby="focus-title" data-said="autofocus">
    <h2 id="focus-title">{{ t("focus.title") }}</h2>
    <div class="row">
      <Field class="grow" :label="t('focus.measure')">
        <Select
          :model-value="measure"
          :options="measures"
          @update:model-value="(value) => (chosen = value as Measure)"
        />
      </Field>
      <Field class="grow" :label="t('focus.sweep')">
        <Select
          :model-value="sweep"
          :options="sweeps"
          @update:model-value="(value) => (sweep = value as Sweep)"
        />
      </Field>
      <Field class="grow" :label="t('focus.reach')">
        <NumberField
          v-model="reach"
          optional
          :min="1"
          :max="2000"
          :step="5"
          unit="µm"
          :placeholder="defaults.value.value?.range_um.toFixed(0)"
        />
      </Field>
    </div>
    <p v-if="about" class="said">{{ about }}</p>
    <div class="row">
      <Button :busy="focus.running.value" :disabled="streaming.value.value !== true" @click="run">
        {{ t("focus.run") }}
      </Button>
      <Button
        compact
        variant="bare"
        :busy="reading"
        :disabled="streaming.value.value !== true"
        @click="measureNow"
      >
        {{ t("focus.measureNow") }}
      </Button>
    </div>
    <p v-if="streaming.value.value !== true" class="said">{{ t("focus.needsStream") }}</p>
    <Progress
      v-if="focus.running.value"
      :value="focus.job.value?.progress.fraction ?? null"
      :max="1"
      :label="focus.job.value?.progress.stage || t('focus.running')"
    />
    <p v-if="nowSaid" class="said" data-said="measures">{{ nowSaid }}</p>
    <p v-if="focus.failed.value" class="said failed" data-said="focused">
      {{ focus.failed.value }}
    </p>
    <p v-else-if="said" class="said" data-said="focused">{{ said }}</p>
    <Chart
      v-if="curve.length"
      :title="t('focus.curve')"
      :series="curve"
      :x="{ title: t('focus.z') }"
      :y="{ title: t(`focus.measures.${last.value.value?.measure ?? measure}`) }"
      :height="140"
      :level="3"
    />
  </section>
</template>

<style scoped>
.focus {
  display: grid;
  gap: var(--space-2);
}

.focus h2,
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
  flex: 1 1 7rem;
}

.said {
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

.said.failed {
  color: var(--danger);
}
</style>
