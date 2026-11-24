<script setup lang="ts">
import { computed, shallowRef } from "vue";
import { useI18n } from "vue-i18n";

import { call, server } from "@/api/client";
import { useAction, useProperty } from "@/api/things/use";
import { cameraSaid } from "@/features/settings/settings";
import Button from "@/ui/Button.vue";
import KeyValue from "@/ui/KeyValue.vue";

/**
 * The devices' saved settings (W8.1): what each sets again when the server
 * starts, the camera's, the lights', the objective fitted, and the stage's
 * speed; each forgotten, or the speed put back to its default.
 */
const { t } = useI18n();
const why = (thrown: unknown) => (thrown instanceof Error ? thrown.message : String(thrown));
const said = shallowRef("");

const camera = useProperty("/api/v1/camera/saved", { every: 5_000 });
const lights = useProperty("/api/v1/illumination/saved", { every: 5_000 });
const objective = useProperty("/api/v1/objective/saved", { every: 5_000 });
const speed = useProperty("/api/v1/stage/speed", { every: 5_000 });
const forget = {
  camera: useAction("/api/v1/camera/forget_saved"),
  lights: useAction("/api/v1/illumination/forget_saved"),
  objective: useAction("/api/v1/objective/forget_saved"),
};

const cameraEntries = computed(() => {
  const saved = camera.value.value;
  if (!saved) return [];
  const words = {
    all: t("settings.devicesPane.allSensor"),
    trigger: (kind: string) => t(`settings.devicesPane.triggers.${kind}`),
  };
  return cameraSaid(saved, words).map(({ term, value }) => ({
    term: t(`settings.devicesPane.camera.${term}`),
    value,
  }));
});
const lightEntries = computed(() =>
  (lights.value.value ?? []).map((light) => ({
    term: light.channel,
    value: light.on
      ? t("settings.devicesPane.lightOn", { percent: Math.round(light.intensity * 100) })
      : t("settings.devicesPane.lightOff"),
  })),
);

async function forgetSaved(which: keyof typeof forget): Promise<void> {
  const done = await forget[which].run({});
  said.value =
    done === undefined && forget[which].error.value
      ? t("settings.devicesPane.refused", { why: forget[which].error.value.message })
      : t(`settings.devicesPane.forgot.${which}`);
}
async function speedBack(): Promise<void> {
  try {
    await call((signal) => server.api.POST("/api/v1/stage/speed/reset", { signal }));
    said.value = t("settings.devicesPane.speedBack");
  } catch (thrown) {
    said.value = t("settings.devicesPane.refused", { why: why(thrown) });
  }
}
</script>

<template>
  <section class="pane" aria-labelledby="devices-title" data-said="devices-pane">
    <h2 id="devices-title">{{ t("settings.devicesPane.title") }}</h2>
    <p class="note">{{ t("settings.devicesPane.note") }}</p>
    <div class="devices">
      <section class="device" data-said="saved-camera">
        <h3>{{ t("settings.devicesPane.cameraTitle") }}</h3>
        <KeyValue v-if="cameraEntries.length" :entries="cameraEntries" />
        <p v-else class="note">{{ t("settings.devicesPane.none") }}</p>
        <Button compact :disabled="!camera.value.value" @click="forgetSaved('camera')">
          {{ t("settings.devicesPane.forget") }}
        </Button>
      </section>
      <section class="device" data-said="saved-lights">
        <h3>{{ t("settings.devicesPane.lightsTitle") }}</h3>
        <KeyValue v-if="lightEntries.length" :entries="lightEntries" />
        <p v-else class="note">{{ t("settings.devicesPane.none") }}</p>
        <Button compact :disabled="!lightEntries.length" @click="forgetSaved('lights')">
          {{ t("settings.devicesPane.forget") }}
        </Button>
      </section>
      <section class="device" data-said="saved-objective">
        <h3>{{ t("settings.devicesPane.objectiveTitle") }}</h3>
        <p v-if="objective.value.value" class="value">{{ objective.value.value }}</p>
        <p v-else class="note">{{ t("settings.devicesPane.none") }}</p>
        <Button compact :disabled="!objective.value.value" @click="forgetSaved('objective')">
          {{ t("settings.devicesPane.forget") }}
        </Button>
      </section>
      <section class="device" data-said="saved-speed">
        <h3>{{ t("settings.devicesPane.speedTitle") }}</h3>
        <p class="value">
          {{
            typeof speed.value.value === "number"
              ? t("settings.devicesPane.speed", { speed: speed.value.value })
              : t("settings.devicesPane.speedOwn")
          }}
        </p>
        <Button compact :disabled="typeof speed.value.value !== 'number'" @click="speedBack">
          {{ t("settings.devicesPane.speedDefault") }}
        </Button>
      </section>
    </div>
    <p class="said" role="status" data-said="devices-said">{{ said }}</p>
  </section>
</template>

<style scoped>
.pane {
  display: grid;
  gap: var(--space-2);
  align-content: start;
}

h2,
h3 {
  margin: 0;
  font-size: var(--font-size-body);
}

.devices {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(16rem, 1fr));
  gap: var(--space-3);
  align-items: start;
}

.device {
  display: grid;
  gap: var(--space-1);
  justify-items: start;
  padding: var(--space-2);
  border: 1px solid var(--border);
}

.value {
  margin: 0;
}

.note,
.said {
  margin: 0;
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
