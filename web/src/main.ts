import "@/ui/tokens/tokens.css";
import "@/ui/tokens/base.css";

import { createPinia } from "pinia";
import { createApp } from "vue";

import App from "@/app/App.vue";
import { i18n } from "@/app/i18n";
import { router } from "@/app/router";
import { useConnection } from "@/api/connection/connection";
import { useDisplay } from "@/features/display/display";
import { guardSession, keepSession } from "@/features/login/keep";

const app = createApp(App).use(createPinia()).use(router).use(i18n);
// The theme and the accent the browser kept, before anything is drawn.
useDisplay().start();
// The server watched, and the session kept, before the first page (W1.3).
useConnection().watch();
guardSession(router);
void keepSession();
app.mount("#app");
