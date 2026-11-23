<script setup lang="ts">
import { shallowRef } from "vue";
import { useI18n } from "vue-i18n";

import { call, server } from "@/api/client";
import { useAction } from "@/api/things/use";
import { documentName, documentOf } from "@/features/settings/settings";
import { host } from "@/host/host";
import Button from "@/ui/Button.vue";

/**
 * All the settings as one document (W8.1, P52): the system's, the
 * simulator's, the display's defaults and each account's own, exported to
 * a file, and imported from one. The server checks each part before it
 * changes any, and says what it imported.
 */
const { t } = useI18n();
const why = (thrown: unknown) => (thrown instanceof Error ? thrown.message : String(thrown));
const said = shallowRef("");
const imported = shallowRef<string[]>();
const importing = useAction("/api/v1/settings/import");

async function exportIt(): Promise<void> {
  try {
    const document = await call((signal) =>
      server.api.GET("/api/v1/settings/document", { signal }),
    );
    const text = `${JSON.stringify(document, null, 2)}\n`;
    const name = documentName(new Date());
    await host().files.save(name, new Blob([text], { type: "application/json" }));
    said.value = t("settings.documentPane.exported", { name });
  } catch (thrown) {
    said.value = t("settings.documentPane.failed", { why: why(thrown) });
  }
}

async function importIt(): Promise<void> {
  const [file] = await host().files.open({ accept: [".json", "application/json"] });
  if (!file) return;
  imported.value = undefined;
  const read = documentOf(await file.text());
  if ("why" in read) {
    said.value = t(`settings.documentPane.${read.why}`, { name: file.name });
    return;
  }
  const parts = await importing.run({ document: read.document });
  if (parts === undefined) {
    said.value = t("settings.documentPane.refused", {
      why: importing.error.value?.message ?? "",
    });
    return;
  }
  imported.value = parts;
  said.value = t("settings.documentPane.imported", { name: file.name, n: parts.length });
}
</script>

<template>
  <section class="pane" aria-labelledby="document-title" data-said="document-pane">
    <h2 id="document-title">{{ t("settings.documentPane.title") }}</h2>
    <p class="note">{{ t("settings.documentPane.note") }}</p>
    <div class="actions">
      <Button icon="download" @click="exportIt">{{ t("settings.documentPane.export") }}</Button>
      <Button icon="upload" :busy="importing.running.value" @click="importIt">
        {{ t("settings.documentPane.import") }}
      </Button>
    </div>
    <p class="said" role="status" data-said="document-said">{{ said }}</p>
    <ul v-if="imported?.length" class="parts" data-said="imported">
      <li v-for="part in imported" :key="part">{{ part }}</li>
    </ul>
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

.actions {
  display: flex;
  gap: var(--space-2);
}

.parts {
  margin: 0;
  padding-inline-start: var(--space-4);
}

.note,
.said {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
