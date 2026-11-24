<script setup lang="ts">
import { computed, onMounted, ref, shallowRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import { finished } from "@/api/jobs";
import { recipeList, recipeNamed, type ListedRecipe, type Recipe } from "@/api/pipeline";
import { projectContents, projectPage, type ProjectSummary } from "@/api/projects";
import { analyse, type Analysis, type Region } from "@/api/workspace";
import type { Contents } from "@/features/gallery/project/project";
import { relistProjects } from "@/features/workspace/projects";
import Button from "@/ui/Button.vue";
import Field from "@/ui/Field.vue";
import NumberField from "@/ui/NumberField.vue";
import Select from "@/ui/Select.vue";
import Switch from "@/ui/Switch.vue";

/**
 * A recipe run on a project's image (W7.4, P48): the editor's recipe, or
 * one of the library's; an acquisition or a result of a project; all of it,
 * or a region in µm. A job, followed to its end.
 */
const props = defineProps<{ editing: Recipe }>();
const emit = defineEmits<{ ran: [project: string] }>();
/** The project chosen, by ID: the results' too. */
const project = defineModel<string>("project");
const { t } = useI18n();
const why = (thrown: unknown) => (thrown instanceof Error ? thrown.message : String(thrown));

const projects = shallowRef<ProjectSummary[]>([]);
/** The projects read again as their list opens: one made since is offered. */
const relisted = () => relistProjects(projects);
const listed = shallowRef<ListedRecipe[]>([]);
const said = shallowRef("");
onMounted(async () => {
  try {
    [projects.value, listed.value] = await Promise.all([
      projectPage({ limit: 500 }).then((page) => page.projects),
      recipeList(),
    ]);
  } catch (thrown) {
    said.value = t("workspace.run.unread", { why: why(thrown) });
  }
});

const source = shallowRef("editor");
const sources = computed(() => [
  { value: "editor", label: t("workspace.run.editing", { name: props.editing.name }) },
  ...listed.value.map((one) => ({ value: one.name, label: one.title || one.name })),
]);

/** The image: `acquisition:ID` or `result:ID`. */
const input = shallowRef<string>();
const contents = shallowRef<Contents>();
watch(
  project,
  async (id) => {
    contents.value = undefined;
    input.value = undefined;
    if (!id) return;
    contents.value = (await projectContents(id)) as Contents;
    const first = contents.value.acquisitions[0];
    input.value = first && `acquisition:${first.id}`;
  },
  { immediate: true },
);
const inputs = computed(() => [
  ...(contents.value?.acquisitions ?? []).map((one) => ({
    value: `acquisition:${one.id}`,
    label: one.name,
  })),
  ...(contents.value?.results ?? []).map((one) => ({
    value: `result:${one.id}`,
    label: t("workspace.run.result", { name: one.name }),
  })),
]);

const whole = shallowRef(true);
const region = ref<Region>({ x: 0, y: 0, width: 500, height: 500 });
const running = shallowRef(false);
async function run(): Promise<void> {
  const [kind, id] = (input.value ?? "").split(":") as ["acquisition" | "result", string];
  if (!project.value || !id) return;
  running.value = true;
  said.value = t("workspace.run.running");
  try {
    const chosen =
      source.value === "editor" ? props.editing : (await recipeNamed(source.value)).recipe;
    // Its nodes' places change nothing it makes: left out, they don't show as a difference.
    const recipe = { ...chosen, layout: {} };
    const analysis: Analysis = {
      recipe,
      input: { kind, id } as Analysis["input"],
      ...(whole.value ? {} : { roi: { ...region.value } }),
    };
    const job = await analyse(project.value, analysis);
    await finished(job, {
      seen: (now) =>
        (said.value = t("workspace.run.progress", {
          percent: Math.round(100 * (now.progress.fraction ?? 0)),
        })),
    });
    said.value = t("workspace.run.ran");
    emit("ran", project.value);
  } catch (thrown) {
    said.value = t("workspace.run.failed", { why: why(thrown) });
  } finally {
    running.value = false;
  }
}
</script>

<template>
  <form class="run-pane" data-said="run" @submit.prevent="run">
    <h2>{{ t("workspace.run.title") }}</h2>
    <Field :label="t('workspace.run.recipe')">
      <Select v-model="source" :options="sources" />
    </Field>
    <Field :label="t('workspace.run.project')">
      <Select
        v-model="project"
        :options="projects.map((one) => ({ value: one.id, label: one.name }))"
        :placeholder="t('workspace.run.chooseProject')"
        @opened="relisted"
      />
    </Field>
    <Field :label="t('workspace.run.input')">
      <Select v-model="input" :options="inputs" :placeholder="t('workspace.run.chooseInput')" />
    </Field>
    <Field :label="t('workspace.run.whole')">
      <Switch v-model="whole" />
    </Field>
    <fieldset v-if="!whole" class="region">
      <legend>{{ t("workspace.run.region") }}</legend>
      <Field
        v-for="side in ['x', 'y', 'width', 'height'] as const"
        :key="side"
        :label="t(`workspace.run.${side}`)"
      >
        <NumberField v-model="region[side]" :min="0" unit="µm" compact />
      </Field>
    </fieldset>
    <Button type="submit" variant="primary" :busy="running" :disabled="!input">
      {{ t("workspace.run.run") }}
    </Button>
    <p class="said" role="status" data-said="run-said">{{ said }}</p>
  </form>
</template>

<style scoped>
.run-pane {
  display: grid;
  gap: var(--space-2);
  align-content: start;
}

h2 {
  margin: 0;
  font-size: var(--font-size-body);
}

.region {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: var(--space-2);
  margin: 0;
  padding: var(--space-2);
  border: 1px solid var(--border);
  border-radius: var(--radius-control);
}

.said {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
