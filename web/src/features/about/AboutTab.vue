<script setup lang="ts">
import { useI18n } from "vue-i18n";

import { serverVersion } from "@/api/system";
import { useServerVersion } from "@/features/about/serverVersion";

/** About (W4.3's, early): the server's version, as the first page showed it (W0.1). */
const { t } = useI18n();
const server = useServerVersion(serverVersion);
</script>

<template>
  <section class="about">
    <h1>{{ t("tabs.about.title") }}</h1>
    <p role="status" :data-state="server.kind">
      <template v-if="server.kind === 'connected'">
        {{ t("home.version", { version: server.version }) }}
      </template>
      <template v-else-if="server.kind === 'unreachable'">
        {{ t("home.unreachable") }}
      </template>
      <template v-else>{{ t("home.connecting") }}</template>
    </p>
    <p class="note">{{ t("shell.comes", { phase: "W4.3" }) }}</p>
  </section>
</template>

<style scoped>
.about {
  display: grid;
  gap: var(--space-2);
  align-content: start;
  padding: var(--space-6);
}

.about h1,
.about p {
  margin: 0;
}

.about p[data-state="unreachable"] {
  color: var(--danger);
}

.note {
  color: var(--text-muted);
}
</style>
