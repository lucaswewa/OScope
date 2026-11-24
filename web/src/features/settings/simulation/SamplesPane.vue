<script setup lang="ts">
import { computed, shallowRef } from "vue";
import { useI18n } from "vue-i18n";

import { upload } from "@/api/projects";
import { thingStore } from "@/api/things/store";
import { useAction, useProperty } from "@/api/things/use";
import { pictureOf, slugOf } from "@/features/settings/simulation/simulation";
import { host } from "@/host/host";
import Badge from "@/ui/Badge.vue";
import Button from "@/ui/Button.vue";
import Field from "@/ui/Field.vue";
import NumberField from "@/ui/NumberField.vue";
import Progress from "@/ui/Progress.vue";
import Select from "@/ui/Select.vue";
import TextField from "@/ui/TextField.vue";

/**
 * What the simulator shows (W8.4, D23): its samples, each with its picture
 * and what it is for, loaded onto the stage or taken off; and one's own
 * image or stack, uploaded (W6.3) and imported as a sample, at its pixel's
 * size, in the light it is to be seen in.
 */
const { t } = useI18n();
const samples = useProperty("/api/v1/simulation/samples", { every: 10_000 });
const loaded = useProperty("/api/v1/simulation/loaded", { every: 5_000 });
const load = useAction("/api/v1/simulation/load");
const unload = useAction("/api/v1/simulation/unload");
const importing = useAction("/api/v1/simulation/import");
const said = shallowRef("");
const simulation = thingStore("simulation");
const refusal = (error: { message: string } | undefined) =>
  t("settings.simulation.refused", { why: error?.message ?? "" });

async function put(name: string | undefined): Promise<void> {
  const done = name === undefined ? await unload.run({}) : await load.run({ name });
  if (done === undefined) said.value = refusal((name ? load : unload).error.value);
  else
    said.value = name
      ? t("settings.simulation.loadedSaid", { name })
      : t("settings.simulation.unloadedSaid");
  await simulation.read("loaded");
}

// One's own: the file chosen, how it is to be seen, and its upload's progress.
const file = shallowRef<File>();
const own = shallowRef({
  name: "",
  umPerPixel: 0.5,
  zStep: null as number | null,
  light: "transmitted",
  dye: "green",
});
const sent = shallowRef<number>();
const lights = computed(() =>
  ["transmitted", "reflected", "fluorescent"].map((value) => ({
    value,
    label: t(`settings.simulation.lights.${value}`),
  })),
);
const dyes = computed(() =>
  ["dapi", "green", "orange", "far-red"].map((value) => ({
    value,
    label: t(`settings.simulation.dyes.${value}`),
  })),
);
async function choose(): Promise<void> {
  const [chosen] = await host().files.open({ accept: [".png", ".jpg", ".jpeg", ".tif", ".tiff"] });
  if (!chosen) return;
  file.value = chosen;
  own.value = { ...own.value, name: chosen.name.replace(/\.[^.]+$/, "") };
}
async function importIt(): Promise<void> {
  const chosen = file.value;
  if (!chosen || !own.value.name) return;
  const { name, umPerPixel, zStep, light, dye } = own.value;
  try {
    sent.value = 0;
    const progress = (now: { done: number }) => (sent.value = now.done / Math.max(1, chosen.size));
    const staged = await upload(chosen, { progress });
    const input = { name, upload: staged.id, um_per_pixel: umPerPixel, z_step: zStep, light, dye };
    const done = await importing.run(input);
    said.value =
      done === undefined
        ? refusal(importing.error.value)
        : t("settings.simulation.imported", { name });
    if (done !== undefined) file.value = undefined;
  } catch (thrown) {
    said.value = refusal(thrown instanceof Error ? thrown : undefined);
  } finally {
    sent.value = undefined;
  }
  await simulation.read("samples");
}
</script>

