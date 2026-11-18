<script setup lang="ts">
import { CollapsibleContent, CollapsibleRoot, CollapsibleTrigger } from "reka-ui";

import Icon from "./Icon.vue";

/**
 * A part that opens and closes under its title (ADR-0042), as a pane's
 * sections do. Enter or Space on the title.
 */
withDefaults(
  defineProps<{
    title: string;
    /** Its title's level among the page's headings. */
    level?: 2 | 3 | 4;
  }>(),
  { level: 3 },
);
const open = defineModel<boolean>("open", { default: false });
</script>

<template>
  <CollapsibleRoot v-model:open="open" class="collapsible">
    <component :is="`h${level}`" class="heading">
      <CollapsibleTrigger class="trigger">
        <span>{{ title }}</span>
        <Icon :name="open ? 'collapse' : 'expand'" small />
      </CollapsibleTrigger>
    </component>
    <CollapsibleContent class="content">
      <slot />
    </CollapsibleContent>
  </CollapsibleRoot>
</template>

<style scoped>
.collapsible {
  border-block: 1px solid var(--border);
}

.collapsible + .collapsible {
  border-block-start: 0;
}

.heading {
  margin: 0;
  color: var(--text);
  font-size: var(--font-size-body);
  font-weight: var(--font-weight-strong);
}

.trigger {
  display: flex;
  align-items: center;
  justify-content: space-between;
  inline-size: 100%;
  block-size: var(--size-control);
  padding-inline: var(--space-1);
  border: 0;
  background: transparent;
  font-weight: inherit;
  text-align: start;
  cursor: pointer;
}

.trigger:hover,
.trigger[data-state="open"] {
  background: var(--surface-open);
}

.content {
  padding: var(--space-2) var(--space-1) var(--space-3);
}
</style>
