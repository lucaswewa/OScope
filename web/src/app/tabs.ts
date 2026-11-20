// The rail's tabs (W2.1): goal.md's nine, the working ones at the top and
// the instrument's own at the bottom, as look.md lays them out. Each is a
// route of its own, its page a chunk loaded when first opened; a tab whose
// stage is still to come says which.
import type { Component } from "vue";

import type { IconName } from "@/ui/icons";

export interface Tab {
  /** Its route's name, and its path. */
  name: string;
  icon: IconName;
  /** In the rail's lower group: the instrument's own. */
  bottom?: boolean;
  /** The stage or phase that fills it, while it waits. */
  comes?: string;
  page: () => Promise<Component>;
}

const waiting = () => import("@/features/shell/TabPlaceholder.vue");

export const TABS: readonly Tab[] = [
  { name: "control", icon: "control", page: () => import("@/features/control/ControlTab.vue") },
  { name: "scan", icon: "scan", comes: "W5", page: waiting },
  { name: "workspace", icon: "workspace", comes: "W7", page: waiting },
  { name: "gallery", icon: "gallery", comes: "W6", page: waiting },
  { name: "jobs", icon: "jobs", page: () => import("@/features/jobs/JobsTab.vue") },
  {
    name: "settings",
    icon: "settings",
    bottom: true,
    page: () => import("@/features/settings/SettingsTab.vue"),
  },
  { name: "logging", icon: "logging", bottom: true, comes: "W4.2", page: waiting },
  {
    name: "about",
    icon: "info",
    bottom: true,
    page: () => import("@/features/about/AboutTab.vue"),
  },
  { name: "power", icon: "power", bottom: true, comes: "W4.3", page: waiting },
];

/** The tab a route shows, if it is one. */
export function tabOf(name: unknown): Tab | undefined {
  return TABS.find((tab) => tab.name === name);
}
