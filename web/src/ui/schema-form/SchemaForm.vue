<script setup lang="ts">
import { computed, ref, watch } from "vue";

import { faultsOf, placed, placesOf, refusals, valueAt, type Fault } from "./check";
import SchemaField from "./SchemaField.vue";
import { initial, shapeOf, type Json } from "./shape";

/**
 * A form drawn from a JSON Schema (W0.6): the scan's, a node's, an action's.
 * It takes a schema and a value, and gives the value changed, and its faults:
 * what it finds itself, and what the server refused, on the fields named.
 *
 * Without a value, it starts from the schema's defaults. A part of the
 * schema it doesn't know how to draw is JSON to write, with the reason.
 */
const props = defineProps<{
  /** The JSON Schema; its `$ref`s point into itself. */
  schema: unknown;
  /** What the form is, to a screen reader. */
  label: string;
  /** The server's answer, refusing the value: a 422's body. Each refusal stands until its field changes. */
  refused?: unknown;
  readonly?: boolean;
}>();
// Undefined unless given: JSON may be `true`, and Vue makes a missing boolean `false`.
const value = defineModel<Json | undefined>({ default: undefined });
const emit = defineEmits<{ faults: [faults: Fault[]] }>();

const shape = computed(() => shapeOf(props.schema));
/**
 * The value it starts from, drawn at once while the page takes it. Drawn
 * empty and filled in the same moment, an input would keep its first, empty
 * text: Vue's `v-model` sets an input's text once it is in the page, from
 * what it was made with.
 */
const start = value.value === undefined ? initial(shape.value) : undefined;
if (start !== undefined) value.value = start;
const current = computed(() => (value.value === undefined ? start : value.value));

/** The server's refusals, each with what its place held when it was refused. */
const standing = ref<{ fault: Fault; was: string | undefined }[]>([]);
const at = (fault: Fault): string | undefined => JSON.stringify(valueAt(current.value, fault.path));
watch(
  () => props.refused,
  (answer) => (standing.value = refusals(answer).map((fault) => ({ fault, was: at(fault) }))),
  { immediate: true },
);

const faults = computed(() => [
  ...faultsOf(shape.value, current.value),
  ...standing.value.filter((one) => at(one.fault) === one.was).map((one) => one.fault),
]);
watch(faults, (now) => emit("faults", now), { immediate: true });
const shown = computed(() => placed(faults.value, placesOf(shape.value, current.value)));
</script>

<template>
  <div class="schema-form" role="group" :aria-label="label">
    <SchemaField
      :model-value="current"
      :shape="shape"
      :path="[]"
      :label="label"
      :faults="shown"
      :readonly="readonly"
      @update:model-value="value = $event"
    />
  </div>
</template>

<style scoped>
.schema-form {
  display: grid;
  gap: var(--space-3);
  max-inline-size: 40rem;
}
</style>
