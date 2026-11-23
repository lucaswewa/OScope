import { createRouter, createWebHashHistory, type RouteRecordRaw } from "vue-router";

import { TABS } from "@/app/tabs";

/**
 * The app's routes, by hash (ADR-0039): the same build then runs from a
 * server's root and inside the desktop app, with nothing for either to
 * rewrite. Each page is a chunk of its own, loaded when first opened.
 */
export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    {
      // The shell (W2.1): the rail, and the tab it shows. The first is Control.
      path: "/",
      component: () => import("@/app/AppShell.vue"),
      children: [
        { path: "", redirect: { name: "control" } },
        ...TABS.map((tab): RouteRecordRaw => ({
          path: tab.name,
          name: tab.name,
          component: tab.page,
          props: { tab: tab.name, comes: tab.comes },
        })),
        // The status in full (W4.3), from the rail's indicator.
        {
          path: "status",
          name: "status",
          component: () => import("@/features/status/StatusPage.vue"),
        },
      ],
    },
    {
      // The login, when the server has accounts (W1.3).
      path: "/login",
      name: "login",
      component: () => import("@/features/login/LoginPage.vue"),
    },
    {
      // The tokens, shown: for whoever builds or checks a component.
      path: "/dev/gallery",
      name: "dev-tokens",
      component: () => import("@/features/dev/GalleryPage.vue"),
    },
    {
      // Every control, in each of its states.
      path: "/dev/gallery/inputs",
      name: "dev-inputs",
      component: () => import("@/features/dev/InputsPage.vue"),
    },
    {
      // What appears over the page, and what says something happened.
      path: "/dev/gallery/overlays",
      name: "dev-overlays",
      component: () => import("@/features/dev/OverlaysPage.vue"),
    },
    {
      // Tabs, panes, a table of 100,000 rows, a tree, and the rest.
      path: "/dev/gallery/data",
      name: "dev-data",
      component: () => import("@/features/dev/DataPage.vue"),
    },
    {
      // Forms drawn from JSON Schemas: the gallery's own, and the server's.
      path: "/dev/gallery/forms",
      name: "dev-forms",
      component: () => import("@/features/dev/FormsPage.vue"),
    },
    {
      // Charts drawn by the app, from data recorded from the simulated instrument.
      path: "/dev/gallery/charts",
      name: "dev-charts",
      component: () => import("@/features/dev/ChartsPage.vue"),
    },
    {
      // Calls through the typed client: an action followed, and failures as one model.
      path: "/dev/gallery/api",
      name: "dev-api",
      component: () => import("@/features/dev/ApiPage.vue"),
    },
    {
      // Live Things: the stage's state, kept by the server.
      path: "/dev/gallery/things",
      name: "dev-things",
      component: () => import("@/features/dev/ThingsPage.vue"),
    },
    {
      // The connection, the session and the control lease (W1.3).
      path: "/dev/gallery/connection",
      name: "dev-connection",
      component: () => import("@/features/dev/ConnectionPage.vue"),
    },
    {
      // The host adapter: the platform's services, tried by hand (W1.4).
      path: "/dev/gallery/host",
      name: "dev-host",
      component: () => import("@/features/dev/HostPage.vue"),
    },
    {
      // The live preview: a worker's frames, drawn with WebGL2 (W1.5).
      path: "/dev/gallery/preview",
      name: "dev-preview",
      component: () => import("@/features/dev/PreviewPage.vue"),
    },
  ],
});
