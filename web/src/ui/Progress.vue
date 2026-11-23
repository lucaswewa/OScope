<script setup lang="ts">
import { computed } from "vue";

import { useControl } from "./control";

/**
 * How far something has got (ADR-0042): a bar, or with `ring` a ring. With
 * a `value`, it shows that share of `max`. Without one, the end isn't known,
 * and it only shows that something is under way.
 */
const props = withDefaults(
  defineProps<{
    /** How far, from 0 to `max`. Not given, or null: no end is known. */
    value?: number | null;
    max?: number;
    /** What is under way, where no `Field` says. */
    label?: string;
    /** A ring, where there is no room for a bar. */
    ring?: boolean;
  }>(),
  { value: null, max: 100 },
);
const control = useControl(props);

/** The share done, from 0 to 1, or nothing where no end is known. */
const share = computed(() => {
  if (props.value === null || !(props.max > 0)) return null;
  return Math.min(Math.max(props.value / props.max, 0), 1);
});
/** A ring of radius 9 in a box of 24: its length, and how much of it is drawn. */
const round = 2 * Math.PI * 9;
const drawn = computed(() => round * (share.value ?? 0.25));
</script>

<template>
  <span
    class="progress"
    :class="{ ring, unknown: share === null }"
    role="progressbar"
    aria-valuemin="0"
    :aria-valuemax="max"
    :aria-valuenow="share === null ? undefined : (value ?? undefined)"
    :aria-label="label"
    :aria-labelledby="label ? undefined : control.labelledBy"
  >
    <svg v-if="ring" class="turn" viewBox="0 0 24 24" aria-hidden="true">
      <circle class="track" cx="12" cy="12" r="9" />
      <circle class="arc" cx="12" cy="12" r="9" :stroke-dasharray="`${drawn} ${round}`" />
    </svg>
    <span v-else class="fill" :style="{ inlineSize: `${100 * (share ?? 0.4)}%` }" />
  </span>
</template>

<style scoped>
.progress {
  display: block;
  inline-size: 100%;
  block-size: var(--space-1);
  overflow: hidden;
  border-radius: var(--radius-round);
  background: var(--border);
}

.fill {
  display: block;
  block-size: 100%;
  border-radius: var(--radius-round);
  background: var(--accent-text);
  transition: inline-size var(--motion-slow) var(--motion-ease);
}

/* No end known: the filled part runs along the bar, over and over. */
.unknown .fill {
  animation: run 1.6s var(--motion-ease) infinite;
}

@keyframes run {
  from {
    transform: translateX(-100%);
  }

  to {
    transform: translateX(250%);
  }
}

.ring {
  display: inline-block;
  inline-size: var(--size-icon);
  block-size: var(--size-icon);
  overflow: visible;
  background: none;
}

.turn {
  display: block;
  inline-size: 100%;
  block-size: 100%;
  transform: rotate(-90deg);
}

.track,
.arc {
  fill: none;
  stroke-width: 3;
}

.track {
  stroke: var(--border);
}

.arc {
  stroke: var(--accent-text);
  stroke-linecap: round;
  transition: stroke-dasharray var(--motion-slow) var(--motion-ease);
}

.unknown .turn {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  from {
    transform: rotate(-90deg);
  }

  to {
    transform: rotate(270deg);
  }
}
</style>
