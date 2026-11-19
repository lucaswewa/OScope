<script setup lang="ts">
import { useI18n } from "vue-i18n";

/** A tab whose stage is still to come (W2.1): its name, and the stage that fills it. */
defineProps<{
  /** The tab's name, as the rail knows it. */
  tab: string;
  /** The stage or phase that fills it. */
  comes?: string | undefined;
}>();

const { t } = useI18n();
</script>

<template>
  <section class="placeholder">
    <h1>{{ t(`tabs.${tab}.title`) }}</h1>
    <p v-if="comes" class="note">{{ t("shell.comes", { phase: comes }) }}</p>
  </section>
</template>

<style scoped>
.placeholder {
  display: grid;
  gap: var(--space-2);
  align-content: start;
  padding: var(--space-6);
}

.placeholder h1 {
  margin: 0;
}

.note {
  margin: 0;
  color: var(--text-muted);
}
</style>
