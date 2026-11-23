<script setup lang="ts">
import "./styles/list.css";
import "./styles/menu.css";

import {
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuPortal,
  DropdownMenuRoot,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "reka-ui";

import Icon from "./Icon.vue";
import type { MenuEntry } from "./menu";

/**
 * A menu under the button in its slot (ADR-0042). Enter, Space or Arrow Down
 * on the button opens it; the arrows, Home, End and a typed letter move
 * through it; Enter does what is chosen; Escape closes it. Either way the
 * focus goes back to the button.
 */
defineProps<{ items: readonly MenuEntry[] }>();
const emit = defineEmits<{ select: [id: string] }>();
</script>

<template>
  <DropdownMenuRoot>
    <DropdownMenuTrigger as-child>
      <slot />
    </DropdownMenuTrigger>
    <DropdownMenuPortal>
      <DropdownMenuContent class="ui-list ui-menu" align="start" :side-offset="4">
        <template v-for="item in items" :key="item.id">
          <DropdownMenuSeparator v-if="item.separated" class="ui-menu-line" />
          <DropdownMenuItem
            class="ui-option"
            :class="{ danger: item.danger }"
            :disabled="item.disabled"
            @select="emit('select', item.id)"
          >
            <Icon v-if="item.icon" :name="item.icon" small />
            <span>{{ item.label }}</span>
          </DropdownMenuItem>
        </template>
      </DropdownMenuContent>
    </DropdownMenuPortal>
  </DropdownMenuRoot>
</template>
