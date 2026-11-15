import { useEffect, useRef, useState } from "react";
import { errorCode } from "../api";
import { useApp, useI18n } from "../context";
import { errorMessage } from "../i18n";
import type { LanguagePreference, ThemePreference } from "../types";
import { AiSection } from "./AiSection";

type Notices = { kind: "loading" } | { kind: "ready"; text: string } | { kind: "failed"; code: string };

function NoticesView({ onBack }: { onBack: () => void }) {
  const { t, language } = useI18n();
  const api = useApp((s) => s.api);
  const [notices, setNotices] = useState<Notices>({ kind: "loading" });

  useEffect(() => {
    let cancelled = false;
    api
      .thirdPartyNotices()
      .then((text) => !cancelled && setNotices({ kind: "ready", text }))
      .catch((reason) => {
        const code = errorCode(reason);
        if (!cancelled) setNotices({ kind: "failed", code: code === "not_found" ? "notices_missing" : code });
      });
    return () => {
      cancelled = true;
    };
  }, [api]);

  return (
    <>
      <button type="button" className="ghost small" onClick={onBack}>
        ← {t("settings.back")}
      </button>
      <h2>{t("settings.notices")}</h2>
      {notices.kind === "ready" && <pre className="notices">{notices.text}</pre>}
      {notices.kind === "failed" && (
        <p className="error-text" role="alert">
          {errorMessage(language, notices.code)}
        </p>
      )}
      {notices.kind === "loading" && <p className="muted" aria-busy="true" />}
    </>
  );
}

function Choice<T extends string>({
  legend,
  name,
  value,
  options,
  onChange,
}: {
  legend: string;
  name: string;
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
}) {
  return (
    <fieldset className="choice">
      <legend>{legend}</legend>
      {options.map((option) => (
        <label key={option.value}>
          <input
            type="radio"
            name={name}
            value={option.value}
            checked={value === option.value}
            onChange={() => onChange(option.value)}
          />
          {option.label}
        </label>
      ))}
    </fieldset>
  );
}

export function SettingsDialog({ onClose }: { onClose: () => void }) {
  const { t } = useI18n();
  const theme = useApp((s) => s.settings.theme);
  const language = useApp((s) => s.settings.language);
  const version = useApp((s) => s.version);
  const setTheme = useApp((s) => s.setTheme);
  const setLanguage = useApp((s) => s.setLanguage);
  const [page, setPage] = useState<"main" | "notices">("main");
  const closeRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    closeRef.current?.focus();
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="overlay" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <div className="dialog" role="dialog" aria-modal="true" aria-labelledby="settings-title">
        <div className="dialog-head">
          <h1 id="settings-title">{t("settings")}</h1>
          <button ref={closeRef} type="button" className="ghost" onClick={onClose}>
            {t("close")}
          </button>
        </div>

        {page === "notices" ? (
          <NoticesView onBack={() => setPage("main")} />
        ) : (
          <>
            <Choice<ThemePreference>
              legend={t("settings.theme")}
              name="theme"
              value={theme}
              onChange={setTheme}
              options={[
                { value: "system", label: t("settings.system") },
                { value: "light", label: t("settings.light") },
                { value: "dark", label: t("settings.dark") },
              ]}
            />
            <Choice<LanguagePreference>
              legend={t("settings.language")}
              name="language"
              value={language}
              onChange={setLanguage}
              options={[
                { value: "system", label: t("settings.system") },
                { value: "zh-Hant", label: "繁體中文" },
                { value: "en", label: "English" },
              ]}
            />
            <AiSection />
            <section className="about">
              <h2>{t("settings.about")}</h2>
              <p>
                {t("appName")} · {t("settings.version", { version })}
              </p>
              <button type="button" className="ghost small" onClick={() => setPage("notices")}>
                {t("settings.notices")}
              </button>
            </section>
          </>
        )}
      </div>
    </div>
  );
}
