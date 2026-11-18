<script setup lang="ts">
import { computed, reactive, ref } from "vue";
import { useI18n } from "vue-i18n";

import GalleryHeader from "@/features/dev/GalleryHeader.vue";
import Button from "@/ui/Button.vue";
import Checkbox from "@/ui/Checkbox.vue";
import Combobox from "@/ui/Combobox.vue";
import Field from "@/ui/Field.vue";
import IconButton from "@/ui/IconButton.vue";
import NumberField from "@/ui/NumberField.vue";
import RadioGroup from "@/ui/RadioGroup.vue";
import Select from "@/ui/Select.vue";
import Slider from "@/ui/Slider.vue";
import Switch from "@/ui/Switch.vue";
import TextArea from "@/ui/TextArea.vue";
import TextField from "@/ui/TextField.vue";
import ToggleButton from "@/ui/ToggleButton.vue";
import ToggleGroup from "@/ui/ToggleGroup.vue";
import Tooltip from "@/ui/Tooltip.vue";

/**
 * Every control, in each of its states (ADR-0042): to try from the keyboard,
 * in both themes. What each holds is written beside it, so that a change can
 * be seen, and checked.
 */
const { t } = useI18n();

const presses = ref(0);
const state = reactive({
  busy: false,
  grid: true,
  objective: "20",
  overlays: ["scale"] as string[],
  invert: false,
  autofocus: true,
  live: false,
  order: "snake",
  channel: "green",
  name: "",
  address: t("inputs.text.addressValue"),
  notes: "",
  x: 1250 as number | null,
  step: 0.5 as number | null,
  overlap: 0.35 as number | null,
  delay: null as number | null,
  exposure: 20,
  focus: 0,
  workflow: "histo",
  onCapture: undefined as string | undefined,
  node: undefined as string | undefined,
});

const objectives = ["4", "10", "20", "40"].map((value) => ({ value, label: `${value}×` }));
const overlays = computed(() => [
  { value: "scale", label: t("inputs.buttons.scaleBar") },
  { value: "grid", label: t("inputs.buttons.grid") },
  { value: "cross", label: t("inputs.buttons.crosshair") },
]);
const orders = computed(() => [
  { value: "snake", label: t("inputs.choices.snake") },
  { value: "raster", label: t("inputs.choices.raster") },
  { value: "spiral", label: t("inputs.choices.spiral"), disabled: true },
]);
const channels = computed(() => [
  { value: "red", label: t("inputs.choices.red") },
  { value: "green", label: t("inputs.choices.green") },
  { value: "blue", label: t("inputs.choices.blue") },
]);
const workflows = computed(() => [
  { value: "histo", label: t("inputs.lists.histo") },
  { value: "grid", label: t("inputs.lists.grid") },
  { value: "chip", label: t("inputs.lists.chip") },
  { value: "lapse", label: t("inputs.lists.timelapse"), disabled: true },
]);
const captures = computed(() => [
  { value: "save", label: t("inputs.lists.save") },
  { value: "download", label: t("inputs.lists.download") },
]);
// The server's own names for its nodes: identifiers, the same in any language.
const nodes = [
  "background.subtract",
  "colour.balance",
  "deconvolve.richardson-lucy",
  "denoise.gaussian",
  "denoise.median",
  "flatfield.correct",
  "focus.stack",
  "register.phase",
  "sharpen.unsharp",
  "stain.separate",
  "stitch.mosaic",
  "threshold.otsu",
].map((value) => ({ value, label: value }));
const cameras = computed(() => [{ value: "sim", label: t("inputs.lists.cameraValue") }]);

const shown = (value: unknown): string =>
  value === null || value === undefined || value === "" ? t("inputs.none") : String(value);
</script>

