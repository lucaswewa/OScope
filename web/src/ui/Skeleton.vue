<script setup lang="ts">
/**
 * The shape of something that hasn't arrived yet (ADR-0042): a grey block
 * where a line of text, a picture or a row will be, so that the page doesn't
 * jump when it comes. It says nothing to a screen reader: what is loading
 * says so itself, with `aria-busy`.
 */
withDefaults(
  defineProps<{
    /** Its width and height, as CSS lengths: a line of text's, when not given. */
    width?: string;
    height?: string;
  }>(),
  { width: "100%", height: "1em" },
);
</script>

<template>
  <span class="skeleton" aria-hidden="true" :style="{ inlineSize: width, blockSize: height }" />
</template>

<style scoped>
.skeleton {
  display: block;
  border-radius: var(--radius-control);
  background: var(--surface-hover);
  animation: wait 1.6s ease-in-out infinite;
}

@keyframes wait {
  50% {
    opacity: 0.5;
  }
}
</style>
