<script setup lang="ts">
import { useI18n } from "vue-i18n";

import Icon from "./Icon.vue";

/**
 * A label put on something (ADR-0042): a sample's stain, a scan's project.
 * With `removable`, it has a button that takes it off, named for the tag it
 * removes.
 */
defineProps<{
  label: string;
  removable?: boolean;
}>();
defineEmits<{ remove: [] }>();
const { t } = useI18n();
</script>

<template>
  <span class="tag">
    <span>{{ label }}</span>
    <button
      v-if="removable"
      class="remove"
      type="button"
      :aria-label="t('ui.tag.remove', { tag: label })"
      @click="$emit('remove')"
    >
      <Icon name="close" small />
    </button>
  </span>
</template>

<style scoped>
.tag {
  display: inline-flex;
  gap: var(--space-1);
  align-items: center;
  block-size: var(--size-icon);
  padding-inline: var(--space-2);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-control);
  background: var(--surface-raised);
  font-size: var(--font-size-small);
  white-space: nowrap;
}

/* As tall as the tag, so that it is a target of 24 px. */
.remove {
  display: grid;
  place-content: center;
  inline-size: var(--size-icon);
  block-size: var(--size-icon);
  margin-inline-end: calc(-1 * var(--space-2));
  padding: 0;
  border: 0;
  border-radius: var(--radius-control);
  background: transparent;
  cursor: pointer;
}

.remove:hover {
  background: var(--surface-hover);
}

.remove :deep(svg) {
  inline-size: var(--space-4);
  block-size: var(--space-4);
}
</style>
