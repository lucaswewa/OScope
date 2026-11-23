<script setup lang="ts">
import { useI18n } from "vue-i18n";

import Icon from "./Icon.vue";
import IconButton from "./IconButton.vue";
import type { Level } from "./toasts";

/**
 * A message that stays in its place on the page (ADR-0042), until what it
 * says is no longer so. A warning or a failure is announced to a screen
 * reader at once; the others when it is free.
 *
 * What can be done about it goes in the `actions` slot.
 */
const props = withDefaults(
  defineProps<{
    level?: Level;
    /** A few words before the message, in a stronger weight. */
    title?: string;
    /** Whether its reader may send it away: it then says `dismiss`. */
    dismissable?: boolean;
  }>(),
  { level: "info" },
);
defineEmits<{ dismiss: [] }>();
const { t } = useI18n();
const urgent = (): boolean => props.level === "warning" || props.level === "danger";
</script>

<template>
  <div class="banner" :class="level" :role="urgent() ? 'alert' : 'status'">
    <Icon :name="level" />
    <p class="message">
      <strong v-if="title">{{ title }}</strong>
      <slot />
    </p>
    <div v-if="$slots.actions" class="actions">
      <slot name="actions" />
    </div>
    <IconButton
      v-if="dismissable"
      icon="close"
      :label="t('ui.dismiss')"
      compact
      @click="$emit('dismiss')"
    />
  </div>
</template>

<style scoped>
.banner {
  display: flex;
  gap: var(--space-3);
  align-items: center;
  padding: var(--space-2) var(--space-3);
  border-inline-start: var(--space-1) solid var(--info);
  border-radius: var(--radius-surface);
  background: var(--info-tint);
  color: var(--text);
}

.banner > svg {
  color: var(--info);
}

.success {
  border-inline-start-color: var(--success);
  background: var(--success-tint);
}

.success > svg {
  color: var(--success);
}

.warning {
  border-inline-start-color: var(--warning);
  background: var(--warning-tint);
}

.warning > svg {
  color: var(--warning);
}

.danger {
  border-inline-start-color: var(--danger);
  background: var(--danger-tint);
}

.danger > svg {
  color: var(--danger);
}

.message {
  flex: 1;
}

.message strong {
  margin-inline-end: var(--space-2);
  font-weight: var(--font-weight-strong);
}

.actions {
  display: flex;
  gap: var(--space-2);
}
</style>
