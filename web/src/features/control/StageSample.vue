<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { useAction, useProperty } from "@/api/things/use";
import Button from "@/ui/Button.vue";

/**
 * The stage's first two controls (W2.1; W3.3 brings the rest): home it, and
 * centre it in its travel. A move before homing is refused by the stage,
 * which the shell tells in its words; while another holds the lease, both
 * are switched off.
 */
const { t } = useI18n();
const capabilities = useProperty("/api/v1/stage/capabilities");
const home = useAction("/api/v1/stage/home");
const centre = useAction("/api/v1/stage/move_to");

const middle = computed(() => {
  const travel = capabilities.value.value;
  if (!travel) return undefined;
  const half = (axis: { min: number; max: number }) => (axis.min + axis.max) / 2;
  return { x: half(travel.x), y: half(travel.y), z: half(travel.z) };
});
</script>

<template>
  <section class="stage" aria-labelledby="stage-title">
    <h2 id="stage-title">{{ t("control.stage.title") }}</h2>
    <p class="note">{{ t("control.stage.comes") }}</p>
    <div class="actions">
      <Button :busy="home.running.value" @click="home.run({})">{{
        t("control.stage.home")
      }}</Button>
      <Button
        :busy="centre.running.value"
        :disabled="!middle"
        @click="middle && centre.run(middle)"
      >
        {{ t("control.stage.centre") }}
      </Button>
    </div>
  </section>
</template>

<style scoped>
.stage {
  display: grid;
  gap: var(--space-2);
}

.stage h2 {
  margin: 0;
}

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
}

.note {
  margin: 0;
  color: var(--text-muted);
}
</style>
