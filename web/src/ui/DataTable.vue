<script setup lang="ts" generic="T">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, useTemplateRef } from "vue";
import { useI18n } from "vue-i18n";

import Icon from "./Icon.vue";
import {
  MIN,
  moved,
  nextSort,
  picked,
  sorted,
  textOf,
  WIDTH,
  windowOf,
  type Cell,
  type Column,
  type How,
  type Key,
  type Picking,
  type Sort,
} from "./table";

/**
 * Rows of data in columns (ADR-0042): a grid, by WAI-ARIA's pattern, that
 * draws only the rows in view, so that 100,000 scroll as easily as ten.
 *
 * - **The keyboard:** the grid is one stop for Tab. The arrows move from cell
 *   to cell, Page Up and Down by a page, Home and End to a row's ends, and
 *   with Ctrl to the grid's. Space picks the row; Ctrl+A picks them all.
 * - **A header:** pressed, or Enter on it, sorts by its column; again, the
 *   other way. Shift and the arrow keys on it, or its edge dragged, change
 *   its column's width.
 * - **Rows** are picked by a press: on their own, with Ctrl one more or one
 *   fewer, with Shift those since the last.
 */
const props = withDefaults(
  defineProps<{
    columns: readonly Column<T>[];
    rows: readonly T[];
    /** What a row goes by, for picking it. */
    rowKey: (row: T) => Key;
    /** What the table is, to a screen reader. */
    label: string;
    picking?: Picking;
  }>(),
  { picking: "none" },
);
const sort = defineModel<Sort | null>("sort", { default: null });
const selected = defineModel<Key[]>("selected", { default: () => [] });
/** Each column's width in pixels, where it was changed: for a page to keep. */
const widths = defineModel<Record<string, number>>("widths", { default: () => ({}) });

const { t } = useI18n();
const grid = useTemplateRef<HTMLElement>("grid");
/** A row's height: the compact control's, 1.875 rem, in this page's pixels. */
const rowHeight = ref(30);
const scrollTop = ref(0);
const viewport = ref(0);
/** The cell the grid's focus is on, or goes back to: the first header's at first. */
const active = ref<Cell>({ row: -1, col: 0 });
/** The row last picked on its own, which Shift picks from. */
let anchor: Key | undefined;

const shown = computed(() => sorted(props.rows, sort.value, props.columns));
// Never more rows than the browser's window can show: a table its page gave
// no height grows to hold them all, and would draw 100,000.
const span = computed(() =>
  windowOf(
    scrollTop.value,
    Math.min(viewport.value, window.innerHeight),
    rowHeight.value,
    shown.value.length,
  ),
);
const visible = computed(() => shown.value.slice(span.value.first, span.value.last));
const chosen = computed(() => new Set(selected.value));
const widthOf = (column: Column<T>): number => widths.value[column.key] ?? column.width ?? WIDTH;
const layout = computed(() => props.columns.map((column) => `${widthOf(column)}px`).join(" "));
const total = computed(() => props.columns.reduce((sum, column) => sum + widthOf(column), 0));

/**
 * The cell that Tab reaches: the active one, if it is drawn; else the same
 * column in the first row drawn.
 */
const tabbable = computed<Cell>(() => {
  const { row, col } = active.value;
  if (row < 0 || (row >= span.value.first && row < span.value.last)) return active.value;
  return { row: span.value.first, col };
});
const isTabbable = (row: number, col: number): boolean =>
  tabbable.value.row === row && tabbable.value.col === col;

let watcher: ResizeObserver | undefined;
onMounted(() => {
  const root = parseFloat(getComputedStyle(document.documentElement).fontSize);
  rowHeight.value = Math.round(1.875 * (root > 0 ? root : 16));
  const element = grid.value!;
  viewport.value = element.clientHeight;
  watcher = new ResizeObserver(() => (viewport.value = element.clientHeight));
  watcher.observe(element);
});
onBeforeUnmount(() => watcher?.disconnect());

function scrolled(): void {
  scrollTop.value = grid.value?.scrollTop ?? 0;
}

