<script setup lang="ts">
import { RadioGroupIndicator, RadioGroupItem, RadioGroupRoot } from "reka-ui";

import { useControl, type ControlProps } from "./control";

/**
 * One choice of a few, all in view (ADR-0042). The arrow keys move the
 * choice; Tab passes the group as one stop.
 */
const props = defineProps<
  ControlProps & {
    options: readonly { value: string; label: string; disabled?: boolean }[];
    /** Side by side, where there is room. */
    row?: boolean;
  }
>();
const chosen = defineModel<string>();
const control = useControl(props);

function change(now: unknown): void {
  if (!props.readonly && typeof now === "string") chosen.value = now;
}

/**
 * An arrow key moves the choice to the next button that can be chosen, round
 * the ends. The library moves the focus, and chooses only while the key is
 * still down a moment later: a quick tap would leave the focus on a button
 * that isn't chosen. So the choice is moved here, at the key itself.
 */
function arrow(event: KeyboardEvent): void {
  const steps: Record<string, number> = { ArrowDown: 1, ArrowRight: 1, ArrowUp: -1, ArrowLeft: -1 };
  const step = steps[event.key];
  if (step === undefined || props.readonly || control.disabled.value) return;
  const open = props.options.filter((option) => !option.disabled);
  const from = (event.target as HTMLElement).getAttribute("value");
  const at = open.findIndex((option) => option.value === from);
  const next = open[(at + step + open.length) % open.length];
  if (at >= 0 && next) chosen.value = next.value;
}
</script>

<template>
  <RadioGroupRoot
    class="radios"
    :class="{ row }"
    :model-value="chosen"
    :orientation="row ? 'horizontal' : 'vertical'"
    :disabled="control.disabled.value"
    :aria-label="label"
    :aria-labelledby="label ? undefined : control.labelledBy"
    :aria-readonly="readonly || undefined"
    :aria-invalid="control.invalid.value || undefined"
    :aria-describedby="control.describedBy.value"
    @update:model-value="change"
    @keydown="arrow"
  >
    <label
      v-for="(option, at) in options"
      :key="option.value"
      class="radio"
      :class="{ off: control.disabled.value || option.disabled, fixed: readonly }"
      :for="`${control.id.value}-${at}`"
    >
      <RadioGroupItem
        :id="`${control.id.value}-${at}`"
        class="ring"
        :value="option.value"
        :disabled="option.disabled"
      >
        <RadioGroupIndicator class="dot" />
      </RadioGroupItem>
      <span>{{ option.label }}</span>
    </label>
  </RadioGroupRoot>
</template>

<style scoped>
.radios {
  display: grid;
  gap: var(--space-1);
  justify-items: start;
}

.row {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-4);
}

.radio {
  display: inline-flex;
  gap: var(--space-2);
  align-items: center;
  min-block-size: var(--size-icon);
  cursor: pointer;
}

.ring {
  display: grid;
  flex: none;
  place-content: center;
  inline-size: 1rem;
  block-size: 1rem;
  padding: 0;
  border: 1px solid var(--border-control);
  border-radius: var(--radius-round);
  background: var(--surface-field);
  cursor: inherit;
}

.ring[data-state="checked"] {
  border-color: var(--accent-text);
}

.dot {
  inline-size: 0.5rem;
  block-size: 0.5rem;
  border-radius: var(--radius-round);
  background: var(--accent-text);
}

.off {
  color: var(--text-disabled);
  cursor: not-allowed;
}

/* Read-only: it shows what is so, and doesn't invite a press. */
.fixed {
  cursor: default;
}

.ring[data-disabled] {
  border-color: var(--border);
  background: var(--surface-open);
}

.ring[data-disabled] .dot {
  background: var(--text-disabled);
}
</style>
