<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";

import { thingStore } from "@/api/things/store";
import { useAction, useProperty } from "@/api/things/use";
import { useJobAction } from "@/features/control/jobAction";
import Banner from "@/ui/Banner.vue";
import Button from "@/ui/Button.vue";
import Field from "@/ui/Field.vue";
import Select from "@/ui/Select.vue";

/**
 * The objective (W3.4): which of the catalogue's is fitted. Objectives are
 * swapped by hand, so choosing one says which is in the light path: the
 * pixel's size at the sample follows, and with it the scale bar and every
 * µm the tab shows. After a change, the objective's check is offered (P32,
 * P33): two small moves measure the pixel, and say whether it is the one.
 */
const { t } = useI18n();
const catalogue = useProperty("/api/v1/objective/catalogue", { every: 60_000 });
const fitted = useProperty("/api/v1/objective/fitted", { every: 5_000 });
const streaming = useProperty("/api/v1/camera/streaming", { every: 1_000 });
const lastCheck = useProperty("/api/v1/calibration/last_check", { every: 5_000 });
const fit = useAction("/api/v1/objective/fit");
const check = useJobAction("/api/v1/calibration/check_objective");

const choices = computed(() =>
  (catalogue.value.value?.objective ?? []).map((objective) => ({
    value: objective.name,
    label: t("objective.choice", { name: objective.name, na: objective.na }),
  })),
);
const about = computed(() => {
  const now = fitted.value.value;
  return now
    ? t("objective.about", {
        model: now.model,
        na: now.na,
        distance: (now.working_distance / 1000).toFixed(1),
      })
    : t("objective.none");
});

/** The objective fitted here, whose check is offered until it is run. */
const changed = ref<string>();
async function choose(name: string | undefined): Promise<void> {
  if (!name || name === fitted.value.value?.name) return;
  if (!(await fit.run({ name }))) return;
  changed.value = name;
  // What the tab draws in µm follows at once, not at the next reading.
  const objective = thingStore("objective");
  await Promise.all([objective.read("fitted"), objective.read("pixel_at_sample")]);
}

async function runCheck(): Promise<void> {
  await check.run({});
  await thingStore("calibration").read("last_check");
  // Failed, it is offered again.
  if (!check.failed.value) changed.value = undefined;
}

/** What the last check found, for the objective fitted now. */
const found = computed(() => {
  const last = lastCheck.value.value;
  if (!last || last.objective !== fitted.value.value?.name) return undefined;
  const said = {
    objective: last.objective,
    measured: last.measured_pixel_um.toFixed(3),
    expected: last.expected_pixel_um.toFixed(3),
    nearest: last.nearest,
  };
  return t(last.plausible ? "objective.plausible" : "objective.implausible", said);
});
</script>

<template>
  <section class="objective" aria-labelledby="objective-title" data-said="objective">
    <h2 id="objective-title">{{ t("objective.title") }}</h2>
    <Field :label="t('objective.fitted')">
      <Select
        :model-value="fitted.value.value?.name"
        :options="choices"
        :placeholder="t('objective.none')"
        @update:model-value="choose"
      />
    </Field>
    <p class="said">{{ about }}</p>
    <Banner
      v-if="changed"
      level="info"
      :title="t('objective.offer', { name: changed })"
      data-said="objective-check"
    >
      {{ streaming.value.value === true ? t("objective.offerWhy") : t("objective.needsStream") }}
      <template #actions>
        <Button
          compact
          :busy="check.running.value"
          :disabled="streaming.value.value !== true"
          @click="runCheck"
        >
          {{ t("objective.check") }}
        </Button>
      </template>
    </Banner>
    <p v-if="check.failed.value" class="said failed" data-said="check">{{ check.failed.value }}</p>
    <p v-else-if="found" class="said" data-said="check">{{ found }}</p>
  </section>
</template>

<style scoped>
.objective {
  display: grid;
  gap: var(--space-2);
}

.objective h2,
.said {
  margin: 0;
}

.said {
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

.said.failed {
  color: var(--danger);
}
</style>
