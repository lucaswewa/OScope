<script setup lang="ts">
import { useI18n } from "vue-i18n";

import StageSample from "@/features/control/StageSample.vue";
import { kept } from "@/host/kept";
import Splitter from "@/ui/Splitter.vue";

/**
 * Control and Live View (W2.1's frame for W3): the control pane and the
 * view, side by side, with a handle between them whose place is kept.
 */
const { t } = useI18n();

const isPair = (value: unknown): value is [number, number] =>
  Array.isArray(value) && value.length === 2 && value.every((one) => typeof one === "number");
const sizes = kept("oscope.control.panes", [33, 67] as [number, number], isPair);
</script>

<template>
  <div class="control">
    <Splitter v-model:sizes="sizes" :label="t('control.handle')" :min="[18, 40]">
      <template #first>
        <section class="pane" :aria-label="t('control.pane')">
          <h1>{{ t("tabs.control.title") }}</h1>
          <p class="note">{{ t("control.comesPane") }}</p>
          <StageSample />
        </section>
      </template>
      <template #second>
        <section class="view" :aria-label="t('control.view')">
          <p class="note">{{ t("control.comesView") }}</p>
        </section>
      </template>
    </Splitter>
  </div>
</template>

<style scoped>
/* A grid, so that the splitter fills the tab: look.md's pane of 400 px is a third of a wide window. */
.control {
  display: grid;
  flex: 1;
  min-block-size: 0;
}

.pane {
  display: grid;
  gap: var(--space-2);
  align-content: start;
  block-size: 100%;
  padding: var(--space-3);
  background: var(--surface-pane);
  overflow: auto;
}

.pane h1 {
  margin: 0;
}

.view {
  display: grid;
  place-items: center;
  block-size: 100%;
  background: var(--surface-page);
}

.note {
  margin: 0;
  color: var(--text-muted);
}
</style>
