<script setup lang="ts">
import "./styles/toggle.css";

import { Toggle } from "reka-ui";

import Button from "./Button.vue";
import { useDisabled } from "./control";
import type { IconName } from "./icons";

/** A button that stays pressed until it is pressed again (ADR-0042). */
const props = defineProps<{
  icon?: IconName;
  compact?: boolean;
  disabled?: boolean;
  /** Shown, and reachable, but not to be changed. */
  readonly?: boolean;
}>();
const pressed = defineModel<boolean>({ default: false });

function change(now: boolean): void {
  if (!props.readonly) pressed.value = now;
}
// Switched off by its own prop, or by a locked part of the page (ADR-0048).
const off = useDisabled(props);
</script>

<template>
  <Toggle :model-value="pressed" :disabled="off" as-child @update:model-value="change">
    <Button class="ui-toggle" :icon="icon" :compact="compact"><slot /></Button>
  </Toggle>
</template>
