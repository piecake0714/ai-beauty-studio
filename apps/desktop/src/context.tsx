import { createContext, useContext, useMemo, type ReactNode } from "react";
import { useStore } from "zustand";
import { resolveLanguage, translate, type Language, type MessageKey } from "./i18n";
import type { AppState, AppStore } from "./store";

const StoreContext = createContext<AppStore | null>(null);

export function StoreProvider({ store, children }: { store: AppStore; children: ReactNode }) {
  return <StoreContext.Provider value={store}>{children}</StoreContext.Provider>;
}

export function useApp<T>(selector: (state: AppState) => T): T {
  const store = useContext(StoreContext);
  if (!store) throw new Error("useApp must be used inside StoreProvider");
  return useStore(store, selector);
}

export type Translate = (key: MessageKey, values?: Record<string, string | number>) => string;

/** The active UI language and a translate function bound to it. */
export function useI18n(): { language: Language; t: Translate } {
  const preference = useApp((s) => s.settings.language);
  return useMemo(() => {
    const language = resolveLanguage(preference, navigator.language);
    return { language, t: (key, values) => translate(language, key, values) };
  }, [preference]);
}
