import { useEffect, useRef, type PointerEvent as ReactPointerEvent } from "react";
import { errorCode, type RenderRequest } from "../api";
import { useApp, useI18n } from "../context";
import { parseFrame, type Frame } from "../frame";
import type { Adjustments } from "../types";
import { placeFrame, sameCamera, visibleRegion, type Camera, type Size } from "../view";

/** After a zoom or pan, wait this long for the movement to pause before
 * asking for a sharp render. Adjustment changes are sent at once. */
const CAMERA_DELAY_MS = 60;

interface Detail {
  canvas: HTMLCanvasElement;
  camera: Camera;
  viewport: Size;
  /** Adjustments and split the render was made with. */
  look: string;
}

function frameToCanvas(frame: Frame): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  canvas.width = frame.width;
  canvas.height = frame.height;
  canvas.getContext("2d")?.putImageData(new ImageData(frame.pixels, frame.width, frame.height), 0, 0);
  return canvas;
}

function lookKey(adjustments: Adjustments, originalColumns: number): string {
  return `${adjustments.exposure}|${adjustments.brightness}|${adjustments.contrast}|${originalColumns}`;
}

function sameViewport(a: Size, b: Size): boolean {
  return a.width === b.width && a.height === b.height;
}

/**
 * The editor's image view. Shows the overview at once, moves and scales the
 * last sharp render instantly while the user zooms or pans, and asks the
 * core for a new sharp render of exactly what is on screen: after a short
 * pause for camera moves, immediately for adjustment changes. At most one
 * render is in flight; the newest wanted view is sent when it returns.
 * All pixels come from the core; this component only places them.
 */
