<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";

import { thingStore } from "@/api/things/store";
import { useEvent, useProperty } from "@/api/things/use";
import GalleryHeader from "@/features/dev/GalleryHeader.vue";
import Badge from "@/ui/Badge.vue";
import Button from "@/ui/Button.vue";
import Card from "@/ui/Card.vue";
import Field from "@/ui/Field.vue";
import NumberField from "@/ui/NumberField.vue";

/**
 * Live Things (W1.2): the stage's state, read once and then kept by the
 * server. Its socket keeps what it can; what it can't is read four times a
 * second, and at each of the stage's events. Moved by another client, the
 * page follows; without the server, its values are marked stale.
 */
const { t } = useI18n();
const stage = thingStore("stage");
const position = useProperty("/api/v1/stage/position", { every: 250 });
const status = useProperty("/api/v1/stage/status", { every: 250 });
const moving = useProperty("/api/v1/stage/moving", { every: 250 });
const speed = useProperty("/api/v1/stage/speed");
const gain = useProperty("/api/v1/camera/gain");
const moved = useEvent<{ position: { x: number; y: number; z: number } }>("/api/v1/stage/moved", {
  keep: 5,
});

const µm = (value: number) => value.toFixed(1);
const rows = computed(() => {
  const at = position.value.value?.position;
  const state = status.value.value as { state: string } | undefined;
  const fast = speed.value.value;
  return [
    {
      name: "position",
      term: t("things.position"),
      value: at ? t("things.at", { x: µm(at.x), y: µm(at.y), z: µm(at.z) }) : t("things.unread"),
      stale: position.stale.value,
    },
    {
      name: "status",
      term: t("things.status"),
      value: state ? state.state : t("things.unread"),
      stale: status.stale.value,
    },
    {
      name: "moving",
      term: t("things.moving"),
      value:
        moving.value.value === undefined
          ? t("things.unread")
          : t(`things.yes.${moving.value.value}`),
      stale: moving.stale.value,
    },
    {
      name: "speed",
      term: t("things.speed"),
      value:
        fast === undefined
          ? t("things.unread")
          : fast === null
            ? t("things.default")
            : `${fast} µm/s`,
      stale: speed.stale.value,
    },
  ];
});

// The field has no range of its own: the device's refusal is the point.
const draft = ref<number | null>(300);
</script>

<template>
  <main class="things">
    <GalleryHeader :title="t('things.title')" :intro="t('things.intro')" />
    <div class="cards">
      <Card :title="t('things.stage')" :level="2">
        <p class="note" data-said="socket">
          {{ stage.open ? t("things.socket.open") : t("things.socket.closed") }}
        </p>
        <dl class="values">
          <div v-for="row in rows" :key="row.name" class="row" :data-said="row.name">
            <dt>{{ row.term }}</dt>
            <dd>
              <span>{{ row.value }}</span>
              <Badge v-if="row.stale" tone="warning">{{ t("things.stale") }}</Badge>
            </dd>
          </div>
        </dl>
        <h3>{{ t("things.moved") }}</h3>
        <p v-if="moved.told.value.length === 0" class="note">{{ t("things.none") }}</p>
        <ol v-else data-said="moved" class="moved">
          <li v-for="one in moved.told.value" :key="one.time">
            {{
              t("things.at", {
                x: µm(one.data.position.x),
                y: µm(one.data.position.y),
                z: µm(one.data.position.z),
              })
            }}
          </li>
        </ol>
      </Card>

      <Card :title="t('things.write.title')" :level="2">
        <p class="note">{{ t("things.write.intro") }}</p>
        <p class="gain" data-said="gain">
          <span>{{
            gain.value.value === undefined
              ? t("things.unread")
              : t("things.write.now", { gain: gain.value.value })
          }}</span>
          <Badge v-if="gain.stale.value" tone="warning">{{ t("things.stale") }}</Badge>
        </p>
        <div class="write">
          <Field class="field" :label="t('things.write.gain')" :error="gain.refused.value?.message">
            <NumberField v-model="draft" :step="10" />
          </Field>
          <Button
            variant="primary"
            :busy="gain.writing.value"
            :disabled="draft === null"
            @click="draft !== null && gain.write(draft)"
          >
            {{ t("things.write.go") }}
          </Button>
        </div>
      </Card>
    </div>
  </main>
</template>

<style scoped>
.things {
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

.values {
  display: grid;
  gap: var(--space-2);
  margin: 0;
}

.row {
  display: grid;
  grid-template-columns: 8rem minmax(0, 1fr);
  gap: var(--space-4);
}

dt {
  color: var(--text-muted);
}

dd {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  align-items: center;
  margin: 0;
  font-variant-numeric: tabular-nums;
}

h3 {
  margin-block: var(--space-4) var(--space-2);
  font-size: var(--font-size-body);
}

.moved {
  margin: 0;
  padding-inline-start: var(--space-6);
  font-variant-numeric: tabular-nums;
}

.write {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3);
  align-items: end;
}

.gain {
  display: flex;
  gap: var(--space-2);
  align-items: center;
  margin: 0 0 var(--space-3);
}

.field {
  min-inline-size: 16rem;
}

.note {
  margin: 0 0 var(--space-3);
  color: var(--text-muted);
  font-size: var(--font-size-small);
}
</style>
