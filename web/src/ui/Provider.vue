<script setup lang="ts">
import { TooltipProvider } from "reka-ui";
</script>

<template>
  <!-- What the components share across the whole app (ADR-0042): one
       tooltip at a time, shown after a pause, and at once after the first. -->
  <TooltipProvider :delay-duration="500" :skip-delay-duration="300">
    <slot />
  </TooltipProvider>
</template>
