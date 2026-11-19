<script setup lang="ts">
import { useI18n } from "vue-i18n";

import { useProperty } from "@/api/things/use";

/**
 * The sensor's temperature, read every few seconds (W3.2): drawn only for a
 * camera that says it has one, so that no other is asked.
 */
const { t } = useI18n();
const temperature = useProperty("/api/v1/camera/temperature", { every: 5_000 });
</script>

<template>
  <span v-if="temperature.value.value !== undefined" data-said="temperature">
    · {{ t("camera.temperature", { degrees: temperature.value.value.toFixed(1) }) }}
  </span>
</template>
