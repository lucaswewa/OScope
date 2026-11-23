<script setup lang="ts">
import { computed, ref, shallowRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import { useProperty } from "@/api/things/use";
import { rectFromMm } from "@/features/scan/plan";
import Button from "@/ui/Button.vue";
import Field from "@/ui/Field.vue";
import NumberField from "@/ui/NumberField.vue";
import type { Point, Rect } from "@/ui/tile-viewer/coordinates";
import { between } from "@/ui/tile-viewer/shapes";

/**
 * A rectangle of the stage (W5.1): typed in mm, its corner and its size, or
 * its two corners taken from where the stage is, one after the other.
 */
const props = defineProps<{ legend: string; said: string }>();
const rect = defineModel<Rect | undefined>();
const { t } = useI18n();
const position = useProperty("/api/v1/stage/position", { every: 1_000 });

const mm = (um: number | undefined) => (um === undefined ? null : Math.round(um) / 1000);
const x = ref<number | null>(null);
const y = ref<number | null>(null);
const width = ref<number | null>(null);
const height = ref<number | null>(null);
// The fields follow the rectangle, however it was set; and set it, once all four are numbers.
watch(
  rect,
  (now) => {
    x.value = mm(now?.x);
    y.value = mm(now?.y);
    width.value = mm(now?.width);
    height.value = mm(now?.height);
  },
  { immediate: true },
);
function typed(): void {
  const now = rectFromMm(x.value, y.value, width.value, height.value);
  if (now) rect.value = now;
}

/** The first corner taken, until the second is. */
const first = shallowRef<Point>();
const here = computed(() => position.value.value?.position);
function corner(): void {
  const at = here.value;
  if (!at) return;
  if (!first.value) {
    first.value = { x: at.x, y: at.y };
    return;
  }
  const made = between(first.value, at);
  first.value = undefined;
  if (made.width > 0 && made.height > 0) rect.value = made;
}
</script>

<template>
  <fieldset class="region" :data-said="props.said">
    <legend>{{ legend }}</legend>
    <div class="row">
      <Field class="grow" :label="t('scan.region.x')">
        <NumberField v-model="x" optional :step="0.5" unit="mm" @update:model-value="typed" />
      </Field>
      <Field class="grow" :label="t('scan.region.y')">
        <NumberField v-model="y" optional :step="0.5" unit="mm" @update:model-value="typed" />
      </Field>
    </div>
    <div class="row">
      <Field class="grow" :label="t('scan.region.width')">
        <NumberField
          v-model="width"
          optional
          :min="0"
          :step="0.5"
          unit="mm"
          @update:model-value="typed"
        />
      </Field>
      <Field class="grow" :label="t('scan.region.height')">
        <NumberField
          v-model="height"
          optional
          :min="0"
          :step="0.5"
          unit="mm"
          @update:model-value="typed"
        />
      </Field>
    </div>
    <div class="row">
      <Button compact variant="bare" :disabled="!here" @click="corner">
        {{ first ? t("scan.region.second") : t("scan.region.first") }}
      </Button>
      <slot />
    </div>
  </fieldset>
</template>

<style scoped>
.region {
  display: grid;
  gap: var(--space-2);
  margin: 0;
  padding: 0;
  border: 0;
}

.region legend {
  padding: 0;
  margin-block-end: var(--space-1);
  font-weight: var(--font-weight-strong);
}

.row {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  align-items: end;
}

.grow {
  flex: 1 1 6rem;
}
</style>
