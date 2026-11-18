import "@/ui/tokens/tokens.css";
import "@/ui/tokens/base.css";

import { createPinia } from "pinia";
import { createApp } from "vue";

import App from "@/app/App.vue";
import { i18n } from "@/app/i18n";
import { router } from "@/app/router";
import { useDisplay } from "@/features/display/display";

const app = createApp(App).use(createPinia()).use(router).use(i18n);
// The theme and the accent the browser kept, before anything is drawn.
useDisplay().start();
app.mount("#app");
