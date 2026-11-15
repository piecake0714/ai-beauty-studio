/**
 * Canvas camera math. All sizes are in device pixels (screen pixels, after
 * Windows display scaling), so 100% zoom shows one image pixel per screen
 * pixel.
 */

export interface Size {
  width: number;
  height: number;
}

export interface Point {
  x: number;
  y: number;
}

export interface Camera {
  /** Screen pixels per image pixel. 1 = 100%. */
  zoom: number;
  /** Image coordinates shown at the centre of the viewport. */
  cx: number;
  cy: number;
}

export const MIN_ZOOM = 0.05;
export const MAX_ZOOM = 8;

/** Preset levels used by zoom in / zoom out. */
export const ZOOM_STEPS = [0.05, 0.1, 0.15, 0.2, 0.25, 1 / 3, 0.5, 2 / 3, 0.75, 1, 1.5, 2, 3, 4, 6, 8];

export function clampZoom(zoom: number): number {
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom));
}

/** Keep the image on screen: centred on an axis where it is smaller than the
 * viewport, otherwise never scrolled past its edges. */
export function clampCamera(camera: Camera, image: Size, viewport: Size): Camera {
  const axis = (centre: number, imageSide: number, viewportSide: number) => {
    const half = viewportSide / (2 * camera.zoom);
    if (imageSide <= 2 * half) return imageSide / 2;
    return Math.min(imageSide - half, Math.max(half, centre));
  };
  return {
    zoom: camera.zoom,
    cx: axis(camera.cx, image.width, viewport.width),
    cy: axis(camera.cy, image.height, viewport.height),
  };
}

/** Whole image visible and centred; never enlarged past 100%. */
export function fitCamera(image: Size, viewport: Size): Camera {
  const zoom = clampZoom(Math.min(1, viewport.width / image.width, viewport.height / image.height));
  return { zoom, cx: image.width / 2, cy: image.height / 2 };
}

/** Image point under a viewport point. */
export function toImage(camera: Camera, viewport: Size, point: Point): Point {
  return {
    x: camera.cx + (point.x - viewport.width / 2) / camera.zoom,
    y: camera.cy + (point.y - viewport.height / 2) / camera.zoom,
  };
}

/** Viewport point showing an image point (the inverse of toImage). */
export function toViewport(camera: Camera, viewport: Size, point: Point): Point {
  return {
    x: (point.x - camera.cx) * camera.zoom + viewport.width / 2,
    y: (point.y - camera.cy) * camera.zoom + viewport.height / 2,
  };
}

/** Change zoom keeping the image point under `anchor` (viewport pixels) fixed. */
export function zoomAt(camera: Camera, zoom: number, anchor: Point, image: Size, viewport: Size): Camera {
  const next = clampZoom(zoom);
  const target = toImage(camera, viewport, anchor);
  return clampCamera(
    {
      zoom: next,
      cx: target.x - (anchor.x - viewport.width / 2) / next,
      cy: target.y - (anchor.y - viewport.height / 2) / next,
    },
    image,
    viewport,
  );
}

/** Move the image by a drag of (dx, dy) viewport pixels. */
export function panBy(camera: Camera, dx: number, dy: number, image: Size, viewport: Size): Camera {
  return clampCamera(
    { zoom: camera.zoom, cx: camera.cx - dx / camera.zoom, cy: camera.cy - dy / camera.zoom },
    image,
    viewport,
  );
}

/** Next preset zoom level above (direction 1) or below (direction -1). */
export function stepZoom(zoom: number, direction: 1 | -1): number {
  const epsilon = 1e-6;
  if (direction > 0) return ZOOM_STEPS.find((step) => step > zoom + epsilon) ?? MAX_ZOOM;
  return [...ZOOM_STEPS].reverse().find((step) => step < zoom - epsilon) ?? MIN_ZOOM;
}

/** Image region covered by the viewport, as a render request. */
export function visibleRegion(camera: Camera, viewport: Size) {
  const width = viewport.width / camera.zoom;
  const height = viewport.height / camera.zoom;
  return {
    x: camera.cx - width / 2,
    y: camera.cy - height / 2,
    width,
    height,
    outWidth: Math.round(viewport.width),
    outHeight: Math.round(viewport.height),
  };
}

/**
 * Where to draw, in the current view, a frame that was rendered for an
 * earlier camera and viewport. Used to show the last sharp render, moved and
 * scaled, while a new one is on its way.
 */
export function placeFrame(rendered: Camera, renderedViewport: Size, camera: Camera, viewport: Size) {
  const topLeft = toImage(rendered, renderedViewport, { x: 0, y: 0 });
  const scale = camera.zoom / rendered.zoom;
  return {
    x: (topLeft.x - camera.cx) * camera.zoom + viewport.width / 2,
    y: (topLeft.y - camera.cy) * camera.zoom + viewport.height / 2,
    width: renderedViewport.width * scale,
    height: renderedViewport.height * scale,
  };
}

export function sameCamera(a: Camera, b: Camera): boolean {
  return a.zoom === b.zoom && a.cx === b.cx && a.cy === b.cy;
}
