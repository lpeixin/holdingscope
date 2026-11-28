import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { messages, type LanguagePreference, type Locale } from "./messages";

const preferenceKey = "holdingscope.language";

function readPreference(): LanguagePreference {
  const stored = window.localStorage.getItem(preferenceKey);
  return stored === "en" || stored === "zh-CN" ? stored : "system";
}

function browserLocale(): Locale {
  const preferred = navigator.languages?.[0] || navigator.language || "";
  return /^zh(?:[-_]|$)/i.test(preferred) ? "zh-CN" : "en";
}

const LocaleContext = createContext<{
  locale: Locale;
  preference: LanguagePreference;
  setPreference: (preference: LanguagePreference) => void;
} | null>(null);

export function LocaleProvider({ children }: { children: ReactNode }) {
  const [preference, setPreference] = useState(readPreference);
  const [systemLocale, setSystemLocale] = useState(browserLocale);
  const locale = preference === "system" ? systemLocale : preference;

  useEffect(() => {
    const update = () => setSystemLocale(browserLocale());
    window.addEventListener("languagechange", update);
    return () => window.removeEventListener("languagechange", update);
  }, []);

  useEffect(() => {
    document.documentElement.lang = locale;
  }, [locale]);

  return (
    <LocaleContext.Provider value={{
      locale,
      preference,
      setPreference: (value) => {
        window.localStorage.setItem(preferenceKey, value);
        setPreference(value);
      },
    }}>
      {children}
    </LocaleContext.Provider>
  );
}

export function useLocale() {
  const settings = useContext(LocaleContext);
  if (!settings) throw new Error("LocaleProvider is missing");
  return { ...settings, t: messages[settings.locale] };
}
