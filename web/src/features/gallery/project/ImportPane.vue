<script setup lang="ts">
import { computed, onMounted, shallowRef } from "vue";
import { useI18n } from "vue-i18n";

import { ApiError } from "@/api/errors";
import type { Format } from "@/api/projects";
import {
  acceptOf,
  clearImports,
  formatsOnce,
  IMPORT_ENDED,
  importFiles,
  importsOf,
  stopImport,
  type Import,
} from "@/features/gallery/imports";
import { sizeOf } from "@/features/logging/logs";
import { host } from "@/host/host";
import Button from "@/ui/Button.vue";
import Progress from "@/ui/Progress.vue";

/**
 * A project's imports (W6.3): files dropped on its page or its card, or
 * chosen here, each uploaded with its progress, then imported as a job;
 * the acquisition each made, chosen from here.
 */
const props = defineProps<{ project: string }>();
const emit = defineEmits<{ open: [acquisition: string] }>();
const { t } = useI18n();

const importers = shallowRef<Format[]>([]);
const unread = shallowRef<string>();
onMounted(async () => {
  try {
    importers.value = (await formatsOnce()).importers;
  } catch (thrown) {
    unread.value = thrown instanceof ApiError ? thrown.message : String(thrown);
  }
});
const kinds = computed(() => importers.value.map((one) => one.name).join(", "));
const mine = computed(() => importsOf(props.project));
const ended = computed(() => mine.value.some((one) => IMPORT_ENDED.has(one.state)));

async function choose(): Promise<void> {
  const files = await host().files.open({ accept: acceptOf(importers.value), multiple: true });
  if (files.length) importFiles(props.project, files, importers.value);
}
const size = (bytes: number) => t("logging.files.size", sizeOf(bytes));
function said(one: Import): string {
  const of = { name: one.name, done: size(one.done), of: size(one.of) };
  return t(`imports.states.${one.state}`, { ...of, why: one.why ?? "" });
}
</script>

<template>
  <section class="imports" aria-labelledby="imports-title" data-said="imports">
    <div class="head">
      <h2 id="imports-title">{{ t("imports.title") }}</h2>
      <Button compact variant="bare" icon="upload" @click="choose">{{
        t("imports.choose")
      }}</Button>
      <Button
        v-if="ended"
        compact
        variant="bare"
        :aria-label="t('imports.clearLabel')"
        @click="clearImports(project)"
      >
        {{ t("imports.clear") }}
      </Button>
    </div>
    <p class="said">{{ unread ?? t("imports.text", { kinds }) }}</p>
    <ul v-if="mine.length" class="list">
      <li v-for="one in mine" :key="one.key" class="one" :data-said="`import-${one.name}`">
        <span class="said" :class="{ failed: one.state === 'failed' || one.state === 'refused' }">
          {{ said(one) }}
        </span>
        <Button
          v-if="one.state === 'uploading'"
          compact
          variant="bare"
          :aria-label="t('imports.stopLabel', { name: one.name })"
          @click="stopImport(one.key)"
        >
          {{ t("imports.stop") }}
        </Button>
        <Button
          v-else-if="one.state === 'imported' && one.acquisition"
          compact
          variant="bare"
          :aria-label="t('imports.openLabel', { name: one.name })"
          @click="emit('open', one.acquisition)"
        >
          {{ t("imports.open") }}
        </Button>
        <Progress
          v-if="one.state === 'uploading' || one.state === 'importing'"
          class="bar"
          :value="one.state === 'uploading' && one.of ? one.done : null"
          :max="one.of || 100"
          :label="said(one)"
        />
      </li>
    </ul>
  </section>
</template>

<style scoped>
.imports {
  display: grid;
  gap: var(--space-1);
}

.head {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  align-items: center;
}

.head h2 {
  margin: 0;
  font-size: var(--font-size-body);
}

.list {
  display: grid;
  gap: var(--space-2);
  margin: 0;
  padding: 0;
  list-style: none;
}

.one {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: var(--space-1) var(--space-2);
  align-items: center;
}

.bar {
  grid-column: 1 / -1;
}

.said {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
  overflow-wrap: anywhere;
}

.said.failed {
  color: var(--danger);
}
</style>
