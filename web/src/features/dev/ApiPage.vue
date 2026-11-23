<script setup lang="ts">
import { computed, onBeforeUnmount, ref, shallowRef } from "vue";
import { useI18n } from "vue-i18n";

import { cancel, follow, start, type Invocation } from "@/api/actions";
import { call, server } from "@/api/client";
import { ApiError, invocationError } from "@/api/errors";
import GalleryHeader from "@/features/dev/GalleryHeader.vue";
import Button from "@/ui/Button.vue";
import Card from "@/ui/Card.vue";
import Field from "@/ui/Field.vue";
import KeyValue from "@/ui/KeyValue.vue";
import Select from "@/ui/Select.vue";

/**
 * The API (W1.1): an action called through the typed client and followed to
 * its end, each state its invocation is seen in; and the server's failures,
 * each read as the client's one model of them.
 */
const { t } = useI18n();
const { api } = server;

// An action, followed.

/** The simulated stage's travel is 100 × 75 × 20 mm. */
const ACTIONS = {
  home: () => start("/api/v1/stage/home", {}),
  middle: () => start("/api/v1/stage/move_to", { x: 50_000, y: 37_500, z: 10_000 }),
  slow: () => start("/api/v1/stage/move_to", { x: 90_000, y: 70_000, z: 10_000, speed: 100 }),
} as const;
type Action = keyof typeof ACTIONS;

const chosen = ref<Action>("home");
const choices = computed(() =>
  (Object.keys(ACTIONS) as Action[]).map((value) => ({
    value,
    label: t(`api.action.actions.${value}`),
  })),
);
const states = ref<{ status: string; after: number }[]>([]);
const running = shallowRef<Invocation>();
const output = ref<string>();
const failed = shallowRef<ApiError>();
let stop = new AbortController();
onBeforeUnmount(() => stop.abort());

function choose(value: string | undefined): void {
  if (value && value in ACTIONS) chosen.value = value as Action;
}

/** Starts the chosen action, and follows it to its end. */
async function go(): Promise<void> {
  stop = new AbortController();
  states.value = [];
  output.value = undefined;
  failed.value = undefined;
  const from = performance.now();
  try {
    const invocation = await ACTIONS[chosen.value]();
    running.value = invocation;
    const ended = await follow(invocation, {
      signal: stop.signal,
      seen: (now) => {
        states.value.push({ status: now.status, after: (performance.now() - from) / 1000 });
      },
    });
    if (ended.status === "completed") output.value = JSON.stringify(ended.output, null, 2);
    else failed.value = invocationError(ended);
  } catch (thrown) {
    failed.value = thrown instanceof ApiError ? thrown : undefined;
  } finally {
    running.value = undefined;
  }
}

/** Cancels the action under way, on the server: it ends `cancelled`. */
async function halt(): Promise<void> {
  if (running.value) await cancel(running.value.id).catch(() => undefined);
}

// Failures, as one model.

const CASES = {
  range: () => call((signal) => api.PUT("/api/v1/camera/gain", { body: -5, signal })),
  invalid: () => {
    // A value the server's description doesn't allow, sent anyway.
    const wrong = { x: "the middle" } as unknown as { x: number; y: number; z: number };
    return start("/api/v1/stage/move_to", wrong);
  },
  missing: () => {
    const id = "00000000-0000-0000-0000-000000000000";
    return call((signal) =>
      api.GET("/api/v1/action_invocations/{id}", { params: { path: { id } }, signal }),
    );
  },
  // A limit no answer can meet: it ends before the next turn of the event loop.
  timeout: () => call((signal) => api.GET("/api/v1/system/version", { signal }), { timeout: 0 }),
} as const;
type Case = keyof typeof CASES;

/** Asks for one case's failure, and shows how it reads. */
async function provoke(which: Case): Promise<void> {
  failed.value = undefined;
  output.value = undefined;
  states.value = [];
  try {
    await CASES[which]();
  } catch (thrown) {
    if (thrown instanceof ApiError) failed.value = thrown;
  }
}

const told = computed(() => {
  const error = failed.value;
  if (!error) return [];
  const entries = [
    { term: t("api.error.failure"), value: t(`api.failures.kinds.${error.failure}`) },
    { term: t("api.error.message"), value: error.message },
  ];
  if (error.refusal) entries.push({ term: t("api.error.refusal"), value: error.refusal });
  if (error.status) entries.push({ term: t("api.error.status"), value: String(error.status) });
  for (const field of error.fields) {
    const at = field.at.length > 0 ? field.at.join(".") : t("api.error.whole");
    entries.push({ term: t("api.error.field", { at }), value: field.message });
  }
  return entries;
});
</script>

<template>
  <main class="calls">
    <GalleryHeader :title="t('api.title')" :intro="t('api.intro')" />
    <div class="cards">
      <Card :title="t('api.action.title')" :level="2">
        <div class="row">
          <Field class="which" :label="t('api.action.which')">
            <Select :model-value="chosen" :options="choices" @update:model-value="choose" />
          </Field>
          <Button variant="primary" :busy="running !== undefined" @click="go">
            {{ t("api.action.start") }}
          </Button>
          <Button :disabled="running === undefined" @click="halt">
            {{ t("api.action.cancel") }}
          </Button>
        </div>
        <h3>{{ t("api.action.states") }}</h3>
        <p v-if="states.length === 0" class="note">{{ t("api.action.none") }}</p>
        <ol v-else data-said="states" class="states">
          <li v-for="(state, n) in states" :key="n">
            {{ t("api.action.state", { status: state.status, after: state.after.toFixed(2) }) }}
          </li>
        </ol>
        <template v-if="output !== undefined">
          <h3>{{ t("api.action.output") }}</h3>
          <pre data-said="output">{{ output }}</pre>
        </template>
      </Card>

      <Card :title="t('api.failures.title')" :level="2">
        <p class="note">{{ t("api.failures.intro") }}</p>
        <div class="row">
          <Button
            v-for="which in Object.keys(CASES) as Case[]"
            :key="which"
            compact
            @click="provoke(which)"
          >
            {{ t(`api.failures.cases.${which}`) }}
          </Button>
        </div>
        <h3>{{ t("api.error.title") }}</h3>
        <p v-if="told.length === 0" class="note">{{ t("api.error.none") }}</p>
        <KeyValue v-else data-said="error" :entries="told" />
      </Card>
    </div>
  </main>
</template>

<style scoped>
.calls {
  display: grid;
  gap: var(--space-4);
  padding: var(--space-6);
}

.cards {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(22rem, 1fr));
  gap: var(--space-6);
  align-items: start;
}

.row {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3);
  align-items: end;
}

.which {
  min-inline-size: 18rem;
}

h3 {
  margin-block: var(--space-4) var(--space-2);
  font-size: var(--font-size-body);
}

.states {
  margin: 0;
  padding-inline-start: var(--space-6);
}

pre {
  max-block-size: 16rem;
  overflow: auto;
  font-family: var(--font-mono);
  font-size: var(--font-size-small);
}

.note {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
