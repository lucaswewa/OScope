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
  ],
});
