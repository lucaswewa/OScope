<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { zAt, type FocusMap } from "@/features/scan/histo/histo";
import type { Scan } from "@/features/scan/scanning";

/**
 * The focus map the run measured (W5.3), once fitted, before its first
 * tile: its surface, how well it foretells focus, and each point's focus
 * found beside the surface's there; the points it rejected, apart.
 */
const props = defineProps<{ scan: Scan }>();
const { t } = useI18n();

const map = computed<FocusMap | undefined>(() => {
  const z = props.scan.progress.value?.plan.z;
  return z?.kind === "map" ? z.map : undefined;
});
const said = computed(() => {
  const now = map.value;
  if (!now) return undefined;
  return t("scan.histo.map", {
    surface: t(`scan.run.surfaces.${now.surface.kind}`),
    used: now.used.length,
    rejected: now.rejected.length,
    rms: now.rms_um === null || now.rms_um === undefined ? "—" : now.rms_um.toFixed(2),
  });
});
const mm = (um: number) => (um / 1000).toFixed(3);
const points = computed(() => {
  const now = map.value;
  if (!now) return [];
  const row = (p: FocusMap["used"][number], rejected: boolean) => ({
    at: t("scan.plan.at", { x: mm(p.x_um), y: mm(p.y_um) }),
    found: p.z_um.toFixed(1),
    surface: zAt(now, { x: p.x_um, y: p.y_um }).toFixed(1),
    rejected,
  });
  return [...now.used.map((p) => row(p, false)), ...now.rejected.map((p) => row(p, true))];
});
</script>

<template>
  <section v-if="said" class="map" aria-labelledby="map-title" data-said="map-said">
    <h3 id="map-title">{{ t("scan.histo.mapTitle") }}</h3>
    <p class="said">{{ said }}</p>
    <table>
      <thead>
        <tr>
          <th scope="col">{{ t("scan.histo.point") }}</th>
          <th scope="col">{{ t("scan.histo.found") }}</th>
          <th scope="col">{{ t("scan.histo.surface") }}</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="(one, i) in points" :key="i" :class="{ rejected: one.rejected }">
          <td>
            {{ one.at }}<template v-if="one.rejected"> {{ t("scan.histo.rejected") }}</template>
          </td>
          <td>{{ one.found }}</td>
          <td>{{ one.surface }}</td>
        </tr>
      </tbody>
    </table>
  </section>
</template>

<style scoped>
.map {
  display: grid;
  gap: var(--space-1);
}

.map h3,
.said {
  margin: 0;
}

.said {
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

table {
  border-collapse: collapse;
  font-size: var(--font-size-small);
  font-variant-numeric: tabular-nums;
}

th,
td {
  padding: var(--space-1) var(--space-2);
  border-block-end: 1px solid var(--border);
  text-align: start;
}

.rejected {
  color: var(--text-muted);
}
</style>
