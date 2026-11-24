<script setup lang="ts">
import { computed, onMounted, shallowRef, useTemplateRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import {
  annotate,
  annotationsOf,
  measurementsAddress,
  measurementsOf,
  removeAnnotation,
  type Measurement,
  type RecordRef,
} from "@/api/annotations";
import { projectContents, projectPage, type ProjectSummary } from "@/api/projects";
import { recordsOf, type AnyRecord, type Contents } from "@/features/gallery/project/project";
import RecordViewer from "@/features/gallery/viewer/RecordViewer.vue";
import { imageOf, PLAIN } from "@/features/gallery/viewer/viewer";
import AnnotationLayer from "@/features/workspace/annotate/AnnotationLayer.vue";
import { drafted, nudge, pressed, type Draft } from "@/features/workspace/annotate/drawing";
import {
  featureOf,
  handlesOf,
  KINDS,
  measureOf,
  moved,
  reshaped,
  shapeOf,
  type Feature,
  type Kind,
  type Shape,
} from "@/features/workspace/annotate/shapes";
import { relistProjects } from "@/features/workspace/projects";
import Button from "@/ui/Button.vue";
import Field from "@/ui/Field.vue";
import Select from "@/ui/Select.vue";
import TextField from "@/ui/TextField.vue";
import ToggleButton from "@/ui/ToggleButton.vue";
import type { Point, Rect } from "@/ui/tile-viewer/coordinates";

/**
 * Annotations and measurements (W7.5; the behaviour spec in the phase's
 * notes): a record of a project chosen, its image shown, and points, lines,
 * rectangles, ellipses and polygons drawn on it, by the pointer or the
 * keyboard; each selected, moved, reshaped, named and deleted; measured as
 * it is drawn; kept with the record, and measured by the server, as a table
 * and its CSV.
 */
const LAYER = "annotations";
const { t } = useI18n();
const why = (thrown: unknown) => (thrown instanceof Error ? thrown.message : String(thrown));
const said = shallowRef("");

// The record.
const projects = shallowRef<ProjectSummary[]>([]);
/** The projects read again as their list opens: one made since is offered. */
const relisted = () => relistProjects(projects);
const project = shallowRef<string>();
const contents = shallowRef<Contents>();
const chosen = shallowRef<string>();
onMounted(async () => {
  try {
    projects.value = (await projectPage({ limit: 500 })).projects;
  } catch (thrown) {
    said.value = t("workspace.annotate.unread", { why: why(thrown) });
  }
});
watch(project, async (id) => {
  [contents.value, chosen.value] = [undefined, undefined];
  if (!id) return;
  contents.value = (await projectContents(id)) as Contents;
  chosen.value = records.value[0] && keyOf(records.value[0]);
});
const keyOf = (one: AnyRecord) => `${one.kind}:${one.record.id}`;
const records = computed(() =>
  contents.value
    ? recordsOf(contents.value).filter((one) => one.kind !== "report" && imageOf(one))
    : [],
);
const one = computed(() => records.value.find((each) => keyOf(each) === chosen.value));
const ref_ = computed<RecordRef | undefined>(() =>
  one.value && project.value && one.value.kind !== "report"
    ? { project: project.value, kind: one.value.kind, id: one.value.record.id }
    : undefined,
);

// The shapes, kept with the record, and what the server measures of them.
const shapes = shallowRef<Shape[]>([]);
const measured = shallowRef<Measurement[]>([]);
async function measure(): Promise<void> {
  if (ref_.value) measured.value = await measurementsOf(ref_.value, LAYER).catch(() => []);
}
watch(ref_, async (record) => {
  [shapes.value, measured.value, selected.value, draft.value] = [[], [], undefined, undefined];
  if (!record) return;
  try {
    const kept = (await annotationsOf(record)).layers[LAYER]?.features ?? [];
    shapes.value = kept.map((feature) => shapeOf(feature as Feature));
    await measure();
  } catch (thrown) {
    said.value = t("workspace.annotate.unread", { why: why(thrown) });
  }
});
/** `shape` kept: in place of `index`, or new. */
async function keep(shape: Shape, index?: number): Promise<void> {
  const record = ref_.value;
  if (!record) return;
  const now = [...shapes.value];
  if (index === undefined) now.push(shape);
  else now[index] = shape;
  shapes.value = now;
  try {
    const kept = shapeOf((await annotate(record, LAYER, featureOf(shape) as never)) as Feature);
    const at = index ?? shapes.value.indexOf(shape);
    shapes.value = shapes.value.map((each, i) => (i === at ? kept : each));
    await measure();
  } catch (thrown) {
    said.value = t("workspace.annotate.unkept", { why: why(thrown) });
  }
}
async function remove(): Promise<void> {
  const index = selected.value;
  const shape = index === undefined ? undefined : shapes.value[index];
  if (!shape?.id || !ref_.value) return;
  selected.value = undefined;
  shapes.value = shapes.value.filter((_, i) => i !== index);
  try {
    await removeAnnotation(ref_.value, LAYER, shape.id);
    await measure();
  } catch (thrown) {
    said.value = t("workspace.annotate.unkept", { why: why(thrown) });
  }
}

// The view: screen pixels to a µm, and its middle, for the keyboard's cross.
const frame = useTemplateRef<HTMLElement>("frame");
const view = shallowRef<Rect>();
const scale = computed(() => (view.value ? (frame.value?.clientWidth ?? 1) / view.value.width : 1));

// Drawing: a tool, the shape so far, the pointer, and a rectangle dragged.
const tool = shallowRef<Kind>();
const draft = shallowRef<Draft>();
const pointer = shallowRef<Point>();
const dragged = shallowRef<Rect>();
const cross = shallowRef<Point>();
function choose(kind: Kind | undefined): void {
  tool.value = kind;
  draft.value = kind && { kind, points: [] };
  [selected.value, cross.value] = [undefined, undefined];
}
function press(at: Point): void {
  if (!draft.value) return;
  const done = pressed(draft.value, at, 6 / scale.value);
  draft.value = done.draft;
  if (done.done) void keep(done.done);
}
function drew(rect: Rect): void {
  dragged.value = undefined;
  const kind = tool.value;
  if (kind !== "rectangle" && kind !== "ellipse") return;
  const corner = { x: rect.x, y: rect.y };
  draft.value = { kind, points: [], corner };
  press({ x: rect.x + rect.width, y: rect.y + rect.height });
}
const drawing = computed(
  () => draft.value && drafted(draft.value, cross.value ?? pointer.value, dragged.value),
);
/** What the shape being drawn, or the one selected, measures, said. */
const measuring = computed(() => {
  const shape =
    drawing.value ?? (selected.value === undefined ? undefined : shapes.value[selected.value]);
  if (!shape) return "";
  const { length, area } = measureOf(shape);
  const unit = measured.value[0]?.unit ?? "µm";
  const figures = { length: length.toFixed(2), area: area.toFixed(2), unit };
  return t(
    `workspace.annotate.measure.${area > 0 ? "area" : length > 0 ? "length" : "point"}`,
    figures,
  );
});

// Editing: the shape selected, and a handle chosen from the keyboard.
const selected = shallowRef<number>();
const handle = shallowRef<number>();
watch(selected, () => (handle.value = undefined));
const name = computed({
  get: () => (selected.value === undefined ? "" : (shapes.value[selected.value]?.name ?? "")),
  set: (now: string) => {
    const index = selected.value;
    if (index === undefined) return;
    shapes.value = shapes.value.map((shape, i) => (i === index ? { ...shape, name: now } : shape));
  },
});
function named(): void {
  const index = selected.value;
  if (index !== undefined) void keep(shapes.value[index]!, index);
}

// The keyboard: the cross, or the shape selected, moved by the arrows.
let still: ReturnType<typeof setTimeout> | undefined;
function keys(event: KeyboardEvent): void {
  if ((event.target as HTMLElement).closest("input, textarea, button, [role=combobox]")) return;
  const by = nudge(event.key, event.shiftKey, scale.value);
  const index = selected.value;
  let handled = true;
  // The cross starts at the view's middle.
  const centre = view.value && {
    x: view.value.x + view.value.width / 2,
    y: view.value.y + view.value.height / 2,
  };
  const from = cross.value ?? centre ?? { x: 0, y: 0 };
  if (draft.value && by) {
    cross.value = { x: from.x + by.x, y: from.y + by.y };
  } else if (draft.value && event.key === "Enter") {
    cross.value = from;
    press(from);
  } else if (event.key === "Escape") {
    if (draft.value) draft.value = { kind: draft.value.kind, points: [] };
    else selected.value = undefined;
  } else if (index !== undefined && by) {
    const shape = shapes.value[index]!;
    const to = handle.value === undefined ? undefined : handlesOf(shape)[handle.value];
    const now = to
      ? reshaped(shape, handle.value!, { x: to.x + by.x, y: to.y + by.y })
      : moved(shape, by);
    shapes.value = shapes.value.map((each, i) => (i === index ? now : each));
    // Kept once the arrows are let be.
    clearTimeout(still);
    still = setTimeout(() => void keep(shapes.value[index]!, index), 500);
  } else if (index !== undefined && event.key.toLowerCase() === "v") {
    const count = handlesOf(shapes.value[index]!).length;
    handle.value =
      handle.value === undefined ? 0 : handle.value + 1 < count ? handle.value + 1 : undefined;
  } else if (index !== undefined && event.key === "Delete") void remove();
  else handled = false;
  if (handled) {
    event.preventDefault();
    event.stopPropagation();
  }
}
const kindOf = (id: string) => shapes.value.find((shape) => shape.id === id)?.kind;
const csv = computed(() => ref_.value && measurementsAddress(ref_.value, LAYER, true));
</script>

<template>
  <section class="annotate" data-said="annotate" aria-labelledby="annotate-title">
    <h2 id="annotate-title">{{ t("workspace.annotate.title") }}</h2>
    <div class="choice">
      <Field :label="t('workspace.run.project')">
        <Select
          v-model="project"
          :options="projects.map((each) => ({ value: each.id, label: each.name }))"
          :placeholder="t('workspace.run.chooseProject')"
          @opened="relisted"
        />
      </Field>
      <Field :label="t('workspace.run.input')">
        <Select
          v-model="chosen"
          :options="records.map((each) => ({ value: keyOf(each), label: each.record.name }))"
          :placeholder="t('workspace.run.chooseInput')"
        />
      </Field>
    </div>
    <div class="tools" role="toolbar" :aria-label="t('workspace.annotate.tools')">
      <ToggleButton :model-value="!tool" compact @update:model-value="choose(undefined)">
        {{ t("workspace.annotate.select") }}
      </ToggleButton>
      <ToggleButton
        v-for="kind in KINDS"
        :key="kind"
        compact
        :model-value="tool === kind"
        @update:model-value="(on) => choose(on ? kind : undefined)"
      >
        {{ t(`workspace.annotate.kinds.${kind}`) }}
      </ToggleButton>
    </div>
    <div class="body">
      <!-- The keyboard's path over the image: arrows, Enter, V, Delete and Escape. -->
      <div ref="frame" class="frame" @keydown.capture="keys">
        <RecordViewer
          v-if="one && contents"
          :contents="contents"
          :one="one"
          :look="PLAIN"
          own
          :drawing="!!tool"
          @view="view = $event"
          @press="press"
          @draw="drew"
          @drag="dragged = $event"
          @hover="pointer = $event"
        >
          <template #over="{ placement }">
            <AnnotationLayer
              :placement="placement"
              :shapes="shapes"
              :selected="selected"
              :handle="handle"
              :draft="drawing"
              :cross="tool ? cross : undefined"
              :scale="scale"
              :editing="!tool"
              @select="selected = $event"
              @change="(index, shape) => keep(shape, index)"
            />
          </template>
        </RecordViewer>
        <p v-else class="said">{{ t("workspace.annotate.choose") }}</p>
      </div>
      <aside class="pane">
        <p class="said" data-said="measuring" aria-live="polite">{{ measuring }}</p>
        <template v-if="selected !== undefined">
          <Field :label="t('workspace.annotate.name')">
            <TextField v-model="name" @blur="named" @keydown.enter="named" />
          </Field>
          <Button compact tone="danger" variant="bare" @click="remove">
            {{ t("workspace.annotate.delete") }}
          </Button>
        </template>
      </aside>
    </div>
    <section v-if="ref_" class="measured" aria-labelledby="measured-title" data-said="measured">
      <h3 id="measured-title">{{ t("workspace.annotate.table") }}</h3>
      <table v-if="measured.length">
        <thead>
          <tr>
            <th scope="col">{{ t("workspace.annotate.kind") }}</th>
            <th scope="col">{{ t("workspace.annotate.name") }}</th>
            <th scope="col">{{ t("workspace.annotate.length", { unit: measured[0]!.unit }) }}</th>
            <th scope="col">{{ t("workspace.annotate.area", { unit: measured[0]!.unit }) }}</th>
            <th scope="col">{{ t("workspace.annotate.centre", { unit: measured[0]!.unit }) }}</th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="(row, n) in measured"
            :key="row.id"
            :class="{ selected: shapes[selected ?? -1]?.id === row.id }"
            tabindex="0"
            @click="selected = shapes.findIndex((shape) => shape.id === row.id)"
            @keydown.enter="selected = shapes.findIndex((shape) => shape.id === row.id)"
          >
            <th scope="row">
              {{ t(`workspace.annotate.kinds.${kindOf(row.id) ?? "polygon"}`) }} {{ n + 1 }}
            </th>
            <td>{{ row.name ?? "" }}</td>
            <td>{{ row.length.toFixed(2) }}</td>
            <td>{{ row.area.toFixed(2) }}</td>
            <td>{{ row.centroid.map((v) => v.toFixed(2)).join(", ") }}</td>
          </tr>
        </tbody>
      </table>
      <p v-else class="said">{{ t("workspace.annotate.none") }}</p>
      <a v-if="csv && measured.length" :href="csv" download="measurements.csv">
        {{ t("workspace.annotate.csv") }}
      </a>
    </section>
    <p class="said" role="status" data-said="annotate-said">{{ said }}</p>
  </section>
</template>

<style scoped>
.annotate {
  display: grid;
  gap: var(--space-2);
  align-content: start;
}

h2,
h3 {
  margin: 0;
  font-size: var(--font-size-body);
}

.choice,
.tools {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  align-items: end;
}

.body {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(12rem, 16rem);
  gap: var(--space-2);
}

.frame {
  display: grid;
  min-block-size: 28rem;
}

.pane {
  display: grid;
  gap: var(--space-2);
  align-content: start;
}

.said {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

table {
  border-collapse: collapse;
  font-variant-numeric: tabular-nums;
}

th,
td {
  padding: var(--space-1) var(--space-2);
  border-block-end: 1px solid var(--border);
  text-align: start;
}

tbody tr {
  cursor: pointer;
}

tbody tr.selected {
  background: var(--surface-hover);
}
</style>
