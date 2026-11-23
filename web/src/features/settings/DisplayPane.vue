<script setup lang="ts">
import { computed, shallowRef } from "vue";
import { useI18n } from "vue-i18n";

import { call, server, type Schema } from "@/api/client";
import { useAccountDisplay } from "@/features/display/account";
import { useDisplay } from "@/features/display/display";
import Button from "@/ui/Button.vue";
import Field from "@/ui/Field.vue";
import Select from "@/ui/Select.vue";
import Switch from "@/ui/Switch.vue";
import { accents, isOneOf, themes } from "@/ui/tokens/theme";

type Display = Schema<"settings_display_value">;

/**
 * How things are shown (W8.1): the theme, the units, the scale bar and the
 * colour map, the account's own, kept by the server and followed in each of
 * its browsers; and the accent, this computer's. One's own needs no
 * control lease.
 */
const { t } = useI18n();
const why = (thrown: unknown) => (thrown instanceof Error ? thrown.message : String(thrown));
const account = useAccountDisplay();
const display = useDisplay();
const said = shallowRef("");

const choices = <T extends string>(values: readonly T[], words: (value: T) => string) =>
  values.map((value) => ({ value, label: words(value) }));
const themeChoices = computed(() => choices(themes, (one) => t(`display.themes.${one}`)));
const accentChoices = computed(() => choices(accents, (one) => t(`display.accents.${one}`)));
const unitChoices = computed(() =>
  choices(["um", "mm"] as const, (one) => t(`stage.units.${one}`)),
);
const mapChoices = computed(() =>
  choices(["grey", "viridis", "magma", "inferno"] as const, (one) => t(`live.values.maps.${one}`)),
);

/** The account's preferences changed by `change`: the theme reaches this page at once. */
async function write(change: Partial<Display>): Promise<void> {
  const now = account.display;
  if (!now) return;
  if (change.theme) display.theme = change.theme;
  await account.write({ ...now, ...change });
  said.value = account.refused
    ? t("settings.displayPane.refused", { why: account.refused.message })
    : t("settings.displayPane.saved");
}
function chooseTheme(value: string | undefined): void {
  if (isOneOf(themes, value)) void write({ theme: value });
}
function chooseAccent(value: string | undefined): void {
  if (isOneOf(accents, value)) display.accent = value;
}
/** One's own preferences forgotten: the defaults again. */
async function reset(): Promise<void> {
  try {
    await call((signal) => server.api.POST("/api/v1/settings/display/reset", { signal }));
    said.value = t("settings.displayPane.reset");
  } catch (thrown) {
    said.value = t("settings.displayPane.refused", { why: why(thrown) });
  }
}
</script>

<template>
  <section class="pane" aria-labelledby="display-title" data-said="display-pane">
    <h2 id="display-title">{{ t("settings.displayPane.title") }}</h2>
    <p class="note">{{ t("settings.displayPane.note") }}</p>
    <div class="fields">
      <Field :label="t('display.theme')">
        <Select
          :model-value="display.theme"
          :options="themeChoices"
          @update:model-value="chooseTheme"
        />
      </Field>
      <Field :label="t('settings.displayPane.units')">
        <Select
          :model-value="account.display?.units"
          :options="unitChoices"
          :disabled="!account.display"
          @update:model-value="(units) => units && write({ units: units as Display['units'] })"
        />
      </Field>
      <Field :label="t('settings.displayPane.map')" :hint="t('settings.displayPane.mapHint')">
        <Select
          :model-value="account.display?.colormap"
          :options="mapChoices"
          :disabled="!account.display"
          @update:model-value="(map) => map && write({ colormap: map as Display['colormap'] })"
        />
      </Field>
      <Field :label="t('settings.displayPane.scaleBar')">
        <Switch
          :model-value="account.display?.scale_bar ?? true"
          :disabled="!account.display"
          @update:model-value="(on: boolean) => write({ scale_bar: on })"
        />
      </Field>
    </div>
    <Button compact @click="reset">{{ t("settings.displayPane.defaults") }}</Button>
    <h3>{{ t("settings.displayPane.computer") }}</h3>
    <Field :label="t('display.accent')" :hint="t('settings.displayPane.accentHint')">
      <Select
        :model-value="display.accent"
        :options="accentChoices"
        @update:model-value="chooseAccent"
      />
    </Field>
    <p class="said" role="status" data-said="display-said">{{ said }}</p>
  </section>
</template>

<style scoped>
.pane {
  display: grid;
  gap: var(--space-2);
  align-content: start;
  justify-items: start;
}

h2,
h3 {
  margin: 0;
  font-size: var(--font-size-body);
}

.fields {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2) var(--space-3);
  align-items: end;
}

.note,
.said {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
