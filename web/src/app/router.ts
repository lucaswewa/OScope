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
  ],
});
