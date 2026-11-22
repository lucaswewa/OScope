<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { fileAddress } from "@/api/media";
import { sizeOf } from "@/features/logging/logs";
import { useDownload } from "@/features/gallery/project/download";
import Button from "@/ui/Button.vue";
import Progress from "@/ui/Progress.vue";

/**
 * One of a record's files (W6.2): downloaded through the host adapter,
 * its progress said; stopped, by its user or a lost connection, and
 * resumed from where it was.
 */
const props = defineProps<{ project: string; path: string }>();
const { t } = useI18n();
const name = computed(() => props.path.split("/").at(-1) ?? props.path);
const download = useDownload(
  () => fileAddress(props.project, props.path),
  () => name.value,
);
const { state, done, of, why } = download;
const said = computed(() => {
  const got = t("logging.files.size", sizeOf(done.value));
  return of.value === undefined
    ? got
    : t("projectPage.download.of", { got, of: t("logging.files.size", sizeOf(of.value)) });
});
</script>

<template>
  <li class="file" :data-said="`file-${name}`">
    <span class="name">{{ name }}</span>
    <Button
      v-if="state === 'idle' || state === 'saved' || state === 'failed'"
      compact
      variant="bare"
      icon="download"
      @click="download.start"
    >
      {{ t("projectPage.download.start", { name }) }}
    </Button>
    <Button v-else-if="state === 'running'" compact variant="bare" @click="download.stop">
      {{ t("projectPage.download.stop") }}
    </Button>
    <Button v-else compact variant="bare" icon="download" @click="download.resume">
      {{ t("projectPage.download.resume") }}
    </Button>
    <Progress
      v-if="state === 'running' || state === 'stopped'"
      class="bar"
      :value="of ? done : null"
      :max="of ?? 100"
      :label="said"
    />
    <span class="said" data-said="download-said">
      <template v-if="state !== 'idle'">{{
        t(`projectPage.download.states.${state}`, { size: said })
      }}</template>
      <template v-if="why">: {{ why }}</template>
    </span>
  </li>
</template>

<style scoped>
.file {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: var(--space-1) var(--space-2);
  align-items: center;
}

.name {
  overflow-wrap: anywhere;
  font-family: var(--font-mono);
  font-size: var(--font-size-small);
}

.bar,
.said {
  grid-column: 1 / -1;
}

.said {
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
