<script setup lang="ts">
import { computed, shallowRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import { finished } from "@/api/jobs";
import { fileAddress } from "@/api/media";
import type { Recipe } from "@/api/pipeline";
import { projectContents } from "@/api/projects";
import {
  deleteResult,
  difference,
  rerun,
  resultList,
  summaryOf,
  type Analysed,
  type Summary,
} from "@/api/workspace";
import { comparison, type Row } from "@/features/workspace/analyses/compare";
import { provenanceOf, type Contents, type Result } from "@/features/gallery/project/project";
import Button from "@/ui/Button.vue";
import Confirm from "@/ui/Confirm.vue";
import ToggleButton from "@/ui/ToggleButton.vue";

/**
 * A project's results (W7.4, P48), the newest first: each with how it was
 * made and what it counted; made again; its recipe opened in the editor;
 * deleted, once asked; and two compared, as a table of what differs.
 */
const props = defineProps<{ project: string | undefined; version: number }>();
const emit = defineEmits<{ open: [recipe: Recipe] }>();
const { t } = useI18n();
const why = (thrown: unknown) => (thrown instanceof Error ? thrown.message : String(thrown));

const results = shallowRef<Analysed[]>([]);
const contents = shallowRef<Contents>();
const summaries = shallowRef<Record<string, Summary | undefined>>({});
const said = shallowRef("");
async function load(): Promise<void> {
  const project = props.project;
  if (!project) return void (results.value = []);
  try {
    const [listed, read] = await Promise.all([resultList(project), projectContents(project)]);
    const summary = (one: Analysed) => summaryOf(project, one.id).catch(() => undefined);
    const read2 = await Promise.all(
      listed.map(async (one) => [one.id, await summary(one)] as const),
    );
    [results.value, contents.value] = [listed.reverse(), read as Contents];
    summaries.value = Object.fromEntries(read2);
    chosen.value = chosen.value.filter((id) => listed.some((one) => one.id === id));
  } catch (thrown) {
    said.value = t("workspace.results.unread", { why: why(thrown) });
  }
}
watch(() => [props.project, props.version], load, { immediate: true });

const provenance = (one: Analysed) =>
  contents.value ? provenanceOf(one as unknown as Result, contents.value) : undefined;
const counts = (one: Analysed) =>
  Object.entries(summaries.value[one.id]?.rows ?? {})
    .map(([table, n]) => t("workspace.results.rows", { table, n }, n))
    .join(", ");

/** The two compared: the first chosen, then the second. */
const chosen = shallowRef<string[]>([]);
function toggle(id: string): void {
  const now = chosen.value.includes(id)
    ? chosen.value.filter((one) => one !== id)
    : [...chosen.value, id];
  chosen.value = now.slice(-2);
}
const rows = shallowRef<Row[]>();
const when = (one: Analysed) => one.created.slice(0, 19).replace("T", " ");
const named = (id: string | undefined) => {
  const one = results.value.find((each) => each.id === id);
  return one ? `${one.name}, ${when(one)}` : "";
};
watch(chosen, async ([a, b]) => {
  rows.value = undefined;
  if (!a || !b || !props.project) return;
  try {
    const changes = await difference(props.project, a, b);
    rows.value = comparison(changes, summaries.value[a], summaries.value[b]);
  } catch (thrown) {
    said.value = t("workspace.results.uncompared", { why: why(thrown) });
  }
});

const busy = shallowRef<string>();
async function again(one: Analysed): Promise<void> {
  busy.value = one.id;
  said.value = t("workspace.results.making", { name: one.name });
  try {
    await finished(await rerun(props.project!, one.id));
    // Said once the list shows it: what is pressed next is where it is.
    await load();
    said.value = t("workspace.results.made", { name: one.name });
  } catch (thrown) {
    said.value = t("workspace.results.unmade", { name: one.name, why: why(thrown) });
  } finally {
    busy.value = undefined;
  }
}

const deleting = shallowRef<Analysed>();
const asking = shallowRef(false);
async function remove(): Promise<void> {
  const one = deleting.value;
  if (!one) return;
  try {
    await deleteResult(props.project!, one.id);
    await load();
    said.value = t("workspace.results.deleted", { name: one.name });
  } catch (thrown) {
    said.value = t("workspace.results.undeleted", { name: one.name, why: why(thrown) });
  }
}
/** The recipe it ran: in its provenance, the analysis by its job's kind (P48). */
const recipeOf = (one: Analysed) =>
  (one.provenance.recipe as Record<string, { recipe?: Recipe }> | null)?.["workspace.run"]?.recipe;
const opened = (one: Analysed) => {
  const recipe = recipeOf(one);
  if (recipe) emit("open", recipe);
};
const file = (one: Analysed, name: string) => fileAddress(one.project, `results/${one.id}/${name}`);
const compared = computed(() => chosen.value.length === 2);
</script>

<template>
  <section class="results" :aria-labelledby="'results-title'" data-said="results">
    <h2 id="results-title">{{ t("workspace.results.title") }}</h2>
    <p v-if="!project" class="said">{{ t("workspace.results.choose") }}</p>
    <p v-else-if="!results.length" class="said">{{ t("workspace.results.none") }}</p>
    <ul class="list">
      <li v-for="one in results" :key="one.id" :data-said="`result-${one.name}`">
        <div class="head">
          <strong>{{ one.name }}</strong>
          <span class="said" data-said="made">{{ when(one) }}</span>
        </div>
        <p class="said" data-said="provenance">
          {{
            t("workspace.results.from", {
              input:
                provenance(one)
                  ?.inputs.map((input) => input.name ?? input.id)
                  .join(", ") ?? "",
              software: one.provenance.software,
            })
          }}
        </p>
        <p v-if="counts(one)" data-said="counts">{{ counts(one) }}</p>
        <p class="files">
          <a v-for="name in one.files" :key="name" :href="file(one, name)" download>{{ name }}</a>
        </p>
        <div class="actions">
          <ToggleButton
            compact
            :model-value="chosen.includes(one.id)"
            :aria-label="t('workspace.results.compareOne', { name: one.name })"
            @update:model-value="toggle(one.id)"
          >
            {{ t("workspace.results.compare") }}
          </ToggleButton>
          <Button
            compact
            :busy="busy === one.id"
            :aria-label="t('workspace.results.againOne', { name: one.name })"
            @click="again(one)"
          >
            {{ t("workspace.results.again") }}
          </Button>
          <Button
            compact
            :aria-label="t('workspace.results.openOne', { name: one.name })"
            :disabled="!recipeOf(one)"
            @click="opened(one)"
          >
            {{ t("workspace.results.open") }}
          </Button>
          <Button
            compact
            variant="bare"
            tone="danger"
            :aria-label="t('workspace.results.deleteOne', { name: one.name })"
            @click="((deleting = one), (asking = true))"
          >
            {{ t("workspace.results.delete") }}
          </Button>
        </div>
      </li>
    </ul>
    <section v-if="compared" class="compared" data-said="compared" aria-labelledby="compared-title">
      <h3 id="compared-title">
        {{ t("workspace.results.compared", { a: named(chosen[0]), b: named(chosen[1]) }) }}
      </h3>
      <p v-if="rows && !rows.length" class="said">{{ t("workspace.results.alike") }}</p>
      <table v-else-if="rows">
        <thead>
          <tr>
            <th scope="col">{{ t("workspace.results.where") }}</th>
            <th scope="col">{{ t("workspace.results.first") }}</th>
            <th scope="col">{{ t("workspace.results.second") }}</th>
            <th scope="col">{{ t("workspace.results.by") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in rows" :key="`${row.counted}:${row.place}`">
            <th scope="row">
              {{ row.counted ? t("workspace.results.counted", { table: row.place }) : row.place }}
            </th>
            <td>{{ row.before }}</td>
            <td>{{ row.after }}</td>
            <td>{{ row.by ?? "" }}</td>
          </tr>
        </tbody>
      </table>
    </section>
    <p class="said" role="status" data-said="results-said">{{ said }}</p>
    <Confirm
      v-model:open="asking"
      :title="t('workspace.results.deleteTitle')"
      :text="t('workspace.results.deleteSaid', { name: deleting?.name ?? '' })"
      :confirm="t('workspace.results.delete')"
      tone="danger"
      @confirm="remove"
    />
  </section>
</template>

<style scoped>
.results {
  display: grid;
  gap: var(--space-2);
  align-content: start;
  min-inline-size: 0;
}

h2,
h3 {
  margin: 0;
  font-size: var(--font-size-body);
}

.list {
  display: grid;
  gap: var(--space-2);
  margin: 0;
  padding: 0;
  list-style: none;
}

.list > li {
  display: grid;
  gap: var(--space-1);
  padding: var(--space-2);
  border: 1px solid var(--border);
  border-radius: var(--radius-control);
  background: var(--surface-raised);
}

.list p {
  margin: 0;
}

.head,
.actions,
.files {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-1) var(--space-3);
  align-items: baseline;
}

.files {
  font-size: var(--font-size-small);
}

.said {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

table {
  border-collapse: collapse;
  font-variant-numeric: tabular-nums;
}

th,
td {
  padding: var(--space-1) var(--space-2);
  border-block-end: 1px solid var(--border);
  text-align: start;
}
</style>
