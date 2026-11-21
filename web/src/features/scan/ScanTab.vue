<script setup lang="ts">
import { useI18n } from "vue-i18n";

import OverviewPane from "@/features/scan/OverviewPane.vue";
import PlanPane from "@/features/scan/PlanPane.vue";
import ScanView from "@/features/scan/ScanView.vue";
import { kept } from "@/host/kept";
import Splitter from "@/ui/Splitter.vue";

/**
 * The scan tab (W5.1): the overview and the plan's panes, and beside them
 * the plan drawn over the overview, with a handle between whose place is
 * kept, as Control's is. Nothing moves until a scan runs (W5.2).
 */
const { t } = useI18n();

const isPair = (value: unknown): value is [number, number] =>
  Array.isArray(value) && value.length === 2 && value.every((one) => typeof one === "number");
const sizes = kept("oscope.scan.panes", [33, 67] as [number, number], isPair);
</script>

<template>
  <div class="scan">
    <Splitter v-model:sizes="sizes" :label="t('scan.handle')" :min="[20, 40]">
      <template #first>
        <section class="pane" :aria-label="t('scan.pane')">
          <h1>{{ t("tabs.scan.title") }}</h1>
          <OverviewPane />
          <PlanPane />
        </section>
      </template>
      <template #second>
        <section class="view" :aria-label="t('scan.view')">
          <ScanView />
        </section>
      </template>
    </Splitter>
  </div>
</template>

<style scoped>
.scan {
  display: grid;
  flex: 1;
  min-block-size: 0;
}

.pane {
  display: grid;
  gap: var(--space-3);
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
