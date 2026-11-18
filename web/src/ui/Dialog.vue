<script setup lang="ts">
import "./styles/layer.css";

import {
  DialogContent,
  DialogDescription,
  DialogOverlay,
  DialogPortal,
  DialogRoot,
  DialogTitle,
} from "reka-ui";
import { useI18n } from "vue-i18n";

import IconButton from "./IconButton.vue";

/**
 * A dialog (ADR-0042): a task of its own, over the dimmed page. The focus
 * goes into it and stays there; Escape, its close button and a press beside
 * it close it; and the focus then goes back to where it was. The page behind
 * doesn't scroll.
 *
 * Its buttons go in the `actions` slot, the one that goes on last.
 */
defineProps<{
  title: string;
  /** A line under the title, which a screen reader reads with it. */
  description?: string;
  /** The narrower width, for a question. */
  narrow?: boolean;
}>();
const open = defineModel<boolean>("open", { default: false });
const { t } = useI18n();
</script>

<template>
  <DialogRoot v-model:open="open">
    <DialogPortal>
      <DialogOverlay class="ui-scrim" />
      <!-- Without a description, the library is told so: it warns otherwise. -->
      <DialogContent
        class="ui-dialog"
        :class="{ narrow }"
        v-bind="description ? {} : { 'aria-describedby': undefined }"
      >
        <DialogTitle class="ui-dialog-title">{{ title }}</DialogTitle>
        <DialogDescription v-if="description" class="ui-dialog-text">
          {{ description }}
        </DialogDescription>
        <slot />
        <footer v-if="$slots.actions" class="ui-dialog-actions">
          <slot name="actions" />
        </footer>
        <!-- Last in the page, though drawn at the top: the focus starts in
             what the dialog is for, and Tab comes to this at the end. -->
        <div class="ui-dialog-close">
          <IconButton icon="close" :label="t('ui.close')" compact @click="open = false" />
        </div>
      </DialogContent>
    </DialogPortal>
  </DialogRoot>
</template>
