<script setup lang="ts">
import { computed, shallowRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import type { Schema } from "@/api/client";
import { useProperty } from "@/api/things/use";
import { changedParts } from "@/features/settings/settings";
import Banner from "@/ui/Banner.vue";
import Button from "@/ui/Button.vue";
import Field from "@/ui/Field.vue";
import NumberField from "@/ui/NumberField.vue";
import TextField from "@/ui/TextField.vue";

type System = NonNullable<Schema<"settings_system_value">>;

/**
 * The system's settings (W8.1, P52): the data and scratch roots, the
 * address and the port, as the configuration file holds them for the next
 * start. Each is checked as that start would, and a change waits for a
 * restart, which the page says, and offers on Power.
 */
const { t } = useI18n();
const system = useProperty("/api/v1/settings/system");
const restart = useProperty("/api/v1/settings/restart_needed", { every: 5_000 });
const said = shallowRef("");

/** What is typed, from the file's until saved. */
const draft = shallowRef<System>();
watch(
  () => system.value.value,
  (now) => {
    if (now && !draft.value) draft.value = { ...now };
  },
  { immediate: true },
);
const changed = computed(() =>
  system.value.value && draft.value ? changedParts(system.value.value, draft.value) : [],
);
function edit(change: Partial<System>): void {
  if (draft.value) draft.value = { ...draft.value, ...change };
}
async function save(): Promise<void> {
  if (!draft.value) return;
  const parts = changed.value.join(", ");
  await system.write(draft.value);
  said.value = system.refused.value
    ? t("settings.systemPane.refused", { why: system.refused.value.message })
    : t("settings.systemPane.saved", { parts });
  if (!system.refused.value && system.value.value) draft.value = { ...system.value.value };
}
</script>

<template>
  <section class="pane" aria-labelledby="system-title" data-said="system-pane">
    <h2 id="system-title">{{ t("settings.systemPane.title") }}</h2>
    <p class="note">{{ t("settings.systemPane.note") }}</p>
    <Banner v-if="restart.value.value" level="warning" data-said="restart-needed">
      {{ t("settings.systemPane.restart") }}
      <template #actions>
        <RouterLink :to="{ name: 'power' }">{{ t("settings.systemPane.restartIt") }}</RouterLink>
      </template>
    </Banner>
    <p v-if="system.value.value === null" class="note" data-said="no-file">
      {{ t("settings.systemPane.noFile") }}
    </p>
    <form v-else-if="draft" class="fields" @submit.prevent="save">
      <Field :label="t('settings.systemPane.dataRoot')">
        <TextField
          :model-value="draft.data_root"
          @update:model-value="(v) => edit({ data_root: v })"
        />
      </Field>
      <Field :label="t('settings.systemPane.scratchRoot')">
        <TextField
          :model-value="draft.scratch_root"
          @update:model-value="(v) => edit({ scratch_root: v })"
        />
      </Field>
      <Field :label="t('settings.systemPane.address')" :hint="t('settings.systemPane.addressHint')">
        <TextField :model-value="draft.address" @update:model-value="(v) => edit({ address: v })" />
      </Field>
      <Field :label="t('settings.systemPane.port')" :hint="t('settings.systemPane.portHint')">
        <NumberField
          :model-value="draft.port"
          :min="0"
          :max="65535"
          @update:model-value="(v: number | null) => v !== null && edit({ port: v })"
        />
      </Field>
      <div class="actions">
        <Button
          type="submit"
          variant="primary"
          :busy="system.writing.value"
          :disabled="!changed.length"
        >
          {{ t("settings.systemPane.save") }}
        </Button>
        <Button
          :disabled="!changed.length"
          @click="draft = system.value.value ? { ...system.value.value } : draft"
        >
          {{ t("settings.systemPane.undo") }}
        </Button>
      </div>
    </form>
    <p class="said" role="status" data-said="system-said">{{ said }}</p>
  </section>
</template>

<style scoped>
.pane {
  display: grid;
  gap: var(--space-2);
  align-content: start;
  justify-items: start;
}

h2 {
  margin: 0;
  font-size: var(--font-size-body);
}

.fields {
  display: grid;
  grid-template-columns: repeat(2, minmax(14rem, 24rem));
  gap: var(--space-2) var(--space-3);
}

.actions {
  display: flex;
  gap: var(--space-2);
  grid-column: 1 / -1;
}

.note,
.said {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
