<script setup lang="ts">
import { useI18n } from "vue-i18n";

import Icon from "./Icon.vue";
import Popover from "./Popover.vue";
import { toasts } from "./toasts";

/**
 * The notices raised (W2.2), the newest first, sent away or not, each with
 * when: a panel opened from its trigger, which the page gives.
 */
const { t } = useI18n();
const open = defineModel<boolean>("open", { default: false });
const history = toasts.history;
const when = (at: Date) => at.toLocaleTimeString();
</script>

<template>
  <Popover v-model:open="open" side="right" align="end">
    <template #trigger>
      <slot name="trigger" />
    </template>
    <section class="history" data-said="notices">
      <h2>{{ t("ui.notices.title") }}</h2>
      <p v-if="history.length === 0" class="none">{{ t("ui.notices.none") }}</p>
      <ol v-else>
        <li v-for="one in history" :key="one.id" :data-level="one.level">
          <Icon :name="one.level" />
          <span class="text">{{ one.text }}</span>
          <time :datetime="one.at.toISOString()">{{ when(one.at) }}</time>
        </li>
      </ol>
    </section>
  </Popover>
</template>

<style scoped>
.history {
  display: grid;
  gap: var(--space-2);
  inline-size: min(26rem, 80vw);
}

.history h2,
.none {
  margin: 0;
}

.none {
  color: var(--text-muted);
}

ol {
  display: grid;
  gap: var(--space-2);
  max-block-size: 60vh;
  margin: 0;
  padding: 0;
  overflow: auto;
  list-style: none;
}

li {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  gap: var(--space-2);
  align-items: start;
}

li[data-level="success"] > :first-child {
  color: var(--success);
}

li[data-level="warning"] > :first-child {
  color: var(--warning);
}

li[data-level="danger"] > :first-child {
  color: var(--danger);
}

time {
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
