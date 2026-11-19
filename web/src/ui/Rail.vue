<script setup lang="ts">
import { ToolbarLink, ToolbarRoot } from "reka-ui";

import Icon from "./Icon.vue";
import type { RailItem } from "./rail";

/**
 * The app's rail (look.md): its tabs, each an icon over a short label, the
 * working ones at the top and the instrument's own at the bottom. It is one
 * toolbar: a single stop for Tab, and the arrow keys, Home and End from tab
 * to tab. The `end` slot, below them, is the instrument's status. In a
 * window too short for them all, the rail scrolls.
 */
defineProps<{
  /** What the rail is, to a screen reader. */
  label: string;
  top: readonly RailItem[];
  bottom: readonly RailItem[];
}>();
</script>

<template>
  <nav class="rail" :aria-label="label">
    <ToolbarRoot class="tabs" orientation="vertical" :aria-label="label">
      <div v-for="(group, at) in [top, bottom]" :key="at" :class="['group', { bottom: at === 1 }]">
        <ToolbarLink v-for="item in group" :key="item.href" as-child>
          <a :href="item.href" class="item" :aria-current="item.current ? 'page' : undefined">
            <Icon :name="item.icon" />
            <span class="label">{{ item.label }}</span>
          </a>
        </ToolbarLink>
      </div>
    </ToolbarRoot>
    <slot name="end" />
  </nav>
</template>

<style scoped>
.rail {
  display: flex;
  flex-direction: column;
  inline-size: 5.25rem;
  block-size: 100%;
  overflow-y: auto;
  border-inline-end: 1px solid var(--border);
  background: var(--surface-rail);
}

.tabs {
  display: flex;
  flex: 1 0 auto;
  flex-direction: column;
}

.group {
  display: flex;
  flex-direction: column;
}

.group.bottom {
  margin-block-start: auto;
}

.item {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  align-items: center;
  justify-content: center;
  block-size: 4rem;
  padding-inline: var(--space-1);
  color: var(--text-muted);
  text-align: center;
  text-decoration: none;
}

.item:hover {
  background: var(--surface-hover);
}

.item[aria-current="page"] {
  background: var(--accent);
  color: var(--accent-on);
}

.item:focus-visible {
  outline: var(--focus-width) solid var(--focus-color);
  outline-offset: calc(-1 * var(--focus-width));
}

.label {
  font-size: var(--font-size-small);
  line-height: 1.2;
}
</style>
