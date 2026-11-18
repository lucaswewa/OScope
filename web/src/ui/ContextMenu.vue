<script setup lang="ts">
import "./styles/list.css";
import "./styles/menu.css";

import {
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuPortal,
  ContextMenuRoot,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "reka-ui";

import Icon from "./Icon.vue";
import type { MenuEntry } from "./menu";

/**
 * A menu for what is in its slot (ADR-0042), opened where the pointer is by
 * its other button, or from the keyboard by the menu key or Shift+F10. Its
 * keys are a menu's.
 *
 * Whatever it offers must be reachable another way too: not everyone can
 * open it, or knows that it is there.
 */
defineProps<{ items: readonly MenuEntry[] }>();
const emit = defineEmits<{ select: [id: string] }>();

/**
 * The keyboard's way in: Shift+F10, or the menu key. Not every browser turns
 * them into the event a press of the other button makes, so the menu asks
 * for itself, at the corner of what it is for; and the browser is kept from
 * opening it a second time.
 */
function keyed(event: KeyboardEvent): void {
  if (event.key !== "ContextMenu" && !(event.key === "F10" && event.shiftKey)) return;
  event.preventDefault();
  const target = event.currentTarget as HTMLElement;
  const box = target.getBoundingClientRect();
  const at = { clientX: box.left + 4, clientY: box.top + 4 };
  target.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, ...at }));
}
</script>

<template>
  <ContextMenuRoot>
    <ContextMenuTrigger as-child @keydown="keyed">
      <slot />
    </ContextMenuTrigger>
    <ContextMenuPortal>
      <ContextMenuContent class="ui-list ui-menu">
        <template v-for="item in items" :key="item.id">
          <ContextMenuSeparator v-if="item.separated" class="ui-menu-line" />
          <ContextMenuItem
            class="ui-option"
            :class="{ danger: item.danger }"
            :disabled="item.disabled"
            @select="emit('select', item.id)"
          >
            <Icon v-if="item.icon" :name="item.icon" small />
            <span>{{ item.label }}</span>
          </ContextMenuItem>
        </template>
      </ContextMenuContent>
    </ContextMenuPortal>
  </ContextMenuRoot>
</template>
