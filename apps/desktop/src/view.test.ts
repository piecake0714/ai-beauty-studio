import { describe, expect, it } from "vitest";
import {
  clampCamera,
  fitCamera,
  MAX_ZOOM,
  MIN_ZOOM,
  panBy,
  placeFrame,
  stepZoom,
  toImage,
  visibleRegion,
  toViewport,
  zoomAt,
} from "./view";

const image = { width: 8000, height: 6000 };
const viewport = { width: 1600, height: 900 };

describe("fitCamera", () => {
  it("fits a large image inside the viewport and centres it", () => {
    const camera = fitCamera(image, viewport);
    expect(camera.zoom).toBeCloseTo(0.15);
    expect(camera).toMatchObject({ cx: 4000, cy: 3000 });
  });

  it("does not enlarge a small image past 100%", () => {
    expect(fitCamera({ width: 64, height: 48 }, viewport).zoom).toBe(1);
  });
});

describe("zoomAt", () => {
  it("keeps the image point under the cursor fixed", () => {
    const camera = { zoom: 0.5, cx: 4000, cy: 3000 };
    const cursor = { x: 1200, y: 300 };
    const before = toImage(camera, viewport, cursor);
    const zoomed = zoomAt(camera, 2, cursor, image, viewport);
    const after = toImage(zoomed, viewport, cursor);
    expect(zoomed.zoom).toBe(2);
    expect(after.x).toBeCloseTo(before.x);
    expect(after.y).toBeCloseTo(before.y);
  });

  it("limits zoom to 5%–800%", () => {
    const camera = fitCamera(image, viewport);
    expect(zoomAt(camera, 100, { x: 0, y: 0 }, image, viewport).zoom).toBe(MAX_ZOOM);
    expect(zoomAt(camera, 0.001, { x: 0, y: 0 }, image, viewport).zoom).toBe(MIN_ZOOM);
  });
});

describe("clampCamera and panBy", () => {
  it("never scrolls past the image edges", () => {
    const camera = { zoom: 1, cx: 4000, cy: 3000 };
    const farLeft = panBy(camera, 100000, 0, image, viewport);
    expect(farLeft.cx).toBe(800); // left edge at the viewport's left side
    const farDown = panBy(camera, 0, -100000, image, viewport);
    expect(farDown.cy).toBe(6000 - 450);
  });

  it("moves the image with the drag", () => {
    const moved = panBy({ zoom: 2, cx: 4000, cy: 3000 }, 100, -50, image, viewport);
    expect(moved.cx).toBe(3950);
    expect(moved.cy).toBe(3025);
  });

  it("centres an axis where the image is smaller than the viewport", () => {
    const camera = clampCamera({ zoom: 0.1, cx: 0, cy: 0 }, image, viewport);
    expect(camera).toMatchObject({ cx: 4000, cy: 3000 });
  });
});

describe("stepZoom", () => {
  it("moves through the preset levels", () => {
    expect(stepZoom(1, 1)).toBe(1.5);
    expect(stepZoom(1, -1)).toBe(0.75);
    expect(stepZoom(0.4, 1)).toBe(0.5);
    expect(stepZoom(0.4, -1)).toBeCloseTo(1 / 3);
    expect(stepZoom(8, 1)).toBe(8);
    expect(stepZoom(0.05, -1)).toBe(0.05);
  });
});

describe("visibleRegion", () => {
  it("covers exactly the viewport at the current zoom", () => {
    const region = visibleRegion({ zoom: 2, cx: 4000, cy: 3000 }, viewport);
    expect(region).toEqual({ x: 3600, y: 2775, width: 800, height: 450, outWidth: 1600, outHeight: 900 });
  });
});

describe("placeFrame", () => {
  it("draws a frame rendered for the current camera over the whole viewport", () => {
    const camera = { zoom: 1, cx: 4000, cy: 3000 };
    expect(placeFrame(camera, viewport, camera, viewport)).toEqual({ x: 0, y: 0, width: 1600, height: 900 });
  });

  it("moves and scales an older frame to match the new camera", () => {
    const rendered = { zoom: 1, cx: 4000, cy: 3000 };
    const panned = { zoom: 1, cx: 4100, cy: 3000 };
    expect(placeFrame(rendered, viewport, panned, viewport)).toEqual({ x: -100, y: 0, width: 1600, height: 900 });
    const zoomed = { zoom: 2, cx: 4000, cy: 3000 };
    expect(placeFrame(rendered, viewport, zoomed, viewport)).toEqual({ x: -800, y: -450, width: 3200, height: 1800 });
  });

  it("maps image points to the viewport and back", () => {
    const camera = { cx: 500, cy: 300, zoom: 0.5 };
    const viewport = { width: 800, height: 600 };
    const point = toViewport(camera, viewport, { x: 700, y: 100 });
    expect(point).toEqual({ x: 500, y: 200 });
    expect(toImage(camera, viewport, point)).toEqual({ x: 700, y: 100 });
  });
});
