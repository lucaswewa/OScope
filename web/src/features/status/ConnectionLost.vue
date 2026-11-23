<script setup lang="ts">
import { useI18n } from "vue-i18n";

import { APP_VERSION, useConnection } from "@/api/connection/connection";
import Spinner from "@/ui/Spinner.vue";

/**
 * The page in place of the tabs while there is no server (W2.2): looking
 * for it, lost and looking again, refused, or of another version. The tab
 * comes back as it was when the server does.
 */
const { t } = useI18n();
const connection = useConnection();
</script>

<template>
  <section class="lost" data-said="no-server">
    <h1>{{ t(`status.lost.${connection.state}.title`) }}</h1>
    <p>
      {{
        t(`status.lost.${connection.state}.said`, {
          server: connection.serverVersion ?? "",
          app: APP_VERSION,
        })
      }}
    </p>
    <p
      v-if="connection.state === 'connecting' || connection.state === 'reconnecting'"
      class="looking"
    >
      <Spinner />
      {{ t(`status.lost.${connection.state}.looking`) }}
    </p>
  </section>
</template>

<style scoped>
.lost {
  display: grid;
  gap: var(--space-3);
  place-content: center;
  justify-items: center;
  flex: 1;
  padding: var(--space-6);
  text-align: center;
}

.lost h1,
.lost p {
  margin: 0;
}

.looking {
  display: flex;
  gap: var(--space-2);
  align-items: center;
  color: var(--text-muted);
}
</style>
