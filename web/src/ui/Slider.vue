<script setup lang="ts">
import { SliderRange, SliderRoot, SliderThumb, SliderTrack } from "reka-ui";

import { useControl, type ControlProps } from "./control";
import NumberField from "./NumberField.vue";

/**
 * A value on a scale, with its number beside it to read and to type
 * (ADR-0042). The arrow keys move it by a step, Page Up and Down by more,
 * Home and End to the ends.
 *
 * The value follows the thumb as it is dragged; `commit` says where it was
 * let go, for whoever should act only then.
 */
const props = withDefaults(
  defineProps<
    ControlProps & {
      min?: number;
      max?: number;
      step?: number;
      unit?: string;
      digits?: number;
      /** Without the number beside it. */
      bare?: boolean;
    }
  >(),
  { min: 0, max: 100, step: 1 },
);
const value = defineModel<number>({ default: 0 });
const emit = defineEmits<{ commit: [value: number] }>();
const control = useControl(props);

function drag(now: number[] | undefined): void {
  if (!props.readonly && now?.[0] !== undefined) value.value = now[0];
}

function typed(now: number | null): void {
  if (now === null) return;
  value.value = now;
  emit("commit", now);
}
</script>

<template>
  <div class="slider">
    <SliderRoot
      class="scale"
      :model-value="[value]"
      :min="min"
      :max="max"
      :step="step"
      :disabled="disabled"
      @update:model-value="drag"
      @value-commit="(now: number[]) => now[0] !== undefined && emit('commit', now[0])"
    >
      <SliderTrack class="track">
        <SliderRange class="filled" />
      </SliderTrack>
      <SliderThumb
        class="thumb"
        :aria-label="label"
        :aria-labelledby="label ? undefined : control.labelledBy"
        :aria-describedby="control.describedBy.value"
        :aria-readonly="readonly || undefined"
        :aria-valuetext="unit ? `${value} ${unit}` : undefined"
      />
    </SliderRoot>
    <div v-if="!bare" class="number">
      <NumberField
        :model-value="value"
        :label="label"
        :min="min"
        :max="max"
        :step="step"
        :unit="unit"
        :digits="digits"
        :disabled="disabled"
        :readonly="readonly"
        compact
        @update:model-value="typed"
      />
    </div>
  </div>
</template>

<style scoped>
.slider {
  display: flex;
  gap: var(--space-3);
  align-items: start;
  inline-size: 100%;
}

.scale {
  position: relative;
  display: flex;
  flex: 1;
  align-items: center;
  block-size: var(--size-control-compact);
  touch-action: none;
  user-select: none;
}

.track {
  position: relative;
  flex: 1;
  block-size: 0.25rem;
  border-radius: var(--radius-round);
  background: var(--border-control);
}

.filled {
  position: absolute;
  block-size: 100%;
  border-radius: var(--radius-round);
  background: var(--accent-text);
}

.thumb {
  display: block;
  inline-size: 1rem;
  block-size: 1rem;
  border: 2px solid var(--accent-text);
  border-radius: var(--radius-round);
  background: var(--surface-raised);
  cursor: grab;
}

.scale[data-disabled] .filled {
  background: var(--text-disabled);
}

.scale[data-disabled] .thumb {
  border-color: var(--text-disabled);
  cursor: not-allowed;
}

.number {
  flex: none;
  inline-size: 7rem;
}
</style>
