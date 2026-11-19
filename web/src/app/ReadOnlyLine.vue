<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { useCanControl, useLease } from "@/api/connection/lease";
import { useSession } from "@/api/connection/session";
import Banner from "@/ui/Banner.vue";
import Button from "@/ui/Button.vue";

/**
 * The read-only mode's one line (ADR-0048): while this app doesn't hold the
 * control lease, and the server has accounts, it says why every control is
 * switched off, and offers the way to control: asking whoever holds it, or
 * taking it while nobody does.
 */
const { t } = useI18n();
const session = useSession();
const lease = useLease();
const canControl = useCanControl();

const shown = computed(() => session.state === "in" && !canControl.value);
const holder = computed(() => lease.lease.value.value?.holder ?? undefined);
const asked = computed(() =>
  Boolean(session.name && lease.lease.value.value?.requests.includes(session.name)),
);
const busy = computed(() => lease.acquire.running.value || lease.request.running.value);
</script>

<template>
  <Banner v-if="shown" level="info" class="read-only" data-said="read-only">
    <template v-if="holder">
      {{
        asked
          ? t("shell.readOnly.asked", { name: holder })
          : t("shell.readOnly.held", { name: holder })
      }}
    </template>
    <template v-else>{{ t("shell.readOnly.free") }}</template>
    <template #actions>
      <Button v-if="holder" compact :busy="busy" :disabled="asked" @click="lease.request.run({})">
        {{ t("shell.readOnly.ask") }}
      </Button>
      <Button v-else compact variant="primary" :busy="busy" @click="lease.acquire.run({})">
        {{ t("shell.readOnly.take") }}
      </Button>
    </template>
  </Banner>
</template>

<style scoped>
.read-only {
  border-radius: 0;
}
</style>
