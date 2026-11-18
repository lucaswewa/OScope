<script setup lang="ts">
import { useId } from "vue";

/**
 * A piece of the page with a title of its own (ADR-0042), on a raised
 * surface. Its buttons go in `actions`.
 */
withDefaults(
  defineProps<{
    title: string;
    /** Its title's level among the page's headings. */
    level?: 2 | 3 | 4;
  }>(),
  { level: 3 },
);
const id = useId();
</script>

<template>
  <section class="card" :aria-labelledby="id">
    <header class="top">
      <component :is="`h${level}`" :id="id" class="title">{{ title }}</component>
      <div v-if="$slots.actions" class="actions">
        <slot name="actions" />
      </div>
    </header>
    <slot />
  </section>
</template>

<style scoped>
.card {
  display: grid;
  gap: var(--space-3);
  align-content: start;
  padding: var(--space-4);
  border: 1px solid var(--border);
  border-radius: var(--radius-surface);
  background: var(--surface-raised);
  box-shadow: var(--shadow-card);
}

.top {
  display: flex;
  gap: var(--space-3);
  align-items: center;
  justify-content: space-between;
}

.title {
  font-size: var(--font-size-title-3);
}

.actions {
  display: flex;
  gap: var(--space-2);
}
</style>