export function ImageCanvas() {
  const { t } = useI18n();
  const api = useApp((s) => s.api);
  const document = useApp((s) => s.document);
  const camera = useApp((s) => s.camera);
  const viewport = useApp((s) => s.viewport);
  const live = useApp((s) => s.live);
  const splitView = useApp((s) => s.splitView);
  const split = useApp((s) => s.split);
  const holdOriginal = useApp((s) => s.holdOriginal);
  const originalColumns = useApp((s) => s.originalColumns);
  const setViewport = useApp((s) => s.setViewport);
  const zoomBy = useApp((s) => s.zoomBy);
  const pan = useApp((s) => s.pan);
  const setSplit = useApp((s) => s.setSplit);
  const setRendering = useApp((s) => s.setRendering);
  const reportError = useApp((s) => s.reportError);

  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const overviewRef = useRef<HTMLCanvasElement | null>(null);
  const detailRef = useRef<Detail | null>(null);
  const requestIdRef = useRef(0);
  const inFlightRef = useRef(false);
  const timerRef = useRef<number | undefined>(undefined);
  const dragRef = useRef<{ x: number; y: number } | null>(null);

  // originalColumns reads splitView, split and holdOriginal from the store;
  // they are listed so this recomputes when they change.
  void splitView;
  void split;
  void holdOriginal;
  const columns = viewport ? originalColumns(viewport.width) : 0;
  const look = lookKey(live, columns);
  // Latest values for callbacks that outlive this render.
  const latestRef = useRef({ document, camera, viewport, live, columns, look });
  latestRef.current = { document, camera, viewport, live, columns, look };

  // Track the canvas size in device pixels.
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const update = () => {
      const ratio = window.devicePixelRatio || 1;
      const width = Math.max(1, Math.round(container.clientWidth * ratio));
      const height = Math.max(1, Math.round(container.clientHeight * ratio));
      setViewport({ width, height });
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(container);
    return () => observer.disconnect();
  }, [setViewport]);

  function detailMatches(detail: Detail | null): boolean {
    const { camera, viewport, look } = latestRef.current;
    return (
      !!detail &&
      !!camera &&
      !!viewport &&
      sameCamera(detail.camera, camera) &&
      sameViewport(detail.viewport, viewport) &&
      detail.look === look
    );
  }

  function draw() {
    const { document, camera, viewport } = latestRef.current;
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d");
    if (!canvas || !context || !viewport) return;
    if (canvas.width !== viewport.width) canvas.width = viewport.width;
    if (canvas.height !== viewport.height) canvas.height = viewport.height;
    context.clearRect(0, 0, canvas.width, canvas.height);
    const overview = overviewRef.current;
    if (!document || !camera || !overview) return;

    const detail = detailRef.current;
    const coversView = !!detail && sameCamera(detail.camera, camera) && sameViewport(detail.viewport, viewport);

    if (!coversView) {
      // The overview covers the whole image, so nothing is left blank.
      context.imageSmoothingEnabled = true;
      context.imageSmoothingQuality = "high";
      context.drawImage(
        overview,
        (0 - camera.cx) * camera.zoom + viewport.width / 2,
        (0 - camera.cy) * camera.zoom + viewport.height / 2,
        document.size.width * camera.zoom,
        document.size.height * camera.zoom,
      );
    }
    if (detail) {
      const place = placeFrame(detail.camera, detail.viewport, camera, viewport);
      // Pixel-exact when drawn 1:1; smoothed while moved or scaled.
      context.imageSmoothingEnabled = !coversView;
      context.drawImage(detail.canvas, place.x, place.y, place.width, place.height);
    }
  }

  /** Send a render of the current view unless one is in flight; the
   * finished render calls this again if the view changed meanwhile. */
  function sendRender() {
    const { document, camera, viewport, live, columns, look } = latestRef.current;
    if (!document || !camera || !viewport || inFlightRef.current) return;
    if (detailMatches(detailRef.current)) {
      setRendering(false);
      return;
    }
    const request: RenderRequest = {
      id: ++requestIdRef.current,
      ...visibleRegion(camera, viewport),
      adjustments: live,
      originalColumns: columns,
    };
    const documentId = document.id;
    inFlightRef.current = true;
    setRendering(true);
    api
      .renderView(request)
      .then((buffer) => {
        if (latestRef.current.document?.id !== documentId) return;
        detailRef.current = { canvas: frameToCanvas(parseFrame(buffer)), camera, viewport, look };
        requestAnimationFrame(draw);
      })
      .catch((reason) => {
        const code = errorCode(reason);
        if (code !== "superseded" && code !== "no_document") reportError(code);
      })
      .finally(() => {
        inFlightRef.current = false;
        if (detailMatches(detailRef.current)) {
          setRendering(false);
        } else {
          sendRender();
        }
      });
  }

  // A different image: forget the previous image's sharp render.
  const documentId = document?.id;
  useEffect(() => {
    detailRef.current = null;
  }, [documentId]);

  // New overview (new image, or adjustments committed): redraw with it.
  const overviewFrame = document?.overview;
  useEffect(() => {
    overviewRef.current = overviewFrame ? frameToCanvas(overviewFrame) : null;
    requestAnimationFrame(draw);
    // draw reads current values through latestRef.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [overviewFrame]);

  // Redraw on every change and schedule a sharp render.
  useEffect(() => {
    requestAnimationFrame(draw);
    if (!document || !camera || !viewport || detailMatches(detailRef.current)) return;
    const detail = detailRef.current;
    const onlyLookChanged = !!detail && sameCamera(detail.camera, camera) && sameViewport(detail.viewport, viewport);
    window.clearTimeout(timerRef.current);
    timerRef.current = window.setTimeout(sendRender, onlyLookChanged ? 0 : CAMERA_DELAY_MS);
    // draw and sendRender read current values through latestRef.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [document, camera, viewport, look]);

  useEffect(() => () => window.clearTimeout(timerRef.current), []);

  // Wheel zoom around the cursor. Registered directly so the default
  // (scrolling, or the web view's own zoom with Ctrl) can be prevented.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      const ratio = window.devicePixelRatio || 1;
      const rect = canvas.getBoundingClientRect();
      const lines = event.deltaMode === 1 ? 40 : event.deltaMode === 2 ? 800 : 1;
      const factor = Math.exp(-event.deltaY * lines * 0.002);
      zoomBy(factor, { x: (event.clientX - rect.left) * ratio, y: (event.clientY - rect.top) * ratio });
    };
    canvas.addEventListener("wheel", onWheel, { passive: false });
    return () => canvas.removeEventListener("wheel", onWheel);
  }, [zoomBy]);

  const onPointerDown = (event: ReactPointerEvent<HTMLCanvasElement>) => {
    if (event.button !== 0 && event.button !== 1) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    dragRef.current = { x: event.clientX, y: event.clientY };
  };

  const onPointerMove = (event: ReactPointerEvent<HTMLCanvasElement>) => {
    const last = dragRef.current;
    if (!last) return;
    const ratio = window.devicePixelRatio || 1;
    pan((event.clientX - last.x) * ratio, (event.clientY - last.y) * ratio);
    dragRef.current = { x: event.clientX, y: event.clientY };
  };

  const endDrag = () => {
    dragRef.current = null;
  };

  // Split divider: dragged with the pointer, moved with arrow keys.
  const onDividerPointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
  };
  const onDividerPointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (!event.currentTarget.hasPointerCapture(event.pointerId)) return;
    const rect = containerRef.current?.getBoundingClientRect();
    if (rect && rect.width > 0) setSplit((event.clientX - rect.left) / rect.width);
  };

  return (
    <div ref={containerRef} className="image-canvas" title={t("zoom.hint")}>
      <canvas
        ref={canvasRef}
        role="img"
        aria-label={document?.info?.name ?? undefined}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
      />
      {holdOriginal && <span className="compare-label single">{t("compare.before")}</span>}
      {splitView && !holdOriginal && (
        <>
          <span className="compare-label left">{t("compare.before")}</span>
          <span className="compare-label right">{t("compare.after")}</span>
          <div
            className="split-divider"
            style={{ left: `${split * 100}%` }}
            role="slider"
            tabIndex={0}
            aria-label={t("compare.divider")}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.round(split * 100)}
            onPointerDown={onDividerPointerDown}
            onPointerMove={onDividerPointerMove}
            onKeyDown={(event) => {
              if (event.key === "ArrowLeft") setSplit(split - 0.02);
              if (event.key === "ArrowRight") setSplit(split + 0.02);
            }}
          />
        </>
      )}
    </div>
  );
}
