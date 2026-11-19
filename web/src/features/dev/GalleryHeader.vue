<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { useDisplay } from "@/features/display/display";
import Field from "@/ui/Field.vue";
import Select from "@/ui/Select.vue";
import { accents, isOneOf, themes } from "@/ui/tokens/theme";

/**
 * The top of each of the gallery's pages: the way to the others, the page's
 * title, and the theme and the accent, so that a page is seen in each.
 */
defineProps<{ title: string; intro: string }>();

const { t } = useI18n();
const display = useDisplay();

const pages = [
  { to: "/dev/gallery", name: "tokens" },
  { to: "/dev/gallery/inputs", name: "inputs" },
  { to: "/dev/gallery/overlays", name: "overlays" },
  { to: "/dev/gallery/data", name: "data" },
  { to: "/dev/gallery/forms", name: "forms" },
  { to: "/dev/gallery/charts", name: "charts" },
  { to: "/dev/gallery/api", name: "api" },
  { to: "/dev/gallery/things", name: "things" },
  { to: "/dev/gallery/connection", name: "connection" },
  { to: "/dev/gallery/host", name: "host" },
] as const;
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
  <header class="header">
    <nav class="pages" :aria-label="t('gallery.pages')">
      <RouterLink v-for="page in pages" :key="page.to" :to="page.to">
        {{ t(`gallery.${page.name}`) }}
      </RouterLink>
    </nav>
    <h1>{{ title }}</h1>
    <p>{{ intro }}</p>
    <div class="choices">
      <Field class="choice" :label="t('display.theme')">
        <Select
          :model-value="display.theme"
          :options="themeChoices"
          compact
          @update:model-value="chooseTheme"
        />
      </Field>
      <Field class="choice" :label="t('display.accent')">
        <Select
          :model-value="display.accent"
          :options="accentChoices"
          compact
          @update:model-value="chooseAccent"
        />
      </Field>
      <p role="status" :data-scheme="display.scheme">
        {{ t("gallery.scheme", { scheme: t(`gallery.schemes.${display.scheme}`) }) }}
      </p>
    </div>
  </header>
</template>

<style scoped>
.header {
  display: grid;
  gap: var(--space-3);
}

.pages {
  display: flex;
  gap: var(--space-4);
}

.pages a[aria-current="page"] {
  color: var(--text);
  font-weight: var(--font-weight-strong);
  text-decoration: none;
}

.choices {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-4);
  align-items: end;
}

.choice {
  inline-size: 14rem;
}
</style>
