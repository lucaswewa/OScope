<script setup lang="ts">
import { useI18n } from "vue-i18n";

import RecipeEditor from "@/features/workspace/editor/RecipeEditor.vue";

/**
 * The Workspace (W7): where data is processed, analysed and reported. Its
 * first part, the graph editor (W7.2): a recipe drawn as a graph.
 */
const { t } = useI18n();
</script>

<template>
  <div class="workspace" data-said="workspace">
    <h1>{{ t("tabs.workspace.title") }}</h1>
    <RecipeEditor />
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
</style>
