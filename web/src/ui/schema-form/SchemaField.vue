<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

import Button from "../Button.vue";
import Checkbox from "../Checkbox.vue";
import Field from "../Field.vue";
import IconButton from "../IconButton.vue";
import NumberField from "../NumberField.vue";
import Select from "../Select.vue";
import Switch from "../Switch.vue";
import TextArea from "../TextArea.vue";
import TextField from "../TextField.vue";
import { keyOf, same, type Fault, type Path } from "./check";
import {
  initial,
  innerOf,
  joined,
  nameOf,
  variantOf,
  without,
  type Field as FieldShape,
  type Json,
  type Of,
  type Shape,
} from "./shape";

/**
 * One value of a form drawn from a JSON Schema (W0.6), by its shape: a field,
 * or a group of them. It holds the groups inside it, so it draws itself.
 */
const props = defineProps<{
  shape: Shape;
  /** Where its value is, from the whole. */
  path: Path;
  /** What it is called. */
  label: string;
  /** Faults by the place each is shown. */
  faults: ReadonlyMap<string, Fault[]>;
  readonly?: boolean;
  /** Its label is said, and not shown: it stands under the box that gives it. */
  quiet?: boolean;
  /** An object's fields go in the group around it, without one of their own. */
  bare?: boolean;
  /** Left out, not `null`, when it isn't given: a field that needn't be there. */
  omit?: boolean;
}>();
const value = defineModel<Json | undefined>({ required: true });
const { t } = useI18n();

/** What is said of a value at a place: the server's words, or ours. */
const say = (fault: Fault): string => fault.text ?? t(`ui.form.${fault.why}`, fault.values ?? {});
const errorAt = (path: Path): string | undefined => {
  const here = props.faults.get(keyOf(path));
  return here?.length ? here.map(say).join(" ") : undefined;
};
const error = computed(() => errorAt(props.path));

/** A description's first sentence: what a hint has room for. */
function brief(text: string | undefined): string | undefined {
  const first = text
    ?.split(/\n\s*\n/)[0]
    ?.replace(/\s+/g, " ")
    .trim();
  if (!first) return undefined;
  const end = first.search(/[.!?](\s|$)/);
  return end >= 0 ? first.slice(0, end + 1) : first;
}
const hint = computed(() => brief(props.shape.description ?? props.shape.title));

const isObject = (one: unknown): one is Record<string, Json> =>
  typeof one === "object" && one !== null && !Array.isArray(one);

// An object, and a map: their fields.
const fields = computed<Record<string, Json>>(() => (isObject(value.value) ? value.value : {}));
/**
 * A field that needn't be given, and has no default, is drawn as one that may
 * be left out. One with a default is given, from its default.
 */
function fieldShape(field: FieldShape): Shape {
  const shape = field.shape;
  if (field.required || shape.kind === "optional" || shape.default !== undefined) return shape;
  return {
    kind: "optional",
    inner: shape,
    ...(shape.title && { title: shape.title }),
    ...(shape.description && { description: shape.description }),
  };
}
/** A value drawn without a name of its own on the screen: one of a list's, or a map's. */
const SIMPLE = new Set(["number", "string", "choice", "boolean", "constant", "any", "json"]);
const simple = (shape: Shape): boolean => SIMPLE.has(shape.kind);
function set(name: string, now: Json | undefined): void {
  value.value = now === undefined ? without(fields.value, name) : { ...fields.value, [name]: now };
}

// A map's names, changed in place.
const taken = ref<string>();
function rename(from: string, to: string): void {
  if (to !== from && to in fields.value) {
    taken.value = from;
    return;
  }
  taken.value = undefined;
  value.value = Object.fromEntries(
    Object.entries(fields.value).map(([key, one]) => [key === from ? to : key, one]),
  );
}
function addEntry(values: Shape): void {
  let n = Object.keys(fields.value).length + 1;
  while (`${t("ui.form.entry")} ${n}` in fields.value) n++;
  value.value = { ...fields.value, [`${t("ui.form.entry")} ${n}`]: initial(values) };
}

// A list.
const items = computed<Json[]>(() => (Array.isArray(value.value) ? value.value : []));
const array = computed(() => props.shape as Of<"array">);
function setItem(at: number, now: Json | undefined): void {
  value.value = items.value.map((item, i) => (i === at ? (now ?? null) : item));
}
const removeItem = (at: number): void => {
  value.value = items.value.filter((_, i) => i !== at);
};
const addItem = (): void => {
  value.value = [...items.value, initial(array.value.items)];
};

// A value that may be left out.
const given = computed(() => value.value !== null && value.value !== undefined);
function give(on: boolean): void {
  const optional = props.shape as Of<"optional">;
  value.value = on
    ? (optional.inner.default ?? initial(optional.inner))
    : props.omit
      ? undefined
      : null;
}

