<script setup lang="ts">
import { SplitterGroup, SplitterPanel, SplitterResizeHandle } from "reka-ui";

/**
 * Two panes, and a handle between them (ADR-0042). The handle is dragged,
 * or moved from the keyboard: the arrow keys by a step, Home and End to the
 * ends. `sizes` is each pane's share in percent, for a page to keep.
 *
 * Its height is the page's to give: the panes fill it.
 */
withDefaults(
  defineProps<{
    /** What the handle is, to a screen reader. */
    label: string;
    /** Side by side, or one above the other. */
    direction?: "horizontal" | "vertical";
    /** The least each pane may have, in percent. */
    min?: readonly [number, number];
  }>(),
  { direction: "horizontal", min: () => [20, 20] },
);
const sizes = defineModel<[number, number]>("sizes", { default: () => [50, 50] });

function laid(now: number[]): void {
  const [first, second] = now;
  if (first === undefined || second === undefined) return;
  if (first !== sizes.value[0] || second !== sizes.value[1]) sizes.value = [first, second];
}
</script>

<template>
  <!-- A box of our own, which takes the page's size: the library's group
       sets its own height, as 100% of what holds it. -->
  <div class="splitter">
    <SplitterGroup
      class="group"
      :class="direction"
      :direction="direction"
      :keyboard-resize-by="5"
      @layout="laid"
    >
      <SplitterPanel class="pane" :default-size="sizes[0]" :min-size="min[0]">
        <slot name="first" />
      </SplitterPanel>
      <SplitterResizeHandle class="handle" :aria-label="label" />
      <SplitterPanel class="pane" :default-size="sizes[1]" :min-size="min[1]">
        <slot name="second" />
      </SplitterPanel>
    </SplitterGroup>
  </div>
</template>

<style scoped>
.splitter {
  min-block-size: 0;
}

.group {
  block-size: 100%;
}

.pane {
  min-inline-size: 0;
  min-block-size: 0;
}

/* A hairline, with room around it for the pointer to find it. */
.handle {
  position: relative;
  flex: none;
  background: var(--border);
}

.horizontal > .handle {
  inline-size: 1px;
  margin-inline: var(--space-1);
}

.vertical > .handle {
  block-size: 1px;
  margin-block: var(--space-1);
}

.handle::after {
  position: absolute;
  inset: calc(-1 * var(--space-1));
  content: "";
}

.handle:hover,
.handle[data-state="drag"] {
  background: var(--accent-text);
}
</style>
