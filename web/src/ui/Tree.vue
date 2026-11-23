<script setup lang="ts">
import { TreeItem, TreeRoot } from "reka-ui";
import { computed } from "vue";

import Icon from "./Icon.vue";
import { findBranch, type Branch } from "./tree";

/**
 * Items under items (ADR-0042), by WAI-ARIA's tree pattern: one stop for
 * Tab; Up and Down from item to item; Right opens an item, or goes into it;
 * Left closes it, or goes to the one above it; Home and End; a letter finds
 * an item; Enter or Space picks it.
 */
const props = defineProps<{
  items: readonly Branch[];
  /** What the tree is, to a screen reader. */
  label: string;
}>();
/** The item picked, by its id. */
const chosen = defineModel<string | undefined>();
/** The items open, by their ids: for a page to keep. */
const expanded = defineModel<string[]>("expanded", { default: () => [] });

const picked = computed(() => findBranch(props.items, chosen.value));

function pick(item: unknown): void {
  chosen.value = (item as Branch | undefined)?.id;
}

/**
 * A letter, or a few typed within a second, goes to the next item whose
 * name starts with them. The library's own search takes every key into its
 * word, Home and the arrows too, so that Home and then a letter found
 * nothing; it never sees a letter now.
 */
let typed = "";
let lastTyped = 0;
function letter(event: KeyboardEvent): void {
  const key = event.key;
  if (key.length !== 1 || key === " " || event.ctrlKey || event.metaKey || event.altKey) {
    typed = "";
    return;
  }
  event.stopPropagation();
  const now = Date.now();
  typed = (now - lastTyped > 1000 ? "" : typed) + key.toLowerCase();
  lastTyped = now;
  const items = [
    ...(event.currentTarget as HTMLElement).querySelectorAll<HTMLElement>("[role=treeitem]"),
  ];
  const at = items.indexOf(event.target as HTMLElement);
  // One letter, or the same one again and again, goes on to the next; more stay if they still fit.
  const same = [...typed].every((one) => one === typed[0]);
  const word = same ? typed[0]! : typed;
  const from = same ? at + 1 : Math.max(at, 0);
  const order = [...items.slice(from), ...items.slice(0, from)];
  order.find((item) => (item.textContent ?? "").trim().toLowerCase().startsWith(word))?.focus();
}
</script>

<template>
  <TreeRoot
    v-slot="{ flattenItems }"
    class="tree"
    :items="items as Branch[]"
    :get-key="(item: Branch) => item.id"
    :get-children="(item: Branch) => item.children"
    :model-value="picked"
    :expanded="expanded"
    :aria-label="label"
    @update:model-value="pick"
    @update:expanded="(now: string[]) => (expanded = now)"
    @keydown.capture="letter"
  >
    <TreeItem
      v-for="item in flattenItems"
      :key="item._id"
      v-slot="{ isExpanded }"
      v-bind="item.bind"
      class="item"
      :style="{ paddingInlineStart: `calc(${item.level - 1} * var(--space-4) + var(--space-1))` }"
    >
      <Icon v-if="item.hasChildren" class="twisty" :name="isExpanded ? 'expand' : 'next'" small />
      <span v-else class="twisty" />
      <Icon v-if="item.value.icon" :name="item.value.icon" small />
      <span class="label">{{ item.value.label }}</span>
    </TreeItem>
  </TreeRoot>
</template>

<style scoped>
.tree {
  display: grid;
  margin: 0;
  padding: 0;
  list-style: none;
}

.item {
  display: flex;
  gap: var(--space-1);
  align-items: center;
  min-block-size: var(--size-control-compact);
  padding-inline-end: var(--space-2);
  cursor: default;
}

.item:hover {
  background: var(--surface-hover);
}

.item[data-selected] {
  background: var(--accent-tint);
  color: var(--accent-text);
}

/* The keyboard's focus is drawn inside the item, so that the next doesn't hide it. */
.item:focus-visible {
  outline-offset: calc(-1 * var(--focus-width));
}

.twisty {
  flex: none;
  inline-size: var(--size-icon-small);
}

.label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
