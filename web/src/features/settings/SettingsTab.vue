<script setup lang="ts">
import { shallowRef } from "vue";
import { useI18n } from "vue-i18n";

import DevicesPane from "@/features/settings/DevicesPane.vue";
import DisplayPane from "@/features/settings/DisplayPane.vue";
import DocumentPane from "@/features/settings/DocumentPane.vue";
import SystemPane from "@/features/settings/SystemPane.vue";
import ControlScope from "@/ui/ControlScope.vue";
import Tabs from "@/ui/Tabs.vue";

/**
 * Settings (W8.1, P52): how things are shown, the account's own, which
 * need no control lease; the system's, for the next start; the devices'
 * saved settings; and all of them as one document.
 */
const { t } = useI18n();
const tab = shallowRef("display");
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
