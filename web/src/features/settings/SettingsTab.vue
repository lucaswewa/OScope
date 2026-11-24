<script setup lang="ts">
import { shallowRef } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute } from "vue-router";

import AccountsPane from "@/features/settings/accounts/AccountsPane.vue";
import CalibrationPane from "@/features/settings/calibration/CalibrationPane.vue";
import DevicesPane from "@/features/settings/DevicesPane.vue";
import DisplayPane from "@/features/settings/DisplayPane.vue";
import DocumentPane from "@/features/settings/DocumentPane.vue";
import ExtensionsPane from "@/features/settings/extensions/ExtensionsPane.vue";
import SimulationPane from "@/features/settings/simulation/SimulationPane.vue";
import SystemPane from "@/features/settings/SystemPane.vue";
import ControlScope from "@/ui/ControlScope.vue";
import Tabs from "@/ui/Tabs.vue";

/**
 * Settings (W8.1, P52): how things are shown, the account's own, which
 * need no control lease; the system's, for the next start; the devices'
 * saved settings; the calibration (W8.2); the simulator (W8.4); the
 * extensions (W8.5); the accounts (W8.6); and all of them as one
 * document. `?view=` opens one.
 */
const { t } = useI18n();
const query = useRoute().query.view;
const tab = shallowRef(typeof query === "string" ? query : "display");
</script>

<template>
  <section class="settings">
    <h1>{{ t("tabs.settings.title") }}</h1>
    <!-- Only the view shown is mounted: the devices' settings, read live, open their Things' sockets only while seen. -->
    <Tabs
      v-model="tab"
      :label="t('settings.tabs')"
      :tabs="[
        { value: 'display', label: t('settings.displayPane.title') },
        { value: 'system', label: t('settings.systemPane.title') },
        { value: 'devices', label: t('settings.devicesPane.title') },
        { value: 'calibration', label: t('settings.calibration.title') },
        { value: 'simulation', label: t('settings.simulation.title') },
        { value: 'extensions', label: t('settings.extensions.title') },
        { value: 'accounts', label: t('settings.accounts.title') },
        { value: 'document', label: t('settings.documentPane.title') },
      ]"
    >
      <template #display>
        <ControlScope :locked="false">
          <DisplayPane />
        </ControlScope>
      </template>
      <template #system>
        <SystemPane />
      </template>
      <template #devices>
        <DevicesPane />
      </template>
      <template #calibration>
        <CalibrationPane />
      </template>
      <template #simulation>
        <SimulationPane />
      </template>
      <template #extensions>
        <ExtensionsPane />
      </template>
      <template #accounts>
        <AccountsPane />
      </template>
      <template #document>
        <DocumentPane />
      </template>
    </Tabs>
  </section>
</template>

<style scoped>
.settings {
  display: grid;
  gap: var(--space-2);
  align-content: start;
  padding: var(--space-3);
}

.settings h1 {
  margin: 0;
}
</style>