/** Sorts by a column, and shows the first rows of the new order. */
function sortBy(column: Column<T>): void {
  if (!column.sortable) return;
  sort.value = nextSort(sort.value, column.key);
  toTop();
}

function toTop(): void {
  if (grid.value) grid.value.scrollTop = 0;
  scrollTop.value = 0;
}

function pick(row: number, how: How): void {
  const item = shown.value[row];
  if (item === undefined || props.picking === "none") return;
  const key = props.rowKey(item);
  const order = how === "range" ? shown.value.map(props.rowKey) : [];
  selected.value = picked(selected.value, key, how, props.picking, anchor, order);
  if (how !== "range") anchor = key;
}

/**
 * Moves the focus to a cell, scrolling its row into view first. The header is
 * always in view; going up into it, or to the grid's start, shows the first
 * rows too.
 */
async function focusCell(cell: Cell, start = false): Promise<void> {
  active.value = cell;
  const element = grid.value;
  if (!element) return;
  if (cell.row < 0 && start) toTop();
  else if (cell.row >= 0) {
    const top = cell.row * rowHeight.value;
    const room = element.clientHeight - rowHeight.value;
    if (top < element.scrollTop) element.scrollTop = top;
    else if (top + rowHeight.value > element.scrollTop + room) {
      element.scrollTop = top + rowHeight.value - room;
    }
    scrollTop.value = element.scrollTop;
  }
  await nextTick();
  element
    .querySelector<HTMLElement>(`[data-cell="${cell.row}:${cell.col}"]`)
    ?.focus({ preventScroll: true });
}

function resize(column: Column<T>, to: number): void {
  widths.value = { ...widths.value, [column.key]: Math.max(column.min ?? MIN, Math.round(to)) };
}

function key(event: KeyboardEvent): void {
  const ctrl = event.ctrlKey || event.metaKey;
  const { row, col } = active.value;
  const column = props.columns[col];
  if (row < 0 && column && event.shiftKey && /^Arrow(Left|Right)$/.test(event.key)) {
    resize(column, widthOf(column) + (event.key === "ArrowRight" ? 16 : -16));
  } else if (row < 0 && column && event.key === "Enter") {
    sortBy(column);
  } else if (row >= 0 && event.key === " ") {
    pick(row, props.picking === "multiple" ? "toggle" : "only");
  } else if (ctrl && event.key.toLowerCase() === "a" && props.picking === "multiple") {
    selected.value = shown.value.map(props.rowKey);
  } else {
    const page = Math.max(1, Math.floor(viewport.value / rowHeight.value) - 1);
    const to = moved(active.value, event.key, ctrl, shown.value.length, props.columns.length, page);
    if (!to) return;
    const upward = to.row < 0 && row >= 0;
    void focusCell(to, upward || (ctrl && event.key === "Home"));
  }
  event.preventDefault();
}

function pressed(row: number, col: number, event: MouseEvent): void {
  active.value = { row, col };
  pick(row, event.shiftKey ? "range" : event.ctrlKey || event.metaKey ? "toggle" : "only");
}

/** A column's edge dragged: its width follows the pointer until it is let go. */
function grip(column: Column<T>, event: PointerEvent): void {
  event.preventDefault();
  event.stopPropagation();
  const handle = event.currentTarget as HTMLElement;
  const from = event.clientX;
  const start = widthOf(column);
  handle.setPointerCapture(event.pointerId);
  const move = (now: PointerEvent): void => resize(column, start + now.clientX - from);
  const done = (): void => {
    handle.removeEventListener("pointermove", move);
    handle.removeEventListener("pointerup", done);
    handle.removeEventListener("pointercancel", done);
  };
  handle.addEventListener("pointermove", move);
  handle.addEventListener("pointerup", done);
  handle.addEventListener("pointercancel", done);
}

const sortOf = (column: Column<T>): "ascending" | "descending" | undefined =>
  sort.value?.key === column.key ? (sort.value.descending ? "descending" : "ascending") : undefined;
</script>

