import { useApp, useI18n } from "../context";

/** Asks the user to point to a project's original photo. */
export function LocateDialog() {
  const { t } = useI18n();
  const pending = useApp((s) => s.pendingLocate);
  const locate = useApp((s) => s.locateOriginal);
  const cancel = useApp((s) => s.cancelLocate);
  if (!pending) return null;
  return (
    <div className="overlay">
      <div className="dialog small-dialog" role="alertdialog" aria-modal="true" aria-labelledby="locate-title">
        <div className="dialog-head">
          <h1 id="locate-title">{t(pending.reason === "original_changed" ? "locate.changedTitle" : "locate.missingTitle")}</h1>
        </div>
        <p>{t(pending.reason === "original_changed" ? "locate.changed" : "locate.missing")}</p>
        {pending.expected && <p className="note path-text">{pending.expected}</p>}
        <p className="note">{t("locate.safe")}</p>
        <div className="dialog-actions">
          <button type="button" className="ghost" onClick={cancel}>
            {t("discard.cancel")}
          </button>
          <button type="button" className="primary" onClick={() => void locate(t("imageFiles"))}>
            {t("locate.choose")}
          </button>
        </div>
      </div>
    </div>
  );
}
