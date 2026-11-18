<script setup lang="ts">
import "./styles/field.css";
import "./styles/list.css";

import {
  SelectContent,
  SelectItem,
  SelectItemIndicator,
  SelectItemText,
  SelectPortal,
  SelectRoot,
  SelectTrigger,
  SelectValue,
  SelectViewport,
} from "reka-ui";
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";

import { useControl, type ControlProps } from "./control";
import Icon from "./Icon.vue";

/**
 * One choice from a list that opens (ADR-0042). Enter, Space or an arrow
 * opens it; the arrows, Home, End and a typed letter move through it; Enter
 * chooses; Escape closes it with the choice as it was.
 */
const props = defineProps<
  ControlProps & {
    options: readonly { value: string; label: string; disabled?: boolean }[];
    /** What the field says before anything is chosen. */
    placeholder?: string;
    /** The smaller size, where several share a row. */
    compact?: boolean;
  }
>();
const chosen = defineModel<string>();
const { t } = useI18n();
const control = useControl(props);
const open = ref(false);

/**
 * The chosen option's label, from the options given. The library would read
 * it from the list's own items, which go as the list closes: the field then
 * showed its placeholder for a moment after every choice.
 */
const chosenLabel = computed(
  () => props.options.find((option) => option.value === chosen.value)?.label,
);

/** A read-only list keeps its focus and its value, and doesn't open. */
function show(now: boolean): void {
  open.value = now && !props.readonly;
}
</script>

<template>
  <SelectRoot v-model="chosen" :open="open" :disabled="disabled" @update:open="show">
    <SelectTrigger
      :id="control.id.value"
      class="ui-field select"
      :class="{ compact }"
      :aria-label="label"
      :aria-readonly="readonly || undefined"
      :aria-invalid="control.invalid.value || undefined"
      :aria-describedby="control.describedBy.value"
    >
      <SelectValue as-child>
        <span class="value" :class="{ empty: chosenLabel === undefined }">
          {{ chosenLabel ?? placeholder ?? t("ui.select.choose") }}
        </span>
      </SelectValue>
      <Icon name="expand" small />
    </SelectTrigger>
    <SelectPortal>
      <SelectContent class="ui-list" position="popper" :side-offset="4">
        <SelectViewport>
          <SelectItem
            v-for="option in options"
            :key="option.value"
            class="ui-option"
            :value="option.value"
            :disabled="option.disabled"
          >
            <SelectItemText>{{ option.label }}</SelectItemText>
            <SelectItemIndicator><Icon name="check" small /></SelectItemIndicator>
          </SelectItem>
        </SelectViewport>
      </SelectContent>
    </SelectPortal>
  </SelectRoot>
</template>

<style scoped>
.select {
  gap: var(--space-2);
  justify-content: space-between;
  cursor: pointer;
}

.value {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* Nothing chosen yet: what stands there is said, not chosen. */
.empty {
  color: var(--text-muted);
}
</style>
