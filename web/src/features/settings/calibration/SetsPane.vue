<script setup lang="ts">
import { computed, shallowRef } from "vue";
import { useI18n } from "vue-i18n";

import { thingStore } from "@/api/things/store";
import { useAction, useProperty } from "@/api/things/use";
import { byObjective, fileOf, type Summary } from "@/features/settings/calibration/calibration";
import { host } from "@/host/host";
import Badge from "@/ui/Badge.vue";
import Button from "@/ui/Button.vue";

/**
 * Every calibration set (W8.2, ADR-0023), by the objective it is for, then
 * the instrument's: each key's newest first, the one in use marked. Another
 * is chosen to be in use; one exported to a file, and imported back as its
 * key's next version, which is used once chosen.
 */
const { t } = useI18n();
const sets = useProperty("/api/v1/calibration/sets", { every: 5_000 });
const activate = useAction("/api/v1/calibration/activate");
const exporting = useAction("/api/v1/calibration/export");
const importing = useAction("/api/v1/calibration/import");
const said = shallowRef("");
const groups = computed(() => byObjective(sets.value.value ?? []));
const kept = async (...names: string[]) => {
  const calibration = thingStore("calibration");
  await Promise.all(names.map((name) => calibration.read(name)));
};

async function use(set: Summary): Promise<void> {
  const done = await activate.run({ id: set.id });
  said.value = done
    ? t("settings.calibration.inUse", { headline: set.headline })
    : t("settings.calibration.refused", { why: activate.error.value?.message ?? "" });
  await kept("sets", "active");
}

async function exportIt(set: Summary): Promise<void> {
  const whole = await exporting.run({ id: set.id });
  if (!whole) {
    said.value = t("settings.calibration.refused", { why: exporting.error.value?.message ?? "" });
    return;
  }
  const name = fileOf(set);
  const text = `${JSON.stringify(whole, null, 2)}\n`;
  await host().files.save(name, new Blob([text], { type: "application/json" }));
  said.value = t("settings.calibration.exported", { name });
}

async function importIt(): Promise<void> {
  const [file] = await host().files.open({ accept: [".json", "application/json"] });
  if (!file) return;
  let set: unknown;
  try {
    set = JSON.parse(await file.text());
  } catch {
    said.value = t("settings.calibration.notJson", { name: file.name });
    return;
  }
  const made = await importing.run({ set });
  said.value = made
    ? t("settings.calibration.imported", { name: file.name, version: made.version })
    : t("settings.calibration.refused", { why: importing.error.value?.message ?? "" });
  await kept("sets");
}
</script>

<template>
  <section class="pane" aria-labelledby="sets-title" data-said="sets">
    <h2 id="sets-title">{{ t("settings.calibration.setsTitle") }}</h2>
    <p class="note">{{ t("settings.calibration.setsNote") }}</p>
    <div class="actions">
      <Button compact @click="importIt">{{ t("settings.calibration.import") }}</Button>
    </div>
    <p v-if="groups.length === 0" class="note">{{ t("settings.calibration.noSets") }}</p>
    <section
      v-for="group in groups"
      :key="group.objective ?? ''"
      class="group"
      :data-said="`sets-${group.objective ?? 'instrument'}`"
    >
      <h3>
        {{
          group.objective
            ? t("settings.calibration.forObjective", { objective: group.objective })
            : t("settings.calibration.forInstrument")
        }}
      </h3>
      <ul>
        <li v-for="set in group.sets" :key="set.id" :data-set="set.id">
          <span class="headline">{{ set.headline }}</span>
          <span class="note">
            {{
              t("settings.calibration.made", {
                version: set.version,
                when: new Date(set.created).toLocaleString(),
              })
            }}
          </span>
          <Badge v-if="set.active" tone="accent">{{ t("settings.calibration.active") }}</Badge>
          <Button v-else compact @click="use(set)">{{ t("settings.calibration.use") }}</Button>
          <Button compact @click="exportIt(set)">{{ t("settings.calibration.export") }}</Button>
        </li>
      </ul>
    </section>
    <p class="said" role="status" data-said="sets-said">{{ said }}</p>
  </section>
</template>

<style scoped>
.pane,
.group {
  display: grid;
  gap: var(--space-2);
  align-content: start;
}

h2,
h3 {
  margin: 0;
  font-size: var(--font-size-body);
}

ul {
  display: grid;
  gap: var(--space-1);
  margin: 0;
  padding: 0;
  list-style: none;
}

li {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  align-items: center;
  padding-block: var(--space-1);
  border-block-end: 1px solid var(--border);
}

.headline {
  flex: 1 1 16rem;
}

.note,
.said {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
