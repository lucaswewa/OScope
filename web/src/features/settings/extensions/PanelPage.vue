<script setup lang="ts">
import { computed, ref, shallowRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import { act } from "@/api/actions";
import { fetched, server } from "@/api/client";
import { readPanel, shownOf, type Read } from "@/features/settings/extensions/panel";
import Banner from "@/ui/Banner.vue";
import Button from "@/ui/Button.vue";
import SchemaForm from "@/ui/schema-form/SchemaForm.vue";
import type { Fault } from "@/ui/schema-form/check";
import { initial, shapeOf, type Json } from "@/ui/schema-form/shape";

/**
 * An extension's panel (W8.5, ADR-0036), at a route of its own under its
 * extension: drawn from its description, a form from its schema, submitted
 * to one of the extension's nodes on the live view's newest frame, or to a
 * Thing's action, and what came back shown as its view says. Its
 * description is data: the app runs no code from it.
 */
const props = defineProps<{ extension: string; panel: string }>();
const { t } = useI18n();
const why = (thrown: unknown) => (thrown instanceof Error ? thrown.message : String(thrown));

const read = shallowRef<Read>();
const unread = shallowRef("");
const value = ref<Json | undefined>();
const faults = shallowRef<Fault[]>([]);
/** What the last submission gave, or why it failed. */
const answer = shallowRef<unknown>();
const failed = shallowRef("");
const running = shallowRef(false);
watch(
  () => props.panel,
  async (name) => {
    [read.value, unread.value, answer.value, failed.value] = [undefined, "", undefined, ""];
    try {
      const address = server.address(`/api/v1/extensions/panel/${encodeURIComponent(name)}`);
      read.value = readPanel(await fetched(address, (reply) => reply.json() as Promise<unknown>));
      const form = read.value.panel?.form;
      value.value = form === undefined ? undefined : initial(shapeOf(form));
    } catch (thrown) {
      unread.value = why(thrown);
    }
  },
  { immediate: true },
);

async function submit(): Promise<void> {
  const panel = read.value?.panel;
  if (!panel || faults.value.length > 0) return;
  [running.value, failed.value] = [true, ""];
  try {
    if (panel.submit.kind === "node") {
      const body = JSON.stringify({ node: panel.submit.node, params: value.value ?? {} });
      const init = { method: "POST", body, headers: { "content-type": "application/json" } };
      const address = server.address("/api/v1/preview/run_node");
      answer.value = await fetched(address, (reply) => reply.json() as Promise<unknown>, {}, init);
    } else {
      const path = `/api/v1/${panel.submit.thing}/${panel.submit.action}`;
      answer.value = await act(path as never, (value.value ?? {}) as never, { timeout: 120_000 });
    }
  } catch (thrown) {
    failed.value = why(thrown);
  } finally {
    running.value = false;
  }
}
const shown = computed(() =>
  read.value?.panel && answer.value !== undefined
    ? shownOf(read.value.panel.view, answer.value)
    : undefined,
);
</script>

<template>
  <section class="panel" :aria-labelledby="'panel-title'" data-said="panel">
    <RouterLink :to="{ name: 'settings', query: { view: 'extensions' } }" class="back">
      {{ t("settings.extensions.back") }}
    </RouterLink>
    <h1 id="panel-title" tabindex="-1">{{ read?.panel?.title ?? panel }}</h1>
    <p class="note">{{ t("settings.extensions.panelOf", { extension }) }}</p>
    <p v-if="unread" class="failed" role="alert">
      {{ t("settings.extensions.unread", { why: unread }) }}
    </p>
    <Banner
      v-if="read && read.unknown.length"
      :level="read.panel ? 'info' : 'warning'"
      :title="t(read.panel ? 'settings.extensions.partly' : 'settings.extensions.cannot')"
      data-said="panel-unknown"
    >
      <ul>
        <li v-for="one in read.unknown" :key="one">{{ one }}</li>
      </ul>
    </Banner>
    <template v-if="read?.panel">
      <p v-if="read.panel.description">{{ read.panel.description }}</p>
      <p class="note" data-said="panel-submits">
        {{
          read.panel.submit.kind === "node"
            ? t("settings.extensions.toNode", { node: read.panel.submit.node })
            : t("settings.extensions.toAction", {
                thing: read.panel.submit.thing,
                action: read.panel.submit.action,
              })
        }}
      </p>
      <form class="form" @submit.prevent="submit">
        <SchemaForm
          v-model="value"
          :schema="read.panel.form"
          :label="read.panel.title"
          @faults="(now) => (faults = now)"
        />
        <div class="actions">
          <Button type="submit" variant="primary" :busy="running" :disabled="faults.length > 0">
            {{ t("settings.extensions.submit") }}
          </Button>
        </div>
      </form>
      <p v-if="failed" class="failed" role="alert">
        {{ t("settings.extensions.failed", { why: failed }) }}
      </p>
      <section
        v-if="shown"
        class="answer"
        :aria-label="t('settings.extensions.answer')"
        data-said="panel-answer"
      >
        <figure v-if="shown.kind === 'image'">
          <img
            :src="shown.src"
            :width="shown.width"
            :height="shown.height"
            :alt="t('settings.extensions.imageOf', { port: shown.port })"
          />
          <figcaption class="note">{{ shown.width }} × {{ shown.height }}</figcaption>
        </figure>
        <table v-else-if="shown.kind === 'table'">
          <thead>
            <tr>
              <th v-for="column in shown.columns" :key="column" scope="col">{{ column }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(row, i) in shown.rows" :key="i">
              <td v-for="(cell, j) in row" :key="j">{{ cell }}</td>
            </tr>
          </tbody>
        </table>
        <pre v-else-if="shown.kind === 'text'">{{ shown.text }}</pre>
        <p v-else class="note">{{ t("settings.extensions.nothing") }}</p>
      </section>
    </template>
  </section>
</template>

<style scoped>
.panel {
  display: grid;
  gap: var(--space-2);
  align-content: start;
  max-inline-size: 60rem;
  padding: var(--space-3);
}

h1 {
  margin: 0;
}

.panel p {
  margin: 0;
}

.form {
  display: grid;
  gap: var(--space-2);
  max-inline-size: 32rem;
}

.actions {
  display: flex;
  gap: var(--space-2);
}

.answer img {
  max-inline-size: 100%;
  block-size: auto;
  border: 1px solid var(--border);
}

.answer figure {
  margin: 0;
}

.answer table {
  border-collapse: collapse;
  font-variant-numeric: tabular-nums;
}

.answer th,
.answer td {
  padding: var(--space-1) var(--space-2);
  border: 1px solid var(--border);
  text-align: start;
}

.answer pre {
  margin: 0;
  padding: var(--space-2);
  overflow: auto;
  background: var(--surface-open);
}

.note {
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

.failed {
  color: var(--danger);
}
</style>
