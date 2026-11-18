<script setup lang="ts">
/**
 * Terms and their values (ADR-0042), as About shows the instrument: the term
 * quiet, its value beside it.
 */
defineProps<{ entries: readonly { term: string; value: string }[] }>();
</script>

<template>
  <dl class="pairs">
    <template v-for="entry in entries" :key="entry.term">
      <dt>{{ entry.term }}</dt>
      <dd>{{ entry.value }}</dd>
    </template>
  </dl>
</template>

<style scoped>
.pairs {
  display: grid;
  grid-template-columns: max-content minmax(0, 1fr);
  gap: var(--space-1) var(--space-6);
  margin: 0;
}

dt {
  color: var(--text-muted);
}

dd {
  margin: 0;
  overflow-wrap: anywhere;
}
</style>
