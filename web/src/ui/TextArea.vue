<script setup lang="ts">
import "./styles/field.css";

import { useControl, type ControlProps } from "./control";

/** Several lines of text (ADR-0042). */
const props = withDefaults(
  defineProps<
    ControlProps & {
      placeholder?: string;
      /** How many lines it shows. */
      rows?: number;
    }
  >(),
  { rows: 3 },
);
const text = defineModel<string>({ default: "" });
const control = useControl(props);
</script>

<template>
  <textarea
    :id="control.id.value"
    v-model="text"
    class="ui-field area"
    :rows="rows"
    :placeholder="placeholder"
    :disabled="disabled"
    :readonly="readonly"
    :aria-label="label"
    :aria-invalid="control.invalid.value || undefined"
    :aria-describedby="control.describedBy.value"
  />
</template>

<style scoped>
.area {
  display: block;
  block-size: auto;
  min-block-size: var(--size-control);
  padding-block: var(--space-2);
  resize: vertical;
}
</style>
