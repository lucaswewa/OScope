<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import HistoScan from "@/features/scan/histo/HistoScan.vue";
import OverviewPane from "@/features/scan/OverviewPane.vue";
import PlanPane from "@/features/scan/PlanPane.vue";
import RunningPane from "@/features/scan/RunningPane.vue";
import RunPane from "@/features/scan/RunPane.vue";
import { useScan } from "@/features/scan/scanning";
import ScanView from "@/features/scan/ScanView.vue";
import { kept } from "@/host/kept";
import Splitter from "@/ui/Splitter.vue";
import Tabs from "@/ui/Tabs.vue";

/**
 * The scan tab (W5.1): the overview and the plan's panes, and beside them
 * the plan drawn over the overview, with a handle between whose place is
 * kept, as Control's is. Nothing moves until a scan runs (W5.2): its run's
 * pane, the scan's, and the scan drawn on the view as it is taken. Or the
 * histo scan's flow (W5.3), its own tab, which the view follows.
 */
const { t } = useI18n();
const scan = useScan();

const isPair = (value: unknown): value is [number, number] =>
  Array.isArray(value) && value.length === 2 && value.every((one) => typeof one === "number");
const sizes = kept("oscope.scan.panes", [33, 67] as [number, number], isPair);
const isFlow = (value: unknown): value is string => value === "plan" || value === "histo";
const flow = kept("oscope.scan.flow", "plan", isFlow);
const flows = computed(() => [
  { value: "plan", label: t("scan.flows.plan") },
  { value: "histo", label: t("scan.flows.histo") },
]);
</script>

<template>
  <div class="scan">
    <Splitter v-model:sizes="sizes" :label="t('scan.handle')" :min="[20, 40]">
      <template #first>
        <section class="pane" :aria-label="t('scan.pane')">
          <h1>{{ t("tabs.scan.title") }}</h1>
          <Tabs v-model="flow" :tabs="flows" :label="t('scan.flows.label')">
            <template #plan>
              <div class="flow">
                <OverviewPane />
                <PlanPane />
                <RunPane :scan="scan" />
                <RunningPane :scan="scan" />
              </div>
            </template>
            <template #histo>
              <HistoScan :scan="scan" />
            </template>
          </Tabs>
        </section>
      </template>
      <template #second>
        <section class="view" :aria-label="t('scan.view')">
          <ScanView :scan="scan" :histo="flow === 'histo'" />
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

.flow {
  display: grid;
  gap: var(--space-3);
}

.view {
  block-size: 100%;
  background: var(--surface-page);
}
</style>
