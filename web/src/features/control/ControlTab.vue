<script setup lang="ts">
import { useI18n } from "vue-i18n";

import CameraPane from "@/features/control/camera/CameraPane.vue";
import LightsPane from "@/features/control/lights/LightsPane.vue";
import ObjectivePane from "@/features/control/objective/ObjectivePane.vue";
import StageHalt from "@/features/control/stage/StageHalt.vue";
import StagePane from "@/features/control/stage/StagePane.vue";
import LiveViewer from "@/features/live/LiveViewer.vue";
import { kept } from "@/host/kept";
import Splitter from "@/ui/Splitter.vue";

/**
 * Control and Live View (W2.1's frame for W3): the control pane and the
 * live image (W3.1), side by side, with a handle between them whose place
 * is kept. The stage's Halt stays at the pane's foot (W3.3).
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
          <CameraPane />
          <LightsPane />
          <ObjectivePane />
          <StagePane />
          <StageHalt />
        </section>
      </template>
      <template #second>
        <section class="view" :aria-label="t('control.view')">
          <LiveViewer />
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
  block-size: 100%;
  background: var(--surface-page);
}
</style>