<template>
  <section class="pane" aria-labelledby="samples-title" data-said="samples">
    <h2 id="samples-title">{{ t("settings.simulation.samplesTitle") }}</h2>
    <p class="note">{{ t("settings.simulation.samplesNote") }}</p>
    <ul class="samples">
      <li v-for="sample in samples.value.value ?? []" :key="sample.name" :data-sample="sample.name">
        <img
          v-if="pictureOf(sample.name)"
          :src="pictureOf(sample.name)"
          :alt="t('settings.simulation.pictureOf', { name: sample.name })"
          width="240"
          height="160"
        />
        <div v-else class="own">{{ t("settings.simulation.ownPicture") }}</div>
        <h3>{{ sample.name }}</h3>
        <p class="note">
          {{
            sample.ground_truth
              ? t(`settings.simulation.purposes.${slugOf(sample.name)}`)
              : t("settings.simulation.yours", { kind: sample.kind })
          }}
        </p>
        <Badge v-if="loaded.value.value === sample.name" tone="accent">
          {{ t("settings.simulation.onStage") }}
        </Badge>
        <Button v-else compact :busy="load.running.value" @click="put(sample.name)">
          {{ t("settings.simulation.load") }}
        </Button>
      </li>
    </ul>
    <div class="actions">
      <Button :disabled="!loaded.value.value" :busy="unload.running.value" @click="put(undefined)">
        {{ t("settings.simulation.unload") }}
      </Button>
    </div>

    <h3>{{ t("settings.simulation.ownTitle") }}</h3>
    <p class="note">{{ t("settings.simulation.ownNote") }}</p>
    <div class="actions">
      <Button @click="choose">{{ t("settings.simulation.choose") }}</Button>
      <span v-if="file" class="note" data-said="own-file">{{ file.name }}</span>
    </div>
    <form v-if="file" class="fields" data-said="own-sample" @submit.prevent="importIt">
      <Field :label="t('settings.simulation.name')">
        <TextField
          :model-value="own.name"
          @update:model-value="(v) => (own = { ...own, name: v })"
        />
      </Field>
      <Field :label="t('settings.simulation.umPerPixel')">
        <NumberField
          :model-value="own.umPerPixel"
          :min="0.001"
          :step="0.01"
          :digits="3"
          @update:model-value="
            (v: number | null) => v !== null && (own = { ...own, umPerPixel: v })
          "
        />
      </Field>
      <Field :label="t('settings.simulation.zStep')" :hint="t('settings.simulation.zStepHint')">
        <NumberField
          :model-value="own.zStep"
          optional
          :min="0.01"
          :step="0.1"
          :digits="2"
          @update:model-value="(v: number | null) => (own = { ...own, zStep: v })"
        />
      </Field>
      <Field :label="t('settings.simulation.light')">
        <Select
          :model-value="own.light"
          :options="lights"
          @update:model-value="(v) => v && (own = { ...own, light: v })"
        />
      </Field>
      <Field v-if="own.light === 'fluorescent'" :label="t('settings.simulation.dye')">
        <Select
          :model-value="own.dye"
          :options="dyes"
          @update:model-value="(v) => v && (own = { ...own, dye: v })"
        />
      </Field>
      <div class="actions">
        <Button type="submit" variant="primary" :busy="sent !== undefined" :disabled="!own.name">
          {{ t("settings.simulation.import") }}
        </Button>
      </div>
      <Progress
        v-if="sent !== undefined"
        :label="t('settings.simulation.uploading')"
        :value="sent"
      />
    </form>
    <p class="said" role="status" data-said="samples-said">{{ said }}</p>
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

.samples {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(15rem, 1fr));
  gap: var(--space-3);
  margin: 0;
  padding: 0;
  list-style: none;
}

.samples li {
  display: grid;
  gap: var(--space-1);
  align-content: start;
  justify-items: start;
  padding: var(--space-2);
  border: 1px solid var(--border);
}

.samples img,
.own {
  inline-size: 100%;
  block-size: auto;
  aspect-ratio: 3 / 2;
  background: var(--surface-image);
}

.own {
  display: grid;
  place-items: center;
  color: var(--text-muted);
  background: var(--surface-open);
}

.fields {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(14rem, 1fr));
  gap: var(--space-2) var(--space-3);
  align-items: end;
}

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  align-items: center;
}

.note,
.said {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
