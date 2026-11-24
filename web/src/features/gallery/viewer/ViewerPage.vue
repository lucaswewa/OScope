<script setup lang="ts">
import { computed, onBeforeUnmount, shallowRef, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";

import { ApiError } from "@/api/errors";
import { projectContents } from "@/api/projects";
import { useAccountDisplay } from "@/features/display/account";
import type { Contents } from "@/features/gallery/project/project";
import RecordViewer from "@/features/gallery/viewer/RecordViewer.vue";
import { imageOf, PLAIN, viewable, type ViewLook } from "@/features/gallery/viewer/viewer";
import { MAPS, type MapName } from "@/features/live/colourMaps";
import { host } from "@/host/host";
import Button from "@/ui/Button.vue";
import Field from "@/ui/Field.vue";
import Select from "@/ui/Select.vue";
import Slider from "@/ui/Slider.vue";
import type { Rect } from "@/ui/tile-viewer/coordinates";

/**
 * The deep-zoom viewer (W6.4, D33): a record's image, from the whole slide
 * down to its pixels, at `?project=…&record=…&view=1`. Its look, the black
 * and white points and gamma, a colour map for one channel, and a stack's
 * plane, set here. Another record beside it, `&beside=…`, their views
 * linked on the stage; and the screen filled.
 */
const props = defineProps<{ project: string; record: string; beside?: string | undefined }>();
const { t } = useI18n();
const route = useRoute();
const router = useRouter();

const contents = shallowRef<Contents>();
const unread = shallowRef<string>();
watch(
  () => props.project,
  async (project) => {
    try {
      contents.value = (await projectContents(project)) as Contents;
      unread.value = undefined;
    } catch (thrown) {
      unread.value = thrown instanceof ApiError ? thrown.message : String(thrown);
    }
  },
  { immediate: true },
);
const records = computed(() => (contents.value ? viewable(contents.value) : []));
const first = computed(() => records.value.find((one) => one.record.id === props.record));
const second = computed(() =>
  props.beside ? records.value.find((one) => one.record.id === props.beside) : undefined,
);
/** Nothing beside it: reka-ui's lists take no empty value. */
const ALONE = "alone";
const choices = computed(() => [
  { value: ALONE, label: t("viewer.alone") },
  ...records.value
    .filter((one) => one.record.id !== props.record)
    .map((one) => ({ value: one.record.id, label: one.record.name })),
]);
const besideChosen = computed({
  get: () => props.beside ?? ALONE,
  set: (beside: string) =>
    void router.replace({
      query: { ...route.query, beside: beside === ALONE ? undefined : beside },
    }),
});

// The look, as committed; the sliders say theirs as they move.
const look = shallowRef<ViewLook>({ ...PLAIN });
const planesOf = (one: typeof first.value) => (one ? (imageOf(one)?.planes ?? 1) : 1);
const planes = computed(() => Math.max(planesOf(first.value), planesOf(second.value)));
const oneChannel = computed(() => first.value && imageOf(first.value)?.channels === 1);
// A one-channel image opens in the account's colour map, until one is chosen here (W8.1).
// Grey is its own colours already.
const account = useAccountDisplay();
watch(
  [() => first.value?.record.id, () => account.display?.colormap],
  ([, colormap]) => {
    if (oneChannel.value && colormap && colormap !== "grey" && look.value.map === "none") {
      set({ map: colormap });
    }
  },
  { immediate: true },
);
const maps = computed(() =>
  MAPS.map((name) => ({ value: name, label: t(`live.values.maps.${name}`) })),
);
function set(change: Partial<ViewLook>): void {
  look.value = { ...look.value, ...change };
}
/** Each slider's value while it is dragged: the look takes it when it is let go. */
type Sliding = "plane" | "black" | "white" | "gamma";
const dragged = shallowRef<Partial<Record<Sliding, number | undefined>>>({});
function drag(which: Sliding, value: number): void {
  dragged.value = { ...dragged.value, [which]: value };
}
function letGo(which: Sliding, change: Partial<ViewLook>): void {
  set(change);
  dragged.value = { ...dragged.value, [which]: undefined };
}
const map = computed({
  get: () => look.value.map,
  set: (name: MapName) => set({ map: name }),
});

// Each view follows the other's, as its user moves it: the one following,
// moved by nobody, says nothing back.
const fits = shallowRef<{ first?: Rect; second?: Rect }>({});
function moved(which: "first" | "second", rect: Rect): void {
  if (!second.value) return;
  fits.value = { ...fits.value, [which === "first" ? "second" : "first"]: rect };
}

const viewers = shallowRef<HTMLElement>();
const full = shallowRef(host().window.isFullScreen());
onBeforeUnmount(host().window.onFullScreen((now) => (full.value = now)));
const shown = shallowRef<InstanceType<typeof RecordViewer>>();
/** A region exported: the first view, as it is drawn, saved as a PNG. */
async function save(): Promise<void> {
  const png = await shown.value?.snapshot();
  if (png && first.value) await host().files.save(`${first.value.record.name}-view.png`, png);
}

async function toggleFull(): Promise<void> {
  if (full.value) await host().window.leaveFullScreen();
  else if (viewers.value) await host().window.enterFullScreen(viewers.value);
}
</script>

<template>
  <section class="viewer-page" aria-labelledby="viewer-title" data-said="viewer">
    <header class="head">
      <RouterLink :to="{ name: 'gallery', query: { project, record } }" class="back">
        {{ t("viewer.back") }}
      </RouterLink>
      <h1 id="viewer-title">{{ first?.record.name ?? "…" }}</h1>
      <Button compact variant="bare" icon="download" @click="save">
        {{ t("viewer.save") }}
      </Button>
      <Button
        compact
        variant="bare"
        :icon="full ? 'leaveFullScreen' : 'fullScreen'"
        @click="toggleFull"
      >
        {{ full ? t("viewer.leaveFullScreen") : t("viewer.fullScreen") }}
      </Button>
    </header>
    <p v-if="unread" class="said failed" role="alert">{{ unread }}</p>
    <p v-else-if="contents && !first" class="said failed" role="alert">
      {{ t("viewer.none") }}
    </p>

    <div v-if="first" class="look" data-said="look">
      <Field class="beside" :label="t('viewer.beside')">
        <Select v-model="besideChosen" :options="choices" compact />
      </Field>
      <Field v-if="planes > 1" :label="t('viewer.plane')">
        <Slider
          :model-value="dragged.plane ?? look.plane + 1"
          :min="1"
          :max="planes"
          @update:model-value="(value) => drag('plane', value)"
          @commit="(value) => letGo('plane', { plane: value - 1 })"
        />
      </Field>
      <Field :label="t('live.values.black')">
        <Slider
          :model-value="dragged.black ?? look.black * 100"
          unit="%"
          @update:model-value="(value) => drag('black', value)"
          @commit="(value) => letGo('black', { black: value / 100 })"
        />
      </Field>
      <Field :label="t('live.values.white')">
        <Slider
          :model-value="dragged.white ?? look.white * 100"
          unit="%"
          @update:model-value="(value) => drag('white', value)"
          @commit="(value) => letGo('white', { white: value / 100 })"
        />
      </Field>
      <Field :label="t('live.values.gamma')">
        <Slider
          :model-value="dragged.gamma ?? look.gamma"
          :min="0.2"
          :max="3"
          :step="0.05"
          :digits="2"
          @update:model-value="(value) => drag('gamma', value)"
          @commit="(value) => letGo('gamma', { gamma: value })"
        />
      </Field>
      <Field v-if="oneChannel" :label="t('live.values.map')">
        <Select v-model="map" :options="maps" compact />
      </Field>
    </div>

    <div
      v-if="contents && first"
      ref="viewers"
      class="viewers"
      :class="{ two: !!second }"
      data-said="viewers"
    >
      <RecordViewer
        ref="shown"
        :contents="contents"
        :one="first"
        :look="look"
        :fit="fits.first"
        @view="(rect) => moved('first', rect)"
      />
      <RecordViewer
        v-if="second"
        :key="second.record.id"
        :contents="contents"
        :one="second"
        :look="look"
        :fit="fits.second"
        @view="(rect) => moved('second', rect)"
      />
    </div>
  </section>
</template>

<style scoped>
.viewer-page {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  block-size: 100%;
  padding: var(--space-3);
  overflow: auto;
}

.head {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2) var(--space-3);
  align-items: center;
}

.head h1 {
  flex: 1 1 auto;
  margin: 0;
}

.look {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(12rem, 1fr));
  gap: var(--space-2) var(--space-3);
  align-items: end;
}

.viewers {
  display: grid;
  flex: 1 1 auto;
  gap: var(--space-2);
  min-block-size: 24rem;
  background: var(--surface);
}

.viewers.two {
  grid-template-columns: repeat(2, minmax(0, 1fr));
}

.said {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

.said.failed {
  color: var(--danger);
}
</style>
