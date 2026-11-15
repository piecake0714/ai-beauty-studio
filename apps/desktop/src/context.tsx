import { createContext, useContext, useMemo, type ReactNode } from "react";
import { useStore } from "zustand";
import { resolveLanguage, translate, type Language, type MessageKey } from "./i18n";
import type { AppState, AppStore, DiscardPrompt } from "./store";

const StoreContext = createContext<AppStore | null>(null);

export function StoreProvider({ store, children }: { store: AppStore; children: ReactNode }) {
  return <StoreContext.Provider value={store}>{children}</StoreContext.Provider>;
}

export function useApp<T>(selector: (state: AppState) => T): T {
  return useStore(useAppStore(), selector);
}

/** The store itself, for reading the newest state inside event handlers. */
export function useAppStore(): AppStore {
  const store = useContext(StoreContext);
  if (!store) throw new Error("useApp must be used inside StoreProvider");
  return store;
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

/** Question asked before unsaved adjustments are lost. */
export function useDiscardPrompt(reason: "open" | "close"): DiscardPrompt {
  const { t } = useI18n();
  return useMemo(
    () => ({
      title: t("discard.title"),
      message: t(reason === "open" ? "discard.open" : "discard.close"),
      yes: t(reason === "open" ? "discard.openYes" : "discard.closeYes"),
      no: t("discard.cancel"),
    }),
    [t, reason],
  );
}
