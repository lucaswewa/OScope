<script setup lang="ts">
import { computed, onScopeDispose, reactive, shallowRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import { logFile, type Level } from "@/api/media";
import { useProperty } from "@/api/things/use";
import LogList from "@/features/logging/LogList.vue";
import { countsOf, LEVELS, sizeOf, TONES, type Filters } from "@/features/logging/logs";
import { useLogs } from "@/features/logging/useLogs";
import { host } from "@/host/host";
import { kept } from "@/host/kept";
import Badge from "@/ui/Badge.vue";
import Button from "@/ui/Button.vue";
import EmptyState from "@/ui/EmptyState.vue";
import Field from "@/ui/Field.vue";
import KeyValue from "@/ui/KeyValue.vue";
import Select from "@/ui/Select.vue";
import Splitter from "@/ui/Splitter.vue";
import TextField from "@/ui/TextField.vue";

/**
 * Logging (W4.2): the server's log, searched and followed. Its records, by
 * level, source, text and time, in a virtual list that follows the tail and
 * holds while scrolled up; each level's count; a record opened, with its
 * fields; and the daily files, listed and downloaded.
 */
const { t } = useI18n();

const START: Filters = { level: "info", source: "", text: "", since: "", until: "" };
const isFilters = (value: unknown): value is Filters =>
  typeof value === "object" &&
  value !== null &&
  LEVELS.includes((value as Filters).level) &&
  ["source", "text", "since", "until"].every(
    (key) => typeof (value as Record<string, unknown>)[key] === "string",
  );
/** The filters asked for, kept between visits; typed into, after a pause. */
const filters = kept("oscope.logging.filters", START, isFilters);
const typing = reactive({ ...filters.value });
let pause: ReturnType<typeof setTimeout> | undefined;
watch(typing, () => {
  clearTimeout(pause);
  pause = setTimeout(() => (filters.value = { ...typing }), 400);
});
onScopeDispose(() => clearTimeout(pause));

const logs = useLogs(filters);
const counts = computed(() => countsOf(logs.records.value));
const levels = computed(() =>
  LEVELS.map((value) => ({ value, label: t(`logging.levels.${value}`) })),
);
const tone = (level: Level) => TONES[level];

const chosenSeq = shallowRef<number>();
const chosen = computed(() => logs.records.value.find((record) => record.seq === chosenSeq.value));
const entries = computed(() => {
  const record = chosen.value;
  if (!record) return [];
  return [
    { term: t("logging.record.seq"), value: String(record.seq) },
    { term: t("logging.record.time"), value: new Date(record.time).toLocaleString() },
    { term: t("logging.record.level"), value: record.level },
    { term: t("logging.record.source"), value: record.target },
    ...Object.entries(record.fields ?? {}).map(([term, value]) => ({
      term,
      value: typeof value === "string" ? value : JSON.stringify(value),
    })),
  ];
});

const files = useProperty("/api/v1/logs/files", { every: 30_000 });
const saving = shallowRef<string>();
async function save(name: string): Promise<void> {
  saving.value = name;
  try {
    await host().files.save(name, await logFile(name));
  } finally {
    saving.value = undefined;
  }
}

const isPair = (value: unknown): value is [number, number] =>
  Array.isArray(value) && value.length === 2 && value.every((one) => typeof one === "number");
const sizes = kept("oscope.logging.panes", [68, 32] as [number, number], isPair);
</script>

<template>
  <div class="logging">
    <Splitter v-model:sizes="sizes" :label="t('logging.handle')" :min="[40, 20]">
      <template #first>
        <section class="main" :aria-label="t('logging.label')">
          <h1>{{ t("tabs.logging.title") }}</h1>
          <div class="filters" data-said="filters">
            <Field :label="t('logging.filters.level')">
              <Select
                :model-value="typing.level"
                :options="levels"
                compact
                @update:model-value="(level) => (typing.level = level as Level)"
              />
            </Field>
            <Field class="grow" :label="t('logging.filters.source')">
              <TextField
                v-model="typing.source"
                compact
                :placeholder="t('logging.filters.sourceHint')"
              />
            </Field>
            <Field class="grow" :label="t('logging.filters.text')">
              <TextField v-model="typing.text" compact />
            </Field>
            <Field :label="t('logging.filters.since')">
              <TextField
                v-model="typing.since"
                compact
                :placeholder="t('logging.filters.timeHint')"
              />
            </Field>
            <Field :label="t('logging.filters.until')">
              <TextField
                v-model="typing.until"
                compact
                :placeholder="t('logging.filters.timeHint')"
              />
            </Field>
          </div>
          <p class="counts" data-said="counts">
            <Badge v-for="level in LEVELS" :key="level" :tone="tone(level)" :data-count="level">
              {{
                t("logging.count", { level: t(`logging.levels.${level}`), count: counts[level] })
              }}
            </Badge>
          </p>
          <p v-if="logs.skipped.value" class="said">
            {{ t("logging.skipped", { count: logs.skipped.value }) }}
          </p>
          <p v-if="logs.failed.value" class="said failed">{{ logs.failed.value }}</p>
          <Button
            v-if="logs.older.value && logs.records.value.length"
            compact
            variant="bare"
            class="older"
            @click="logs.showOlder"
          >
            {{ t("logging.older") }}
          </Button>
          <LogList
            class="list"
            :records="logs.records.value"
            :label="t('logging.list')"
            :chosen="chosenSeq"
            @open="(seq) => (chosenSeq = seq)"
          />
        </section>
      </template>
      <template #second>
        <section class="side" :aria-label="t('logging.side')">
          <section v-if="chosen" class="record" aria-labelledby="record-title" data-said="record">
            <h2 id="record-title">{{ chosen.message }}</h2>
            <KeyValue :entries="entries" />
            <RouterLink v-if="chosen.job" :to="{ name: 'jobs', query: { job: chosen.job } }">
              {{ t("logging.record.job") }}
            </RouterLink>
          </section>
          <EmptyState v-else :title="t('logging.none')" icon="logging" />
          <section class="files" aria-labelledby="files-title" data-said="files">
            <h2 id="files-title">{{ t("logging.files.title") }}</h2>
            <ul>
              <li v-for="file in files.value.value ?? []" :key="file.name">
                <span class="name">{{ file.name }}</span>
                <span class="said">{{ t("logging.files.size", sizeOf(file.bytes)) }}</span>
                <Button
                  compact
                  variant="bare"
                  icon="download"
                  :busy="saving === file.name"
                  :aria-label="t('logging.files.save', { name: file.name })"
                  @click="save(file.name)"
                >
                  {{ t("logging.files.download") }}
                </Button>
              </li>
            </ul>
          </section>
        </section>
      </template>
    </Splitter>
  </div>
</template>

<style scoped>
.logging {
  display: grid;
  flex: 1;
  min-block-size: 0;
}

/* A column: the list takes what the lines above it leave, whichever of them show. */
.main {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  block-size: 100%;
  min-block-size: 0;
  padding: var(--space-3);
}

.list {
  flex: 1 1 0;
  min-block-size: 0;
}

.main h1,
.counts,
.said,
.side h2,
.files ul {
  margin: 0;
}

.filters {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  align-items: end;
}

.grow {
  flex: 1 1 10rem;
}

.counts {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-1);
}

.older {
  align-self: start;
}

.side {
  display: grid;
  gap: var(--space-3);
  align-content: start;
  block-size: 100%;
  padding: var(--space-3);
  background: var(--surface-pane);
  overflow: auto;
}

.record,
.files {
  display: grid;
  gap: var(--space-2);
}

.record h2 {
  overflow-wrap: anywhere;
}

.files ul {
  display: grid;
  gap: var(--space-1);
  padding: 0;
  list-style: none;
}

.files li {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  align-items: center;
}

.name {
  font-variant-numeric: tabular-nums;
}

.said {
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

.said.failed {
  color: var(--danger);
}
</style>
