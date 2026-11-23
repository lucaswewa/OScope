<script setup lang="ts">
import { useI18n } from "vue-i18n";

import { useDisabled } from "./control";
import Icon from "./Icon.vue";
import type { IconName } from "./icons";
import Spinner from "./Spinner.vue";

/**
 * A button (ADR-0042). `primary` is the one a task has; `outline` the others;
 * `bare` an action with no box, as in a toolbar. The danger tone is for what
 * deletes or stops.
 */
const props = withDefaults(
  defineProps<{
    variant?: "primary" | "outline" | "bare";
    tone?: "accent" | "danger";
    /** The smaller size, where several share a row. */
    compact?: boolean;
    /** An icon before the text. */
    icon?: IconName;
    /** Under way: it keeps the focus, says so, and takes no press. */
    busy?: boolean;
    disabled?: boolean;
    type?: "button" | "submit" | "reset";
  }>(),
  { variant: "outline", tone: "accent", type: "button" },
);

const { t } = useI18n();

/** A busy button stays where the focus is, but a press does nothing. */
function swallow(event: MouseEvent): void {
  if (!props.busy) return;
  event.preventDefault();
  event.stopImmediatePropagation();
}
// Switched off by its own prop, or by a locked part of the page (ADR-0048).
const off = useDisabled(props);
</script>

<template>
  <button
    class="button"
    :class="[variant, tone, { compact }]"
    :type="type"
    :disabled="off"
    :aria-busy="busy || undefined"
    :aria-disabled="busy || undefined"
    @click.capture="swallow"
  >
    <template v-if="busy">
      <Spinner />
      <span class="hidden">{{ t("ui.busy") }}</span>
    </template>
    <Icon v-else-if="icon" :name="icon" small />
    <slot />
  </button>
</template>

<style scoped>
.button {
  position: relative;
  display: inline-flex;
  gap: var(--space-2);
  align-items: center;
  justify-content: center;
  block-size: var(--size-control);
  padding-inline: var(--space-3);
  border: 1px solid transparent;
  border-radius: var(--radius-control);
  font-weight: var(--font-weight-strong);
  white-space: nowrap;
  cursor: pointer;
  transition:
    background-color var(--motion-fast) var(--motion-ease),
    border-color var(--motion-fast) var(--motion-ease);
}

.compact {
  block-size: var(--size-control-compact);
  padding-inline: var(--space-2);
}

.primary {
  background: var(--accent);
  color: var(--accent-on);
}

.primary:hover {
  background: var(--accent-hover);
}

.primary:active {
  background: var(--accent-pressed);
}

.outline {
  border-color: var(--accent-text);
  background: var(--surface-raised);
  color: var(--accent-text);
}

.outline:hover,
.outline:active {
  background: var(--accent-tint);
}

.bare {
  background: transparent;
  color: var(--text);
}

.bare:hover,
.bare:active {
  background: var(--surface-hover);
}

.danger.primary {
  background: var(--danger-fill);
  color: var(--danger-on);
}

.danger.primary:hover {
  background: var(--danger-fill-hover);
}

.danger.primary:active {
  background: var(--danger-fill-pressed);
}

.danger.outline {
  border-color: var(--danger);
  color: var(--danger);
}

.danger.bare {
  color: var(--danger);
}

.danger.outline:hover,
.danger.outline:active,
.danger.bare:hover,
.danger.bare:active {
  background: var(--danger-tint);
}

/* Switched off: no fill of its own, whatever it was. */
.button:disabled,
.button:disabled:hover {
  border-color: var(--border);
  background: var(--surface-open);
  color: var(--text-disabled);
  cursor: not-allowed;
}

.button[aria-busy] {
  cursor: progress;
}

/* Said to a screen reader, and not drawn. */
.hidden {
  position: absolute;
  inline-size: 1px;
  block-size: 1px;
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}
</style>
