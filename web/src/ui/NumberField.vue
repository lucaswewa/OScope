<script setup lang="ts">
import "./styles/field.css";

import { computed, ref, useId, useTemplateRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import { useControl, type ControlProps } from "./control";
import { read, stepped, written, type Refusal } from "./number";

/**
 * A number, with its unit beside it (ADR-0042).
 *
 * What is typed is taken on Enter or on leaving the field, not at each key.
 * A value that isn't a number, or is outside the range, isn't taken: the
 * field says why, and keeps the value it had. The arrow keys and the wheel
 * move it by a step, Page Up and Down by ten, Home and End to the range's
 * ends, each taken at once.
 */
const props = withDefaults(
  defineProps<
    ControlProps & {
      min?: number;
      max?: number;
      /** What an arrow key or a notch of the wheel moves it by. */
      step?: number;
      /** Shown after the number, and said with it. */
      unit?: string;
      /** How many decimals a value is written with. As it is, when not given. */
      digits?: number;
      /** Whether no value at all is a value. */
      optional?: boolean;
      /** The smaller size, where several share a row. */
      compact?: boolean;
      placeholder?: string;
    }
  >(),
  { step: 1 },
);
const value = defineModel<number | null>({ default: null });

const { t } = useI18n();
const control = useControl(props);
const input = useTemplateRef("input");
const reasonId = useId();
const text = ref(written(value.value, props.digits));
const refusal = ref<Refusal | null>(null);

const reason = computed(() => {
  if (refusal.value === null) return undefined;
  return t(`ui.number.${refusal.value}`, { least: props.min, most: props.max });
});
const describedBy = computed(() => {
  const ids = [reason.value ? reasonId : undefined, control.describedBy.value];
  return ids.filter(Boolean).join(" ") || undefined;
});
const fixed = computed(() => control.disabled.value || props.readonly);
const range = computed(() => ({ min: props.min, max: props.max }));

// A value from outside is shown at once, except under the hand that is typing.
watch(value, (now) => {
  if (document.activeElement === input.value) return;
  text.value = written(now, props.digits);
  refusal.value = null;
});

function take(now: number | null): void {
  text.value = written(now, props.digits);
  refusal.value = null;
  if (now !== value.value) value.value = now;
}

/** Takes what was typed, or says why not. */
function commit(): void {
  if (fixed.value) return;
  const result = read(text.value, range.value, props.optional);
  if (result.ok) take(result.value);
  else refusal.value = result.why;
}

/** Moves by steps from what is typed, if that is a number, or else from the value. */
function move(steps: number): void {
  if (fixed.value) return;
  const typed = read(text.value, {}, true);
  const from = typed.ok && typed.value !== null ? typed.value : (value.value ?? props.min ?? 0);
  take(stepped(from, steps, props.step, range.value));
}

function key(event: KeyboardEvent): void {
  if (event.key === "ArrowUp") move(1);
  else if (event.key === "ArrowDown") move(-1);
  else if (event.key === "PageUp") move(10);
  else if (event.key === "PageDown") move(-10);
  else if (event.key === "Home" && props.min !== undefined && !fixed.value) take(props.min);
  else if (event.key === "End" && props.max !== undefined && !fixed.value) take(props.max);
  else if (event.key === "Enter") commit();
  else if (event.key === "Escape") take(value.value);
  else return;
  event.preventDefault();
}

/** The wheel moves the field with the focus, and no other: a page scrolled past it changes nothing. */
function wheel(event: WheelEvent): void {
  if (document.activeElement !== input.value || fixed.value) return;
  event.preventDefault();
  move(event.deltaY < 0 ? 1 : -1);
}
</script>

<template>
  <div class="number">
    <span class="ui-field box" :class="{ compact }">
      <input
        :id="control.id.value"
        ref="input"
        v-model="text"
        role="spinbutton"
        type="text"
        inputmode="decimal"
        autocomplete="off"
        spellcheck="false"
        :placeholder="placeholder"
        :disabled="control.disabled.value"
        :readonly="readonly"
        :aria-label="label"
        :aria-valuenow="value ?? undefined"
        :aria-valuemin="min"
        :aria-valuemax="max"
        :aria-valuetext="unit && value !== null ? `${written(value, digits)} ${unit}` : undefined"
        :aria-invalid="refusal !== null || control.invalid.value || undefined"
        :aria-describedby="describedBy"
        @keydown="key"
        @wheel="wheel"
        @blur="commit"
      />
      <span v-if="unit" class="unit" aria-hidden="true">{{ unit }}</span>
    </span>
    <p v-if="reason" :id="reasonId" class="reason" role="alert">{{ reason }}</p>
  </div>
</template>

<style scoped>
/* One column, no wider than the field's place: a grid's own would grow to the input's natural width. */
.number {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  gap: var(--space-1);
  inline-size: 100%;
}

.box {
  gap: var(--space-2);
}

/* The ring goes round the box, unit and all. */
.box:focus-within {
  outline: var(--focus-width) solid var(--focus-color);
  outline-offset: var(--focus-offset);
}

input {
  flex: 1;
  min-inline-size: 0;
  padding: 0;
  border: 0;
  outline: none;
  background: transparent;
}

.unit {
  color: var(--text-muted);
  white-space: nowrap;
}

.reason {
  color: var(--danger);
  font-size: var(--font-size-small);
}
</style>
