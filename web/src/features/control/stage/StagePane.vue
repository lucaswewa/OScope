<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";

import { call, server } from "@/api/client";
import { thingStore } from "@/api/things/store";
import { useAction, useProperty } from "@/api/things/use";
import {
  AXES,
  PER,
  waitsForHoming,
  within,
  written,
  type Axis,
  type Step,
} from "@/features/control/stage/stage";
import { useChoices, XY_STEPS, Z_STEPS } from "@/features/control/stage/choices";
import { usePointing } from "@/features/control/stage/usePointing";
import { useStage } from "@/features/control/stage/useStage";
import Badge from "@/ui/Badge.vue";
import Banner from "@/ui/Banner.vue";
import Button from "@/ui/Button.vue";
import Field from "@/ui/Field.vue";
import IconButton from "@/ui/IconButton.vue";
import type { IconName } from "@/ui/icons";
import NumberField from "@/ui/NumberField.vue";
import Switch from "@/ui/Switch.vue";
import ToggleGroup from "@/ui/ToggleGroup.vue";
import { notify } from "@/ui/toasts";

/**
 * The stage (W3.3): where it is, in the display's unit, and what it is
 * doing; an alarm's reason, and clearing it; homing, which it waits for
 * after a start. Moves to a place, and by a step or for as long as a
 * button is held, from the jog pad; the speed. A move past the limits is
 * refused before it is sent. Whether a drag on the image moves the stage,
 * and what pointing at it lacks. Halt is the control pane's, at its foot.
 */
const { t } = useI18n();
const stage = useStage();
const choices = useChoices();
const pointing = usePointing();
const speed = useProperty("/api/v1/stage/speed", { every: 5_000 });
const home = useAction("/api/v1/stage/home");
const clearAlarm = useAction("/api/v1/stage/clear_alarm");

const unit = stage.unit;
const unitWord = computed(() => t(`stage.units.${unit.value}`));
const caps = computed(() => stage.capabilities.value.value);
/** Moves are checked against the limits from where the stage is: both must be known. */
const ready = computed(() => Boolean(caps.value && stage.here.value));
const state = computed(() => stage.status.value.value?.state);
const TONES = {
  idle: "neutral",
  moving: "accent",
  homing: "accent",
  hold: "warning",
  alarm: "danger",
} as const;
const alarm = computed(() => {
  const now = stage.status.value.value;
  return now?.state === "alarm" ? now.reason : undefined;
});
const waits = computed(
  () => alarm.value !== undefined && caps.value?.homing === true && waitsForHoming(alarm.value),
);

/** An axis's travel in the display's unit. */
function travel(axis: Axis): { min?: number; max?: number } {
  const limits = caps.value?.[axis];
  return limits ? { min: limits.min / PER[unit.value], max: limits.max / PER[unit.value] } : {};
}

// The place to move to, typed in the display's unit: an axis left empty stays where it is,
// within the travel, as the stage takes no target past it: homing leaves it a micrometre past.
const target = ref<Record<Axis, number | null>>({ x: null, y: null, z: null });
function moveToTarget(): void {
  const here = stage.here.value;
  const limits = caps.value;
  if (!here || !limits) return;
  const to = (axis: Axis) => {
    const typed = target.value[axis];
    return typed === null ? within(here[axis], limits[axis]) : typed * PER[unit.value];
  };
  void stage.goTo({ x: to("x"), y: to("y"), z: to("z") });
}
/** The middle of the travel, as W2.1 had it. */
function centre(): void {
  const limits = caps.value;
  if (!limits) return;
  const half = (axis: Axis) => (limits[axis].min + limits[axis].max) / 2;
  void stage.goTo({ x: half("x"), y: half("y"), z: half("z") });
}

// The jog pad: a step at each press, or a jog for as long as it is held.
// A step is said in the unit that keeps it short: 0.5 µm, 1 mm.
function stepWord(size: string): string {
  if (size === "continuous") return t("stage.continuous");
  const um = Number(size);
  return um >= 1000
    ? t("stage.length.mm", { length: um / 1000 })
    : t("stage.length.um", { length: um });
}
const xyChoices = computed(() => XY_STEPS.map((value) => ({ value, label: stepWord(value) })));
const zChoices = computed(() => Z_STEPS.map((value) => ({ value, label: stepWord(value) })));
const PAD: readonly { step: Step; icon: IconName; place: string }[] = [
  { step: { axis: "y", towardsMax: false }, icon: "up", place: "y-min" },
  { step: { axis: "x", towardsMax: false }, icon: "back", place: "x-min" },
  { step: { axis: "x", towardsMax: true }, icon: "forward", place: "x-max" },
  { step: { axis: "y", towardsMax: true }, icon: "down", place: "y-max" },
  { step: { axis: "z", towardsMax: true }, icon: "raise", place: "z-max" },
  { step: { axis: "z", towardsMax: false }, icon: "lower", place: "z-min" },
];
const padLabel = (step: Step) =>
  t(step.towardsMax ? "stage.towardsMax" : "stage.towardsMin", { axis: step.axis.toUpperCase() });
