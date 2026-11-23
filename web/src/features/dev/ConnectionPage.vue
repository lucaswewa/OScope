<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";

import { useConnection } from "@/api/connection/connection";
import { useCanControl } from "@/api/connection/lease";
import { useSession } from "@/api/connection/session";
import GalleryHeader from "@/features/dev/GalleryHeader.vue";
import LeaseCard from "@/features/dev/LeaseCard.vue";
import Button from "@/ui/Button.vue";
import Card from "@/ui/Card.vue";
import KeyValue from "@/ui/KeyValue.vue";
import Switch from "@/ui/Switch.vue";

/**
 * The connection, the session and the control lease (W1.3): whether the
 * app has its server, who is using it, and whether it may change anything.
 */
const { t } = useI18n();
const connection = useConnection();
const session = useSession();
const canControl = useCanControl();
const lit = ref(false);

const server = computed(() => [
  { term: t("connection.state"), value: t(`connection.states.${connection.state}`) },
  { term: t("connection.version"), value: connection.serverVersion ?? t("connection.unread") },
]);
const who = computed(() => [
  { term: t("connection.session"), value: t(`connection.sessions.${session.state}`) },
  ...(session.name ? [{ term: t("connection.name"), value: session.name }] : []),
]);
</script>

<template>
  <main class="connection">
    <GalleryHeader :title="t('connection.title')" :intro="t('connection.intro')" />
    <div class="cards">
      <Card :title="t('connection.server')" :level="2">
        <KeyValue data-said="connection" :entries="server" />
      </Card>

      <Card :title="t('connection.who')" :level="2">
        <KeyValue data-said="session" :entries="who" />
        <Button v-if="session.state === 'in'" class="out" @click="session.logout()">
          {{ t("connection.logout") }}
        </Button>
      </Card>

      <Card :title="t('connection.lease.title')" :level="2">
        <p v-if="session.state === 'open'" class="note" data-said="lease">
          {{ t("connection.lease.open") }}
        </p>
        <LeaseCard v-else-if="session.state === 'in'" />
        <div class="control">
          <Switch v-model="lit" :label="t('connection.control')" :readonly="!canControl" />
          <span class="note" data-said="control">
            {{ canControl ? t("connection.may") : t("connection.mayNot") }}
          </span>
        </div>
      </Card>
    </div>
  </main>
</template>

<style scoped>
.connection {
  display: grid;
  gap: var(--space-4);
  padding: var(--space-6);
}

.cards {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(20rem, 1fr));
  gap: var(--space-6);
  align-items: start;
}

.out {
  margin-block-start: var(--space-4);
}

.control {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3);
  align-items: center;
  margin-block-start: var(--space-4);
}

.note {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
