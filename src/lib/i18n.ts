import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import LanguageDetector from "i18next-browser-languagedetector";

import de from "@/locales/de";
import en from "@/locales/en";
import fr from "@/locales/fr";
import it from "@/locales/it";
import pl from "@/locales/pl";
import es from "@/locales/es";

export const SUPPORTED_LANGUAGES = [
  { code: "de", label: "Deutsch" },
  { code: "en", label: "English" },
  { code: "fr", label: "Français" },
  { code: "it", label: "Italiano" },
  { code: "pl", label: "Polski" },
  { code: "es", label: "Español" },
] as const;

export type LanguageCode = (typeof SUPPORTED_LANGUAGES)[number]["code"];

i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources: {
      de: { translation: de },
      en: { translation: en },
      fr: { translation: fr },
      it: { translation: it },
      pl: { translation: pl },
      es: { translation: es },
    },
    fallbackLng: "de",
    supportedLngs: ["de", "en", "fr", "it", "pl", "es"],
    interpolation: { escapeValue: false },
    detection: {
      order: ["localStorage", "navigator"],
      lookupLocalStorage: "processfox-locale",
      caches: ["localStorage"],
    },
  });

export function applyPersistedLocale(locale: string | null) {
  if (locale) {
    i18n.changeLanguage(locale);
  }
}

export default i18n;