function click(step: Step): void {
  const size = choices.size(step.axis);
  if (size !== undefined) void stage.step(step, size);
}
function press(event: PointerEvent, step: Step): void {
  if (event.button !== 0 || choices.size(step.axis) !== undefined) return;
  (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  stage.jog(step);
}
const pressing = (event: KeyboardEvent) => event.key === " " || event.key === "Enter";
function keyDown(event: KeyboardEvent, step: Step): void {
  if (!pressing(event) || choices.size(step.axis) !== undefined) return;
  // The press is the jog's: no click follows it.
  event.preventDefault();
  if (!event.repeat) stage.jog(step);
}
function keyUp(event: KeyboardEvent): void {
  if (pressing(event)) void stage.endJog();
}
const release = () => void stage.endJog();

// The speed, in the display's unit a second: cleared, it is the stage's own again.
const speedIn = computed(() => {
  const now = speed.value.value;
  return now === null || now === undefined ? null : now / PER[unit.value];
});
async function setSpeed(value: number | null): Promise<void> {
  if (value !== null) {
    await speed.write(value * PER[unit.value]);
    return;
  }
  try {
    await call((signal) => server.api.POST("/api/v1/stage/speed/reset", { signal }));
  } catch (thrown) {
    const why = thrown instanceof Error ? thrown.message : String(thrown);
    notify({ level: "danger", text: t("stage.notReset", { why }) });
  }
  await thingStore("stage").read("speed");
}
</script>

<template>
  <section class="stage" aria-labelledby="stage-title" data-said="stage">
    <div class="title">
      <h2 id="stage-title">{{ t("stage.title") }}</h2>
      <Badge v-if="state" :tone="TONES[state]" data-said="stage-state">
        {{ t(`stage.states.${state}`) }}
      </Badge>
    </div>
    <dl class="where" data-said="position">
      <div v-for="axis in AXES" :key="axis">
        <dt>{{ axis.toUpperCase() }}</dt>
        <dd>
          {{ stage.here.value ? written(stage.here.value[axis], unit) : t("stage.none") }}
          {{ unitWord }}
        </dd>
      </div>
    </dl>

    <Banner
      v-if="alarm !== undefined"
      :level="waits ? 'warning' : 'danger'"
      :title="waits ? t('stage.waits') : t('stage.inAlarm')"
      data-said="alarm"
    >
      {{ waits ? t("stage.homeFirst") : alarm }}
      <template v-if="!waits" #actions>
        <Button compact :busy="clearAlarm.running.value" @click="clearAlarm.run({})">
          {{ t("stage.clear") }}
        </Button>
      </template>
    </Banner>

    <div class="row">
      <Button :busy="home.running.value" @click="home.run({})">{{ t("stage.home") }}</Button>
      <Button :disabled="!caps" @click="centre">{{ t("stage.centre") }}</Button>
    </div>

    <fieldset class="set">
      <legend>{{ t("stage.moveTo") }}</legend>
      <div class="axes">
        <Field v-for="axis in AXES" :key="axis" :label="axis.toUpperCase()">
          <NumberField
            v-model="target[axis]"
            compact
            optional
            :min="travel(axis).min"
            :max="travel(axis).max"
            :unit="unitWord"
            :placeholder="stage.here.value ? written(stage.here.value[axis], unit) : undefined"
          />
        </Field>
      </div>
      <Button :busy="stage.moveTo.running.value" :disabled="!ready" @click="moveToTarget">
        {{ t("stage.move") }}
      </Button>
    </fieldset>

    <fieldset class="set">
      <legend>{{ t("stage.jog") }}</legend>
      <Field :label="t('stage.stepXy')">
        <ToggleGroup v-model="choices.xy" :options="xyChoices" required compact />
      </Field>
      <Field :label="t('stage.stepZ')">
        <ToggleGroup v-model="choices.z" :options="zChoices" required compact />
      </Field>
      <div class="pad" role="group" :aria-label="t('stage.pad')">
        <IconButton
          v-for="one in PAD"
          :key="one.place"
          :style="{ gridArea: one.place, touchAction: 'none' }"
          :icon="one.icon"
          :label="padLabel(one.step)"
          variant="outline"
          :disabled="!ready"
          @click="click(one.step)"
          @pointerdown="press($event, one.step)"
          @pointerup="release"
          @pointercancel="release"
          @lostpointercapture="release"
          @keydown="keyDown($event, one.step)"
          @keyup="keyUp"
        />
      </div>
    </fieldset>

    <fieldset class="set">
      <legend>{{ t("stage.pointing.title") }}</legend>
      <Switch v-model="choices.drags">{{ t("stage.pointing.drags") }}</Switch>
      <p class="said" data-said="pointing">
        {{ pointing.missing.value ?? t("stage.pointing.ready") }}
      </p>
    </fieldset>

    <Field :label="t('stage.speed')" :error="speed.refused.value?.message">
      <NumberField
        :model-value="speedIn"
        optional
        :min="1 / PER[unit]"
        :max="caps ? caps.max_speed / PER[unit] : undefined"
        :unit="t(`stage.speeds.${unit}`)"
        :placeholder="
          caps ? t('stage.ownSpeed', { speed: written(caps.default_speed, unit) }) : undefined
        "
        @update:model-value="setSpeed"
      />
    </Field>
  </section>
</template>

<style scoped>
.stage {
  display: grid;
  gap: var(--space-2);
}

.title {
  display: flex;
  gap: var(--space-2);
  align-items: center;
}

.title h2 {
  margin: 0;
}

.where {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: var(--space-2);
  margin: 0;
  font-variant-numeric: tabular-nums;
}

.where dt {
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

.where dd {
  margin: 0;
  white-space: nowrap;
}

.row {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
}

.set {
  display: grid;
  gap: var(--space-2);
  justify-items: start;
  margin: 0;
  padding: var(--space-2);
  border: 1px solid var(--border);
  border-radius: var(--radius-surface);
}

.set legend {
  padding-inline: var(--space-1);
}

.said {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

.axes {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: var(--space-2);
  justify-self: stretch;
}

/* A cross for x and y, and a column for z beside it. */
.pad {
  display: grid;
  grid-template-areas:
    ". y-min . . z-max"
    "x-min . x-max . ."
    ". y-max . . z-min";
  grid-template-columns: repeat(3, auto) var(--space-3) auto;
  gap: var(--space-1);
}
</style>
