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
/* W0.2 brings the tokens: until then, the page is plain. */
.home {
  display: grid;
  place-content: center;
  gap: 0.5rem;
  min-height: 100vh;
  margin: 0;
  font-family: system-ui, sans-serif;
  text-align: center;
}

.home h1 {
  margin: 0;
  font-size: 2rem;
  font-weight: 600;
}

.home p {
  margin: 0;
}

.home p[data-state="unreachable"] {
  color: #b3261e;
}
</style>
