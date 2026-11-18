<script setup lang="ts">
import "./styles/field.css";
import "./styles/list.css";

import {
  ComboboxAnchor,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxItemIndicator,
  ComboboxPortal,
  ComboboxRoot,
  ComboboxTrigger,
  ComboboxViewport,
} from "reka-ui";
import { ref } from "vue";
import { useI18n } from "vue-i18n";

import { useControl, type ControlProps } from "./control";
import Icon from "./Icon.vue";

/**
 * One choice from a long list, found by typing (ADR-0042). What is typed
 * narrows the list; the arrows move through it; Enter chooses; Escape closes
 * it. The value is always one of the choices, never what was typed.
 */
const props = defineProps<
  ControlProps & {
    options: readonly { value: string; label: string; disabled?: boolean }[];
    placeholder?: string;
    /** The smaller size, where several share a row. */
    compact?: boolean;
  }
>();
const chosen = defineModel<string>();
const { t } = useI18n();
const control = useControl(props);
const open = ref(false);

const labelOf = (value: string | undefined): string =>
  props.options.find((option) => option.value === value)?.label ?? "";

/** A read-only combobox keeps its focus and its value, and doesn't open. */
function show(now: boolean): void {
  open.value = now && !props.readonly;
}
</script>

<template>
  <!-- A box of our own around the library's: its root takes no style from here. -->
  <div class="root">
    <ComboboxRoot
      v-model="chosen"
      :open="open"
      :disabled="disabled"
      open-on-click
      @update:open="show"
    >
      <ComboboxAnchor class="ui-field combobox" :class="{ compact }">
        <ComboboxInput
          :id="control.id.value"
          class="input"
          :display-value="labelOf"
          :placeholder="placeholder"
          :readonly="readonly"
          :aria-label="label"
          :aria-invalid="control.invalid.value || undefined"
          :aria-describedby="control.describedBy.value"
        />
        <ComboboxTrigger class="opener" tabindex="-1" :aria-label="t('ui.combobox.open')">
          <Icon name="expand" small />
        </ComboboxTrigger>
      </ComboboxAnchor>
      <ComboboxPortal>
        <ComboboxContent class="ui-list" position="popper" :side-offset="4">
          <ComboboxViewport>
            <ComboboxEmpty class="ui-list-empty">{{ t("ui.combobox.nothing") }}</ComboboxEmpty>
            <ComboboxItem
              v-for="option in options"
              :key="option.value"
              class="ui-option"
              :value="option.value"
              :text-value="option.label"
              :disabled="option.disabled"
            >
              <span>{{ option.label }}</span>
              <ComboboxItemIndicator><Icon name="check" small /></ComboboxItemIndicator>
            </ComboboxItem>
          </ComboboxViewport>
        </ComboboxContent>
      </ComboboxPortal>
    </ComboboxRoot>
  </div>
</template>

<style scoped>
.root {
  inline-size: 100%;
}

.combobox {
  gap: var(--space-2);
}

/* The ring goes round the box, as a number field's does. */
.combobox:focus-within {
  outline: var(--focus-width) solid var(--focus-color);
  outline-offset: var(--focus-offset);
}

.input {
  flex: 1;
  min-inline-size: 0;
  padding: 0;
  border: 0;
  outline: none;
  background: transparent;
}

.opener {
  display: grid;
  flex: none;
  place-content: center;
  padding: 0;
  border: 0;
  background: transparent;
  cursor: pointer;
}
</style>
