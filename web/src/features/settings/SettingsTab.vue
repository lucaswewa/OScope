<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { useDisplay } from "@/features/display/display";
import ControlScope from "@/ui/ControlScope.vue";
import Field from "@/ui/Field.vue";
import Select from "@/ui/Select.vue";
import { accents, isOneOf, themes } from "@/ui/tokens/theme";

/**
 * Settings (W8's, early): the display's theme and accent, which this
 * computer keeps. They change nothing on the instrument, so the read-only
 * mode leaves them free.
 */
const { t } = useI18n();
const display = useDisplay();

const themeChoices = computed(() =>
  themes.map((value) => ({ value, label: t(`display.themes.${value}`) })),
);
const accentChoices = computed(() =>
  accents.map((value) => ({ value, label: t(`display.accents.${value}`) })),
);

function chooseTheme(value: string | undefined): void {
  if (isOneOf(themes, value)) display.theme = value;
}

function chooseAccent(value: string | undefined): void {
  if (isOneOf(accents, value)) display.accent = value;
}
</script>

<template>
  <section class="settings">
    <h1>{{ t("tabs.settings.title") }}</h1>
    <p class="note">{{ t("settings.intro") }}</p>
    <ControlScope :locked="false">
      <div class="display">
        <h2>{{ t("settings.display") }}</h2>
        <Field :label="t('display.theme')">
          <Select
            :model-value="display.theme"
            :options="themeChoices"
            @update:model-value="chooseTheme"
          />
        </Field>
        <Field :label="t('display.accent')">
          <Select
            :model-value="display.accent"
            :options="accentChoices"
            @update:model-value="chooseAccent"
          />
        </Field>
      </div>
    </ControlScope>
  </section>
</template>

<style scoped>
.settings {
  display: grid;
  gap: var(--space-3);
  align-content: start;
  max-inline-size: 40rem;
  padding: var(--space-6);
}

.settings h1,
.settings h2 {
  margin: 0;
}

.display {
  display: grid;
  gap: var(--space-4);
  max-inline-size: 20rem;
}

.note {
  margin: 0;
  color: var(--text-muted);
}
</style>
