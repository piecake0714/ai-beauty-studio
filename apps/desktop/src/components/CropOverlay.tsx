import { useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { CROP_RATIOS, dragCrop, fitCropToRatio, FULL_CROP, orientRatio, type CropHandle } from "../adjustments";
import { useApp, useI18n } from "../context";
import type { MessageKey } from "../i18n";
import { toImage, toViewport } from "../view";

const HANDLES: CropHandle[] = ["nw", "n", "ne", "e", "se", "s", "sw", "w"];

/** The crop frame over the whole photo: drag its edges, corners or inside;
 * aspect presets; apply, cancel or reset. */
export function CropOverlay() {
  const { t } = useI18n();
  const document = useApp((s) => s.document);
  const camera = useApp((s) => s.camera);
  const viewport = useApp((s) => s.viewport);
  const crop = useApp((s) => s.live.crop);
  const setCropLive = useApp((s) => s.setCropLive);
  const applyCrop = useApp((s) => s.applyCrop);
  const cancelCrop = useApp((s) => s.cancelCrop);
  const [ratioKey, setRatioKey] = useState("free");
  const dragRef = useRef<{ handle: CropHandle; start: { x: number; y: number }; rect: Rect } | null>(null);

  if (!document || !camera || !viewport) return null;
  const size = document.size;
  const dpr = window.devicePixelRatio || 1;
  const rect: Rect = {
    x: crop.x * size.width,
    y: crop.y * size.height,
    width: crop.width * size.width,
    height: crop.height * size.height,
  };
  const corner = toViewport(camera, viewport, { x: rect.x, y: rect.y });
  const ratioOf = (key: string): number | null => {
    const preset = CROP_RATIOS.find((r) => r.key === key)?.ratio ?? null;
    return preset === -1 ? size.width / size.height : preset;
  };

  const point = (event: ReactPointerEvent<HTMLElement>) => {
    const box = (event.currentTarget.closest(".image-canvas") as HTMLElement | null)?.getBoundingClientRect();
    if (!box) return null;
    return toImage(camera, viewport, { x: (event.clientX - box.left) * dpr, y: (event.clientY - box.top) * dpr });
  };
  const begin = (handle: CropHandle) => (event: ReactPointerEvent<HTMLElement>) => {
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    const start = point(event);
    if (start) dragRef.current = { handle, start, rect };
  };
  const move = (event: ReactPointerEvent<HTMLElement>) => {
    const drag = dragRef.current;
    if (!drag || !event.currentTarget.hasPointerCapture(event.pointerId)) return;
    const now = point(event);
    if (!now) return;
    const ratio = ratioOf(ratioKey);
    const oriented = ratio ? orientRatio(ratio, drag.rect.width, drag.rect.height) : null;
    const next = dragCrop(drag.rect, drag.handle, now.x - drag.start.x, now.y - drag.start.y, oriented, size);
    setCropLive({
      x: next.x / size.width,
      y: next.y / size.height,
      width: next.width / size.width,
      height: next.height / size.height,
    });
  };
  const end = () => {
    dragRef.current = null;
  };
  const chooseRatio = (key: string) => {
    setRatioKey(key);
    const ratio = ratioOf(key);
    if (ratio) setCropLive(fitCropToRatio(crop, ratio, size));
  };

  return (
    <div className="crop-layer">
      <div
        className="crop-frame"
        style={{
          left: `${corner.x / dpr}px`,
          top: `${corner.y / dpr}px`,
          width: `${(rect.width * camera.zoom) / dpr}px`,
          height: `${(rect.height * camera.zoom) / dpr}px`,
        }}
        onPointerDown={begin("move")}
        onPointerMove={move}
        onPointerUp={end}
        role="group"
        aria-label={t("crop.frame")}
      >
        {HANDLES.map((handle) => (
          <div
            key={handle}
            className={`crop-handle ${handle}`}
            onPointerDown={begin(handle)}
            onPointerMove={move}
            onPointerUp={end}
          />
        ))}
      </div>
      <div className="crop-bar" role="toolbar" aria-label={t("crop.title")}>
        <div className="segmented small" role="radiogroup" aria-label={t("crop.ratio")}>
          {CROP_RATIOS.map(({ key }) => (
            <button
              key={key}
              type="button"
              role="radio"
              aria-checked={ratioKey === key}
              className={ratioKey === key ? "active" : ""}
              onClick={() => chooseRatio(key)}
            >
              {key === "free" || key === "original" ? t(`crop.${key}` as MessageKey) : key}
            </button>
          ))}
        </div>
        <button type="button" className="ghost small" onClick={() => setCropLive(FULL_CROP)}>
          {t("crop.reset")}
        </button>
        <button type="button" className="ghost small" onClick={cancelCrop}>
          {t("discard.cancel")}
        </button>
        <button type="button" className="primary small" onClick={() => void applyCrop()}>
          {t("crop.apply")}
        </button>
      </div>
    </div>
  );
}

interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}
