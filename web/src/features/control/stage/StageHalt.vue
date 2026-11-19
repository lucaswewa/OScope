<script setup lang="ts">
import { onBeforeUnmount, onMounted } from "vue";
import { useI18n } from "vue-i18n";

import { useStage } from "@/features/control/stage/useStage";
import Button from "@/ui/Button.vue";
import { useDisabled } from "@/ui/control";

/**
 * Halt (W3.3): at the control pane's foot, where scrolling never takes it,
 * and Escape anywhere on the tab, in a field too. It stops a move, a jog
 * or homing. Like every control, it needs the lease.
 */
const { t } = useI18n();
const stage = useStage();
const locked = useDisabled({});

// Caught on its way down, before anything on the page can keep it.
function escape(event: KeyboardEvent): void {
  if (event.key === "Escape" && !event.repeat && !locked.value) void stage.halt();
}
onMounted(() => globalThis.addEventListener("keydown", escape, true));
onBeforeUnmount(() => globalThis.removeEventListener("keydown", escape, true));
</script>

<template>
  <div class="halt">
    <Button
      variant="primary"
      tone="danger"
      icon="stop"
      :aria-keyshortcuts="'Escape'"
      @click="stage.halt"
    >
      {{ t("stage.halt") }}
    </Button>
    <span class="key" aria-hidden="true">{{ t("stage.haltKey") }}</span>
  </div>
</template>

<style scoped>
/* Stuck to the pane's foot as it scrolls, over what passes under it. */
.halt {
  position: sticky;
  inset-block-end: calc(-1 * var(--space-3));
  display: flex;
  gap: var(--space-2);
  align-items: center;
  margin-inline: calc(-1 * var(--space-3));
  margin-block-end: calc(-1 * var(--space-3));
  padding: var(--space-2) var(--space-3);
  border-block-start: 1px solid var(--border);
  background: var(--surface-pane);
}

.key {
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
