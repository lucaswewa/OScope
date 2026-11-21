<script setup lang="ts">
import { computed, shallowRef } from "vue";
import { useI18n } from "vue-i18n";

import { useAction } from "@/api/things/use";
import { useJobAction } from "@/features/control/jobAction";
import { resultPlace } from "@/features/jobs/jobs";
import { standingOf } from "@/features/scan/running";
import type { Scan } from "@/features/scan/scanning";
import Banner from "@/ui/Banner.vue";
import Button from "@/ui/Button.vue";
import Confirm from "@/ui/Confirm.vue";
import Progress from "@/ui/Progress.vue";

/**
 * The scan (W5.2): the latest, whoever started it. Its tiles done and when
 * it should end, paused, resumed and cancelled, with the lease; its fault,
 * with its tile; what a cancel or a fault kept in scratch; and at its end,
 * the acquisition it made, opened in the Gallery, or stitched as a job
 * (P39).
 */
const props = defineProps<{ scan: Scan }>();
const { t } = useI18n();
const pause = useAction("/api/v1/jobs/pause");
const resume = useAction("/api/v1/jobs/resume");
const cancel = useAction("/api/v1/jobs/cancel");
const stitch = useJobAction("/api/v1/jobs/submit");

const standing = computed(() => {
  const progress = props.scan.progress.value;
  return progress && standingOf(progress, props.scan.job.value, Date.now());
});
const id = computed(() => props.scan.progress.value?.job ?? "");
const pausable = computed(() => props.scan.job.value?.pausable === true);
const ends = computed(() => standing.value?.endsAt?.toLocaleTimeString());
const fault = computed(() => standing.value?.fault);
const tiles = computed(() => {
  const now = standing.value;
  if (!now) return "";
  const state = t(`jobs.states.${now.state}`);
  return t("scan.running.tiles", { state, done: now.done, total: now.total });
});

/** Cancelling is asked first: the scan's frames so far are kept in scratch, not in the project. */
const asking = shallowRef(false);

async function stitchIt(): Promise<void> {
  const record = standing.value?.record;
  if (!record) return;
  await stitch.run({
    kind: "stitch.run",
    params: { project: record.project, acquisition: record.record },
  });
}
const stitched = computed(() =>
  stitch.job.value?.results.map(resultPlace).find((place) => place !== undefined),
);
</script>

<template>
  <section v-if="standing" class="running" aria-labelledby="running-title" data-said="scan">
    <h2 id="running-title">{{ t("scan.running.title") }}</h2>
    <p class="state" data-said="scan-tiles">{{ tiles }}</p>
    <Progress :value="standing.done" :max="standing.total" :label="tiles" />
    <p v-if="ends" class="said" data-said="ends">{{ t("scan.running.ends", { time: ends }) }}</p>
    <p v-if="standing.refused" class="said">
      {{ t("scan.running.refused", { n: standing.refused }) }}
    </p>
    <div v-if="scan.goes.value" class="row">
      <Button
        v-if="standing.state === 'running' && pausable"
        compact
        :busy="pause.running.value"
        @click="pause.run({ id })"
      >
        {{ t("scan.running.pause") }}
      </Button>
      <Button
        v-if="standing.state === 'paused'"
        compact
        :busy="resume.running.value"
        @click="resume.run({ id })"
      >
        {{ t("scan.running.resume") }}
      </Button>
      <Button compact variant="bare" tone="danger" @click="asking = true">
        {{ t("scan.running.cancel") }}
      </Button>
    </div>
    <Banner
      v-if="fault"
      level="danger"
      :title="
        fault.tile ? t('scan.running.faultAt', { n: fault.tile.number }) : t('scan.running.fault')
      "
      data-said="fault"
    >
      {{ fault.why }}
    </Banner>
    <p v-if="standing.kept" class="said" data-said="kept">
      {{ t("scan.running.kept", { kept: standing.kept }) }}
    </p>
    <div v-if="standing.record" class="made" data-said="made">
      <RouterLink :to="{ name: 'gallery', query: standing.record }">
        {{ t("scan.running.open") }}
      </RouterLink>
      <Button compact :busy="stitch.running.value" @click="stitchIt">
        {{ t("scan.running.stitch") }}
      </Button>
      <p v-if="stitch.job.value" class="said" data-said="stitch">
        {{ t("scan.running.stitching", { state: t(`jobs.states.${stitch.job.value.state}`) }) }}
        <RouterLink v-if="stitched" :to="{ name: 'gallery', query: stitched }">
          {{ t("scan.running.openStitched") }}
        </RouterLink>
      </p>
      <p v-else-if="stitch.failed.value" class="said failed" data-said="stitch">
        {{ stitch.failed.value }}
      </p>
    </div>
    <Confirm
      v-model:open="asking"
      :title="t('scan.running.cancelling.title')"
      :text="t('scan.running.cancelling.text')"
      :confirm="t('scan.running.cancelling.confirm')"
      tone="danger"
      @confirm="cancel.run({ id })"
    />
  </section>
</template>

<style scoped>
.running {
  display: grid;
  gap: var(--space-2);
}

.running h2,
.state,
.said {
  margin: 0;
}

.row,
.made {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  align-items: center;
}

.said {
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

.said.failed {
  color: var(--danger);
}
</style>
