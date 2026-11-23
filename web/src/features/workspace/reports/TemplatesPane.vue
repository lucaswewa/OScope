<script setup lang="ts">
import { computed, onBeforeUnmount, shallowRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import { deleteTemplate, previewTemplate, saveTemplate, templateNamed } from "@/api/reports";
import type { Contents } from "@/features/gallery/project/project";
import Button from "@/ui/Button.vue";
import Confirm from "@/ui/Confirm.vue";
import Field from "@/ui/Field.vue";
import Select from "@/ui/Select.vue";
import TextArea from "@/ui/TextArea.vue";
import TextField from "@/ui/TextField.vue";
import ToggleGroup from "@/ui/ToggleGroup.vue";

/**
 * The report templates (W7.9, P47): one's Typst, for PDF, and its HTML,
 * edited beside a preview made from a project's result or the sample
 * report, which keeps nothing; saved under a name of the user's, once the
 * server has rendered the sample through it; and deleted.
 */
const props = defineProps<{
  project?: string | undefined;
  contents?: Contents | undefined;
  /** The templates' names. */
  names: readonly string[];
}>();
/** A template was saved or deleted: the names are to be listed again. */
const emit = defineEmits<{ changed: [] }>();
const { t } = useI18n();
const why = (thrown: unknown) => (thrown instanceof Error ? thrown.message : String(thrown));
const said = shallowRef("");
const wrong = shallowRef("");

const chosen = shallowRef("standard");
const builtin = shallowRef(true);
const name = shallowRef("");
const typst = shallowRef("");
const html = shallowRef("");
watch(
  chosen,
  async (now) => {
    try {
      const read = await templateNamed(now);
      if (now !== chosen.value) return;
      [typst.value, html.value, builtin.value] = [read.typst, read.html, read.builtin];
      // OScope's own can't be changed: it is saved under a name of the user's.
      name.value = read.builtin ? "" : read.name;
      wrong.value = "";
    } catch (thrown) {
      said.value = t("workspace.reports.unread", { why: why(thrown) });
    }
  },
  { immediate: true },
);

// The preview: on a result of the project's, or the sample report.
const on = shallowRef("sample");
const ons = computed(() => [
  { value: "sample", label: t("workspace.reports.sample") },
  ...(props.contents?.results ?? []).map((one) => ({ value: one.id, label: one.name })),
]);
watch(
  () => props.contents,
  () => {
    if (!ons.value.some((one) => one.value === on.value)) on.value = "sample";
  },
);
const shown = shallowRef<{ pdf: string; html: string }>();
const form = shallowRef("html");
const forget = () => shown.value && URL.revokeObjectURL(shown.value.pdf);
onBeforeUnmount(forget);
const busy = shallowRef("");
async function preview(): Promise<void> {
  busy.value = "preview";
  try {
    const result = on.value === "sample" ? undefined : on.value;
    const where = props.project && result ? { project: props.project, result } : undefined;
    const made = await previewTemplate({ typst: typst.value, html: html.value }, where);
    forget();
    const pdf = new Blob([made.pdf], { type: "application/pdf" });
    shown.value = { pdf: URL.createObjectURL(pdf), html: made.html };
    wrong.value = "";
    said.value = t("workspace.reports.previewed");
  } catch (thrown) {
    wrong.value = why(thrown);
  } finally {
    busy.value = "";
  }
}
async function save(): Promise<void> {
  busy.value = "save";
  try {
    await saveTemplate({ name: name.value, typst: typst.value, html: html.value });
    emit("changed");
    chosen.value = name.value;
    wrong.value = "";
    said.value = t("workspace.reports.saved", { name: name.value });
  } catch (thrown) {
    wrong.value = why(thrown);
  } finally {
    busy.value = "";
  }
}
const asking = shallowRef(false);
async function remove(): Promise<void> {
  const gone = chosen.value;
  try {
    await deleteTemplate(gone);
    emit("changed");
    chosen.value = "standard";
    said.value = t("workspace.reports.deleted", { name: gone });
  } catch (thrown) {
    wrong.value = why(thrown);
  }
}
</script>

<template>
  <div class="templates-pane" data-said="templates">
    <div class="sources">
      <div class="row">
        <Field :label="t('workspace.reports.template')">
          <Select v-model="chosen" :options="names.map((one) => ({ value: one, label: one }))" />
        </Field>
        <Field
          :label="t('workspace.reports.name')"
          :hint="
            builtin
              ? `${t('workspace.reports.builtin')} ${t('workspace.reports.nameRule')}`
              : t('workspace.reports.nameRule')
          "
        >
          <TextField v-model="name" compact />
        </Field>
      </div>
      <Field class="source" :label="t('workspace.reports.typst')">
        <TextArea v-model="typst" :rows="14" />
      </Field>
      <Field class="source" :label="t('workspace.reports.html')">
        <TextArea v-model="html" :rows="14" />
      </Field>
      <p v-if="wrong" class="wrong" role="alert" data-said="template-wrong">{{ wrong }}</p>
      <div class="row">
        <Field :label="t('workspace.reports.on')">
          <Select v-model="on" :options="ons" />
        </Field>
        <Button :busy="busy === 'preview'" @click="preview">{{
          t("workspace.reports.preview")
        }}</Button>
        <Button variant="primary" :busy="busy === 'save'" :disabled="!name" @click="save">{{
          t("workspace.reports.save")
        }}</Button>
        <Button v-if="!builtin" tone="danger" @click="asking = true">{{
          t("workspace.reports.delete")
        }}</Button>
      </div>
      <p class="said" role="status" data-said="templates-said">{{ said }}</p>
    </div>
    <div class="preview" data-said="preview">
      <template v-if="shown">
        <ToggleGroup
          v-model="form"
          required
          compact
          :label="t('workspace.reports.form')"
          :options="[
            { value: 'html', label: 'HTML' },
            { value: 'pdf', label: 'PDF' },
          ]"
        />
        <iframe
          v-if="form === 'html'"
          class="frame"
          sandbox=""
          :srcdoc="shown.html"
          :title="t('workspace.reports.previewHtml')"
          data-said="preview-html"
        />
        <object
          v-else
          class="frame"
          type="application/pdf"
          :data="shown.pdf"
          :aria-label="t('workspace.reports.previewPdf')"
          data-said="preview-pdf"
        >
          <p class="said">{{ t("workspace.reports.noViewer") }}</p>
        </object>
      </template>
      <p v-else class="said">{{ t("workspace.reports.notYet") }}</p>
    </div>
    <Confirm
      v-model:open="asking"
      tone="danger"
      :title="t('workspace.reports.deleteTitle', { name: chosen })"
      :text="t('workspace.reports.deleteText')"
      :confirm="t('workspace.reports.delete')"
      @confirm="remove"
    />
  </div>
</template>

<style scoped>
.templates-pane {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  gap: var(--space-3);
  align-items: start;
}

.sources,
.preview {
  display: grid;
  gap: var(--space-2);
  align-content: start;
}

.row {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2) var(--space-3);
  align-items: end;
}

.source :deep(textarea) {
  font-family: var(--font-mono);
  font-size: var(--font-size-small);
}

.frame {
  inline-size: 100%;
  block-size: 40rem;
  border: 1px solid var(--border);
  background: var(--surface-raised);
}

.wrong {
  margin: 0;
  color: var(--danger);
  font-size: var(--font-size-small);
  white-space: pre-wrap;
}

.said {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
