<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, shallowRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import type { Level, LogRecord } from "@/api/media";
import { atEnd, TONES } from "@/features/logging/logs";
import Badge from "@/ui/Badge.vue";
import Button from "@/ui/Button.vue";
import { windowOf } from "@/ui/table";

/**
 * The records, in a virtual list (W4.2): the oldest at the top, the newest at
 * the foot, a screenful drawn at a time. At its end, it follows the tail;
 * scrolled up, it holds, and says how many came since; at its end again, it
 * follows again. A record is opened by its row.
 */
const props = defineProps<{
  records: readonly LogRecord[];
  label: string;
  /** The record open, by its number. */
  chosen?: number | undefined;
}>();
const emit = defineEmits<{ open: [seq: number] }>();
const { t } = useI18n();

const box = ref<HTMLElement>();
/** A row's height: a line and a half of the list's text, 24 px at least. */
const row = ref(24);
const top = ref(0);
const height = ref(0);
const following = shallowRef(true);
/** Records that came while the list held. */
const since = shallowRef(0);

const span = computed(() => windowOf(top.value, height.value, row.value, props.records.length));
const visible = computed(() => props.records.slice(span.value.first, span.value.last));

function scrolled(): void {
  const element = box.value;
  if (!element) return;
  top.value = element.scrollTop;
  following.value = atEnd(element.scrollTop, element.clientHeight, element.scrollHeight);
  if (following.value) since.value = 0;
}
/** To the end, following again. */
function toEnd(): void {
  const element = box.value;
  if (!element) return;
  element.scrollTop = element.scrollHeight;
  top.value = element.scrollTop;
  following.value = true;
  since.value = 0;
}
watch(
  () => props.records.length,
  (now, before) => {
    if (following.value) void nextTick(toEnd);
    else if (now > before) since.value += now - before;
  },
);

let watcher: ResizeObserver | undefined;
onMounted(() => {
  const element = box.value!;
  const font = parseFloat(getComputedStyle(element).fontSize);
  // A target a finger can take, as axe asks: 24 px at least.
  row.value = Math.max(24, Math.round(1.5 * (font > 0 ? font : 16)));
  height.value = element.clientHeight;
  watcher = new ResizeObserver(() => {
    height.value = element.clientHeight;
    if (following.value) toEnd();
  });
  watcher.observe(element);
  toEnd();
});
onBeforeUnmount(() => watcher?.disconnect());

const tone = (level: Level) => TONES[level];
const timeOf = (record: LogRecord) => new Date(record.time).toLocaleTimeString();
</script>

<template>
  <div class="log-list">
    <div
      ref="box"
      class="box"
      role="list"
      :aria-label="label"
      tabindex="0"
      data-said="records"
      @scroll.passive="scrolled"
    >
      <div class="all" :style="{ blockSize: `${records.length * row}px` }">
        <div
          v-for="(record, at) in visible"
          :key="record.seq"
          class="row"
          role="listitem"
          :aria-posinset="span.first + at + 1"
          :aria-setsize="records.length"
          :style="{ blockSize: `${row}px`, transform: `translateY(${(span.first + at) * row}px)` }"
        >
          <button
            type="button"
            class="open"
            :aria-current="record.seq === chosen || undefined"
            :data-seq="record.seq"
            @click="emit('open', record.seq)"
          >
            <time :datetime="record.time">{{ timeOf(record) }}</time>
            <Badge :tone="tone(record.level)" :data-level="record.level">{{ record.level }}</Badge>
            <span class="target">{{ record.target }}</span>
            <span class="message">{{ record.message }}</span>
          </button>
        </div>
      </div>
      <p v-if="!records.length" class="nothing">{{ t("logging.nothing") }}</p>
    </div>
    <Button v-if="since > 0" compact class="since" data-said="since" @click="toEnd">
      {{ t("logging.since", { count: since }, since) }}
    </Button>
  </div>
</template>

<style scoped>
.log-list {
  position: relative;
  display: grid;
  min-block-size: 0;
}

.box {
  position: relative;
  min-block-size: 0;
  overflow: auto;
  border: 1px solid var(--border);
  border-radius: var(--radius-surface);
  background: var(--surface-raised);
  font-size: var(--font-size-small);
}

.all {
  position: relative;
}

.row {
  position: absolute;
  inset-inline: 0;
}

.open {
  display: grid;
  grid-template-columns: auto 5rem minmax(6rem, 14rem) minmax(0, 1fr);
  gap: var(--space-2);
  align-items: center;
  inline-size: 100%;
  block-size: 100%;
  padding: 0 var(--space-2);
  border: 0;
  background: none;
  color: var(--text);
  font: inherit;
  text-align: start;
  white-space: nowrap;
  cursor: pointer;
}

.open:hover {
  background: var(--surface-hover);
}

.open[aria-current="true"] {
  background: var(--accent-tint);
}

.open time {
  color: var(--text-muted);
  font-variant-numeric: tabular-nums;
}

.target,
.message {
  overflow: hidden;
  text-overflow: ellipsis;
}

.target {
  color: var(--text-muted);
}

.nothing {
  margin: 0;
  padding: var(--space-3);
  color: var(--text-muted);
}

/* Over the list's foot: the records that came while it held, and the way back to them. */
.since {
  position: absolute;
  inset-block-end: var(--space-2);
  justify-self: center;
}
</style>
