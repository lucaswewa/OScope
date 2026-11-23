<script setup lang="ts">
import { CheckboxIndicator, CheckboxRoot } from "reka-ui";

import { useControl, type ControlProps } from "./control";
import Icon from "./Icon.vue";

/** A checkbox, with its label after it (ADR-0042). Space ticks and unticks it. */
const props = defineProps<
  ControlProps & {
    /** Neither ticked nor not: some of what it stands for is. */
    indeterminate?: boolean;
  }
>();
const ticked = defineModel<boolean>({ default: false });
const control = useControl(props);

function change(now: boolean | "indeterminate"): void {
  if (!props.readonly) ticked.value = now === true;
}
</script>

<template>
  <label
    class="checkbox"
    :class="{ off: control.disabled.value, fixed: readonly }"
    :for="control.id.value"
  >
    <CheckboxRoot
      :id="control.id.value"
      class="box"
      :model-value="indeterminate ? 'indeterminate' : ticked"
      :disabled="control.disabled.value"
      :aria-label="label"
      :aria-readonly="readonly || undefined"
      :aria-invalid="control.invalid.value || undefined"
      :aria-describedby="control.describedBy.value"
      @update:model-value="change"
    >
      <CheckboxIndicator class="mark">
        <Icon :name="indeterminate ? 'remove' : 'check'" />
      </CheckboxIndicator>
    </CheckboxRoot>
    <span><slot /></span>
  </label>
</template>

<style scoped>
.checkbox {
  display: inline-flex;
  gap: var(--space-2);
  align-items: center;
  min-block-size: var(--size-icon);
  cursor: pointer;
}

.box {
  display: grid;
  flex: none;
  place-content: center;
  inline-size: 1rem;
  block-size: 1rem;
  padding: 0;
  border: 1px solid var(--border-control);
  border-radius: var(--radius-control);
  background: var(--surface-field);
  color: var(--accent-on);
  cursor: inherit;
}

.box[data-state="checked"],
.box[data-state="indeterminate"] {
  border-color: var(--accent);
  background: var(--accent);
}

.mark,
.mark :deep(svg) {
  inline-size: 0.875rem;
  block-size: 0.875rem;
}

.off {
  color: var(--text-disabled);
  cursor: not-allowed;
}

/* Read-only: it shows what is so, and doesn't invite a press. */
.fixed {
  cursor: default;
}

.box[data-disabled] {
  border-color: var(--border);
  background: var(--surface-open);
  color: var(--text-disabled);
}
</style>