<template>
  <!-- Out of the Tab order itself: Firefox makes a box that scrolls a stop of
       its own, and the grid is one stop, at its cell. -->
  <div
    ref="grid"
    class="table"
    role="grid"
    :aria-label="label"
    :aria-rowcount="shown.length + 1"
    :aria-colcount="columns.length"
    :aria-multiselectable="picking === 'multiple' || undefined"
    tabindex="-1"
    :style="{ '--columns': layout, '--total': `${total}px`, '--row': `${rowHeight}px` }"
    @scroll.passive="scrolled"
    @keydown="key"
  >
    <div class="head" role="rowgroup">
      <div class="row" role="row" aria-rowindex="1">
        <div
          v-for="(column, col) in columns"
          :key="column.key"
          class="cell header"
          :class="[column.align, { sortable: column.sortable }]"
          role="columnheader"
          :aria-colindex="col + 1"
          :aria-sort="sortOf(column)"
          :data-cell="`-1:${col}`"
          :tabindex="isTabbable(-1, col) ? 0 : -1"
          @focus="active = { row: -1, col }"
          @click="sortBy(column)"
        >
          <span class="label">{{ column.label }}</span>
          <Icon v-if="sortOf(column)" :name="sort?.descending ? 'down' : 'up'" small />
          <span class="grip" aria-hidden="true" @pointerdown="grip(column, $event)" @click.stop />
        </div>
      </div>
    </div>
    <div class="body" role="rowgroup" :style="{ blockSize: `${shown.length * rowHeight}px` }">
      <div
        v-for="(row, at) in visible"
        :key="rowKey(row)"
        class="row"
        role="row"
        :aria-rowindex="span.first + at + 2"
        :aria-selected="picking === 'none' ? undefined : chosen.has(rowKey(row))"
        :style="{ transform: `translateY(${(span.first + at) * rowHeight}px)` }"
      >
        <div
          v-for="(column, col) in columns"
          :key="column.key"
          class="cell"
          :class="column.align"
          role="gridcell"
          :data-cell="`${span.first + at}:${col}`"
          :tabindex="isTabbable(span.first + at, col) ? 0 : -1"
          @focus="active = { row: span.first + at, col }"
          @click="pressed(span.first + at, col, $event)"
        >
          {{ textOf(row, column) }}
        </div>
      </div>
    </div>
    <p v-if="shown.length === 0" class="nothing">{{ t("ui.table.nothing") }}</p>
  </div>
</template>

<style scoped>
.table {
  position: relative;
  overflow: auto;
  border: 1px solid var(--border);
  background: var(--surface-raised);
  font-variant-numeric: tabular-nums;
}

.head {
  position: sticky;
  inset-block-start: 0;
  z-index: 1;
  inline-size: max(100%, var(--total));
  background: var(--surface-pane);
  border-block-end: 1px solid var(--border-strong);
}

.body {
  position: relative;
  inline-size: max(100%, var(--total));
}

.row {
  display: grid;
  grid-template-columns: var(--columns);
  block-size: var(--row);
}

.body .row {
  position: absolute;
  inset-inline: 0;
  inset-block-start: 0;
  border-block-end: 1px solid var(--border);
}

.body .row:hover {
  background: var(--surface-hover);
}

.body .row[aria-selected="true"] {
  background: var(--accent-tint);
}

.cell {
  display: flex;
  align-items: center;
  min-inline-size: 0;
  padding-inline: var(--space-2);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* The cell with the focus: a ring inside it, so that its neighbours don't hide it. */
.cell:focus-visible {
  outline-offset: calc(-1 * var(--focus-width));
}

.end {
  justify-content: end;
  text-align: end;
}

.header {
  position: relative;
  gap: var(--space-1);
  font-weight: var(--font-weight-strong);
}

.label {
  overflow: hidden;
  text-overflow: ellipsis;
}

.sortable {
  cursor: pointer;
}

.sortable:hover {
  background: var(--surface-hover);
}

.grip {
  position: absolute;
  inset-block: 0;
  inset-inline-end: 0;
  inline-size: var(--space-2);
  cursor: col-resize;
  touch-action: none;
}

.grip:hover {
  background: var(--border-control);
}

.nothing {
  padding: var(--space-3);
  color: var(--text-muted);
}
</style>
