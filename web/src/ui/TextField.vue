<script setup lang="ts">
import "./styles/field.css";

import { useControl, type ControlProps } from "./control";

/** A line of text (ADR-0042). */
const props = defineProps<
  ControlProps & {
    /** The smaller size, where several share a row. */
    compact?: boolean;
    placeholder?: string;
    /** A secret, a password: not shown, and filled in by the browser's keeper of them. */
    secret?: boolean;
    /** What the browser may fill it with: `username`, `current-password`… */
    autocomplete?: string;
  }
>();
const text = defineModel<string>({ default: "" });
const control = useControl(props);
</script>

<template>
  <input
    :id="control.id.value"
    v-model="text"
    class="ui-field"
    :class="{ compact }"
    :type="secret ? 'password' : 'text'"
    :autocomplete="autocomplete"
    :placeholder="placeholder"
    :disabled="disabled"
    :readonly="readonly"
    :aria-label="label"
    :aria-invalid="control.invalid.value || undefined"
    :aria-describedby="control.describedBy.value"
  />
</template>
