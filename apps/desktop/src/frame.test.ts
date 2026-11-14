import { describe, expect, it } from "vitest";
import { FRAME_HEADER_LEN, parseFrame } from "./frame";

function makeFrame(width: number, height: number, sourceWidth: number, sourceHeight: number, pixelBytes: number) {
  const buffer = new ArrayBuffer(FRAME_HEADER_LEN + pixelBytes);
  const view = new DataView(buffer);
  view.setUint32(0, width, true);
  view.setUint32(4, height, true);
  view.setUint32(8, sourceWidth, true);
  view.setUint32(12, sourceHeight, true);
  new Uint8Array(buffer, FRAME_HEADER_LEN).forEach((_, i, bytes) => {
    bytes[i] = i % 256;
  });
  return buffer;
}

describe("parseFrame", () => {
  it("reads the header and exposes the pixels without copying", () => {
    const buffer = makeFrame(2, 3, 200, 300, 2 * 3 * 4);
    const frame = parseFrame(buffer);
    expect(frame.width).toBe(2);
    expect(frame.height).toBe(3);
    expect(frame.sourceWidth).toBe(200);
    expect(frame.sourceHeight).toBe(300);
    expect(frame.pixels.length).toBe(24);
    expect(Array.from(frame.pixels.slice(0, 4))).toEqual([0, 1, 2, 3]);
    expect(frame.pixels.buffer).toBe(buffer);
  });

  it("rejects a buffer shorter than the header", () => {
    expect(() => parseFrame(new ArrayBuffer(8))).toThrow(/header/);
  });

  it("rejects a pixel payload that does not match the header", () => {
    expect(() => parseFrame(makeFrame(2, 3, 2, 3, 20))).toThrow(/size/);
  });

  it("rejects an empty image", () => {
    expect(() => parseFrame(makeFrame(0, 0, 0, 0, 0))).toThrow(/empty/);
  });
});
