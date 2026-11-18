<template>
  <!-- A turning ring, for something under way. For a reader who asks for less
       motion it stands still, and still reads as a ring with a gap. -->
  <span class="spinner" aria-hidden="true" />
</template>

<style scoped>
.spinner {
  flex: none;
  inline-size: var(--size-icon-small);
  block-size: var(--size-icon-small);
  border: 2px solid currentColor;
  border-inline-end-color: transparent;
  border-radius: var(--radius-round);
  animation: turn 0.8s linear infinite;
}

@keyframes turn {
  to {
    transform: rotate(1turn);
  }
}
</style>
