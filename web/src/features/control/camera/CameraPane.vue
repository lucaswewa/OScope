<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

import { useAction, useProperty } from "@/api/things/use";
import {
  cameraFields,
  differences,
  formatKey,
  type Format,
  type Rect,
  type Trigger,
} from "@/features/control/camera/cameraFields";
import CameraTemperature from "@/features/control/camera/CameraTemperature.vue";
import { useJobAction } from "@/features/control/jobAction";
import Button from "@/ui/Button.vue";
import Field from "@/ui/Field.vue";
import NumberField from "@/ui/NumberField.vue";
import Select from "@/ui/Select.vue";

/**
 * The camera (W3.2): its exposure and gain, its mode and pixel format, the
 * region read out and the trigger, each within what the camera says it can
 * do, and its temperature. Auto exposure and white balance run as jobs, and
 * say what they found. A refusal shows at its field, in the camera's words.
 * Settings changed behind the pane, as auto exposure changes the exposure,
 * differ from those saved, and can be put back.
 */
const { t } = useI18n();
const capabilities = useProperty("/api/v1/camera/capabilities", { every: 60_000 });
const fields = computed(() => {
  const caps = capabilities.value.value;
  return caps ? cameraFields(caps) : undefined;
});
// Read every few seconds, and again at each of the camera's events and
// after each write; `saved` is kept by the camera's socket.
const exposure = useProperty("/api/v1/camera/exposure", { every: 5_000 });
const gain = useProperty("/api/v1/camera/gain", { every: 5_000 });
const roi = useProperty("/api/v1/camera/roi", { every: 5_000 });
const trigger = useProperty("/api/v1/camera/trigger", { every: 5_000 });
const settings = useProperty("/api/v1/camera/settings", { every: 5_000 });
const saved = useProperty("/api/v1/camera/saved");
const streaming = useProperty("/api/v1/camera/streaming", { every: 1_000 });
const setMode = useAction("/api/v1/camera/set_mode");
const auto = useJobAction("/api/v1/exposure/auto");
const balance = useJobAction("/api/v1/exposure/white_balance");
const last = useProperty("/api/v1/exposure/last", { every: 5_000 });
const lastBalance = useProperty("/api/v1/exposure/last_white_balance", { every: 5_000 });

const one = (value: number, digits = 1) => value.toFixed(digits);

/** A format, as people say it. */
function formatName(format: Format): string {
  switch (format.kind) {
    case "bayer16":
      return t("camera.formats.bayer16", {
        pattern: format.pattern,
        bits: format.significant_bits,
      });
    case "bayer8":
      return t("camera.formats.bayer8", { pattern: format.pattern });
    case "planar_f32":
      return t("camera.formats.planar", { planes: format.planes });
    default:
      return t(`camera.formats.${format.kind}`);
  }
}

// The mode and the format are chosen, then applied together.
const mode = ref<string>();
const format = ref<string>();
// The chosen mode and format follow the camera's when those change, not at
// each read of its settings: a choice not yet applied is kept.
watch(
  () => {
    const now = settings.value.value;
    return now && `${now.mode}|${formatKey(now.format)}`;
  },
  () => {
    const now = settings.value.value;
    if (!now) return;
    mode.value = now.mode;
    format.value = formatKey(now.format);
  },
  { immediate: true },
);
const modes = computed(() =>
  (fields.value?.modes ?? []).map((item) => {
    const said = { name: item.name, width: item.width, height: item.height, binning: item.binning };
    return {
      value: item.name,
      label: t(item.binning > 1 ? "camera.modeBinned" : "camera.mode", said),
    };
  }),
);
const formats = computed(() =>
  mode.value && fields.value
    ? fields.value.formats(mode.value).map((choice) => ({
        value: choice.value,
        label: t("camera.formatAt", { format: formatName(choice.format), fps: one(choice.fps) }),
      }))
    : [],
);
watch(formats, (now) => {
  if (now.length && !now.some((choice) => choice.value === format.value)) {
    format.value = now[0]!.value;
  }
});
const modeChanged = computed(() => {
  const now = settings.value.value;
  return Boolean(now && (now.mode !== mode.value || formatKey(now.format) !== format.value));
});
function applyMode(): void {
  const chosen = fields.value
    ?.formats(mode.value ?? "")
    .find((choice) => choice.value === format.value);
  if (mode.value && chosen) void setMode.run({ mode: mode.value, format: chosen.format });
}

