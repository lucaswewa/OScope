<script setup lang="ts">
import { computed, reactive } from "vue";
import { useI18n } from "vue-i18n";

import { thingStore } from "@/api/things/store";
import { useAction, useProperty } from "@/api/things/use";
import { lightRows, unsaved, type LightRow } from "@/features/control/lights/lights";
import Button from "@/ui/Button.vue";
import Slider from "@/ui/Slider.vue";
import Switch from "@/ui/Switch.vue";

/**
 * The lights (W3.4): each channel the lights say they have, on or off, and
 * dimmed if it can be, set as it is changed. Lights changed behind the
 * pane differ from those saved, and can be put back.
 */
const { t } = useI18n();
const capabilities = useProperty("/api/v1/illumination/capabilities", { every: 60_000 });
const lights = useProperty("/api/v1/illumination/lights", { every: 5_000 });
const saved = useProperty("/api/v1/illumination/saved");
const setLight = useAction("/api/v1/illumination/set_light");

const rows = computed(() =>
  lightRows(capabilities.value.value?.channels ?? [], lights.value.value),
);
/** An intensity being dragged, by channel, in %: shown until the light is set. */
const dragged = reactive<Record<string, number>>({});
const percent = (row: LightRow) => dragged[row.name] ?? Math.round(row.intensity * 100);

/** Sets a channel, then reads the lights again: what was applied, not what was asked. */
async function set(row: LightRow, on: boolean, intensity = row.intensity): Promise<void> {
  await setLight.run({ channel: row.name, on, intensity });
  await thingStore("illumination").read("lights");
  delete dragged[row.name];
}

const differ = computed(() => unsaved(lights.value.value, saved.value.value));
async function putBack(): Promise<void> {
  for (const light of differ.value) await setLight.run(light);
  await thingStore("illumination").read("lights");
}
</script>

<template>
  <section class="lights" aria-labelledby="lights-title" data-said="lights">
    <h2 id="lights-title">{{ t("lights.title") }}</h2>
    <ul class="channels">
      <li v-for="row in rows" :key="row.name" :data-light="row.name">
        <Switch :model-value="row.on" @update:model-value="(on) => set(row, on)">
          {{ row.name }}
        </Switch>
        <Slider
          v-if="row.dimmable"
          :label="t('lights.intensity', { name: row.name })"
          :model-value="percent(row)"
          unit="%"
          @update:model-value="(value) => (dragged[row.name] = value)"
          @commit="(value) => set(row, row.on, value / 100)"
        />
      </li>
    </ul>
    <div v-if="differ.length" class="saved" data-said="lights-saved">
      <p>{{ t("lights.differ", { names: differ.map((light) => light.channel).join(", ") }) }}</p>
      <Button :busy="setLight.running.value" @click="putBack">{{ t("lights.putBack") }}</Button>
    </div>
  </section>
</template>

<style scoped>
.lights {
  display: grid;
  gap: var(--space-2);
}

.lights h2 {
  margin: 0;
}

.channels {
  display: grid;
  gap: var(--space-2);
  margin: 0;
  padding: 0;
  list-style: none;
}

.channels li {
  display: grid;
  gap: var(--space-1);
}

.saved {
  display: grid;
  gap: var(--space-2);
  justify-items: start;
  padding: var(--space-2);
  border-radius: var(--radius-surface);
  background: var(--warning-tint);
}

.saved p {
  margin: 0;
}
</style>
