import { createI18n } from "vue-i18n";
import zh from "./locales/zh.json";
import en from "./locales/en.json";

function getInitialLocale(): string {
  const saved = localStorage.getItem("locale");
  if (saved === "zh" || saved === "en") return saved;
  // Auto-detect from browser/system
  const lang = navigator.language || (navigator as any).userLanguage || "zh";
  if (lang.startsWith("zh")) return "zh";
  return "en";
}

const i18n = createI18n({
  locale: getInitialLocale(),
  fallbackLocale: "zh",
  messages: { zh, en },
});

export default i18n;