// The region, typed, then applied; or the whole sensor.
const region = ref<Rect>({ x: 0, y: 0, width: 0, height: 0 });
// The typed region follows the camera's when that changes, as the chosen mode does.
watch(
  () => JSON.stringify([roi.value.value ?? null, fields.value?.roi?.width]),
  () => {
    const sensor = fields.value?.roi;
    const now = roi.value.value;
    region.value = now ?? { x: 0, y: 0, width: sensor?.width ?? 0, height: sensor?.height ?? 0 };
  },
  { immediate: true },
);
const sides = ["x", "y", "width", "height"] as const;
/**
 * The whole sensor, as a region: the server takes no `null` for a property
 * (it answers that a body is needed), so the region is written whole.
 */
const wholeSensor = (): Rect => ({
  x: 0,
  y: 0,
  width: fields.value?.roi?.width ?? 0,
  height: fields.value?.roi?.height ?? 0,
});

// The trigger: its kind, and an external one's edge.
const triggers = computed(() =>
  (fields.value?.triggers ?? []).map((kind) => ({
    value: kind,
    label: t(`camera.triggers.${kind}`),
  })),
);
const edges = computed(() =>
  (["rising", "falling"] as const).map((edge) => ({
    value: edge,
    label: t(`camera.edges.${edge}`),
  })),
);
function setTrigger(kind: string | undefined, edge: "rising" | "falling" = "rising"): void {
  const next = (kind === "external" ? { kind, edge } : { kind }) as Trigger;
  void trigger.write(next);
}

// What the jobs found.
const found = computed(() => {
  const now = last.value.value;
  if (!now) return undefined;
  const values = {
    exposure: one(now.exposure_ms, 2),
    level: one(now.level, 2),
    steps: now.steps.length,
  };
  return now.converged ? t("camera.auto.found", values) : t("camera.auto.short", values);
});
const gains = computed(() => {
  const now = lastBalance.value.value;
  return now
    ? t("camera.balance.gains", { gains: now.gains.map((g) => one(g, 2)).join(", ") })
    : undefined;
});

// Settings changed behind the pane, and putting the saved ones back.
const changed = computed(() =>
  differences(
    settings.value.value as Record<string, unknown> | undefined,
    saved.value.value as Record<string, unknown> | undefined,
  ),
);
const putting = ref(false);
async function putBack(): Promise<void> {
  const was = saved.value.value;
  if (!was) return;
  putting.value = true;
  try {
    if (changed.value.includes("mode") || changed.value.includes("format")) {
      await setMode.run({ mode: was.mode, format: was.format });
    }
    if (changed.value.includes("exposure")) await exposure.write(was.exposure);
    if (changed.value.includes("gain")) await gain.write(was.gain);
    if (changed.value.includes("roi")) await roi.write(was.roi ?? null);
    if (changed.value.includes("trigger")) await trigger.write(was.trigger);
  } finally {
    putting.value = false;
  }
}
</script>

