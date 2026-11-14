import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { parseFrame, type Frame } from "./frame";
import { detectLanguage, errorMessage, translate, type MessageKey } from "./i18n";

const IMAGE_EXTENSIONS = ["jpg", "jpeg", "png", "tif", "tiff", "webp"];

export function App() {
  const language = useMemo(() => detectLanguage(navigator.language), []);
  const t = (key: MessageKey) => translate(language, key);

  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [frame, setFrame] = useState<Frame | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    document.documentElement.lang = language;
  }, [language]);

  // Draw the current frame. The pixels come from the Rust core; the UI only
  // displays them.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !frame) return;
    canvas.width = frame.width;
    canvas.height = frame.height;
    const context = canvas.getContext("2d");
    if (!context) return;
    context.putImageData(new ImageData(frame.pixels, frame.width, frame.height), 0, 0);
  }, [frame]);

  async function handleOpen() {
    const path = await open({
      multiple: false,
      directory: false,
      filters: [{ name: t("imageFiles"), extensions: IMAGE_EXTENSIONS }],
    });
    if (typeof path !== "string") return; // dialog cancelled

    setBusy(true);
    setError(null);
    try {
      const buffer = await invoke<ArrayBuffer>("open_image", { path });
      setFrame(parseFrame(buffer));
    } catch (reason) {
      // The core returns a stable error code; anything else is internal.
      const code = typeof reason === "string" ? reason : "internal";
      setError(errorMessage(language, code));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="app">
      <header className="toolbar">
        <button type="button" onClick={handleOpen} disabled={busy}>
          {busy ? t("opening") : t("openImage")}
        </button>
        {frame && (
          <span className="info">
            {t("sizeLabel")}: {frame.sourceWidth} × {frame.sourceHeight}
          </span>
        )}
      </header>
      {error && (
        <div className="error" role="alert">
          {error}
        </div>
      )}
      <main className="stage">
        {frame ? <canvas ref={canvasRef} /> : <p className="hint">{t("emptyHint")}</p>}
      </main>
    </div>
  );
}