// A union: the kind chosen, and what it holds.
const union = computed(() => props.shape as Of<"union">);
const kind = computed(() =>
  value.value === undefined ? undefined : variantOf(union.value, value.value),
);
const kinds = computed(() =>
  union.value.variants.map((one) => ({ value: one.value, label: one.label })),
);
function choose(name: string | undefined): void {
  const chosen = union.value.variants.find((one) => one.value === name);
  if (chosen && chosen !== kind.value)
    value.value = joined(union.value, chosen, initial(chosen.shape));
}
const tagPath = computed(() =>
  union.value.tag === null ? props.path : [...props.path, union.value.tag],
);
const innerPath = computed(() =>
  union.value.tag === null && kind.value ? [...props.path, kind.value.value] : props.path,
);

// A number, a choice.
const number = computed(() => props.shape as Of<"number">);
const step = computed(() => {
  const { integer, min, max } = number.value;
  return !integer && min !== undefined && max !== undefined && max - min <= 10 ? 0.1 : 1;
});
const options = computed(() =>
  props.shape.kind === "choice"
    ? props.shape.options.map((option) => ({
        value: JSON.stringify(option.value),
        label: option.label,
      }))
    : [],
);
const chosenOption = computed(() =>
  props.shape.kind === "choice"
    ? props.shape.options.find((option) => same(option.value, value.value))
    : undefined,
);

// JSON, written by hand: taken whenever it reads as JSON.
const written = (one: Json | undefined): string => JSON.stringify(one ?? null, null, 2);
const json = ref(written(value.value));
const unread = ref<string>();
function typed(text: string): void {
  json.value = text;
  try {
    const read = JSON.parse(text) as Json;
    unread.value = undefined;
    value.value = read;
  } catch (failure) {
    unread.value = t("ui.form.json", { why: (failure as Error).message });
  }
}
watch(value, (now) => {
  if (unread.value !== undefined) return;
  let shown: unknown;
  try {
    shown = JSON.parse(json.value);
  } catch {
    shown = undefined;
  }
  if (!same(shown, now ?? null)) json.value = written(now);
});
</script>

