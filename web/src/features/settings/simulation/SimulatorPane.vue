<script setup lang="ts">
import { computed, shallowRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import { act } from "@/api/actions";
import { useAction, useProperty } from "@/api/things/use";
import { changedParts } from "@/features/settings/settings";
import {
  formOf,
  settingsOf,
  wrongIn,
  type Form,
  type SimulatorSettings,
} from "@/features/settings/simulation/simulation";
import Banner from "@/ui/Banner.vue";
import Button from "@/ui/Button.vue";
import Field from "@/ui/Field.vue";
import NumberField from "@/ui/NumberField.vue";
import Select from "@/ui/Select.vue";
import Switch from "@/ui/Switch.vue";

/**
 * The simulator itself (W8.4, D23): its clock's rate and its modality,
 * now; its imperfections and what it starts with, for the next start, as
 * the configuration file holds them; where the slide is in focus under the
 * stage, the truth autofocus is held to; how its tile cache has done; and
 * faults to inject, to try the status with.
 */
const { t } = useI18n();
const rate = useProperty("/api/v1/simulation/rate", { every: 5_000 });
const confocal = useProperty("/api/v1/simulation/confocal", { every: 5_000 });
const cache = useProperty("/api/v1/simulation/tile_cache", { every: 2_000 });
const settings = useProperty("/api/v1/settings/simulator");
const restart = useProperty("/api/v1/settings/restart_needed", { every: 5_000 });
const catalogue = useProperty("/api/v1/objective/catalogue", { every: 60_000 });
const samples = useProperty("/api/v1/simulation/samples", { every: 10_000 });
const position = useProperty("/api/v1/stage/position", { every: 1_000 });
const inject = useAction("/api/v1/simulation/inject_fault");
const said = shallowRef("");

// The next start's settings: a draft of the form, from the file's until saved.
const saved = computed(() => settings.value.value as SimulatorSettings | null | undefined);
const draft = shallowRef<Form>();
watch(
  saved,
  (now) => {
    if (now && !draft.value) draft.value = formOf(now);
  },
  { immediate: true },
);
const changed = computed(() =>
  saved.value && draft.value ? changedParts(formOf(saved.value), draft.value) : [],
);
const wrong = computed(() => (draft.value ? wrongIn(draft.value) : undefined));
function edit(change: Partial<Form>): void {
  if (draft.value) draft.value = { ...draft.value, ...change };
}
/** A number typed: none left, it can't be, and is said so at its field. */
const number = (field: keyof Form, value: number | null) => edit({ [field]: value ?? Number.NaN });
async function save(): Promise<void> {
  if (!draft.value || !saved.value || wrong.value) return;
  const parts = changed.value.map((field) => t(`settings.simulation.fields.${field}`)).join(", ");
  await settings.write(settingsOf(draft.value, saved.value) as never);
  said.value = settings.refused.value
    ? t("settings.simulation.refused", { why: settings.refused.value.message })
    : t("settings.simulation.saved", { parts });
  if (!settings.refused.value && saved.value) draft.value = formOf(saved.value);
}

const sensors = computed(() =>
  ["full", "quarter"].map((value) => ({ value, label: t(`settings.simulation.sensors.${value}`) })),
);
const objectives = computed(() =>
  (catalogue.value.value?.objective ?? []).map(({ name }) => ({ value: name, label: name })),
);
/** No sample at the start, as the list says it: a list's value can't be empty. */
const NONE = "(none)";
const startSamples = computed(() => [
  { value: NONE, label: t("settings.simulation.none") },
  ...(samples.value.value ?? []).map(({ name }) => ({ value: name, label: name })),
]);

// Where the slide is in focus under the stage, as the simulator knows it.
const focus = shallowRef<number | null>();
async function findFocus(): Promise<void> {
  const at = position.value.value?.position;
  if (!at) return;
  focus.value = await act("/api/v1/simulation/in_focus", { x: at.x, y: at.y }).catch(() => null);
}

const FAULTS = ["camera_unplugged", "stage_alarm", "scratch_full"] as const;
const injected = shallowRef(new Set<string>());
async function fault(name: (typeof FAULTS)[number], active: boolean): Promise<void> {
  if ((await inject.run({ fault: name, active })) === undefined) return;
  const now = new Set(injected.value);
  if (active) now.add(name);
  else now.delete(name);
  injected.value = now;
}
</script>

<template>
  <section class="pane" aria-labelledby="simulator-title" data-said="simulator">
    <h2 id="simulator-title">{{ t("settings.simulation.simulatorTitle") }}</h2>
    <h3>{{ t("settings.simulation.nowTitle") }}</h3>
    <div class="fields">
      <Field :label="t('settings.simulation.rate')" :hint="t('settings.simulation.rateHint')">
        <NumberField
          :model-value="rate.value.value ?? null"
          :min="0.01"
          :max="1000"
          :step="1"
          :digits="2"
          @update:model-value="(v: number | null) => v !== null && rate.write(v)"
        />
      </Field>
      <Switch
        :model-value="confocal.value.value ?? false"
        @update:model-value="(on: boolean) => confocal.write(on)"
      >
        {{ t("settings.simulation.confocal") }}
      </Switch>
    </div>

    <h3>{{ t("settings.simulation.startTitle") }}</h3>
    <p class="note">{{ t("settings.simulation.startNote") }}</p>
    <Banner v-if="restart.value.value" level="warning" data-said="simulator-restart">
      {{ t("settings.systemPane.restart") }}
      <template #actions>
        <RouterLink :to="{ name: 'power' }">{{ t("settings.systemPane.restartIt") }}</RouterLink>
      </template>
    </Banner>
    <p v-if="saved === null" class="note">{{ t("settings.systemPane.noFile") }}</p>
    <form v-else-if="draft" class="fields" data-said="simulator-form" @submit.prevent="save">
      <Field :label="t('settings.simulation.fields.sensor')">
        <Select
          :model-value="draft.sensor"
          :options="sensors"
          @update:model-value="(v) => v && edit({ sensor: v })"
        />
      </Field>
      <Field :label="t('settings.simulation.fields.objective')">
        <Select
          :model-value="draft.objective"
          :options="objectives"
          @update:model-value="(v) => v && edit({ objective: v })"
        />
      </Field>
      <Field :label="t('settings.simulation.fields.sample')">
        <Select
          :model-value="draft.sample || NONE"
          :options="startSamples"
          @update:model-value="(v) => v && edit({ sample: v === NONE ? '' : v })"
        />
      </Field>
      <Field
        v-for="field in [
          'focus',
          'tiltX',
          'tiltY',
          'rotation',
          'scale',
          'backlash',
          'latency',
          'lostChance',
          'lostUm',
          'vignetting',
        ] as const"
        :key="field"
        :label="t(`settings.simulation.fields.${field}`)"
        :error="wrong?.field === field ? t(`settings.simulation.wrong.${wrong.why}`) : undefined"
      >
        <NumberField
          :model-value="Number.isFinite(draft[field]) ? draft[field] : null"
          :step="field === 'lostChance' || field === 'scale' ? 0.01 : 0.1"
          :digits="field === 'lostChance' || field === 'scale' ? 3 : 2"
          @update:model-value="(v: number | null) => number(field, v)"
        />
      </Field>
      <Switch
        :model-value="draft.mirrored"
        @update:model-value="(v: boolean) => edit({ mirrored: v })"
      >
        {{ t("settings.simulation.fields.mirrored") }}
      </Switch>
      <Switch
        :model-value="draft.encoders"
        @update:model-value="(v: boolean) => edit({ encoders: v })"
      >
        {{ t("settings.simulation.fields.encoders") }}
      </Switch>
      <Switch :model-value="draft.gpu" @update:model-value="(v: boolean) => edit({ gpu: v })">
        {{ t("settings.simulation.fields.gpu") }}
      </Switch>
      <div class="actions">
        <Button
          type="submit"
          variant="primary"
          :busy="settings.writing.value"
          :disabled="!changed.length || wrong !== undefined"
        >
          {{ t("settings.systemPane.save") }}
        </Button>
        <Button :disabled="!changed.length" @click="draft = saved ? formOf(saved) : draft">
          {{ t("settings.systemPane.undo") }}
        </Button>
      </div>
    </form>
    <p class="said" role="status" data-said="simulator-said">{{ said }}</p>

    <h3>{{ t("settings.simulation.toolsTitle") }}</h3>
    <div class="actions">
      <Button :disabled="!position.value.value" @click="findFocus">
        {{ t("settings.simulation.findFocus") }}
      </Button>
      <span class="note" data-said="in-focus">
        {{
          focus === undefined
            ? ""
            : focus === null
              ? t("settings.simulation.noFocus")
              : t("settings.simulation.inFocus", { z: focus.toFixed(1) })
        }}
      </span>
    </div>
    <p v-if="cache.value.value" class="note" data-said="tile-cache">
      {{
        t("settings.simulation.cache", {
          tiles: cache.value.value.tiles,
          mb: (cache.value.value.bytes / 1e6).toFixed(1),
          hits: cache.value.value.hits,
          misses: cache.value.value.misses,
        })
      }}
    </p>
    <div class="faults" data-said="faults">
      <Switch
        v-for="name in FAULTS"
        :key="name"
        :model-value="injected.has(name)"
        @update:model-value="(on: boolean) => fault(name, on)"
      >
        {{ t(`settings.simulation.faults.${name}`) }}
      </Switch>
    </div>
  </section>
</template>

<style scoped>
.pane {
  display: grid;
  gap: var(--space-2);
  align-content: start;
}

h2,
h3 {
  margin: 0;
  font-size: var(--font-size-body);
}

.fields {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(12rem, 1fr));
  gap: var(--space-2) var(--space-3);
  align-items: end;
}

.actions,
.faults {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2) var(--space-3);
  align-items: center;
  grid-column: 1 / -1;
}

.note,
.said {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
