<script setup lang="ts">
import { computed, shallowRef } from "vue";
import { useI18n } from "vue-i18n";

import type { Job, JobState } from "@/api/jobs";
import { useAction } from "@/api/things/use";
import { movedTo, percentOf } from "@/features/jobs/jobs";
import Badge from "@/ui/Badge.vue";
import Progress from "@/ui/Progress.vue";

/**
 * The queue (W4.1): what runs, what is paused, and what waits, in its order.
 * A waiting job is moved by dragging it onto another's place, or by Alt
 * with the arrows, Home and End on it; `jobs/reorder` takes it there.
 */
const props = defineProps<{ jobs: readonly Job[]; chosen?: string | undefined }>();
const emit = defineEmits<{ choose: [id: string] }>();
const { t } = useI18n();
const reorder = useAction("/api/v1/jobs/reorder");

const waiting = computed(() => props.jobs.filter((job) => job.state === "queued").length);
const tone = (state: JobState) =>
  state === "running" ? "accent" : state === "paused" ? "warning" : "neutral";
const placeSaid = (job: Job) =>
  job.state === "queued" && job.position !== null
    ? t("jobs.queue.place", { place: job.position + 1 })
    : (job.progress.stage ?? "");

function move(job: Job, position: number): void {
  void reorder.run({ id: job.id, position });
}
function key(job: Job, event: KeyboardEvent): void {
  if (job.state !== "queued" || job.position === null) return;
  const to = movedTo(event, job.position, waiting.value);
  if (to === undefined) return;
  event.preventDefault();
  move(job, to);
}

/** The waiting job being dragged. */
const dragged = shallowRef<string>();
function start(job: Job, event: DragEvent): void {
  dragged.value = job.id;
  event.dataTransfer?.setData("text/plain", job.id);
  if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
}
function over(job: Job, event: DragEvent): void {
  if (!dragged.value || job.state !== "queued") return;
  event.preventDefault();
  if (event.dataTransfer) event.dataTransfer.dropEffect = "move";
}
function drop(job: Job, event: DragEvent): void {
  event.preventDefault();
  const moving = props.jobs.find((one) => one.id === dragged.value);
  dragged.value = undefined;
  if (moving && moving.id !== job.id && job.position !== null) move(moving, job.position);
}
</script>

<template>
  <section class="queue" aria-labelledby="queue-title" data-said="queue">
    <h2 id="queue-title">{{ t("jobs.queue.title") }}</h2>
    <p v-if="!jobs.length" class="said">{{ t("jobs.queue.none") }}</p>
    <ol v-else class="list">
      <li
        v-for="job in jobs"
        :key="job.id"
        :draggable="job.state === 'queued'"
        :class="{ dragged: dragged === job.id }"
        @dragstart="start(job, $event)"
        @dragover="over(job, $event)"
        @drop="drop(job, $event)"
        @dragend="dragged = undefined"
      >
        <button
          type="button"
          class="row"
          :aria-current="job.id === chosen || undefined"
          :data-job="job.id"
          @click="emit('choose', job.id)"
          @keydown="key(job, $event)"
        >
          <span class="title">{{ job.title }}</span>
          <Badge :tone="tone(job.state)">{{ t(`jobs.states.${job.state}`) }}</Badge>
          <span class="said">{{ placeSaid(job) }}</span>
          <Progress
            v-if="job.state === 'running'"
            class="bar"
            :value="percentOf(job) ?? null"
            :max="100"
            :label="t('jobs.queue.progress', { title: job.title })"
          />
        </button>
      </li>
    </ol>
    <p v-if="waiting > 1" class="said">{{ t("jobs.queue.keys") }}</p>
  </section>
</template>

<style scoped>
.queue {
  display: grid;
  gap: var(--space-2);
}

.queue h2,
.list,
.said {
  margin: 0;
}

.list {
  display: grid;
  gap: var(--space-1);
  padding: 0;
  list-style: none;
}

.row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: var(--space-1) var(--space-2);
  align-items: center;
  inline-size: 100%;
  padding: var(--space-2);
  border: 1px solid var(--border);
  border-radius: var(--radius-control);
  background: var(--surface-raised);
  color: var(--text);
  font: inherit;
  text-align: start;
  cursor: pointer;
}

.row:hover {
  background: var(--surface-hover);
}

.row[aria-current="true"] {
  border-color: var(--accent);
  background: var(--accent-tint);
}

.title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.bar {
  grid-column: 1 / -1;
}

.dragged {
  opacity: 0.5;
}

.said {
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
