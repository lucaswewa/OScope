<script setup lang="ts">
import { computed, ref, shallowRef } from "vue";
import { useI18n } from "vue-i18n";

import { thingStore } from "@/api/things/store";
import { useAction, useProperty } from "@/api/things/use";
import { needsOf, recipeFrom, STOCK, type Recipe } from "@/features/control/recipe/recipes";
import Button from "@/ui/Button.vue";
import Field from "@/ui/Field.vue";
import NumberField from "@/ui/NumberField.vue";
import Select from "@/ui/Select.vue";
import TextField from "@/ui/TextField.vue";

/**
 * The live recipe (W3.7): what the preview runs on each frame in place of
 * the look (P22): none, one that comes with the app, or one opened from a
 * file, until W7 keeps and edits recipes. The server checks each as it is
 * set, and says what it costs against its budget; and the references a
 * recipe reads are captured here, the camera's next frames' mean.
 */
const { t } = useI18n();
const live = useProperty("/api/v1/preview/live_recipe", { every: 5_000 });
const budget = useProperty("/api/v1/preview/recipe_budget_ms", { every: 30_000 });
const sessions = useProperty("/api/v1/preview/sessions", { every: 2_000 });
const references = useProperty("/api/v1/preview/references", { every: 10_000 });
const streaming = useProperty("/api/v1/camera/streaming", { every: 1_000 });
const set = useAction("/api/v1/preview/set_live_recipe");
const capture = useAction("/api/v1/preview/capture_reference");

/** A recipe opened from a file, and why the last one opened wasn't one. */
const opened = shallowRef<Recipe>();
const unopened = shallowRef<string>();
const file = ref<HTMLInputElement>();

/** What the preview runs now, as one of the choices: `live` for one set elsewhere. */
const current = computed(() => {
  const now = live.value.value;
  if (!now) return "none";
  const stock = STOCK.find((one) => one.recipe.name === now.name);
  if (stock) return stock.key;
  return opened.value?.name === now.name ? "file" : "live";
});
const choices = computed(() => [
  { value: "none", label: t("recipe.none") },
  ...STOCK.map((one) => ({ value: one.key, label: t(`recipe.stock.${one.key}`) })),
  ...(opened.value
    ? [{ value: "file", label: t("recipe.file", { name: opened.value.name }) }]
    : []),
  ...(current.value === "live"
    ? [{ value: "live", label: t("recipe.elsewhere", { name: live.value.value?.name ?? "" }) }]
    : []),
]);
const recipeOf = (choice: string): Recipe | undefined =>
  choice === "file" ? opened.value : STOCK.find((one) => one.key === choice)?.recipe;

/** The recipe sent last, which the server may have refused. */
const tried = shallowRef<Recipe>();
async function choose(choice: string | undefined): Promise<void> {
  if (!choice || choice === current.value || choice === "live") return;
  tried.value = recipeOf(choice);
  await set.run(tried.value ? { recipe: tried.value } : {});
  await thingStore("preview").read("live_recipe");
}

async function open(event: Event): Promise<void> {
  const chosen = (event.target as HTMLInputElement).files?.[0];
  if (!chosen) return;
  unopened.value = undefined;
  try {
    opened.value = recipeFrom(await chosen.text());
  } catch (thrown) {
    unopened.value = t("recipe.unopened", { file: chosen.name, why: (thrown as Error).message });
    return;
  } finally {
    if (file.value) file.value.value = "";
  }
  await choose("file");
}

/** What the server's check found as it was set, and what the frames take now. */
const cost = computed(() => {
  const checked = set.output.value;
  if (!checked || current.value === "none") return undefined;
  return t("recipe.checked", {
    time: checked.time_ms.toFixed(1),
    width: checked.width,
    height: checked.height,
    budget: checked.budget_ms.toFixed(0),
  });
});
const now = computed(() => {
  const times = (sessions.value.value ?? []).flatMap((one) => one.recipe_ms ?? []);
  if (!times.length || current.value === "none") return undefined;
  return t("recipe.now", {
    time: Math.max(...times).toFixed(1),
    budget: (budget.value.value ?? 0).toFixed(0),
  });
});

/** The references the recipe refused, or set, reads that haven't been captured. */
const missing = computed(() => {
  const recipe = (set.error.value ? tried.value : live.value.value) ?? undefined;
  const have = new Set(references.value.value ?? []);
  return recipe ? needsOf(recipe).filter((name) => !have.has(name)) : [];
});

const reference = shallowRef("flat");
const frames = shallowRef<number | null>(8);
const made = computed(() => {
  const got = capture.output.value;
  return got ? t("recipe.captured", { ...got }) : undefined;
});
async function captureReference(): Promise<void> {
  const name = reference.value.trim();
  if (!name) return;
  if (await capture.run({ name, ...(frames.value === null ? {} : { frames: frames.value }) })) {
    await thingStore("preview").read("references");
  }
}
</script>

<template>
  <section class="recipe" aria-labelledby="recipe-title" data-said="recipe">
    <h2 id="recipe-title">{{ t("recipe.title") }}</h2>
    <div class="row">
      <Field class="grow" :label="t('recipe.choice')" :error="set.error.value?.message">
        <Select :model-value="current" :options="choices" @update:model-value="choose" />
      </Field>
      <Button compact variant="outline" @click="file?.click()">{{ t("recipe.open") }}</Button>
      <input
        ref="file"
        class="file"
        type="file"
        accept=".json,application/json"
        tabindex="-1"
        aria-hidden="true"
        @change="open"
      />
    </div>
    <p v-if="unopened" class="said failed" data-said="unopened">{{ unopened }}</p>
    <p v-if="cost" class="said" data-said="cost">{{ cost }}</p>
    <p v-if="now" class="said" data-said="cost-now">{{ now }}</p>
    <p v-if="missing.length" class="said failed" data-said="missing">
      {{ t("recipe.missing", { names: missing.join(", ") }) }}
    </p>
    <div class="row">
      <Field class="grow" :label="t('recipe.reference')">
        <TextField v-model="reference" />
      </Field>
      <Field class="count" :label="t('recipe.frames')">
        <NumberField v-model="frames" optional :min="1" :max="64" :step="1" :digits="0" compact />
      </Field>
      <Button
        compact
        :busy="capture.running.value"
        :disabled="streaming.value.value !== true || !reference.trim()"
        @click="captureReference"
      >
        {{ t("recipe.capture") }}
      </Button>
    </div>
    <p v-if="streaming.value.value !== true" class="said">{{ t("recipe.needsStream") }}</p>
    <p v-if="made" class="said" data-said="reference">{{ made }}</p>
    <p class="said" data-said="references">
      {{
        references.value.value?.length
          ? t("recipe.references", { names: references.value.value.join(", ") })
          : t("recipe.noReferences")
      }}
    </p>
  </section>
</template>

<style scoped>
.recipe {
  display: grid;
  gap: var(--space-2);
}

.recipe h2,
.said {
  margin: 0;
}

.row {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  align-items: end;
}

.grow {
  flex: 1 1 9rem;
}

.count {
  flex: 0 1 6rem;
}

/* Opened by its button: a file input's own look can't follow the theme. */
.file {
  display: none;
}

.said {
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

.said.failed {
  color: var(--danger);
}
</style>
