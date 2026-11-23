<script setup lang="ts">
import { computed, onMounted, onScopeDispose, shallowRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import { act } from "@/api/actions";
import { ApiError } from "@/api/errors";
import { finished } from "@/api/jobs";
import { waitingRecordings, type Recording } from "@/api/projects";
import { useEvent } from "@/api/things/use";
import { sizeOf } from "@/features/logging/logs";
import { useProjectChoice } from "@/features/projects/projectChoice";
import ProjectFields from "@/features/projects/ProjectFields.vue";
import Button from "@/ui/Button.vue";

/**
 * The recordings waiting in the scratch space (W6.3, ADR-0022): a scan
 * cancelled, a capture whose project went, one a crash cut short. Each is
 * ingested into the project chosen by the `projects.ingest` job, or left
 * where it is. Shown only while there are some.
 */
const { t } = useI18n();
const choice = useProjectChoice("oscope.gallery.ingest");

const recordings = shallowRef<Recording[]>([]);
async function load(): Promise<void> {
  try {
    recordings.value = await waitingRecordings();
  } catch {
    // Said as none: the Gallery is about its projects.
  }
}
onMounted(load);
// An ingest changes the projects; and a recording may end at any time.
const changed = useEvent("/api/v1/projects/changed", { keep: 1 });
watch(() => changed.last.value, load);
const every = setInterval(load, 10_000);
onScopeDispose(() => clearInterval(every));

/** Each recording's ingest under way, or why it failed. */
const ingests = shallowRef(new Map<string, { running: boolean; why?: string }>());
function told(id: string, now: { running: boolean; why?: string }): void {
  ingests.value = new Map(ingests.value).set(id, now);
}
async function ingest(one: Recording): Promise<void> {
  const project = choice.chosen.value?.id;
  if (!project) return;
  told(one.id, { running: true });
  try {
    const params = { recording: one.id, project, name: one.name };
    await finished(await act("/api/v1/jobs/submit", { kind: "projects.ingest", params }));
    told(one.id, { running: false });
    await choice.list();
    await load();
  } catch (thrown) {
    told(one.id, {
      running: false,
      why: thrown instanceof ApiError ? thrown.message : String(thrown),
    });
  }
}

const into = computed(() => choice.chosen.value?.name);
function about(one: Recording): string {
  const size = t("logging.files.size", sizeOf(one.bytes));
  const parts = [
    new Date(one.created).toLocaleString(),
    t("waiting.frames", { n: one.frames }, one.frames),
    size,
  ];
  if (!one.ended) parts.push(t("waiting.recording"));
  if (one.recovered) parts.push(t("waiting.recovered"));
  if (one.error) parts.push(one.error);
  return parts.join(" · ");
}
</script>

<template>
  <section
    v-if="recordings.length"
    class="waiting"
    aria-labelledby="waiting-title"
    data-said="waiting"
  >
    <h2 id="waiting-title">
      {{ t("waiting.title", { n: recordings.length }, recordings.length) }}
    </h2>
    <p class="said">{{ t("waiting.text") }}</p>
    <div class="fields"><ProjectFields :choice="choice" /></div>
    <ul class="list">
      <li v-for="one in recordings" :key="one.id" class="one" :data-said="`recording-${one.name}`">
        <span class="name">{{ one.name }}</span>
        <Button
          compact
          :disabled="!into || !one.ended"
          :busy="ingests.get(one.id)?.running"
          @click="ingest(one)"
        >
          {{ into ? t("waiting.ingest", { project: into }) : t("waiting.choose") }}
        </Button>
        <span class="said">{{ about(one) }}</span>
        <span v-if="ingests.get(one.id)?.why" class="said failed" role="alert">
          {{ ingests.get(one.id)?.why }}
        </span>
      </li>
    </ul>
  </section>
</template>

<style scoped>
.waiting {
  display: grid;
  gap: var(--space-2);
  padding: var(--space-3);
  border: 1px solid var(--border);
  border-radius: var(--radius-surface);
  background: var(--surface-raised);
}

.waiting h2 {
  margin: 0;
  font-size: var(--font-size-body);
}

.fields {
  display: grid;
  gap: var(--space-2);
  max-inline-size: 36rem;
}

.list {
  display: grid;
  gap: var(--space-2);
  margin: 0;
  padding: 0;
  list-style: none;
}

.one {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: var(--space-1) var(--space-2);
  align-items: center;
}

.one .said {
  grid-column: 1 / -1;
}

.name {
  overflow-wrap: anywhere;
}

.said {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

.said.failed {
  color: var(--danger);
}
</style>
