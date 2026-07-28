import i18next from "i18next";
import { initReactI18next } from "react-i18next";
import en from "./locales/en.json";
import zhCN from "./locales/zh-CN.json";

function resolveSystemLanguage(): "en" | "zh-CN" {
  const language =
    typeof navigator === "undefined" ? "en" : navigator.language.toLowerCase();

  return language.startsWith("zh-") ? "zh-CN" : "en";
}

export const i18n = i18next.createInstance();

if (!i18n.isInitialized) {
  void i18n.use(initReactI18next).init({
    fallbackLng: "en",
    initAsync: false,
    interpolation: {
      escapeValue: false,
    },
    lng: resolveSystemLanguage(),
    load: "currentOnly",
    resources: {
      en: { translation: en },
      "zh-CN": { translation: zhCN },
    },
    showSupportNotice: false,
    supportedLngs: ["en", "zh-CN"],
  });
}
