<script setup lang="ts">
import { ToolbarRoot } from "reka-ui";

/**
 * A row of buttons for one thing (ADR-0042): one stop for Tab, and the
 * arrow keys from button to button. Its buttons are `ToolbarButton`s.
 */
defineProps<{
  /** What the buttons are for, to a screen reader. */
  label: string;
}>();
</script>

<template>
  <ToolbarRoot class="toolbar" :aria-label="label">
    <slot />
  </ToolbarRoot>
</template>

<style scoped>
.toolbar {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-1);
  align-items: center;
}
</style>
