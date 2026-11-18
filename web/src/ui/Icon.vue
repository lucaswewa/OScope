<script setup lang="ts">
import { BOX, icons, type IconName } from "./icons";

/**
 * An icon from the registry (ADR-0041), in the text's colour. Beside text it
 * says nothing to a screen reader; standing alone it is given a `label`, and
 * is then an image with that name.
 */
defineProps<{
  name: IconName;
  /** What the icon means, where no text beside it says so. */
  label?: string;
  /** The smaller size, as inside a button. */
  small?: boolean;
}>();
</script>

<template>
  <svg
    class="icon"
    :class="{ small }"
    :viewBox="BOX"
    :role="label ? 'img' : undefined"
    :aria-label="label"
    :aria-hidden="label ? undefined : 'true'"
    focusable="false"
  >
    <path :d="icons[name]" />
  </svg>
</template>

<style scoped>
.icon {
  flex: none;
  inline-size: var(--size-icon);
  block-size: var(--size-icon);
  fill: currentColor;
}

.small {
  inline-size: var(--size-icon-small);
  block-size: var(--size-icon-small);
}
</style>
