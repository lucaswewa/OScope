<script setup lang="ts">
import Icon from "./Icon.vue";
import type { IconName } from "./icons";

/**
 * What stands where a list or a picture would be, while there is none
 * (ADR-0042): what is missing, why, and in the `actions` slot what to do
 * about it.
 */
defineProps<{
  title: string;
  icon?: IconName;
}>();
</script>

<template>
  <div class="empty">
    <Icon v-if="icon" class="sign" :name="icon" />
    <p class="title">{{ title }}</p>
    <p v-if="$slots.default" class="more"><slot /></p>
    <div v-if="$slots.actions" class="actions">
      <slot name="actions" />
    </div>
  </div>
</template>

<style scoped>
.empty {
  display: grid;
  gap: var(--space-2);
  justify-items: center;
  padding: var(--space-8) var(--space-4);
  text-align: center;
}

/* Larger than an icon's own size, which this must outweigh. */
.empty > .sign {
  inline-size: var(--size-control);
  block-size: var(--size-control);
  color: var(--text-muted);
}

.title {
  font-size: var(--font-size-title-3);
  font-weight: var(--font-weight-light);
}

.more {
  max-inline-size: 30rem;
  color: var(--text-muted);
}

.actions {
  display: flex;
  gap: var(--space-2);
  margin-block-start: var(--space-2);
}
</style>
