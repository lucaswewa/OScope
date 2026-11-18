<script setup lang="ts">
import { computed, useId } from "vue";

import { provideField } from "./control";

/**
 * A control with its label, and a hint or an error below it (ADR-0042). The
 * control inside takes its id and its description from here: nothing is
 * wired by hand.
 */
const props = defineProps<{
  label: string;
  /** A line of help, shown while there is no error. */
  hint?: string;
  /** What is wrong. Shown in the hint's place, and the control is then in error. */
  error?: string;
}>();

const id = useId();
const labelId = `${id}-label`;
const noteId = `${id}-note`;

provideField({
  id,
  labelId,
  describedBy: computed(() => (props.error || props.hint ? noteId : undefined)),
  invalid: computed(() => Boolean(props.error)),
});
</script>

<template>
  <div class="field">
    <label :id="labelId" class="label" :for="id">{{ label }}</label>
    <slot />
    <p v-if="error" :id="noteId" class="note error">{{ error }}</p>
    <p v-else-if="hint" :id="noteId" class="note">{{ hint }}</p>
  </div>
</template>

<style scoped>
/* One column as wide as the field: a control fills it, or keeps its own width at its start. */
.field {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  gap: var(--space-1);
  justify-items: start;
}

.label {
  font-weight: var(--font-weight-strong);
}

.note {
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

.error {
  color: var(--danger);
}
</style>
