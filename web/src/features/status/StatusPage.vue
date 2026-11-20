<script setup lang="ts">
import { computed, onMounted, shallowRef } from "vue";
import { useI18n } from "vue-i18n";

import { call, server } from "@/api/client";
import { useAction } from "@/api/things/use";
import { detailsOf } from "@/features/status/details";
import { iconOf, toneOf, useInstrument } from "@/features/status/status";
import Badge from "@/ui/Badge.vue";
import Button from "@/ui/Button.vue";
import Card from "@/ui/Card.vue";
import Icon from "@/ui/Icon.vue";
import KeyValue from "@/ui/KeyValue.vue";

/**
 * The status in full (W4.3): the instrument's state and why, then each
 * subsystem's state, message and details. With the simulator, its faults,
 * to see each state: injected and taken away.
 */
const { t, te } = useI18n();
const instrument = useInstrument();
const tone = computed(() => toneOf(instrument.value?.state));

/** A subsystem's name, as the app says it: the server's, for one it doesn't know. */
const named = (subsystem: string) => {
  const key = `status.subsystems.${subsystem}`;
  return te(key) ? t(key) : subsystem;
};
const word = (measure: string) => {
  const key = `status.details.${measure}`;
  return te(key) ? t(key) : undefined;
};

// The simulator's faults: only when the server has the simulator.
const FAULTS = ["camera_unplugged", "stage_alarm", "scratch_full"] as const;
const simulated = shallowRef(false);
onMounted(async () => {
  try {
    await call((signal) => server.api.GET("/api/v1/simulation/loaded", { signal }));
    simulated.value = true;
  } catch {
    // A real instrument: no simulator to fault.
  }
});
const inject = useAction("/api/v1/simulation/inject_fault");
const namedAs = (which: (typeof FAULTS)[number], deed: "inject" | "clear") =>
  t("status.faults.named", {
    fault: t(`status.faults.${which}`),
    deed: t(`status.faults.${deed}`),
  });
const busy = shallowRef<string>();
async function fault(which: (typeof FAULTS)[number], active: boolean): Promise<void> {
  busy.value = `${which}-${active}`;
  await inject.run({ fault: which, active });
  busy.value = undefined;
}
</script>

<template>
  <section class="status-page">
    <h1>{{ t("status.page.title") }}</h1>
    <p class="overall" :data-tone="tone" data-said="overall">
      <Icon :name="iconOf(tone)" />
      <span v-if="instrument">
        <strong>{{ t(`status.states.${instrument.state}`) }}</strong>
        {{ instrument.message }}
      </span>
      <span v-else>{{ t("status.untold") }}</span>
    </p>
    <div class="subsystems">
      <Card
        v-for="one in instrument?.subsystems ?? []"
        :key="one.subsystem"
        :title="named(one.subsystem)"
        :level="2"
        :data-subsystem="one.subsystem"
        :data-state="one.state"
      >
        <template #actions>
          <Badge :tone="toneOf(one.state)">{{ t(`status.states.${one.state}`) }}</Badge>
        </template>
        <p>{{ one.message }}</p>
        <KeyValue
          v-if="detailsOf(one.details, word).length"
          :entries="detailsOf(one.details, word)"
        />
      </Card>
    </div>
    <Card v-if="simulated" :title="t('status.faults.title')" :level="2" data-said="faults">
      <p class="said">{{ t("status.faults.text") }}</p>
      <ul class="faults">
        <li v-for="which in FAULTS" :key="which" :data-fault="which">
          <span>{{ t(`status.faults.${which}`) }}</span>
          <Button
            compact
            :busy="busy === `${which}-true`"
            :aria-label="namedAs(which, 'inject')"
            @click="fault(which, true)"
          >
            {{ t("status.faults.inject") }}
          </Button>
          <Button
            compact
            variant="bare"
            :busy="busy === `${which}-false`"
            :aria-label="namedAs(which, 'clear')"
            @click="fault(which, false)"
          >
            {{ t("status.faults.clear") }}
          </Button>
        </li>
      </ul>
    </Card>
  </section>
</template>

<style scoped>
.status-page {
  display: grid;
  gap: var(--space-4);
  align-content: start;
  max-inline-size: 72rem;
  padding: var(--space-6);
}

.status-page h1,
.status-page p,
.faults {
  margin: 0;
}

.overall {
  display: flex;
  gap: var(--space-2);
  align-items: center;
}

.overall[data-tone="success"] > :first-child {
  color: var(--success);
}

.overall[data-tone="warning"] > :first-child {
  color: var(--warning);
}

.overall[data-tone="danger"] > :first-child {
  color: var(--danger);
}

.subsystems {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(18rem, 1fr));
  gap: var(--space-4);
}

.said {
  color: var(--text-muted);
}

.faults {
  display: grid;
  gap: var(--space-2);
  padding: 0;
  list-style: none;
}

.faults li {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  align-items: center;
}

.faults span {
  flex: 1 1 12rem;
}
</style>
