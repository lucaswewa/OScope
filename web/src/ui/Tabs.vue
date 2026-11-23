<script setup lang="ts">
import { TabsContent, TabsList, TabsRoot, TabsTrigger } from "reka-ui";

import Icon from "./Icon.vue";
import type { IconName } from "./icons";

/**
 * Views of one thing, one at a time (ADR-0042). The arrow keys go from tab
 * to tab and show each at once; Home and End go to the ends; Tab goes on
 * into the view shown. Each view is the slot named by its tab's value.
 */
defineProps<{
  tabs: readonly { value: string; label: string; icon?: IconName; disabled?: boolean }[];
  /** What the tabs are, to a screen reader. */
  label: string;
}>();
const chosen = defineModel<string>({ required: true });

function choose(value: string | number): void {
  chosen.value = String(value);
}
</script>

<template>
  <TabsRoot class="tabs" :model-value="chosen" @update:model-value="choose">
    <TabsList class="list" :aria-label="label">
      <TabsTrigger
        v-for="tab in tabs"
        :key="tab.value"
        class="tab"
        :value="tab.value"
        :disabled="tab.disabled"
      >
        <Icon v-if="tab.icon" :name="tab.icon" small />
        <span>{{ tab.label }}</span>
      </TabsTrigger>
    </TabsList>
    <TabsContent v-for="tab in tabs" :key="tab.value" class="view" :value="tab.value">
      <slot :name="tab.value" />
    </TabsContent>
  </TabsRoot>
</template>

<style scoped>
.tabs {
  display: grid;
  grid-template-rows: auto 1fr;
  min-block-size: 0;
}

.list {
  display: flex;
  gap: var(--space-1);
  border-block-end: 1px solid var(--border);
}

.tab {
  display: inline-flex;
  gap: var(--space-2);
  align-items: center;
  block-size: var(--size-control);
  margin-block-end: -1px;
  padding-inline: var(--space-3);
  border: 0;
  border-block-end: var(--focus-width) solid transparent;
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
}

.tab:hover {
  background: var(--surface-hover);
  color: var(--text);
}

.tab[data-state="active"] {
  border-block-end-color: var(--accent-text);
  color: var(--text);
  font-weight: var(--font-weight-strong);
}

.tab[data-disabled] {
  color: var(--text-disabled);
  cursor: not-allowed;
}

.view {
  min-block-size: 0;
  padding-block-start: var(--space-4);
}
</style>
