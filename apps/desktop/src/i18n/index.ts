// Sets up i18next with the two languages of the app. Which one is shown
// is decided in `lib/settings.ts`.

import i18n from "i18next";
import { initReactI18next } from "react-i18next";

import { en } from "./en";
import { ja } from "./ja";

export const LOCALES = ["en", "ja"] as const;
export type Locale = (typeof LOCALES)[number];

declare module "i18next" {
  interface CustomTypeOptions {
    defaultNS: "translation";
    resources: { translation: typeof en };
  }
}

void i18n.use(initReactI18next).init({
  resources: { en: { translation: en }, ja: { translation: ja } },
  lng: "en",
  fallbackLng: "en",
  // React escapes what it renders.
  interpolation: { escapeValue: false },
});

export default i18n;
