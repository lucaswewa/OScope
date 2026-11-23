import { createI18n } from "vue-i18n";

import en from "@/app/locales/en";

/** Every string the app shows comes from here (D29): English, for now. */
export const i18n = createI18n({
  legacy: false,
  locale: "en",
  fallbackLocale: "en",
  messages: { en },
});
