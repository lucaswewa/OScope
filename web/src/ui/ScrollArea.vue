<script setup lang="ts">
/**
 * A part of the page that scrolls on its own (ADR-0042): a region with a
 * name, which the keyboard reaches, and scrolls with the arrow keys. Its
 * height is the page's to give.
 */
defineProps<{ label: string }>();
</script>

<template>
  <div class="scroll" role="region" :aria-label="label" tabindex="0">
    <slot />
  </div>
</template>

<style scoped>
.scroll {
  overflow: auto;
  overscroll-behavior: contain;
}
</style>
