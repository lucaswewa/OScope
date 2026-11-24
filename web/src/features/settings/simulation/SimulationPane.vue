<script setup lang="ts">
import SamplesPane from "@/features/settings/simulation/SamplesPane.vue";
import SimulatorPane from "@/features/settings/simulation/SimulatorPane.vue";

/** The simulated instrument (W8.4): what it shows, and how it behaves. */
</script>

<template>
  <div class="simulation">
    <SamplesPane />
    <SimulatorPane />
  </div>
</template>

<style scoped>
.simulation {
  display: grid;
  gap: var(--space-4);
  align-content: start;
}
</style>
