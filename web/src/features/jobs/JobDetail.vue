<script setup lang="ts">
import { computed, onScopeDispose, shallowRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import { JOB_ENDED, type Job } from "@/api/jobs";
import { logRecords, type LogRecord } from "@/api/media";
import { useAction } from "@/api/things/use";
import { percentOf, ranFor, resultPlace, timeLeft } from "@/features/jobs/jobs";
import Badge from "@/ui/Badge.vue";
import Banner from "@/ui/Banner.vue";
import Button from "@/ui/Button.vue";
import Confirm from "@/ui/Confirm.vue";
import KeyValue from "@/ui/KeyValue.vue";
import Progress from "@/ui/Progress.vue";

/**
 * A job (W4.1): its kind, title, account and state; its progress, with the
 * stage it is in and when it should end; its results, each leading to its
 * record; its error; steering, with the lease: cancel, pause and resume;
 * and its own log, from `logs/records`.
 */
const props = defineProps<{ job: Job }>();
const { t } = useI18n();
const cancel = useAction("/api/v1/jobs/cancel");
const pause = useAction("/api/v1/jobs/pause");
const resume = useAction("/api/v1/jobs/resume");

// A second's tick, for the time a running job has run.
const now = shallowRef(Date.now());
const tick = setInterval(() => (now.value = Date.now()), 1_000);
onScopeDispose(() => clearInterval(tick));

const when = (time: string | null | undefined) => (time ? new Date(time).toLocaleString() : "—");
const entries = computed(() => {
  const job = props.job;
  const ran = ranFor(job, now.value);
  return [
    { term: t("jobs.detail.kind"), value: job.kind },
    { term: t("jobs.detail.state"), value: t(`jobs.states.${job.state}`) },
    ...(job.account ? [{ term: t("jobs.detail.account"), value: job.account }] : []),
    { term: t("jobs.detail.submitted"), value: when(job.submitted) },
    { term: t("jobs.detail.started"), value: when(job.started) },
    { term: t("jobs.detail.finished"), value: when(job.finished) },
    {
      term: t("jobs.detail.ran"),
      value: ran === undefined ? "—" : t("jobs.seconds", { s: ran.toFixed(1) }),
    },
  ];
});
const percent = computed(() => percentOf(props.job));
const left = computed(() => {
  const time = timeLeft(props.job.progress.eta_s);
  return time ? t("jobs.detail.left", time) : undefined;
});

/** Cancelling what runs, or is paused, is asked first: its work so far stops there. */
const asking = shallowRef(false);
function cancelIt(): void {
  if (props.job.state === "queued") void cancel.run({ id: props.job.id });
  else asking.value = true;
}

// Its log: fetched again as it changes, at most every two seconds.
const records = shallowRef<LogRecord[]>([]);
let fetching: ReturnType<typeof setTimeout> | undefined;
let lastFetch = 0;
async function fetchLog(): Promise<void> {
  lastFetch = Date.now();
  const id = props.job.id;
  try {
    const page = await logRecords({ job: id, limit: 200 });
    if (props.job.id === id) records.value = page.records;
  } catch {
    // The log is a convenience: the job is said without it.
  }
}
function soon(): void {
  if (fetching) return;
  fetching = setTimeout(
    () => {
      fetching = undefined;
      void fetchLog();
    },
    Math.max(0, 2_000 - (Date.now() - lastFetch)),
  );
}
watch(
  () => props.job.id,
  () => {
    records.value = [];
    void fetchLog();
  },
  { immediate: true },
);
watch(() => [props.job.state, props.job.progress.stage], soon);
onScopeDispose(() => clearTimeout(fetching));
const levelTone = (level: LogRecord["level"]) =>
  level === "error" || level === "critical" ? "danger" : level === "warn" ? "warning" : "neutral";
</script>

<template>
  <section class="detail" aria-labelledby="job-title" data-said="job">
    <h2 id="job-title">{{ job.title }}</h2>
    <KeyValue :entries="entries" />
    <div v-if="job.state === 'running' || job.state === 'paused'" class="progress">
      <Progress
        :value="percent ?? null"
        :max="100"
        :label="job.progress.stage || t(`jobs.states.${job.state}`)"
      />
      <p v-if="left" class="said" data-said="left">{{ left }}</p>
    </div>
    <Banner v-if="job.error" level="danger" :title="t('jobs.detail.error')" data-said="error">
      {{ job.error }}
    </Banner>
    <div v-if="!JOB_ENDED.has(job.state)" class="actions">
      <Button
        v-if="job.state === 'running' && job.pausable"
        compact
        :busy="pause.running.value"
        @click="pause.run({ id: job.id })"
      >
        {{ t("jobs.detail.pause") }}
      </Button>
      <Button
        v-if="job.state === 'paused'"
        compact
        :busy="resume.running.value"
        @click="resume.run({ id: job.id })"
      >
        {{ t("jobs.detail.resume") }}
      </Button>
      <Button compact variant="bare" tone="danger" :busy="cancel.running.value" @click="cancelIt">
        {{ t("jobs.detail.cancel") }}
      </Button>
    </div>
    <section v-if="job.results.length" class="results" aria-labelledby="results-title">
      <h3 id="results-title">{{ t("jobs.detail.results") }}</h3>
      <ul>
        <li v-for="result in job.results" :key="result">
          <RouterLink
            v-if="resultPlace(result)"
            :to="{ name: 'gallery', query: resultPlace(result) }"
            data-said="result"
          >
            {{ result }}
          </RouterLink>
          <span v-else>{{ result }}</span>
        </li>
      </ul>
    </section>
    <section class="log" aria-labelledby="log-title">
      <h3 id="log-title">{{ t("jobs.detail.log") }}</h3>
      <ol v-if="records.length" class="records" data-said="job-log">
        <li v-for="record in records" :key="record.seq">
          <time :datetime="record.time">{{ new Date(record.time).toLocaleTimeString() }}</time>
          <Badge :tone="levelTone(record.level)">{{ record.level }}</Badge>
          <span>{{ record.message }}</span>
        </li>
      </ol>
      <p v-else class="said">{{ t("jobs.detail.noLog") }}</p>
    </section>
    <Confirm
      v-model:open="asking"
      :title="t('jobs.cancelling.title', { title: job.title })"
      :text="t('jobs.cancelling.text')"
      :confirm="t('jobs.cancelling.confirm')"
      tone="danger"
      @confirm="cancel.run({ id: job.id })"
    />
  </section>
</template>

<style scoped>
.detail {
  display: grid;
  gap: var(--space-3);
  align-content: start;
  padding: var(--space-3);
}

.detail h2,
.detail h3,
.said,
.results ul,
.records {
  margin: 0;
}

.progress,
.results,
.log {
  display: grid;
  gap: var(--space-2);
}

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
}

.results ul {
  padding-inline-start: var(--space-4);
}

.records {
  display: grid;
  gap: var(--space-1);
  padding: 0;
  list-style: none;
  font-size: var(--font-size-small);
}

.records li {
  display: grid;
  grid-template-columns: auto auto minmax(0, 1fr);
  gap: var(--space-2);
  align-items: baseline;
}

.records time {
  color: var(--text-muted);
  font-variant-numeric: tabular-nums;
}

.said {
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
