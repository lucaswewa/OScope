<script setup lang="ts">
import "./styles/toggle.css";

import { ToggleGroupItem, ToggleGroupRoot } from "reka-ui";

import Button from "./Button.vue";
import { useControl, type ControlProps } from "./control";
import type { IconName } from "./icons";

/**
 * Buttons that stay pressed, side by side (ADR-0042): one of them, or with
 * `multiple` any of them. The arrow keys move between them; Tab passes the
 * group as one stop.
 */
const props = defineProps<
  ControlProps & {
    options: readonly { value: string; label: string; icon?: IconName; disabled?: boolean }[];
    /** Any number may be pressed, and the value is a list. */
    multiple?: boolean;
    /** One must stay pressed: pressing it again leaves it so. */
    required?: boolean;
    compact?: boolean;
  }
>();
const pressed = defineModel<string | string[]>();
const control = useControl(props);

function change(now: unknown): void {
  if (props.readonly) return;
  const none = now === undefined || now === "" || (Array.isArray(now) && now.length === 0);
  if (none && props.required) return;
  pressed.value = props.multiple ? ((now ?? []) as string[]) : ((now ?? "") as string);
}
</script>

<template>
  <ToggleGroupRoot
    class="group"
    :type="multiple ? 'multiple' : 'single'"
    :model-value="pressed"
    :disabled="control.disabled.value"
    :aria-label="label"
    :aria-labelledby="label ? undefined : control.labelledBy"
    :aria-describedby="control.describedBy.value"
    @update:model-value="change"
  >
    <ToggleGroupItem
      v-for="option in options"
      :key="option.value"
      :value="option.value"
      :disabled="option.disabled"
      as-child
    >
      <Button class="ui-toggle item" :icon="option.icon" :compact="compact">
        {{ option.label }}
      </Button>
    </ToggleGroupItem>
  </ToggleGroupRoot>
</template>

<style scoped>
.group {
  display: inline-flex;
}

/* One edge between neighbours, and corners only at the group's ends. */
.item {
  border-radius: 0;
}

.item + .item {
  margin-inline-start: -1px;
}

.item:first-child {
  border-start-start-radius: var(--radius-control);
  border-end-start-radius: var(--radius-control);
}

.item:last-child {
  border-start-end-radius: var(--radius-control);
  border-end-end-radius: var(--radius-control);
}

/* The one with the focus, or pressed, is drawn over its neighbours' edges. */
.item:focus-visible,
.item[data-state="on"] {
  z-index: 1;
}
</style>
