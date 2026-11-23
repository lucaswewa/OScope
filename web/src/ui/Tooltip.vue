<script setup lang="ts">
import "./styles/tooltip.css";

import { TooltipContent, TooltipPortal, TooltipRoot, TooltipTrigger } from "reka-ui";
import { ref } from "vue";

/**
 * A tooltip (ADR-0042): a few words about the control inside it, shown when
 * the pointer rests on the control or the keyboard's focus reaches it, and
 * hidden by Escape. It describes; it never holds anything to press.
 */
withDefaults(
  defineProps<{
    text: string;
    side?: "top" | "right" | "bottom" | "left";
  }>(),
  { side: "top" },
);

const open = ref(false);
/** The keyboard's focus is on the control, and Escape hasn't sent the tooltip away. */
const held = ref(false);

/**
 * The library hides a tooltip when its control scrolls: right for one the
 * pointer left behind, wrong for one the keyboard has just brought into view,
 * which is a scroll too. An icon alone would then go unnamed for whoever
 * reached it by Tab. So while the focus holds it, it stays.
 */
function show(now: boolean): void {
  open.value = now || held.value;
}

function focused(event: FocusEvent): void {
  held.value = (event.target as HTMLElement).matches(":focus-visible");
  if (held.value) open.value = true;
}

function left(): void {
  held.value = false;
  open.value = false;
}
</script>

<template>
  <TooltipRoot :open="open" @update:open="show">
    <TooltipTrigger as-child @focus="focused" @blur="left" @keydown.escape="held = false">
      <slot />
    </TooltipTrigger>
    <TooltipPortal>
      <TooltipContent class="ui-tooltip" :side="side" :side-offset="6" :collision-padding="8">
        {{ text }}
      </TooltipContent>
    </TooltipPortal>
  </TooltipRoot>
</template>
