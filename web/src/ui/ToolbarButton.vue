<script setup lang="ts">
import "./styles/toggle.css";

import { ToolbarButton } from "reka-ui";

import Button from "./Button.vue";
import type { IconName } from "./icons";

/**
 * A button of a `Toolbar` (ADR-0042). With `pressed`, it stays pressed or
 * not, and says which.
 */
defineOptions({ inheritAttrs: false });
// `pressed` is undefined unless given: Vue would make a missing one false,
// and every button of a toolbar would then say it can be pressed and let go.
withDefaults(
  defineProps<{
    icon?: IconName;
    /** Whether it is pressed, for a button that stays so. */
    pressed?: boolean;
    disabled?: boolean;
  }>(),
  { pressed: undefined },
);
</script>

<template>
  <ToolbarButton as-child :disabled="disabled">
    <Button
      v-bind="$attrs"
      variant="bare"
      compact
      :icon="icon"
      :class="{ 'ui-toggle': pressed !== undefined }"
      :aria-pressed="pressed"
      :data-state="pressed === undefined ? undefined : pressed ? 'on' : 'off'"
    >
      <slot />
    </Button>
  </ToolbarButton>
</template>
