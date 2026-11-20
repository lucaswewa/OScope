<script setup lang="ts">
import { computed, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";

import type { Job } from "@/api/jobs";
import JobDetail from "@/features/jobs/JobDetail.vue";
import JobQueue from "@/features/jobs/JobQueue.vue";
import SubmitJob from "@/features/jobs/SubmitJob.vue";
import { ranFor } from "@/features/jobs/jobs";
import { useJobs } from "@/features/jobs/useJobs";
import { kept } from "@/host/kept";
import Button from "@/ui/Button.vue";
import DataTable from "@/ui/DataTable.vue";
import EmptyState from "@/ui/EmptyState.vue";
import Splitter from "@/ui/Splitter.vue";
import type { Column } from "@/ui/table";

/**
 * Jobs (W4.1): every long piece of work, watched and steered from one tab.
 * The queue, in its order, and the history of the jobs that ended, beside
 * the job chosen. A job is chosen by its row, or by the address: the rail's
 * running job and a job's end lead here. Submitting a job by its kind is
 * for development builds.
 */
const { t } = useI18n();
const route = useRoute();
const router = useRouter();
const jobs = useJobs();
const development = import.meta.env.DEV;

const chosenId = computed({
  get: () => (typeof route.query.job === "string" ? route.query.job : undefined),
  set: (id) => void router.replace({ query: id ? { job: id } : {} }),
});
const chosen = computed(() => jobs.find(chosenId.value));
watch(chosenId, (id) => jobs.fetchMissing(id), { immediate: true });

const isPair = (value: unknown): value is [number, number] =>
  Array.isArray(value) && value.length === 2 && value.every((one) => typeof one === "number");
const sizes = kept("oscope.jobs.panes", [55, 45] as [number, number], isPair);

const columns = computed<Column<Job>[]>(() => [
  { key: "title", label: t("jobs.history.columns.title"), width: 220 },
  { key: "kind", label: t("jobs.history.columns.kind"), width: 150 },
  {
    key: "state",
    label: t("jobs.history.columns.state"),
    width: 110,
    text: (job) => t(`jobs.states.${job.state}`),
  },
  {
    key: "finished",
    label: t("jobs.history.columns.finished"),
    width: 170,
    text: (job) => (job.finished ? new Date(job.finished).toLocaleString() : "—"),
  },
  {
    key: "ran",
    label: t("jobs.history.columns.ran"),
    width: 90,
    align: "end",
    text: (job) => {
      const ran = ranFor(job, Date.now());
      return ran === undefined ? "—" : t("jobs.seconds", { s: ran.toFixed(1) });
    },
  },
]);
</script>

<template>
  <div class="jobs">
    <Splitter v-model:sizes="sizes" :label="t('jobs.handle')" :min="[30, 25]">
      <template #first>
        <section class="lists" :aria-label="t('jobs.lists')">
          <h1>{{ t("tabs.jobs.title") }}</h1>
          <JobQueue :jobs="jobs.queue.value" :chosen="chosenId" @choose="(id) => (chosenId = id)" />
          <section class="history" aria-labelledby="history-title" data-said="history">
            <h2 id="history-title">{{ t("jobs.history.title") }}</h2>
            <p v-if="jobs.failed.value" class="said failed">{{ jobs.failed.value }}</p>
            <DataTable
              :columns="columns"
              :rows="jobs.history.value"
              :row-key="(job) => job.id"
              :label="t('jobs.history.label')"
              picking="single"
              :selected="chosenId ? [chosenId] : []"
              @update:selected="
                (keys) => (chosenId = keys[0] === undefined ? undefined : String(keys[0]))
              "
            />
            <Button v-if="jobs.older.value" compact variant="bare" @click="jobs.showOlder">
              {{ t("jobs.history.older") }}
            </Button>
          </section>
          <SubmitJob v-if="development" />
        </section>
      </template>
      <template #second>
        <JobDetail v-if="chosen" :job="chosen" />
        <EmptyState v-else :title="t('jobs.none')" icon="jobs" />
      </template>
    </Splitter>
  </div>
</template>

<style scoped>
.jobs {
  display: grid;
  flex: 1;
  min-block-size: 0;
}

.lists {
  display: grid;
  gap: var(--space-3);
  align-content: start;
  block-size: 100%;
  padding: var(--space-3);
  background: var(--surface-pane);
  overflow: auto;
}

.lists h1,
.history h2,
.said {
  margin: 0;
}

.history {
  display: grid;
  gap: var(--space-2);
}

.said {
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

.said.failed {
  color: var(--danger);
}
</style>
