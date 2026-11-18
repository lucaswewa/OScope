<script setup lang="ts">
import "./styles/layer.css";

import { PopoverContent, PopoverPortal, PopoverRoot, PopoverTrigger } from "reka-ui";

/**
 * A panel that opens beside the control in its `trigger` slot (ADR-0042),
 * for a few settings or some detail. The focus goes into it; Escape or a
 * press outside closes it, and the focus goes back to its control. The page
 * behind stays in use. A screen reader knows the panel by its control's name.
 */
withDefaults(
  defineProps<{
    side?: "top" | "right" | "bottom" | "left";
    align?: "start" | "center" | "end";
  }>(),
  { side: "bottom", align: "start" },
);
const open = defineModel<boolean>("open", { default: false });
</script>

<template>
  <PopoverRoot v-model:open="open">
    <PopoverTrigger as-child>
      <slot name="trigger" />
    </PopoverTrigger>
    <PopoverPortal>
      <PopoverContent
        class="ui-popover"
        :side="side"
        :align="align"
        :side-offset="6"
        :collision-padding="8"
      >
        <slot />
      </PopoverContent>
    </PopoverPortal>
  </PopoverRoot>
</template>
