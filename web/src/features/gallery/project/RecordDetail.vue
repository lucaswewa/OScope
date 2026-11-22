<script setup lang="ts">
import { computed, onScopeDispose, shallowRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import { projectThumbnail } from "@/api/projects";
import { useAction } from "@/api/things/use";
import DownloadRow from "@/features/gallery/project/DownloadRow.vue";
import ExportPane from "@/features/gallery/project/ExportPane.vue";
import {
  detailsOf,
  filesOf,
  provenanceOf,
  type AnyRecord,
  type Contents,
} from "@/features/gallery/project/project";
import Button from "@/ui/Button.vue";
import Confirm from "@/ui/Confirm.vue";
import KeyValue from "@/ui/KeyValue.vue";

/**
 * A record of a project (W6.2): its thumbnail; when it was made, and whose;
 * its image's size and levels; an acquisition's camera, objective,
 * calibration and the stage's place; a result's provenance, the recipe,
 * its nodes, and its inputs with their hashes; its files, downloaded; its
 * image exported (W6.3); and its deletion, asked first.
 */
const props = defineProps<{ contents: Contents; one: AnyRecord }>();
const emit = defineEmits<{ deleted: [] }>();
const { t } = useI18n();
const remove = useAction("/api/v1/projects/delete_record");

const thumbnail = shallowRef<string>();
watch(
  () => props.one.record.id,
  async (record) => {
    if (thumbnail.value) URL.revokeObjectURL(thumbnail.value);
    thumbnail.value = undefined;
    if (props.one.kind === "report") return;
    try {
      const picture = await projectThumbnail(props.contents.project.id, { record });
      if (picture && props.one.record.id === record) thumbnail.value = URL.createObjectURL(picture);
    } catch {
      // Said without its picture.
    }
  },
  { immediate: true },
);
onScopeDispose(() => {
  if (thumbnail.value) URL.revokeObjectURL(thumbnail.value);
});

const details = computed(() =>
  detailsOf(props.one, props.contents.project.owner).map(({ key, value }) => ({
    term: t(`projectPage.details.${key}`),
    value:
      key === "made"
        ? new Date(value).toLocaleString()
        : key === "calibration"
          ? t(`projectPage.calibrations.${value}`)
          : value,
  })),
);
const provenance = computed(() =>
  props.one.kind === "result" ? provenanceOf(props.one.record, props.contents) : undefined,
);
const files = computed(() => filesOf(props.one));

const asking = shallowRef(false);
async function deleteIt(): Promise<void> {
  const done = await remove.run({
    project: props.contents.project.id,
    kind: props.one.kind,
    record: props.one.record.id,
  });
  if (done !== undefined) emit("deleted");
}
</script>

<template>
  <article class="record" :aria-labelledby="`record-${one.record.id}`" data-said="record">
    <h2 :id="`record-${one.record.id}`">{{ one.record.name }}</h2>
    <p class="said">{{ t(`projectPage.kinds.${one.kind}`) }}</p>
    <img v-if="thumbnail" class="picture" :src="thumbnail" :alt="t('projectPage.thumbnail')" />
    <KeyValue :entries="details" data-said="details" />

    <section
      v-if="provenance"
      class="part"
      aria-labelledby="provenance-title"
      data-said="provenance"
    >
      <h3 id="provenance-title">{{ t("projectPage.provenance.title") }}</h3>
      <p v-if="provenance.recipe" class="said">
        {{ t("projectPage.provenance.recipe", { name: provenance.recipe }) }}
      </p>
      <p v-if="provenance.nodes.length" class="said">
        {{ t("projectPage.provenance.nodes", { nodes: provenance.nodes.join(", ") }) }}
      </p>
      <ul class="inputs">
        <li v-for="input in provenance.inputs" :key="input.id">
          {{ t(`projectPage.kinds.${input.kind}`) }}: {{ input.name ?? input.id }}
          <code v-if="input.hash" class="hash">{{ input.hash }}</code>
        </li>
      </ul>
    </section>

    <section class="part" aria-labelledby="files-title">
      <h3 id="files-title">{{ t("projectPage.files") }}</h3>
      <ul class="files" data-said="files">
        <DownloadRow
          v-for="path in files"
          :key="path"
          :project="contents.project.id"
          :path="path"
        />
      </ul>
    </section>

    <ExportPane
      v-if="one.kind !== 'report'"
      :project="contents.project.id"
      :record="one.record.id"
      :kind="one.kind"
    />

    <div>
      <Button compact variant="bare" tone="danger" icon="delete" @click="asking = true">
        {{ t("projectPage.delete.button") }}
      </Button>
    </div>
    <Confirm
      v-model:open="asking"
      :title="t('projectPage.delete.title', { name: one.record.name })"
      :text="t('projectPage.delete.text')"
      :confirm="t('projectPage.delete.confirm')"
      tone="danger"
      @confirm="deleteIt"
    />
  </article>
</template>

<style scoped>
.record,
.part {
  display: grid;
  gap: var(--space-2);
  align-content: start;
}

.record h2,
.part h3,
.said {
  margin: 0;
}

.picture {
  max-inline-size: 100%;
  max-block-size: 16rem;
  justify-self: start;
  border-radius: var(--radius-surface);
}

.inputs,
.files {
  display: grid;
  gap: var(--space-2);
  margin: 0;
  padding: 0;
  list-style: none;
}

.hash {
  display: block;
  overflow-wrap: anywhere;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

.said {
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
