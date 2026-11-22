<script setup lang="ts">
import { computed, onScopeDispose, shallowRef, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";

import { ApiError } from "@/api/errors";
import { projectContents } from "@/api/projects";
import { useAction, useEvent } from "@/api/things/use";
import { recordsOf, type AnyRecord, type Contents } from "@/features/gallery/project/project";
import RecordDetail from "@/features/gallery/project/RecordDetail.vue";
import Button from "@/ui/Button.vue";
import EmptyState from "@/ui/EmptyState.vue";

/**
 * A project's page (W6.2), the Gallery's at `?project=`: its acquisitions,
 * its results and its reports, each one's details beside them, the one
 * chosen in `?record=`; kept live as the project changes. The index of
 * every project is rebuilt from here, from their folders.
 */
const props = defineProps<{ project: string }>();
const { t } = useI18n();
const route = useRoute();
const router = useRouter();
const rebuild = useAction("/api/v1/projects/rebuild_index");

const contents = shallowRef<Contents>();
const unread = shallowRef<string>();
let asked: AbortController | undefined;
async function load(): Promise<void> {
  asked?.abort();
  const mine = (asked = new AbortController());
  try {
    const read = (await projectContents(props.project, { signal: mine.signal })) as Contents;
    if (!mine.signal.aborted) {
      contents.value = read;
      unread.value = undefined;
    }
  } catch (thrown) {
    if (!mine.signal.aborted)
      unread.value = thrown instanceof ApiError ? thrown.message : String(thrown);
  }
}
watch(() => props.project, load, { immediate: true });
onScopeDispose(() => asked?.abort());
// Kept live: a change to the projects reads it again.
const changed = useEvent("/api/v1/projects/changed", { keep: 1 });
watch(
  () => changed.last.value,
  () => void load(),
);

const records = computed(() => (contents.value ? recordsOf(contents.value) : []));
const chosen = computed<AnyRecord | undefined>(() => {
  const id = route.query.record;
  return records.value.find((one) => one.record.id === id) ?? records.value[0];
});
const kinds = computed(() =>
  (["acquisition", "result", "report"] as const)
    .map((kind) => ({ kind, list: records.value.filter((one) => one.kind === kind) }))
    .filter((group) => group.list.length > 0),
);
function choose(one: AnyRecord): void {
  void router.replace({ query: { ...route.query, record: one.record.id } });
}
async function deleted(): Promise<void> {
  void router.replace({ query: { project: props.project } });
  await load();
}
const rebuilt = computed(() => {
  const output = rebuild.output.value;
  return output ? t("projectPage.rebuilt", { n: output.projects }, output.projects) : undefined;
});
const when = (time: string) => new Date(time).toLocaleDateString();
</script>

<template>
  <section class="project" aria-labelledby="project-title" data-said="project">
    <header class="head">
      <RouterLink :to="{ name: 'gallery' }" class="back">{{ t("projectPage.back") }}</RouterLink>
      <h1 id="project-title">{{ contents?.project.name ?? "…" }}</h1>
      <Button
        compact
        variant="bare"
        icon="refresh"
        :busy="rebuild.running.value"
        @click="rebuild.run({})"
      >
        {{ t("projectPage.rebuild") }}
      </Button>
    </header>
    <p v-if="unread" class="said failed" data-said="unread">{{ unread }}</p>
    <p v-if="rebuilt" class="said" data-said="rebuilt">{{ rebuilt }}</p>
    <p v-if="contents?.project.sample" class="said">{{ contents.project.sample }}</p>

    <EmptyState v-if="contents && !records.length" :title="t('projectPage.empty')" icon="folder" />
    <div v-else-if="contents" class="body">
      <nav class="records" :aria-label="t('projectPage.records')">
        <section v-for="group in kinds" :key="group.kind" :aria-labelledby="`kind-${group.kind}`">
          <h2 :id="`kind-${group.kind}`">
            {{ t(`projectPage.groups.${group.kind}`, { n: group.list.length }) }}
          </h2>
          <ul :data-said="`records-${group.kind}`">
            <li v-for="one in group.list" :key="one.record.id">
              <button
                type="button"
                class="entry"
                :aria-current="one.record.id === chosen?.record.id ? 'true' : undefined"
                @click="choose(one)"
              >
                <span class="name">{{ one.record.name }}</span>
                <span class="said">{{ when(one.record.created) }}</span>
              </button>
            </li>
          </ul>
        </section>
      </nav>
      <RecordDetail
        v-if="chosen"
        :key="chosen.record.id"
        :contents="contents"
        :one="chosen"
        @deleted="deleted"
      />
    </div>
  </section>
</template>

<style scoped>
.project {
  display: grid;
  gap: var(--space-3);
  align-content: start;
  padding: var(--space-3);
  overflow: auto;
}

.head {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2) var(--space-3);
  align-items: center;
}

.head h1 {
  flex: 1 1 auto;
  margin: 0;
}

.body {
  display: grid;
  grid-template-columns: minmax(12rem, 18rem) minmax(0, 1fr);
  gap: var(--space-4, var(--space-3));
  align-items: start;
}

.records {
  display: grid;
  gap: var(--space-3);
}

.records h2 {
  margin: 0 0 var(--space-1);
  font-size: var(--font-size-body);
}

.records ul {
  display: grid;
  gap: var(--space-1);
  margin: 0;
  padding: 0;
  list-style: none;
}

.entry {
  display: grid;
  inline-size: 100%;
  padding: var(--space-1) var(--space-2);
  border: 1px solid transparent;
  border-radius: var(--radius-control);
  background: none;
  color: inherit;
  font: inherit;
  text-align: start;
  cursor: pointer;
}

.entry:hover {
  background: var(--surface-hover);
}

.entry[aria-current="true"] {
  border-color: var(--accent);
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
