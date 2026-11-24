<script setup lang="ts">
import { onMounted, shallowRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import { projectContents, projectPage, type ProjectSummary } from "@/api/projects";
import { templateNames } from "@/api/reports";
import type { Contents } from "@/features/gallery/project/project";
import { relistProjects } from "@/features/workspace/projects";
import ReportsPane from "@/features/workspace/reports/ReportsPane.vue";
import TemplatesPane from "@/features/workspace/reports/TemplatesPane.vue";
import Field from "@/ui/Field.vue";
import Select from "@/ui/Select.vue";
import Tabs from "@/ui/Tabs.vue";

/**
 * A project's results as reports (W7.9, P47): made, read and downloaded;
 * and the templates they are made through, edited beside a preview.
 */
const { t } = useI18n();
const why = (thrown: unknown) => (thrown instanceof Error ? thrown.message : String(thrown));
const said = shallowRef("");
const projects = shallowRef<ProjectSummary[]>([]);
/** The projects read again as their list opens: one made since is offered. */
const relisted = () => relistProjects(projects);
const project = shallowRef<string>();
const contents = shallowRef<Contents>();
/** The templates' names, OScope's own first: listed again when one is saved or deleted. */
const names = shallowRef<string[]>(["standard"]);
async function relist(): Promise<void> {
  names.value = await templateNames().catch(() => names.value);
}
onMounted(async () => {
  void relist();
  projects.value = (await projectPage({ limit: 500 }).catch(() => ({ projects: [] }))).projects;
});
/** The project's records, read again after a report is made. */
async function reread(): Promise<void> {
  const id = project.value;
  try {
    const now = id ? ((await projectContents(id)) as Contents) : undefined;
    if (id === project.value) contents.value = now;
  } catch (thrown) {
    said.value = t("workspace.reports.unread", { why: why(thrown) });
  }
}
watch(project, () => {
  contents.value = undefined;
  void reread();
});
const tab = shallowRef("reports");
</script>

<template>
  <section class="reports-page" aria-labelledby="reports-title" data-said="reports">
    <h2 id="reports-title">{{ t("workspace.reports.title") }}</h2>
    <Field class="project" :label="t('workspace.run.project')">
      <Select
        v-model="project"
        :options="projects.map((one) => ({ value: one.id, label: one.name }))"
        :placeholder="t('workspace.run.chooseProject')"
        @opened="relisted"
      />
    </Field>
    <Tabs
      v-model="tab"
      keep
      :label="t('workspace.reports.tabs')"
      :tabs="[
        { value: 'reports', label: t('workspace.reports.made') },
        { value: 'templates', label: t('workspace.reports.templates') },
      ]"
    >
      <template #reports>
        <ReportsPane :project="project" :contents="contents" :names="names" @made="reread" />
      </template>
      <template #templates>
        <TemplatesPane :project="project" :contents="contents" :names="names" @changed="relist" />
      </template>
    </Tabs>
    <p class="said" role="status">{{ said }}</p>
  </section>
</template>

<style scoped>
.reports-page {
  display: grid;
  gap: var(--space-2);
  align-content: start;
}

.project {
  max-inline-size: 24rem;
}

h2 {
  margin: 0;
  font-size: var(--font-size-body);
}

.said {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
