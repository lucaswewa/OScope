<script setup lang="ts">
import { SwitchRoot, SwitchThumb } from "reka-ui";

import { useControl, type ControlProps } from "./control";

/** A switch, with its label after it (ADR-0042): on or off, at once. */
const props = defineProps<ControlProps>();
const on = defineModel<boolean>({ default: false });
const control = useControl(props);

function change(now: boolean): void {
  if (!props.readonly) on.value = now;
}
</script>

<template>
  <label
    class="switch"
    :class="{ off: control.disabled.value, fixed: readonly }"
    :for="control.id.value"
  >
    <SwitchRoot
      :id="control.id.value"
      class="track"
      :model-value="on"
      :disabled="control.disabled.value"
      :aria-label="label"
      :aria-readonly="readonly || undefined"
      :aria-describedby="control.describedBy.value"
      @update:model-value="change"
    >
      <SwitchThumb class="knob" />
    </SwitchRoot>
    <span><slot /></span>
  </label>
</template>

<style scoped>
.switch {
  display: inline-flex;
  gap: var(--space-2);
  align-items: center;
  min-block-size: var(--size-icon);
  cursor: pointer;
}

.track {
  flex: none;
  inline-size: 2.25rem;
  block-size: 1.25rem;
  padding: 0;
  border: 1px solid var(--border-control);
  border-radius: var(--radius-round);
  background: var(--surface-field);
  cursor: inherit;
  transition: background-color var(--motion-fast) var(--motion-ease);
}

.knob {
  display: block;
  inline-size: 0.75rem;
  block-size: 0.75rem;
  border-radius: var(--radius-round);
  background: var(--border-control);
  transform: translateX(0.1875rem);
  transition: transform var(--motion-fast) var(--motion-ease);
}

.track[data-state="checked"] {
  border-color: var(--accent);
  background: var(--accent);
}

.track[data-state="checked"] .knob {
  background: var(--accent-on);
  transform: translateX(1.1875rem);
}

.off {
  color: var(--text-disabled);
  cursor: not-allowed;
}

/* Read-only: it shows what is so, and doesn't invite a press. */
.fixed {
  cursor: default;
}

.track[data-disabled] {
  border-color: var(--border);
  background: var(--surface-open);
}

.track[data-disabled] .knob {
  background: var(--text-disabled);
}
</style>
