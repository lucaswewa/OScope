<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";

import { useLease } from "@/api/connection/lease";
import Banner from "@/ui/Banner.vue";
import Button from "@/ui/Button.vue";
import Field from "@/ui/Field.vue";
import KeyValue from "@/ui/KeyValue.vue";
import Select from "@/ui/Select.vue";

/** The control lease, with a session: who holds it, who asked, and what this app may do with it. */
const { t } = useI18n();
const lease = useLease();
const to = ref<string>();
const state = computed(() => lease.lease.value.value);
const others = computed(() =>
  ((lease.accounts.value.value as string[] | undefined) ?? [])
    .filter((name) => name !== lease.me.value?.name)
    .map((name) => ({ value: name, label: name })),
);
const rows = computed(() => [
  { term: t("connection.lease.holder"), value: state.value?.holder ?? t("connection.lease.none") },
  {
    term: t("connection.lease.asked"),
    value: state.value?.requests.length
      ? state.value.requests.join(", ")
      : t("connection.lease.nobody"),
  },
]);
const busy = computed(() =>
  [lease.acquire, lease.request, lease.handOver, lease.release].some((one) => one.running.value),
);
</script>

<template>
  <div class="lease" data-said="lease">
    <KeyValue :entries="rows" />
    <Banner v-if="lease.lapsing.value" level="warning">
      {{ t("connection.lease.lapsing", { s: Math.ceil((lease.lapsesIn.value ?? 0) / 1000) }) }}
    </Banner>
    <div v-if="lease.holding.value" class="row">
      <Field :label="t('connection.lease.to')">
        <Select v-model="to" :options="others" />
      </Field>
      <Button :disabled="!to || busy" @click="to && lease.handOver.run({ to })">
        {{ t("connection.lease.handOver") }}
      </Button>
      <Button :disabled="busy" @click="lease.release.run({})">
        {{ t("connection.lease.release") }}
      </Button>
    </div>
    <Button v-else-if="state?.holder" :disabled="busy" @click="lease.request.run({})">
      {{ t("connection.lease.request") }}
    </Button>
    <Button v-else variant="primary" :disabled="busy" @click="lease.acquire.run({})">
      {{ t("connection.lease.acquire") }}
    </Button>
  </div>
</template>

<style scoped>
.lease {
  display: grid;
  gap: var(--space-3);
  justify-items: start;
}

.row {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3);
  align-items: end;
}
</style>
