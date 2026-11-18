<script setup lang="ts">
import { useI18n } from "vue-i18n";

import { useDisplay } from "@/features/display/display";
import Icon from "@/ui/Icon.vue";
import { icons, type IconName } from "@/ui/icons";
import { pairs } from "@/ui/tokens/pairs";
import { accents, themes } from "@/ui/tokens/theme";
import sheet from "@/ui/tokens/tokens.css?raw";

/**
 * The tokens, shown (ADR-0041): every one the stylesheet names, found by
 * reading it, so that a new token appears here without being listed.
 */
const { t } = useI18n();
const display = useDisplay();

const names = [...new Set([...sheet.matchAll(/^\s*--([a-z0-9-]+):/gm)].map((found) => found[1]!))];
const named = (...starts: string[]): string[] =>
  names.filter((name) => starts.some((start) => name === start || name.startsWith(`${start}-`)));

const surfaces = named("surface");
const colours = named("text", "border", "accent", "info", "success", "warning", "danger");
const sizes = named("font-size");
const weights = named("font-weight");
const spaces = named("space");
const lengths = named("size");
const corners = named("radius");
const shadows = named("shadow").filter((name) => name !== "shadow-color");
const drawn = Object.keys(icons) as IconName[];
</script>

<template>
  <main class="gallery">
    <header>
      <h1>{{ t("gallery.title") }}</h1>
      <p>{{ t("gallery.intro") }}</p>
      <div class="choices">
        <label>
          {{ t("display.theme") }}
          <select v-model="display.theme">
            <option v-for="theme in themes" :key="theme" :value="theme">
              {{ t(`display.themes.${theme}`) }}
            </option>
          </select>
        </label>
        <label>
          {{ t("display.accent") }}
          <select v-model="display.accent">
            <option v-for="accent in accents" :key="accent" :value="accent">
              {{ t(`display.accents.${accent}`) }}
            </option>
          </select>
        </label>
        <p role="status" :data-scheme="display.scheme">
          {{ t("gallery.scheme", { scheme: t(`gallery.schemes.${display.scheme}`) }) }}
        </p>
      </div>
    </header>

    <section>
      <h2>{{ t("gallery.surfaces") }}</h2>
      <ul class="swatches">
        <li v-for="name in surfaces" :key="name">
          <span class="swatch wide" :data-token="name" :style="{ background: `var(--${name})` }" />
          <code>{{ name }}</code>
        </li>
      </ul>
    </section>

    <section>
      <h2>{{ t("gallery.colours") }}</h2>
      <ul class="swatches">
        <li v-for="name in colours" :key="name">
          <span class="swatch" :data-token="name" :style="{ background: `var(--${name})` }" />
          <code>{{ name }}</code>
        </li>
      </ul>
    </section>

    <section>
      <h2>{{ t("gallery.pairs") }}</h2>
      <p>{{ t("gallery.pairsIntro") }}</p>
      <ul class="pairs">
        <li v-for="pair in pairs" :key="`${pair.ink} ${pair.on}`">
          <span
            class="pair"
            :class="{ line: pair.least === 3 }"
            :style="{ color: `var(--${pair.ink})`, background: `var(--${pair.on})` }"
          >
            <template v-if="pair.least !== 3">{{ t("gallery.sample") }}</template>
          </span>
          <code>{{ pair.ink }}</code>
          <code>{{ pair.on }}</code>
        </li>
      </ul>
    </section>

    <section>
      <h2>{{ t("gallery.type") }}</h2>
      <p v-for="name in sizes" :key="name" class="row">
        <code>{{ name }}</code>
        <span :style="{ fontSize: `var(--${name})` }">{{ t("gallery.typeSample") }}</span>
      </p>
      <p v-for="name in weights" :key="name" class="row">
        <code>{{ name }}</code>
        <span :style="{ fontWeight: `var(--${name})` }">{{ t("gallery.typeSample") }}</span>
      </p>
      <p class="row">
        <code>font-mono</code>
        <span class="mono">{{ t("gallery.typeSample") }}</span>
      </p>
    </section>

    <section>
      <h2>{{ t("gallery.spacing") }}</h2>
      <p v-for="name in [...spaces, ...lengths]" :key="name" class="row">
        <code>{{ name }}</code>
        <span class="bar" :style="{ inlineSize: `var(--${name})` }" />
      </p>
    </section>

    <section>
      <h2>{{ t("gallery.corners") }}</h2>
      <ul class="swatches">
        <li v-for="name in corners" :key="name">
          <span class="swatch wide" :style="{ borderRadius: `var(--${name})` }" />
          <code>{{ name }}</code>
        </li>
        <li v-for="name in shadows" :key="name">
          <span class="swatch wide raised" :style="{ boxShadow: `var(--${name})` }" />
          <code>{{ name }}</code>
        </li>
        <li>
          <span class="swatch wide focused" />
          <code>focus-color</code>
        </li>
      </ul>
    </section>

    <section>
      <h2>{{ t("gallery.icons") }}</h2>
      <ul class="swatches">
        <li v-for="name in drawn" :key="name">
          <span class="icons"><Icon :name="name" /><Icon :name="name" small /></span>
          <code>{{ name }}</code>
        </li>
      </ul>
    </section>
  </main>
</template>

<style scoped>
.gallery {
  display: grid;
  gap: var(--space-8);
  max-inline-size: 72rem;
  padding: var(--space-6);
}

header,
section {
  display: grid;
  gap: var(--space-3);
}

.choices {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-4);
  align-items: center;
}

label {
  display: flex;
  gap: var(--space-2);
  align-items: center;
  font-weight: var(--font-weight-strong);
}

select {
  block-size: var(--size-control-compact);
  padding-inline: var(--space-2);
  border: 1px solid var(--border-control);
  border-radius: var(--radius-field);
  background: var(--surface-field);
  font-weight: var(--font-weight-regular);
}

ul {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3);
  margin: 0;
  padding: 0;
  list-style: none;
}

li {
  display: grid;
  gap: var(--space-1);
  justify-items: start;
}

code {
  color: var(--text-muted);
  font-family: var(--font-mono);
  font-size: var(--font-size-small);
}

.swatch {
  inline-size: var(--size-control);
  block-size: var(--size-control);
  border: 1px solid var(--border);
}

.wide {
  inline-size: calc(2 * var(--size-rail-item));
}

.raised {
  border-color: transparent;
  border-radius: var(--radius-surface);
  background: var(--surface-raised);
}

.focused {
  outline: var(--focus-width) solid var(--focus-color);
  outline-offset: var(--focus-offset);
}

.pairs li {
  inline-size: calc(2.5 * var(--size-rail-item));
}

.pair {
  display: grid;
  place-content: center;
  inline-size: 100%;
  block-size: var(--size-control);
  border: 1px solid var(--border);
}

/* A line's pair: the ink as a border, as a field's or the focus ring is. */
.pair.line {
  border: var(--focus-width) solid currentColor;
}

.row {
  display: grid;
  grid-template-columns: 12rem 1fr;
  gap: var(--space-4);
  align-items: center;
}

.mono {
  font-family: var(--font-mono);
}

.bar {
  block-size: var(--space-3);
  background: var(--accent);
}

.icons {
  display: flex;
  gap: var(--space-2);
  align-items: center;
  inline-size: calc(1.5 * var(--size-rail-item));
}
</style>