<template>
  <main class="inputs">
    <GalleryHeader :title="t('inputs.title')" :intro="t('inputs.intro')" />

    <section>
      <h2>{{ t("inputs.buttons.title") }}</h2>
      <div class="row">
        <Button variant="primary" @click="presses++">{{ t("inputs.buttons.move") }}</Button>
        <Button>{{ t("inputs.buttons.setHome") }}</Button>
        <Button variant="bare" icon="refresh">{{ t("inputs.buttons.refresh") }}</Button>
        <Button icon="camera">{{ t("inputs.buttons.capture") }}</Button>
        <Button tone="danger">{{ t("inputs.buttons.remove") }}</Button>
        <Button variant="primary" tone="danger">{{ t("inputs.buttons.removeAll") }}</Button>
        <Button variant="primary" disabled>{{ t("inputs.buttons.off") }}</Button>
        <Button disabled>{{ t("inputs.buttons.off") }}</Button>
      </div>
      <div class="row">
        <Button variant="primary" compact>{{ t("inputs.buttons.compact") }}</Button>
        <Button compact icon="download">{{ t("inputs.buttons.compact") }}</Button>
        <Button variant="primary" :busy="state.busy" @click="presses++">
          {{ t("inputs.buttons.busy") }}
        </Button>
        <Switch v-model="state.busy">{{ t("inputs.buttons.makeBusy") }}</Switch>
        <output data-value="presses">{{ t("inputs.buttons.pressed", { count: presses }) }}</output>
      </div>

      <h3>{{ t("inputs.buttons.icons") }}</h3>
      <div class="row">
        <IconButton icon="add" :label="t('inputs.buttons.add')" />
        <IconButton icon="settings" :label="t('inputs.buttons.settings')" variant="outline" />
        <IconButton icon="download" :label="t('inputs.buttons.download')" variant="primary" />
        <IconButton icon="close" :label="t('inputs.buttons.close')" compact />
        <IconButton icon="delete" :label="t('inputs.buttons.remove')" tone="danger" disabled />
      </div>

      <h3>{{ t("inputs.buttons.toggles") }}</h3>
      <div class="row">
        <ToggleButton v-model="state.grid" icon="menu">{{ t("inputs.buttons.grid") }}</ToggleButton>
        <output data-value="grid">{{ t("inputs.now", { value: shown(state.grid) }) }}</output>
      </div>
      <div class="row">
        <Field :label="t('inputs.buttons.objective')">
          <ToggleGroup v-model="state.objective" :options="objectives" required />
        </Field>
        <output data-value="objective">{{
          t("inputs.now", { value: shown(state.objective) })
        }}</output>
      </div>
      <div class="row">
        <Field :label="t('inputs.buttons.overlays')">
          <ToggleGroup v-model="state.overlays" :options="overlays" multiple compact />
        </Field>
        <output data-value="overlays">{{
          t("inputs.now", { value: shown(state.overlays.join(", ")) })
        }}</output>
      </div>
    </section>

    <section>
      <h2>{{ t("inputs.choices.title") }}</h2>
      <div class="column">
        <Checkbox v-model="state.invert">{{ t("inputs.choices.invert") }}</Checkbox>
        <Checkbox indeterminate>{{ t("inputs.choices.some") }}</Checkbox>
        <Checkbox disabled>{{ t("inputs.choices.off") }}</Checkbox>
        <Checkbox :model-value="true" readonly>{{ t("inputs.choices.fixed") }}</Checkbox>
        <output data-value="invert">{{ t("inputs.now", { value: shown(state.invert) }) }}</output>
      </div>
      <div class="fields">
        <Field :label="t('inputs.choices.order')">
          <RadioGroup v-model="state.order" :options="orders" />
        </Field>
        <Field :label="t('inputs.choices.channel')">
          <RadioGroup v-model="state.channel" :options="channels" row />
        </Field>
        <output data-value="order">{{ t("inputs.now", { value: shown(state.order) }) }}</output>
      </div>
      <div class="column">
        <Switch v-model="state.autofocus">{{ t("inputs.choices.autofocus") }}</Switch>
        <Switch v-model="state.live">{{ t("inputs.choices.live") }}</Switch>
        <Switch disabled>{{ t("inputs.choices.off") }}</Switch>
        <Switch :model-value="true" readonly>{{ t("inputs.choices.fixed") }}</Switch>
        <output data-value="live">{{ t("inputs.now", { value: shown(state.live) }) }}</output>
      </div>
    </section>

    <section>
      <h2>{{ t("inputs.text.title") }}</h2>
      <div class="grid">
        <Field :label="t('inputs.text.name')" :hint="t('inputs.text.nameHint')">
          <TextField v-model="state.name" />
        </Field>
        <Field :label="t('inputs.text.address')" :error="t('inputs.text.addressError')">
          <TextField v-model="state.address" :placeholder="t('inputs.text.placeholder')" />
        </Field>
        <Field :label="t('inputs.text.operator')">
          <TextField :model-value="t('inputs.choices.off')" disabled />
        </Field>
        <Field :label="t('inputs.text.serial')">
          <TextField model-value="MIC-20-0421" readonly />
        </Field>
        <Field :label="t('inputs.text.compact')">
          <TextField v-model="state.name" compact />
        </Field>
        <Field :label="t('inputs.text.notes')" :hint="t('inputs.text.notesHint')">
          <TextArea v-model="state.notes" />
        </Field>
      </div>
      <output data-value="name">{{ t("inputs.now", { value: shown(state.name) }) }}</output>
    </section>

    <section>
      <h2>{{ t("inputs.numbers.title") }}</h2>
      <div class="grid">
        <Field :label="t('inputs.numbers.x')">
          <NumberField v-model="state.x" :min="-5000" :max="5000" :step="10" unit="µm" />
        </Field>
        <Field :label="t('inputs.numbers.step')">
          <NumberField v-model="state.step" :min="0" :step="0.1" unit="µm" compact />
        </Field>
        <Field :label="t('inputs.numbers.overlap')" :hint="t('inputs.numbers.overlapHint')">
          <NumberField v-model="state.overlap" :min="0.1" :max="0.7" :step="0.05" :digits="2" />
        </Field>
        <Field :label="t('inputs.numbers.delay')" :hint="t('inputs.numbers.delayHint')">
          <NumberField v-model="state.delay" :min="0" unit="s" optional />
        </Field>
        <Field :label="t('inputs.numbers.temperature')">
          <NumberField :model-value="36.6" unit="°C" readonly />
        </Field>
        <Field :label="t('inputs.numbers.gain')">
          <NumberField :model-value="2" unit="dB" disabled />
        </Field>
      </div>
      <output data-value="x">{{ t("inputs.now", { value: shown(state.x) }) }}</output>
      <output data-value="step">{{ t("inputs.now", { value: shown(state.step) }) }}</output>
      <output data-value="overlap">{{ t("inputs.now", { value: shown(state.overlap) }) }}</output>
      <output data-value="delay">{{ t("inputs.now", { value: shown(state.delay) }) }}</output>
      <div class="grid">
        <Field :label="t('inputs.numbers.exposure')">
          <Slider v-model="state.exposure" :min="1" :max="200" unit="ms" />
        </Field>
        <Field :label="t('inputs.numbers.focus')">
          <Slider v-model="state.focus" :min="-50" :max="50" :step="5" bare />
        </Field>
        <Field :label="t('inputs.numbers.brightness')">
          <Slider :model-value="60" unit="%" disabled />
        </Field>
        <Field :label="t('inputs.numbers.lamp')">
          <Slider :model-value="40" unit="%" readonly />
        </Field>
      </div>
      <output data-value="exposure">{{ t("inputs.now", { value: shown(state.exposure) }) }}</output>
      <output data-value="focus">{{ t("inputs.now", { value: shown(state.focus) }) }}</output>
    </section>

    <section>
      <h2>{{ t("inputs.lists.title") }}</h2>
      <div class="grid">
        <Field :label="t('inputs.lists.workflow')">
          <Select v-model="state.workflow" :options="workflows" />
        </Field>
        <Field :label="t('inputs.lists.onCapture')">
          <Select v-model="state.onCapture" :options="captures" compact />
        </Field>
        <Field :label="t('inputs.lists.node')" :hint="t('inputs.lists.nodeHint')">
          <Combobox v-model="state.node" :options="nodes" />
        </Field>
        <Field :label="t('inputs.lists.camera')">
          <Select model-value="sim" :options="cameras" readonly />
        </Field>
        <Field :label="t('inputs.lists.camera')">
          <Select model-value="sim" :options="cameras" disabled />
        </Field>
      </div>
      <output data-value="workflow">{{ t("inputs.now", { value: shown(state.workflow) }) }}</output>
      <output data-value="onCapture">{{
        t("inputs.now", { value: shown(state.onCapture) })
      }}</output>
      <output data-value="node">{{ t("inputs.now", { value: shown(state.node) }) }}</output>
    </section>

    <section>
      <h2>{{ t("inputs.tooltips.title") }}</h2>
      <div class="row">
        <Tooltip :text="t('inputs.tooltips.fillTip')">
          <Button>{{ t("inputs.tooltips.fill") }}</Button>
        </Tooltip>
      </div>
    </section>
  </main>
</template>

<style scoped>
.inputs {
  display: grid;
  gap: var(--space-8);
  max-inline-size: 72rem;
  padding: var(--space-6);
}

section {
  display: grid;
  gap: var(--space-3);
  justify-items: start;
}

.row {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3);
  align-items: center;
}

/* Fields side by side, each as wide as what it holds. */
.fields {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-6);
  align-items: start;
}

.column {
  display: grid;
  gap: var(--space-1);
  justify-items: start;
}

/* Fields as wide as a control pane's, side by side while there is room. */
.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, 22rem);
  gap: var(--space-4);
  align-items: start;
  inline-size: 100%;
}

.grid > * {
  inline-size: 100%;
}

output {
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
