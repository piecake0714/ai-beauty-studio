import { useEffect, useRef, type PointerEvent as ReactPointerEvent } from "react";
import { errorCode } from "../api";
import { useApp, useI18n } from "../context";
import { parseFrame, type Frame } from "../frame";
import { placeFrame, sameCamera, visibleRegion, type Camera, type Size } from "../view";

/** Wait this long after the last zoom or pan before asking for a sharp render. */
const RENDER_DELAY_MS = 60;

interface Detail {
  canvas: HTMLCanvasElement;
  camera: Camera;
  viewport: Size;
}

function frameToCanvas(frame: Frame): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  canvas.width = frame.width;
  canvas.height = frame.height;
  canvas.getContext("2d")?.putImageData(new ImageData(frame.pixels, frame.width, frame.height), 0, 0);
  return canvas;
}

/**
 * The editor's image view. Shows the overview at once, moves and scales the
 * last sharp render instantly while the user zooms or pans, and asks the
 * core for a new sharp render of exactly what is on screen once they pause.
 * All pixels come from the core; this component only places them.
 */
export function ImageCanvas() {
  const { t } = useI18n();
  const api = useApp((s) => s.api);
  const document = useApp((s) => s.document);
  const camera = useApp((s) => s.camera);
  const viewport = useApp((s) => s.viewport);
  const setViewport = useApp((s) => s.setViewport);
  const zoomBy = useApp((s) => s.zoomBy);
  const pan = useApp((s) => s.pan);
  const setRendering = useApp((s) => s.setRendering);
  const reportError = useApp((s) => s.reportError);

  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const overviewRef = useRef<HTMLCanvasElement | null>(null);
  const detailRef = useRef<Detail | null>(null);
  const requestIdRef = useRef(0);
  const dragRef = useRef<{ x: number; y: number } | null>(null);
  // Latest values for drawing from callbacks that outlive this render.
  const latestRef = useRef({ document, camera, viewport });
  latestRef.current = { document, camera, viewport };

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

  // New image: new overview, forget the old sharp render.
  useEffect(() => {
    overviewRef.current = document ? frameToCanvas(document.overview) : null;
    detailRef.current = null;
  }, [document]);

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
    const detailIsCurrent = detail && sameCamera(detail.camera, camera) &&
      detail.viewport.width === viewport.width && detail.viewport.height === viewport.height;

    if (!detailIsCurrent) {
      // The overview always covers the whole image, so nothing is left blank.
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
      context.imageSmoothingEnabled = !detailIsCurrent;
      context.drawImage(detail.canvas, place.x, place.y, place.width, place.height);
    }
  }

  // Redraw whenever the view changes; ask for a sharp render after a pause.
  useEffect(() => {
    const frameHandle = requestAnimationFrame(draw);
    if (!document || !camera || !viewport) return () => cancelAnimationFrame(frameHandle);

    const detail = detailRef.current;
    if (detail && sameCamera(detail.camera, camera) &&
      detail.viewport.width === viewport.width && detail.viewport.height === viewport.height) {
      return () => cancelAnimationFrame(frameHandle);
    }

    const timer = window.setTimeout(() => {
      const id = ++requestIdRef.current;
      const requestedCamera = camera;
      const requestedViewport = viewport;
      setRendering(true);
      api
        .renderView({ id, ...visibleRegion(requestedCamera, requestedViewport) })
        .then((buffer) => {
          if (id !== requestIdRef.current) return;
          detailRef.current = {
            canvas: frameToCanvas(parseFrame(buffer)),
            camera: requestedCamera,
            viewport: requestedViewport,
          };
          requestAnimationFrame(draw);
        })
        .catch((reason) => {
          const code = errorCode(reason);
          if (code !== "superseded" && code !== "no_document" && id === requestIdRef.current) reportError(code);
        })
        .finally(() => {
          if (id === requestIdRef.current) setRendering(false);
        });
    }, RENDER_DELAY_MS);

    return () => {
      cancelAnimationFrame(frameHandle);
      window.clearTimeout(timer);
    };
  }, [api, document, camera, viewport, setRendering, reportError]);

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
    </div>
  );
}
