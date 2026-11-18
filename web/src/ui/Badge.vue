<script setup lang="ts">
import type { Level } from "./toasts";

/**
 * A word or a count beside something (ADR-0042), saying what state it is in
 * or how many it has. Its colour is never the only sign: it has its word.
 */
withDefaults(defineProps<{ tone?: "neutral" | "accent" | Level }>(), { tone: "neutral" });
</script>

<template>
  <span class="badge" :class="tone"><slot /></span>
</template>

<style scoped>
.badge {
  display: inline-flex;
  align-items: center;
  min-inline-size: var(--size-icon-small);
  block-size: var(--size-icon-small);
  padding-inline: var(--space-2);
  justify-content: center;
  border-radius: var(--radius-round);
  background: var(--surface-open);
  color: var(--text);
  font-size: var(--font-size-small);
  font-weight: var(--font-weight-strong);
  white-space: nowrap;
}

.accent {
  background: var(--accent);
  color: var(--accent-on);
}

.info {
  background: var(--info-tint);
  color: var(--info);
}

.success {
  background: var(--success-tint);
  color: var(--success);
}

.warning {
  background: var(--warning-tint);
  color: var(--warning);
}

.danger {
  background: var(--danger-tint);
  color: var(--danger);
}
</style>
