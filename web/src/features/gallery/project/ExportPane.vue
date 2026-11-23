<script setup lang="ts">
import { computed, onMounted, shallowRef } from "vue";
import { useI18n } from "vue-i18n";

import { act } from "@/api/actions";
import { ApiError } from "@/api/errors";
import { finished } from "@/api/jobs";
import type { Format } from "@/api/projects";
import { exportedOf, exportersOf, formatsOnce } from "@/features/gallery/imports";
import DownloadRow from "@/features/gallery/project/DownloadRow.vue";
import { kept } from "@/host/kept";
import Button from "@/ui/Button.vue";
import Field from "@/ui/Field.vue";
import Select from "@/ui/Select.vue";

/**
 * A record's image exported (W6.3): in a format other software reads,
 * OME-TIFF, 16-bit TIFF or PNG, or an extension's, by the
 * `projects.export` job; then its file downloaded.
 */
const props = defineProps<{
  project: string;
  record: string;
  kind: "acquisition" | "result";
}>();
const { t } = useI18n();

const exporters = shallowRef<Format[]>([]);
const why = shallowRef<string>();
const isText = (value: unknown): value is string => typeof value === "string";
const format = kept("oscope.gallery.export", "ome-tiff", isText);
onMounted(async () => {
  try {
    exporters.value = exportersOf(await formatsOnce());
    if (!exporters.value.some((one) => one.id === format.value) && exporters.value[0])
      format.value = exporters.value[0].id;
  } catch (thrown) {
    why.value = thrown instanceof ApiError ? thrown.message : String(thrown);
  }
});
const choices = computed(() => exporters.value.map((one) => ({ value: one.id, label: one.name })));

const running = shallowRef(false);
/** The file the last export wrote, within the project. */
const exported = shallowRef<string>();
async function exportIt(): Promise<void> {
  running.value = true;
  why.value = undefined;
  exported.value = undefined;
  try {
    const params = {
      project: props.project,
      record: props.record,
      kind: props.kind,
      format: format.value,
    };
    const job = await act("/api/v1/jobs/submit", { kind: "projects.export", params });
    exported.value = exportedOf(await finished(job), props.project);
  } catch (thrown) {
    why.value = thrown instanceof ApiError ? thrown.message : String(thrown);
  } finally {
    running.value = false;
  }
}
</script>

<template>
  <section class="part" aria-labelledby="export-title" data-said="export">
    <h3 id="export-title">{{ t("exports.title") }}</h3>
    <div class="row">
      <Field class="grow" :label="t('exports.format')">
        <Select v-model="format" :options="choices" compact />
      </Field>
      <Button compact :busy="running" :disabled="!choices.length" @click="exportIt">
        {{ t("exports.start") }}
      </Button>
    </div>
    <p v-if="running" class="said" role="status">{{ t("exports.running") }}</p>
    <p v-if="why" class="said failed" role="alert">{{ why }}</p>
    <ul v-if="exported" class="files" data-said="exported">
      <DownloadRow :project="project" :path="exported" />
    </ul>
  </section>
</template>

<style scoped>
.part {
  display: grid;
  gap: var(--space-2);
}

.part h3 {
  margin: 0;
  font-size: var(--font-size-body);
}

.row {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  align-items: end;
}

.grow {
  flex: 1 1 12rem;
}

.files {
  margin: 0;
  padding: 0;
  list-style: none;
}

.said {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

.said.failed {
  color: var(--danger);
}
</style>
