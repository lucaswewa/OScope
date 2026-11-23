<script setup lang="ts">
import { computed, shallowRef } from "vue";
import { useI18n } from "vue-i18n";

import { useConnection } from "@/api/connection/connection";
import { iconOf, toneOf, useInstrument } from "@/features/status/status";
import Icon from "@/ui/Icon.vue";
import Popover from "@/ui/Popover.vue";

/**
 * The instrument's status in the rail (W2.2): ok, degraded or fault, in
 * its colour, an icon over its word as the rail's tabs are; a click away,
 * why, and each subsystem with its message, and the way to the status in
 * full (W4.3). Without the server, it isn't known.
 */
const { t, te } = useI18n();
const open = shallowRef(false);
const connection = useConnection();
const told = useInstrument();
const instrument = computed(() => (connection.state === "connected" ? told.value : undefined));
const tone = computed(() => toneOf(instrument.value?.state));
const label = computed(() =>
  instrument.value ? t(`status.states.${instrument.value.state}`) : t("status.unknown"),
);
/** A subsystem's name, as the app says it: the server's, for one it doesn't know. */
const named = (subsystem: string) => {
  const key = `status.subsystems.${subsystem}`;
  return te(key) ? t(key) : subsystem;
};
</script>

<template>
  <Popover v-model:open="open" side="right" align="end">
    <template #trigger>
      <button
        class="indicator"
        type="button"
        :data-tone="tone"
        data-said="indicator"
        :aria-label="t('status.label', { state: label })"
      >
        <Icon :name="iconOf(tone)" />
        <span class="label">{{ label }}</span>
      </button>
    </template>
    <section class="panel" data-said="status">
      <h2>{{ t("status.title") }}</h2>
      <p>{{ instrument?.message ?? t("status.untold") }}</p>
      <ul>
        <li
          v-for="one in instrument?.subsystems ?? []"
          :key="one.subsystem"
          :data-tone="toneOf(one.state)"
        >
          <Icon :name="iconOf(toneOf(one.state))" />
          <span>
            <strong>{{ named(one.subsystem) }}</strong>
            {{
              t("status.health", { state: t(`status.states.${one.state}`), message: one.message })
            }}
          </span>
        </li>
      </ul>
      <RouterLink :to="{ name: 'status' }" data-said="status-page" @click="open = false">
        {{ t("status.page.link") }}
      </RouterLink>
    </section>
  </Popover>
</template>

<style scoped>
.indicator {
  display: flex;
  flex-direction: column;
  gap: 2px;
  align-items: center;
  justify-content: center;
  inline-size: 100%;
  block-size: 2.875rem;
  padding-inline: var(--space-1);
  border: 0;
  background: none;
  color: var(--text-muted);
  font: inherit;
  cursor: pointer;
}

.indicator:hover {
  background: var(--surface-hover);
}

.indicator:focus-visible {
  outline: var(--focus-width) solid var(--focus-color);
  outline-offset: calc(-1 * var(--focus-width));
}

.label {
  overflow: hidden;
  font-size: var(--font-size-small);
  text-overflow: ellipsis;
  white-space: nowrap;
}

[data-tone="success"] > :first-child {
  color: var(--success);
}

[data-tone="warning"] > :first-child {
  color: var(--warning);
}

[data-tone="danger"] > :first-child {
  color: var(--danger);
}

.panel {
  display: grid;
  gap: var(--space-2);
  max-inline-size: 24rem;
}

.panel h2,
.panel p {
  margin: 0;
}

.panel ul {
  display: grid;
  gap: var(--space-2);
  margin: 0;
  padding: 0;
  list-style: none;
}

.panel li {
  display: flex;
  gap: var(--space-2);
  align-items: start;
}
</style>
