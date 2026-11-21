<script setup lang="ts">
import { computed, onScopeDispose, shallowRef } from "vue";
import { useI18n } from "vue-i18n";

import { download } from "@/api/media";
import { useProperty } from "@/api/things/use";
import { useJobAction } from "@/features/control/jobAction";
import ProjectFields from "@/features/projects/ProjectFields.vue";
import { useProjectChoice } from "@/features/projects/projectChoice";
import Button from "@/ui/Button.vue";
import Field from "@/ui/Field.vue";
import NumberField from "@/ui/NumberField.vue";
import Progress from "@/ui/Progress.vue";
import Select from "@/ui/Select.vue";
import TextField from "@/ui/TextField.vue";

/**
 * Capture (W3.7): the camera's next frame, or a burst of them, kept as an
 * acquisition of a project (P28, P29): one chosen, kept between visits, or
 * made here; named; with a rendition, PNG or JPEG, if asked. A job,
 * followed: the record it made is shown, its thumbnail, with a link to it
 * in the gallery, which W6 fills.
 */
const { t } = useI18n();
const streaming = useProperty("/api/v1/camera/streaming", { every: 1_000 });
const single = useJobAction("/api/v1/capture/single");
const burst = useJobAction("/api/v1/capture/burst");

const choice = useProjectChoice("oscope.control.capture.project");
const { project, projects, chosen, list } = choice;

const name = shallowRef("");
const frames = shallowRef<number | null>(1);
const rendition = shallowRef<"none" | "png" | "jpeg">("none");
const renditions = computed(() =>
  (["none", "png", "jpeg"] as const).map((value) => ({
    value,
    label: t(`capture.renditions.${value}`),
  })),
);

/** The job of the last capture: one frame's, or a burst's. */
const one = shallowRef(true);
const action = computed(() => (one.value ? single : burst));
const running = computed(() => single.running.value || burst.running.value);

/** The record the last capture made, and its thumbnail's address, once fetched. */
const captured = shallowRef<{ project: string; record: string; thumbnail?: string }>();
function forget(): void {
  if (captured.value?.thumbnail) URL.revokeObjectURL(captured.value.thumbnail);
  captured.value = undefined;
}
onScopeDispose(forget);

async function capture(): Promise<void> {
  const into = project.value;
  const count = frames.value ?? 1;
  const common = {
    project: into,
    ...(name.value.trim() ? { name: name.value.trim() } : {}),
    ...(rendition.value === "none" ? {} : { rendition: rendition.value }),
  };
  forget();
  one.value = count === 1;
  const ended = one.value
    ? await single.run(common)
    : await burst.run({ ...common, frames: count });
  const record = ended?.results[0]?.split("/").at(-1);
  if (record) await show(into, record);
  await list();
}

/** The record made, then its thumbnail: a convenience, which the record doesn't need. */
async function show(into: string, record: string): Promise<void> {
  captured.value = { project: into, record };
  try {
    const got = await download(into, `acquisitions/${record}/thumbnail.jpg`);
    const picture = new Blob([got.bytes], { type: got.type || "image/jpeg" });
    if (captured.value?.record === record) {
      captured.value = { project: into, record, thumbnail: URL.createObjectURL(picture) };
    }
  } catch {
    // Without its thumbnail, the record is said, and linked, all the same.
  }
}
const madeSaid = computed(() => {
  const made = captured.value;
  const into = projects.value.find((one) => one.id === made?.project);
  return made ? t("capture.made", { project: into?.name ?? made.project }) : undefined;
});
</script>

<template>
  <section class="capture" aria-labelledby="capture-title" data-said="capture">
    <h2 id="capture-title">{{ t("capture.title") }}</h2>
    <ProjectFields :choice="choice" />
    <Field :label="t('capture.name')">
      <TextField
        v-model="name"
        :placeholder="(frames ?? 1) > 1 ? t('capture.burstName', { frames }) : t('capture.oneName')"
      />
    </Field>
    <div class="row">
      <Field class="grow" :label="t('capture.frames')">
        <NumberField v-model="frames" :min="1" :max="10000" :step="1" :digits="0" compact />
      </Field>
      <Field class="grow" :label="t('capture.rendition')">
        <Select v-model="rendition" :options="renditions" compact />
      </Field>
    </div>
    <Button
      :busy="running"
      :disabled="streaming.value.value !== true || !chosen"
      data-said="capture-run"
      @click="capture"
    >
      {{ t("capture.run") }}
    </Button>
    <p v-if="streaming.value.value !== true" class="said">{{ t("capture.needsStream") }}</p>
    <p v-else-if="!chosen" class="said">{{ t("capture.needsProject") }}</p>
    <Progress
      v-if="running"
      :value="action.job.value?.progress.fraction ?? null"
      :max="1"
      :label="action.job.value?.progress.stage || t('capture.running')"
    />
    <p v-if="action.failed.value" class="said failed" data-said="captured">
      {{ action.failed.value }}
    </p>
    <figure v-else-if="captured" class="record" data-said="captured">
      <img
        v-if="captured.thumbnail"
        :src="captured.thumbnail"
        :alt="t('capture.thumbnail')"
        data-said="thumbnail"
      />
      <figcaption class="said">
        {{ madeSaid }}
        <RouterLink
          :to="{
            name: 'gallery',
            query: { project: captured.project, record: captured.record },
          }"
        >
          {{ t("capture.open") }}
        </RouterLink>
      </figcaption>
    </figure>
  </section>
</template>

<style scoped>
.capture {
  display: grid;
  gap: var(--space-2);
}

.capture h2,
.said,
.record {
  margin: 0;
}

.row {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  align-items: end;
}

.grow {
  flex: 1 1 9rem;
}

.record {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  gap: var(--space-2);
  align-items: center;
}

/* A box of its own, whatever the frame's shape: the pane doesn't move. */
.record img {
  inline-size: 6rem;
  block-size: 4.5rem;
  object-fit: contain;
  border-radius: var(--radius-control);
}

.said {
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

.said.failed {
  color: var(--danger);
}
</style>
