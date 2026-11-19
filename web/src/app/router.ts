import { createRouter, createWebHashHistory } from "vue-router";

/**
 * The app's routes, by hash (ADR-0039): the same build then runs from a
 * server's root and inside the desktop app, with nothing for either to
 * rewrite. Each page is a chunk of its own, loaded when first opened.
 */
export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    {
      path: "/",
      name: "home",
      component: () => import("@/features/home/HomePage.vue"),
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
      name: "gallery",
      component: () => import("@/features/dev/GalleryPage.vue"),
    },
    {
      // Every control, in each of its states.
      path: "/dev/gallery/inputs",
      name: "gallery-inputs",
      component: () => import("@/features/dev/InputsPage.vue"),
    },
    {
      // What appears over the page, and what says something happened.
      path: "/dev/gallery/overlays",
      name: "gallery-overlays",
      component: () => import("@/features/dev/OverlaysPage.vue"),
    },
    {
      // Tabs, panes, a table of 100,000 rows, a tree, and the rest.
      path: "/dev/gallery/data",
      name: "gallery-data",
      component: () => import("@/features/dev/DataPage.vue"),
    },
    {
      // Forms drawn from JSON Schemas: the gallery's own, and the server's.
      path: "/dev/gallery/forms",
      name: "gallery-forms",
      component: () => import("@/features/dev/FormsPage.vue"),
    },
    {
      // Charts drawn by the app, from data recorded from the simulated instrument.
      path: "/dev/gallery/charts",
      name: "gallery-charts",
      component: () => import("@/features/dev/ChartsPage.vue"),
    },
    {
      // Calls through the typed client: an action followed, and failures as one model.
      path: "/dev/gallery/api",
      name: "gallery-api",
      component: () => import("@/features/dev/ApiPage.vue"),
    },
    {
      // Live Things: the stage's state, kept by the server.
      path: "/dev/gallery/things",
      name: "gallery-things",
      component: () => import("@/features/dev/ThingsPage.vue"),
    },
    {
      // The connection, the session and the control lease (W1.3).
      path: "/dev/gallery/connection",
      name: "gallery-connection",
      component: () => import("@/features/dev/ConnectionPage.vue"),
    },
    {
      // The host adapter: the platform's services, tried by hand (W1.4).
      path: "/dev/gallery/host",
      name: "gallery-host",
      component: () => import("@/features/dev/HostPage.vue"),
    },
    {
      // The live preview: a worker's frames, drawn with WebGL2 (W1.5).
      path: "/dev/gallery/preview",
      name: "gallery-preview",
      component: () => import("@/features/dev/PreviewPage.vue"),
    },
  ],
});
