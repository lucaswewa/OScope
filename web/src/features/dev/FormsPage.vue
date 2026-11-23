<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import { offer, published, type Published } from "@/api/schemas";
import { EXAMPLE_SCHEMA } from "@/features/dev/example-schema";
import GalleryHeader from "@/features/dev/GalleryHeader.vue";
import Banner from "@/ui/Banner.vue";
import Button from "@/ui/Button.vue";
import Card from "@/ui/Card.vue";
import Combobox from "@/ui/Combobox.vue";
import Field from "@/ui/Field.vue";
import type { Fault } from "@/ui/schema-form/check";
import SchemaForm from "@/ui/schema-form/SchemaForm.vue";
import { outside, shapeOf, type Json } from "@/ui/schema-form/shape";
import Spinner from "@/ui/Spinner.vue";
import Tabs from "@/ui/Tabs.vue";

/**
 * Forms drawn from JSON Schemas (W0.6): the gallery's own, and every one the
 * server publishes, one at a time or all at once.
 */
const { t } = useI18n();
const tab = ref("one");
const tabs = computed(() => [
  { value: "one", label: t("forms.tabs.one") },
  { value: "every", label: t("forms.tabs.every") },
]);

const example = computed<Published>(() => ({
  id: "example",
  label: t("forms.example"),
  part: "input",
  schema: EXAMPLE_SCHEMA,
}));
const server = shallowRef<Published[]>([]);
const reading = ref(true);
const unread = ref<string>();
const stop = new AbortController();
onMounted(async () => {
  try {
    server.value = await published(stop.signal);
  } catch (failure) {
    if (!stop.signal.aborted) unread.value = (failure as Error).message;
  } finally {
    reading.value = false;
  }
});
onBeforeUnmount(() => stop.abort());

// One schema, and its value.
const all = computed(() => [example.value, ...server.value]);
const choices = computed(() => all.value.map((one) => ({ value: one.id, label: one.label })));
const chosenId = ref("example");
const chosen = computed(() => all.value.find((one) => one.id === chosenId.value) ?? example.value);
const value = ref<Json | undefined>();
const faults = ref<Fault[]>([]);
const refused = ref<unknown>();
const answer = ref<string>();
watch(chosenId, () => {
  value.value = undefined;
  refused.value = undefined;
  answer.value = undefined;
});

/** Sends the value while it has a fault: the server refuses it, and does nothing. */
async function ask(): Promise<void> {
  const href = chosen.value.href;
  if (!href || faults.value.length === 0) return;
  const { taken, answer: said } = await offer(href, value.value ?? null);
  refused.value = taken ? undefined : said;
  answer.value = taken ? t("forms.took") : t("forms.refused");
}

// Every schema, and those drawn as JSON.
const asJson = computed(() =>
  server.value.flatMap((one) =>
    outside(shapeOf(one.schema)).map((at) => ({ label: one.label, ...at })),
  ),
);
</script>

<template>
  <main class="forms">
    <GalleryHeader :title="t('forms.title')" :intro="t('forms.intro')" />
    <Banner v-if="unread" level="danger">{{ t("forms.unread", { why: unread }) }}</Banner>

    <Tabs v-model="tab" class="tabs" :tabs="tabs" :label="t('forms.tabs.label')">
      <template #one>
        <div class="one">
          <div class="form">
            <Field :label="t('forms.schema')" :hint="reading ? t('forms.reading') : undefined">
              <Combobox v-model="chosenId" :options="choices" />
            </Field>
            <SchemaForm
              :key="chosen.id"
              v-model="value"
              :schema="chosen.schema"
              :label="chosen.label"
              :refused="refused"
              @faults="faults = $event"
            />
          </div>
          <Card class="value" :title="t('forms.value')" :level="2">
            <output data-said="faults">
              {{
                faults.length === 0
                  ? t("forms.faults.none")
                  : t("forms.faults.some", { count: faults.length })
              }}
            </output>
            <pre data-said="value">{{ JSON.stringify(value ?? null, null, 2) }}</pre>
            <template v-if="chosen.href">
              <Button :disabled="faults.length === 0" @click="ask">{{ t("forms.ask") }}</Button>
              <p class="note">{{ t("forms.askHint") }}</p>
              <output data-said="server">{{ answer }}</output>
            </template>
          </Card>
        </div>
      </template>
      <template #every>
        <Spinner v-if="reading" :label="t('forms.reading')" />
        <template v-else>
          <output data-said="every">
            {{
              asJson.length === 0
                ? t("forms.every.none", { count: server.length })
                : t("forms.every.some", { count: server.length, json: asJson.length })
            }}
          </output>
          <ul v-if="asJson.length" class="outside">
            <li v-for="one in asJson" :key="`${one.label}${one.where}`">
              {{ one.label }}, {{ one.where }}: {{ one.why }}
            </li>
          </ul>
          <div class="every">
            <Card v-for="one in server" :key="one.id" :title="one.label" :level="2">
              <SchemaForm :schema="one.schema" :label="one.label" />
            </Card>
          </div>
        </template>
      </template>
    </Tabs>
  </main>
</template>

<style scoped>
.forms {
  display: grid;
  gap: var(--space-4);
  padding: var(--space-6);
}

.one {
  display: grid;
  grid-template-columns: minmax(0, 3fr) minmax(16rem, 2fr);
  gap: var(--space-6);
  align-items: start;
  padding-block-start: var(--space-4);
}

.form {
  display: grid;
  gap: var(--space-4);
}

.value pre {
  max-block-size: 32rem;
  overflow: auto;
  font-family: var(--font-mono);
  font-size: var(--font-size-small);
}

.note {
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

.every {
  display: grid;
  gap: var(--space-4);
  padding-block-start: var(--space-4);
}
</style>
