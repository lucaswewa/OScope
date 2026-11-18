<script setup lang="ts">
import Button from "./Button.vue";
import Icon from "./Icon.vue";
import type { IconName } from "./icons";
import Tooltip from "./Tooltip.vue";

/**
 * A button that is an icon alone (ADR-0042). It must say what it does:
 * `label` is its name to a screen reader, and its tooltip to everyone else.
 */
withDefaults(
  defineProps<{
    icon: IconName;
    label: string;
    variant?: "primary" | "outline" | "bare";
    tone?: "accent" | "danger";
    compact?: boolean;
    busy?: boolean;
    disabled?: boolean;
  }>(),
  { variant: "bare", tone: "accent" },
);
</script>

<template>
  <Tooltip :text="label">
    <Button
      class="icon-button"
      :variant="variant"
      :tone="tone"
      :compact="compact"
      :busy="busy"
      :disabled="disabled"
      :aria-label="label"
    >
      <Icon v-if="!busy" :name="icon" :small="compact" />
    </Button>
  </Tooltip>
</template>

<style scoped>
.icon-button {
  flex: none;
  inline-size: var(--size-control);
  padding-inline: 0;
}

.icon-button.compact {
  inline-size: var(--size-control-compact);
}
</style>
