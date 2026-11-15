import { useEffect, useMemo, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { errorCode, type RenderRequest } from "../api";
import { useApp, useI18n } from "../context";
import { parseFrame, type Frame } from "../frame";
import { adjustmentsKey, cropRect, FULL_CROP, sameAdjustments } from "../adjustments";
import type { Adjustments } from "../types";
import { placeFrame, sameCamera, toImage, toViewport, visibleRegion, type Camera, type Size } from "../view";
import { CropOverlay } from "./CropOverlay";

/** After a zoom or pan, wait this long for the movement to pause before
 * asking for a sharp render. Adjustment changes are sent at once. */
const CAMERA_DELAY_MS = 60;

/** While a slider is being dragged, render at this fraction of the screen
 * resolution so the picture keeps up; full resolution follows on release. */
const DRAFT_SCALE = 0.5;

interface Detail {
  canvas: HTMLCanvasElement;
  camera: Camera;
  viewport: Size;
  /** Adjustments, split and resolution the render was made with. */
  look: string;
  /** Rendered at reduced resolution. */
  draft: boolean;
}

function frameToCanvas(frame: Frame): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  canvas.width = frame.width;
  canvas.height = frame.height;
  canvas.getContext("2d")?.putImageData(new ImageData(frame.pixels, frame.width, frame.height), 0, 0);
  return canvas;
}

function lookKey(adjustments: Adjustments, originalColumns: number, draft: boolean, separated: boolean): string {
  return `${adjustmentsKey(adjustments)}|${originalColumns}|${draft ? "draft" : "full"}|${separated ? "separated" : ""}`;
}

