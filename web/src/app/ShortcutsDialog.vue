<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { shortcuts } from "@/app/shortcuts";
import Dialog from "@/ui/Dialog.vue";
import KeyValue from "@/ui/KeyValue.vue";

/**
 * The keyboard's shortcuts, listed (W2.1): opened with `?`. The list takes
 * the focus first, and scrolls from the keyboard: the close button would
 * show its tooltip, which the first Escape would send away in place of the
 * dialog.
 */
const open = defineModel<boolean>("open", { default: false });
const { t } = useI18n();

const entries = computed(() =>
  shortcuts.value.map((one) => ({ term: one.key, value: one.label() })),
);
</script>

<template>
  <Dialog
    v-model:open="open"
    :title="t('shell.keys.title')"
    :description="t('shell.keys.intro')"
    narrow
  >
    <div class="list" role="region" tabindex="0" :aria-label="t('shell.keys.title')">
      <KeyValue data-said="shortcuts" :entries="entries" />
    </div>
  </Dialog>
</template>

<style scoped>
.list {
  max-block-size: 60vh;
  overflow: auto;
}

.list:focus-visible {
  outline: var(--focus-width) solid var(--focus-color);
  outline-offset: var(--focus-offset);
}
</style>
