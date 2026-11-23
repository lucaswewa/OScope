<script setup lang="ts">
import { shallowRef, useTemplateRef } from "vue";
import { useI18n } from "vue-i18n";

import { emptyRecipe } from "@/features/workspace/editor/graph";
import type { Recipe } from "@/api/pipeline";
import ResultsPane from "@/features/workspace/analyses/ResultsPane.vue";
import AnnotatePage from "@/features/workspace/annotate/AnnotatePage.vue";
import SurfacePage from "@/features/workspace/three-d/SurfacePage.vue";
import RunPane from "@/features/workspace/analyses/RunPane.vue";
import RecipeEditor from "@/features/workspace/editor/RecipeEditor.vue";
import Tabs from "@/ui/Tabs.vue";

/**
 * The Workspace (W7): where data is processed, analysed and reported. The
 * graph editor (W7.2, W7.3), and the analyses (W7.4): a recipe run on a
 * project's image, and its results; and annotations (W7.5), drawn and
 * measured on a record's image. Each tab keeps what it holds while another
 * is shown.
 */
const { t } = useI18n();
const tab = shallowRef("recipe");
const tabs = [
  { value: "recipe", label: t("workspace.tabs.recipe") },
  { value: "analyses", label: t("workspace.tabs.analyses") },
  { value: "annotate", label: t("workspace.tabs.annotate") },
  { value: "surfaces", label: t("workspace.tabs.surfaces") },
];
const editor = useTemplateRef<InstanceType<typeof RecipeEditor>>("editor");
/** The editor's recipe, as it is now: the one a run takes, unless another is chosen. */
const editing = shallowRef<Recipe>(emptyRecipe(""));
const project = shallowRef<string>();
/** Bumped by each run, for the results to be read again. */
const runs = shallowRef(0);

async function open(recipe: Recipe): Promise<void> {
  tab.value = "recipe";
  await editor.value?.open(recipe);
}
</script>

<template>
  <div class="workspace" data-said="workspace">
    <h1>{{ t("tabs.workspace.title") }}</h1>
    <Tabs v-model="tab" :tabs="tabs" :label="t('workspace.tabs.label')" keep>
      <template #recipe>
        <RecipeEditor ref="editor" @recipe="editing = $event" />
      </template>
      <template #analyses>
        <div class="analyses">
          <RunPane v-model:project="project" :editing="editing" @ran="runs++" />
          <ResultsPane :project="project" :version="runs" @open="open" />
        </div>
      </template>
      <template #annotate>
        <AnnotatePage />
      </template>
      <template #surfaces>
        <SurfacePage />
      </template>
    </Tabs>
  </div>
</template>

<style scoped>
.workspace {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: var(--space-2);
  min-block-size: 0;
  padding: var(--space-3);
}

.workspace h1 {
  margin: 0;
}

.workspace > :deep(.tabs) {
  flex: 1;
}

.analyses {
  display: grid;
  grid-template-columns: minmax(16rem, 22rem) minmax(0, 1fr);
  gap: var(--space-4);
  align-items: start;
}
</style>