<template>
  <section class="camera" aria-labelledby="camera-title" data-said="camera">
    <h2 id="camera-title">{{ t("camera.title") }}</h2>
    <p v-if="capabilities.value.value" class="about">
      {{
        t("camera.about", {
          model: capabilities.value.value.model,
          width: capabilities.value.value.sensor_width,
          height: capabilities.value.value.sensor_height,
        })
      }}
      <CameraTemperature v-if="fields?.temperature" />
    </p>

    <div class="row">
      <Field class="grow" :label="t('camera.exposure')" :error="exposure.refused.value?.message">
        <NumberField
          :model-value="exposure.value.value ?? null"
          :min="fields?.exposure.min"
          :max="fields?.exposure.max"
          :step="fields?.exposure.step"
          unit="ms"
          :digits="2"
          @update:model-value="(value) => value !== null && exposure.write(value)"
        />
      </Field>
      <Button
        :busy="auto.running.value"
        :disabled="streaming.value.value !== true"
        @click="auto.run({})"
      >
        {{ t("camera.auto.run") }}
      </Button>
    </div>
    <p v-if="streaming.value.value !== true" class="said">{{ t("camera.needsStream") }}</p>
    <p v-if="auto.failed.value" class="said failed" data-said="auto">{{ auto.failed.value }}</p>
    <p v-else-if="found" class="said" data-said="auto">{{ found }}</p>

    <Field :label="t('camera.gain')" :error="gain.refused.value?.message">
      <NumberField
        :model-value="gain.value.value ?? null"
        :min="fields?.gain.min"
        :max="fields?.gain.max"
        :step="fields?.gain.step"
        :digits="1"
        @update:model-value="(value) => value !== null && gain.write(value)"
      />
    </Field>

    <div class="row">
      <Field class="grow" :label="t('camera.modeLabel')">
        <Select v-model="mode" :options="modes" />
      </Field>
      <Field class="grow" :label="t('camera.format')">
        <Select v-model="format" :options="formats" />
      </Field>
    </div>
    <div class="row">
      <Button
        :disabled="!modeChanged || streaming.value.value === true"
        :busy="setMode.running.value"
        @click="applyMode"
      >
        {{ t("camera.applyMode") }}
      </Button>
      <p v-if="streaming.value.value === true" class="said">{{ t("camera.stopFirst") }}</p>
    </div>

    <fieldset v-if="fields?.roi" class="region">
      <legend>{{ t("camera.region") }}</legend>
      <div class="sides">
        <Field v-for="side in sides" :key="side" :label="t(`camera.sides.${side}`)">
          <NumberField
            v-model="region[side]"
            compact
            :min="0"
            :step="side === 'x' || side === 'width' ? fields.roi.xStep : fields.roi.yStep"
          />
        </Field>
      </div>
      <p v-if="roi.refused.value" class="said failed" data-said="region">
        {{ roi.refused.value.message }}
      </p>
      <div class="row">
        <Button @click="roi.write({ ...region })">{{ t("camera.applyRegion") }}</Button>
        <Button variant="bare" @click="roi.write(wholeSensor())">
          {{ t("camera.wholeSensor") }}
        </Button>
      </div>
    </fieldset>

    <div class="row">
      <Field class="grow" :label="t('camera.trigger')" :error="trigger.refused.value?.message">
        <Select
          :model-value="trigger.value.value?.kind"
          :options="triggers"
          @update:model-value="(kind) => setTrigger(kind)"
        />
      </Field>
      <Field v-if="trigger.value.value?.kind === 'external'" class="grow" :label="t('camera.edge')">
        <Select
          :model-value="trigger.value.value.edge"
          :options="edges"
          @update:model-value="(edge) => setTrigger('external', edge as 'rising' | 'falling')"
        />
      </Field>
    </div>

    <div class="row">
      <Button
        :busy="balance.running.value"
        :disabled="streaming.value.value !== true"
        @click="balance.run({})"
      >
        {{ t("camera.balance.run") }}
      </Button>
      <p v-if="balance.failed.value" class="said failed" data-said="balance">
        {{ balance.failed.value }}
      </p>
      <p v-else-if="gains" class="said" data-said="balance">{{ gains }}</p>
    </div>

    <div v-if="changed.length" class="saved" data-said="saved">
      <p>
        {{
          t("camera.saved.differ", {
            names: changed.map((name) => t(`camera.names.${name}`)).join(", "),
          })
        }}
      </p>
      <Button :busy="putting" @click="putBack">{{ t("camera.saved.putBack") }}</Button>
    </div>
  </section>
</template>

<style scoped>
.camera {
  display: grid;
  gap: var(--space-2);
}

.camera h2,
.about,
.said {
  margin: 0;
}

.about,
.said {
  color: var(--text-muted);
  font-size: var(--font-size-small);
}

.said.failed {
  color: var(--danger);
}

.row {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  align-items: end;
}

.grow {
  flex: 1 1 10rem;
}

.region {
  display: grid;
  gap: var(--space-2);
  margin: 0;
  padding: var(--space-2);
  border: 1px solid var(--border);
  border-radius: var(--radius-surface);
}

.region legend {
  padding-inline: var(--space-1);
}

.sides {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--space-2);
}

.saved {
  display: grid;
  gap: var(--space-2);
  justify-items: start;
  padding: var(--space-2);
  border-radius: var(--radius-surface);
  background: var(--warning-tint);
}

.saved p {
  margin: 0;
}
</style>