<template>
  <!-- An object: its fields, in a group named by its label. -->
  <component
    :is="bare || path.length === 0 ? 'div' : 'fieldset'"
    v-if="shape.kind === 'object'"
    class="group"
    :class="{ bare: bare || path.length === 0 }"
  >
    <legend v-if="!bare && path.length > 0" :class="{ quiet }">{{ label }}</legend>
    <p v-if="error" class="note error">{{ error }}</p>
    <p v-else-if="hint && !bare && path.length > 0" class="note">{{ hint }}</p>
    <SchemaField
      v-for="field in shape.fields"
      :key="field.name"
      :shape="fieldShape(field)"
      :path="[...path, field.name]"
      :label="nameOf(field.name)"
      :faults="faults"
      :readonly="readonly"
      :omit="!field.required"
      :model-value="fields[field.name]"
      @update:model-value="set(field.name, $event)"
    />
    <p v-if="shape.fields.length === 0" class="note">{{ t("ui.form.empty") }}</p>
  </component>

  <!-- A map: names, each with its value. -->
  <fieldset v-else-if="shape.kind === 'map'" class="group">
    <legend :class="{ quiet }">{{ label }}</legend>
    <p v-if="error" class="note error">{{ error }}</p>
    <div v-for="(key, at) in Object.keys(fields)" :key="at" class="entry pair">
      <Field
        :label="t('ui.form.name', { name: key })"
        quiet
        :error="taken === key ? t('ui.form.taken') : undefined"
      >
        <TextField
          :model-value="key"
          :readonly="readonly"
          @update:model-value="rename(key, $event)"
        />
      </Field>
      <SchemaField
        :shape="shape.values"
        :path="[...path, key]"
        :label="key"
        :quiet="simple(shape.values)"
        :faults="faults"
        :readonly="readonly"
        :model-value="fields[key]"
        @update:model-value="set(key, $event ?? null)"
      />
      <IconButton
        v-if="!readonly"
        class="remove"
        icon="delete"
        :label="t('ui.form.remove', { name: key })"
        @click="set(key, undefined)"
      />
    </div>
    <Button v-if="!readonly" class="add" icon="add" compact @click="addEntry(shape.values)">
      {{ t("ui.form.addTo", { name: label }) }}
    </Button>
  </fieldset>

  <!-- A list: its items, each with a button to take it out. -->
  <fieldset v-else-if="shape.kind === 'array'" class="group">
    <legend :class="{ quiet }">{{ label }}</legend>
    <p v-if="error" class="note error">{{ error }}</p>
    <p v-else-if="hint" class="note">{{ hint }}</p>
    <div v-for="(item, at) in items" :key="at" class="entry">
      <SchemaField
        :shape="shape.items"
        :path="[...path, at]"
        :label="t('ui.form.item', { name: label, n: at + 1 })"
        :quiet="simple(shape.items)"
        :faults="faults"
        :readonly="readonly"
        :model-value="item"
        @update:model-value="setItem(at, $event)"
      />
      <IconButton
        v-if="!readonly"
        class="remove"
        icon="delete"
        :label="t('ui.form.remove', { name: t('ui.form.item', { name: label, n: at + 1 }) })"
        :disabled="items.length <= (shape.minItems ?? 0)"
        @click="removeItem(at)"
      />
    </div>
    <Button
      v-if="!readonly"
      class="add"
      icon="add"
      compact
      :disabled="shape.maxItems !== undefined && items.length >= shape.maxItems"
      @click="addItem"
    >
      {{ t("ui.form.addTo", { name: label }) }}
    </Button>
  </fieldset>

  <!-- A value that may be left out: a box that gives it, and the value under it. -->
  <div v-else-if="shape.kind === 'optional'" class="optional">
    <Checkbox :model-value="given" :readonly="readonly" @update:model-value="give">{{
      label
    }}</Checkbox>
    <SchemaField
      v-if="given"
      v-model="value"
      :shape="shape.inner"
      :path="path"
      :label="label"
      :faults="faults"
      :readonly="readonly"
      quiet
    />
    <p v-else-if="error" class="note error">{{ error }}</p>
    <p v-else-if="hint" class="note">{{ hint }}</p>
  </div>

  <!-- A union: its kind, and what that kind holds. -->
  <fieldset v-else-if="shape.kind === 'union'" class="group">
    <legend :class="{ quiet }">{{ label }}</legend>
    <p v-if="error && shape.tag !== null" class="note error">{{ error }}</p>
    <Field
      :label="shape.tag === null ? t('ui.form.kind') : nameOf(shape.tag)"
      :hint="brief(kind?.description)"
      :error="errorAt(tagPath)"
    >
      <Select
        :model-value="kind?.value"
        :options="kinds"
        :readonly="readonly"
        @update:model-value="choose"
      />
    </Field>
    <SchemaField
      v-if="kind && !kind.unit"
      :shape="kind.shape"
      :path="innerPath"
      :label="kind.label"
      :faults="faults"
      :readonly="readonly"
      bare
      :model-value="innerOf(shape, value ?? null)"
      @update:model-value="value = joined(shape, kind, $event ?? null)"
    />
  </fieldset>

  <Field
    v-else-if="shape.kind === 'number'"
    :label="label"
    :quiet="quiet"
    :hint="hint"
    :error="error"
  >
    <NumberField
      :model-value="typeof value === 'number' ? value : null"
      :min="shape.min"
      :max="shape.max"
      :step="step"
      :unit="shape.unit"
      :readonly="readonly"
      @update:model-value="value = $event"
    />
  </Field>

  <Field
    v-else-if="shape.kind === 'string'"
    :label="label"
    :quiet="quiet"
    :hint="hint"
    :error="error"
  >
    <TextField
      :model-value="typeof value === 'string' ? value : ''"
      :readonly="readonly"
      @update:model-value="value = $event"
    />
  </Field>

  <Field
    v-else-if="shape.kind === 'choice'"
    :label="label"
    :quiet="quiet"
    :hint="brief(chosenOption?.description) ?? hint"
    :error="error"
  >
    <Select
      :model-value="chosenOption ? JSON.stringify(chosenOption.value) : undefined"
      :options="options"
      :readonly="readonly"
      @update:model-value="value = $event === undefined ? null : (JSON.parse($event) as Json)"
    />
  </Field>

  <Field
    v-else-if="shape.kind === 'boolean'"
    :label="label"
    :quiet="quiet"
    :hint="hint"
    :error="error"
  >
    <Switch
      :model-value="value === true"
      :readonly="readonly"
      @update:model-value="value = $event"
    />
  </Field>

  <Field
    v-else-if="shape.kind === 'constant'"
    :label="label"
    :quiet="quiet"
    :hint="hint"
    :error="error"
  >
    <TextField :model-value="JSON.stringify(shape.value)" readonly />
  </Field>

  <!-- Any value, or one outside what the form draws: JSON, written by hand. -->
  <Field
    v-else
    :label="label"
    :quiet="quiet"
    :hint="shape.kind === 'json' ? t('ui.form.asJson', { why: shape.why }) : hint"
    :error="unread ?? error"
    :data-fallback="shape.kind === 'json' ? shape.why : undefined"
  >
    <TextArea
      class="json"
      :model-value="json"
      :readonly="readonly"
      :rows="4"
      @update:model-value="typed"
    />
  </Field>
</template>

<style scoped>
.group {
  display: grid;
  gap: var(--space-3);
  min-inline-size: 0;
  margin: 0;
  padding: var(--space-3);
  border: 1px solid var(--border);
  border-radius: var(--radius-control);
}

.group.bare {
  padding: 0;
  border: 0;
}

legend {
  padding-inline: var(--space-1);
  font-weight: var(--font-weight-strong);
}

/* Said by a screen reader, and not drawn: the box above it shows its name. */
legend.quiet {
  position: absolute;
  inline-size: 1px;
  block-size: 1px;
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}

.entry {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: var(--space-2);
  align-items: start;
}

/* A map's entry: its name, its value, and its button. */
.pair {
  grid-template-columns: minmax(8rem, 1fr) minmax(0, 2fr) auto;
}

.optional {
  display: grid;
  gap: var(--space-1);
}

.add {
  justify-self: start;
}

.note {
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

.error {
  color: var(--danger);
}

.json :deep(textarea) {
  font-family: var(--font-mono);
}
</style>
