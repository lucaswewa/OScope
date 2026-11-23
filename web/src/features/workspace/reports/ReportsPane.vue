<script setup lang="ts">
import { computed, shallowRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import { finished } from "@/api/jobs";
import { fileAddress } from "@/api/media";
import { makeReport, type Format } from "@/api/reports";
import type { Contents, Result } from "@/features/gallery/project/project";
import DownloadRow from "@/features/gallery/project/DownloadRow.vue";
import { everyOf, keptOf, madeReport, makingOf } from "@/features/workspace/reports/choice";
import Button from "@/ui/Button.vue";
import Checkbox from "@/ui/Checkbox.vue";
import Field from "@/ui/Field.vue";
import Select from "@/ui/Select.vue";
import ToggleGroup from "@/ui/ToggleGroup.vue";

/**
 * A report made (W7.9, P47): the project's results chosen, a template and
 * its forms, a job followed to its end; and the project's reports read,
 * the PDF and the HTML each in a frame, the HTML's sandboxed, and their
 * files downloaded.
 */
const props = defineProps<{
  project?: string | undefined;
  contents?: Contents | undefined;
  /** The templates' names. */
  names: readonly string[];
}>();
const emit = defineEmits<{ made: [] }>();
const { t } = useI18n();
const why = (thrown: unknown) => (thrown instanceof Error ? thrown.message : String(thrown));
const said = shallowRef("");

const template = shallowRef("standard");
watch(
  () => props.names,
  (now) => {
    if (!now.includes(template.value)) template.value = "standard";
  },
);

// The results ticked: every one at first, kept as the project's change.
const results = computed<Result[]>(() => props.contents?.results ?? []);
const ticked = shallowRef(new Set<string>());
watch(results, (now, before) => {
  ticked.value = before?.length ? keptOf(ticked.value, before, now) : everyOf(now);
});
function tick(id: string, on: boolean): void {
  const next = new Set(ticked.value);
  if (on) next.add(id);
  else next.delete(id);
  ticked.value = next;
}
const pdf = shallowRef(true);
const html = shallowRef(true);
const making = computed(() => {
  const formats = [pdf.value && "pdf", html.value && "html"].filter(Boolean) as Format[];
  return makingOf(results.value, ticked.value, template.value, formats);
});

// The reports, the newest first; the one read.
const reports = computed(() =>
  [...(props.contents?.reports ?? [])].sort((a, b) => b.created.localeCompare(a.created)),
);
const chosen = shallowRef<string>();
const report = computed(() => reports.value.find((one) => one.id === chosen.value));
watch(reports, (now) => {
  if (!now.some((one) => one.id === chosen.value)) chosen.value = now[0]?.id;
});
const form = shallowRef<Format>("html");
const forms = computed(() =>
  (["html", "pdf"] as const).filter((one) => report.value?.files.includes(`report.${one}`)),
);
watch(forms, (now) => {
  if (!now.includes(form.value) && now[0]) form.value = now[0];
});
const address = (file: string) =>
  props.project && report.value
    ? fileAddress(props.project, `reports/${report.value.id}/${file}`)
    : undefined;

const running = shallowRef(false);
async function make(): Promise<void> {
  const asked = making.value;
  if (!props.project || "why" in asked) return;
  running.value = true;
  said.value = t("workspace.reports.making");
  try {
    const job = await makeReport(props.project, asked);
    const ended = await finished(job, {
      seen: (now) =>
        (said.value = t("workspace.run.progress", {
          percent: Math.round(100 * (now.progress.fraction ?? 0)),
        })),
    });
    chosen.value = madeReport(ended.results);
    said.value = t("workspace.reports.madeOne");
    emit("made");
  } catch (thrown) {
    said.value = t("workspace.reports.failed", { why: why(thrown) });
  } finally {
    running.value = false;
  }
}
</script>

<template>
  <div class="reports-pane" data-said="reports-pane">
    <form class="make" data-said="make" @submit.prevent="make">
      <h3>{{ t("workspace.reports.make") }}</h3>
      <fieldset class="results">
        <legend>{{ t("workspace.reports.results") }}</legend>
        <p v-if="!results.length" class="said">{{ t("workspace.reports.noResults") }}</p>
        <Checkbox
          v-for="one in results"
          :key="one.id"
          :model-value="ticked.has(one.id)"
          @update:model-value="(on: boolean) => tick(one.id, on)"
          >{{ one.name }}</Checkbox
        >
      </fieldset>
      <Field :label="t('workspace.reports.template')">
        <Select v-model="template" :options="names.map((name) => ({ value: name, label: name }))" />
      </Field>
      <fieldset class="formats">
        <legend>{{ t("workspace.reports.formats") }}</legend>
        <Checkbox v-model="pdf">PDF</Checkbox>
        <Checkbox v-model="html">HTML</Checkbox>
      </fieldset>
      <p v-if="project && 'why' in making" class="said" data-said="cannot">
        {{ t(`workspace.reports.${making.why}`) }}
      </p>
      <Button
        type="submit"
        variant="primary"
        :busy="running"
        :disabled="!project || 'why' in making"
      >
        {{ t("workspace.reports.makeIt") }}
      </Button>
      <p class="said" role="status" data-said="making">{{ said }}</p>
    </form>
    <div class="read" data-said="read">
      <Field :label="t('workspace.reports.report')">
        <Select
          v-model="chosen"
          :options="reports.map((one) => ({ value: one.id, label: one.name }))"
          :placeholder="t('workspace.reports.none')"
        />
      </Field>
      <template v-if="report">
        <ToggleGroup
          v-if="forms.length > 1"
          v-model="form"
          required
          compact
          :label="t('workspace.reports.form')"
          :options="forms.map((one) => ({ value: one, label: one.toUpperCase() }))"
        />
        <!-- A PDF in the browser's own viewer: where there is none, its download. -->
        <object
          v-if="form === 'pdf' && forms.includes('pdf')"
          :key="`pdf-${report.id}`"
          class="frame"
          type="application/pdf"
          :data="address('report.pdf')"
          :aria-label="t('workspace.reports.pdfLabel', { name: report.name })"
          data-said="report-pdf"
        >
          <p class="said">{{ t("workspace.reports.noViewer") }}</p>
        </object>
        <iframe
          v-else-if="forms.includes('html')"
          :key="`html-${report.id}`"
          class="frame"
          sandbox=""
          :src="address('report.html')"
          :title="t('workspace.reports.htmlLabel', { name: report.name })"
          data-said="report-html"
        />
        <ul class="files">
          <DownloadRow
            v-for="file in report.files"
            :key="file"
            :project="project!"
            :path="`reports/${report.id}/${file}`"
          />
        </ul>
      </template>
    </div>
  </div>
</template>

<style scoped>
.reports-pane {
  display: grid;
  grid-template-columns: minmax(16rem, 1fr) minmax(0, 3fr);
  gap: var(--space-3);
  align-items: start;
}

.make,
.read {
  display: grid;
  gap: var(--space-2);
  align-content: start;
}

h3 {
  margin: 0;
  font-size: var(--font-size-body);
}

fieldset {
  display: grid;
  gap: var(--space-1);
  margin: 0;
  padding: var(--space-2);
  border: 1px solid var(--border);
}

legend {
  padding: 0 var(--space-1);
  font-weight: 600;
}

.frame {
  inline-size: 100%;
  block-size: 34rem;
  border: 1px solid var(--border);
  background: var(--surface-raised);
}

.files {
  display: grid;
  gap: var(--space-1);
  margin: 0;
  padding: 0;
  list-style: none;
}

.said {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
