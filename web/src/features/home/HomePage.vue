<script setup lang="ts">
import { useI18n } from "vue-i18n";

import { serverVersion } from "@/api/system";
import { useServerVersion } from "@/features/home/serverVersion";

const { t } = useI18n();
const server = useServerVersion(serverVersion);
</script>

<template>
  <main class="home">
    <h1>{{ t("app.name") }}</h1>
    <p role="status" :data-state="server.kind">
      <template v-if="server.kind === 'connected'">
        {{ t("home.version", { version: server.version }) }}
      </template>
      <template v-else-if="server.kind === 'unreachable'">
        {{ t("home.unreachable") }}
      </template>
      <template v-else>{{ t("home.connecting") }}</template>
    </p>
  </main>
</template>

<style scoped>
.home {
  display: grid;
  place-content: center;
  gap: var(--space-2);
  min-block-size: 100vh;
  text-align: center;
}

.home p[data-state="unreachable"] {
  color: var(--danger);
}
</style>