/** A light/dark checkerboard shown behind a removed background. */
function checkerPattern(context: CanvasRenderingContext2D): CanvasPattern | null {
  const tile = document.createElement("canvas");
  tile.width = tile.height = 16;
  const c = tile.getContext("2d");
  if (!c) return null;
  c.fillStyle = "#ffffff";
  c.fillRect(0, 0, 16, 16);
  c.fillStyle = "#d4d4d4";
  c.fillRect(0, 0, 8, 8);
  c.fillRect(8, 8, 8, 8);
  return context.createPattern(tile, "repeat");
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
  const committed = useApp((s) => s.edit?.adjustments ?? null);
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
  const separated = useApp((s) => s.background?.state === "ready");
  const shaped = useApp((s) => s.shape?.state === "ready");
  const setLive = useApp((s) => s.setLive);
  const commitLive = useApp((s) => s.commitLive);
  const tool = useApp((s) => s.tool);
  const brush = useApp((s) => s.brush);
  const canPaint = useApp(
    (s) => (s.tool === "brush" || s.tool === "eraser") && s.live.layers.some((l) => l.id === s.selectedLayer),
  );
  const beginStroke = useApp((s) => s.beginStroke);
  const extendStroke = useApp((s) => s.extendStroke);
  const endStroke = useApp((s) => s.endStroke);
  const crop = useApp((s) => s.live.crop);
  const bounds = useMemo(
    () => (document ? (tool === "crop" ? { x: 0, y: 0, ...document.size } : cropRect(crop, document.size)) : null),
    [document, tool, crop],
  );
  const [cursor, setCursor] = useState<{ x: number; y: number } | null>(null);

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
  // Uncommitted adjustments mean a slider is being dragged.
  // The crop does not change pixels (only which are shown), so it does
  // not call for a new render.
  const renderLive = { ...live, crop: FULL_CROP };
  const draft = committed !== null && !sameAdjustments(renderLive, { ...committed, crop: FULL_CROP });
  const look = lookKey(renderLive, columns, draft, separated) + (shaped ? "|shaped" : "");
  // Latest values for callbacks that outlive this render.
  const latestRef = useRef({ document, camera, viewport, live, columns, look, draft, bounds });
  latestRef.current = { document, camera, viewport, live, columns, look, draft, bounds };

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
    context.save();
    try {
      drawImage(context, viewport);
    } finally {
      context.restore();
    }
  }

  /** Draw the image, clipped to the shown part (the crop). */
  function drawImage(context: CanvasRenderingContext2D, viewport: Size) {
    const { document, camera, bounds: shown } = latestRef.current;
    const overview = overviewRef.current;
    if (!document || !camera || !overview) return;
    if (shown) {
      const corner = toViewport(camera, viewport, { x: shown.x, y: shown.y });
      context.beginPath();
      context.rect(corner.x, corner.y, shown.width * camera.zoom, shown.height * camera.zoom);
      context.clip();
    }

    if (latestRef.current.live.backgroundMode === "transparent") {
      // Checkerboard under the image, so removed areas read as transparent.
      const pattern = checkerPattern(context);
      if (pattern) {
        context.fillStyle = pattern;
        context.fillRect(
          (0 - camera.cx) * camera.zoom + viewport.width / 2,
          (0 - camera.cy) * camera.zoom + viewport.height / 2,
          document.size.width * camera.zoom,
          document.size.height * camera.zoom,
        );
      }
    }

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
      // Pixel-exact when drawn 1:1; smoothed while moved, scaled or a draft.
      context.imageSmoothingEnabled = !coversView || detail.draft;
      context.drawImage(detail.canvas, place.x, place.y, place.width, place.height);
    }
  }

  /** Send a render of the current view unless one is in flight; the
   * finished render calls this again if the view changed meanwhile. */
  function sendRender() {
    const { document, camera, viewport, live, columns, look, draft } = latestRef.current;
    if (!document || !camera || !viewport || inFlightRef.current) return;
    if (detailMatches(detailRef.current)) {
      setRendering(false);
      return;
    }
    const region = visibleRegion(camera, viewport);
    const scale = draft ? DRAFT_SCALE : 1;
    const request: RenderRequest = {
      id: ++requestIdRef.current,
      ...region,
      outWidth: Math.max(1, Math.round(region.outWidth * scale)),
      outHeight: Math.max(1, Math.round(region.outHeight * scale)),
      adjustments: live,
      originalColumns: Math.round(columns * scale),
    };
    const documentId = document.id;
    inFlightRef.current = true;
    setRendering(true);
    api
      .renderView(request)
      .then((buffer) => {
        if (latestRef.current.document?.id !== documentId) return;
        detailRef.current = { canvas: frameToCanvas(parseFrame(buffer)), camera, viewport, look, draft };
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

  /** Source pixel under a pointer event. */
  const sourcePoint = (event: ReactPointerEvent<HTMLElement>) => {
    const rect = containerRef.current?.getBoundingClientRect();
    const { camera: cam, viewport: view } = latestRef.current;
    if (!rect || !cam || !view) return null;
    const ratio = window.devicePixelRatio || 1;
    return toImage(cam, view, { x: (event.clientX - rect.left) * ratio, y: (event.clientY - rect.top) * ratio });
  };

  const onPointerDown = (event: ReactPointerEvent<HTMLCanvasElement>) => {
    if (event.button !== 0 && event.button !== 1) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    if (canPaint && event.button === 0) {
      const point = sourcePoint(event);
      if (point) beginStroke(point.x, point.y);
      return;
    }
    dragRef.current = { x: event.clientX, y: event.clientY };
  };

  const onPointerMove = (event: ReactPointerEvent<HTMLCanvasElement>) => {
    if (canPaint) {
      const rect = containerRef.current?.getBoundingClientRect();
      if (rect) setCursor({ x: event.clientX - rect.left, y: event.clientY - rect.top });
      if (event.currentTarget.hasPointerCapture(event.pointerId)) {
        const point = sourcePoint(event);
        if (point) extendStroke(point.x, point.y);
        return;
      }
    }
    const last = dragRef.current;
    if (!last) return;
    const ratio = window.devicePixelRatio || 1;
    pan((event.clientX - last.x) * ratio, (event.clientY - last.y) * ratio);
    dragRef.current = { x: event.clientX, y: event.clientY };
  };

  const endDrag = () => {
    dragRef.current = null;
    void endStroke();
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

  // The light's dot, in CSS pixels over the canvas.
  const ratio = typeof window === "undefined" ? 1 : window.devicePixelRatio || 1;
  const lightDot =
    document && camera && viewport && live.lightKind !== "none"
      ? toViewport(camera, viewport, { x: live.lightX * document.size.width, y: live.lightY * document.size.height })
      : null;
  const moveLightTo = (clientX: number, clientY: number) => {
    const rect = containerRef.current?.getBoundingClientRect();
    const { document: doc, camera: cam, viewport: view } = latestRef.current;
    if (!rect || !doc || !cam || !view) return;
    const point = toImage(cam, view, { x: (clientX - rect.left) * ratio, y: (clientY - rect.top) * ratio });
    const clamp = (v: number) => Math.min(2, Math.max(-1, v));
    setLive({ ...latestRef.current.live, lightX: clamp(point.x / doc.size.width), lightY: clamp(point.y / doc.size.height) });
  };
  const nudgeLight = (dx: number, dy: number) => {
    const current = latestRef.current.live;
    setLive({ ...current, lightX: current.lightX + dx, lightY: current.lightY + dy });
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
        onPointerLeave={() => setCursor(null)}
        className={canPaint ? "painting" : undefined}
      />
      {canPaint && cursor && camera && document && (
        <div
          className="brush-cursor"
          aria-hidden="true"
          style={{
            left: `${cursor.x}px`,
            top: `${cursor.y}px`,
            width: `${(brush.size * Math.max(document.size.width, document.size.height) * camera.zoom) / (window.devicePixelRatio || 1)}px`,
            height: `${(brush.size * Math.max(document.size.width, document.size.height) * camera.zoom) / (window.devicePixelRatio || 1)}px`,
          }}
        />
      )}
      {tool === "crop" && <CropOverlay />}
      {lightDot && tool !== "crop" && (
        <div
          className="light-handle"
          style={{ left: `${lightDot.x / ratio}px`, top: `${lightDot.y / ratio}px` }}
          role="slider"
          tabIndex={0}
          aria-label={t("light.handle")}
          aria-valuetext={`${Math.round(live.lightX * 100)}%, ${Math.round(live.lightY * 100)}%`}
          onPointerDown={(event) => {
            event.stopPropagation();
            event.currentTarget.setPointerCapture(event.pointerId);
          }}
          onPointerMove={(event) => {
            if (event.currentTarget.hasPointerCapture(event.pointerId)) moveLightTo(event.clientX, event.clientY);
          }}
          onPointerUp={() => void commitLive()}
          onKeyDown={(event) => {
            const step = 0.02;
            if (event.key === "ArrowLeft") nudgeLight(-step, 0);
            else if (event.key === "ArrowRight") nudgeLight(step, 0);
            else if (event.key === "ArrowUp") nudgeLight(0, -step);
            else if (event.key === "ArrowDown") nudgeLight(0, step);
            else return;
            event.preventDefault();
          }}
          onKeyUp={() => void commitLive()}
        />
      )}
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
