<script setup lang="ts">
import { computed, shallowRef } from "vue";
import { useI18n } from "vue-i18n";

import type { Kind } from "@/api/calibration";
import { thingStore } from "@/api/things/store";
import { useProperty } from "@/api/things/use";
import { useJobAction } from "@/features/control/jobAction";
import ResultsPane from "@/features/settings/calibration/ResultsPane.vue";
import SetsPane from "@/features/settings/calibration/SetsPane.vue";
import Banner from "@/ui/Banner.vue";
import Button from "@/ui/Button.vue";
import Progress from "@/ui/Progress.vue";

/**
 * The calibration (W8.2, P32, P33, ADR-0023): its five routines, run one
 * at a time, each a job whose set is in use once measured; the objective's
 * check; what the sets in use measured; and every set, by objective.
 */
const { t } = useI18n();
const streaming = useProperty("/api/v1/camera/streaming", { every: 1_000 });
const lastCheck = useProperty("/api/v1/calibration/last_check", { every: 5_000 });

const ROUTINES: readonly Kind[] = ["dark", "flat", "colour", "geometry", "stage"];
const jobs = {
  dark: useJobAction("/api/v1/calibration/dark"),
  flat: useJobAction("/api/v1/calibration/flat"),
  colour: useJobAction("/api/v1/calibration/colour"),
  geometry: useJobAction("/api/v1/calibration/geometry"),
  stage: useJobAction("/api/v1/calibration/stage"),
  check: useJobAction("/api/v1/calibration/check_objective"),
};
type Run = keyof typeof jobs;

/** The routine running, or last run, and what became of it. */
const last = shallowRef<Run>();
const running = computed(() => Object.values(jobs).some((job) => job.running.value));
const job = computed(() => (last.value ? jobs[last.value] : undefined));
const said = computed(() => {
  const now = job.value;
  if (!last.value || !now || now.running.value) return "";
  const name = t(`settings.calibration.routines.${last.value}`);
  if (now.failed.value) return t("settings.calibration.failed", { name, why: now.failed.value });
  return now.job.value?.state === "completed" ? t("settings.calibration.done", { name }) : "";
});

async function run(which: Run): Promise<void> {
  last.value = which;
  await jobs[which].run({});
  const calibration = thingStore("calibration");
  await Promise.all(["sets", "active", "last_check"].map((name) => calibration.read(name)));
}

/** What the last check found. */
const checked = computed(() => {
  const found = lastCheck.value.value;
  if (!found) return t("settings.calibration.neverChecked");
  const said = {
    objective: found.objective,
    measured: found.measured_pixel_um.toFixed(3),
    expected: found.expected_pixel_um.toFixed(3),
    nearest: found.nearest,
  };
  return t(found.plausible ? "objective.plausible" : "objective.implausible", said);
});
</script>

<template>
  <div class="calibration">
    <section class="pane" aria-labelledby="routines-title" data-said="routines">
      <h2 id="routines-title">{{ t("settings.calibration.routinesTitle") }}</h2>
      <p class="note">{{ t("settings.calibration.routinesNote") }}</p>
      <Banner
        v-if="streaming.value.value !== true"
        level="info"
        :title="t('settings.calibration.needsStream')"
      />
      <ul class="routines">
        <li v-for="which in ROUTINES" :key="which">
          <Button
            compact
            :busy="jobs[which].running.value"
            :disabled="running || streaming.value.value !== true"
            @click="run(which)"
          >
            {{ t(`settings.calibration.routines.${which}`) }}
          </Button>
          <span class="note">{{ t(`settings.calibration.needs.${which}`) }}</span>
        </li>
        <li>
          <Button
            compact
            :busy="jobs.check.running.value"
            :disabled="running || streaming.value.value !== true"
            @click="run('check')"
          >
            {{ t("objective.check") }}
          </Button>
          <span class="note" data-said="last-check">{{ checked }}</span>
        </li>
      </ul>
      <Progress
        v-if="job?.running.value"
        :label="job.job.value?.progress.stage || t(`settings.calibration.routines.${last}`)"
        :value="job.job.value?.progress.fraction ?? undefined"
      />
      <p class="said" role="status" data-said="routine-said">{{ said }}</p>
    </section>
    <ResultsPane />
    <SetsPane />
  </div>
</template>

<style scoped>
.calibration,
.pane {
  display: grid;
  gap: var(--space-2);
  align-content: start;
}

.calibration {
  gap: var(--space-4);
}

h2 {
  margin: 0;
  font-size: var(--font-size-body);
}

.routines {
  display: grid;
  gap: var(--space-1);
  margin: 0;
  padding: 0;
  list-style: none;
}

.routines li {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  align-items: center;
}

.note,
.said {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
