/**
 * Display frame sent by the Rust core (`application_core::encode_frame`):
 * four little-endian u32 (width, height, source width, source height)
 * followed by width * height * 4 bytes of RGBA8 pixels.
 */
export const FRAME_HEADER_LEN = 16;

export interface Frame {
  width: number;
  height: number;
  /** Size of the image in the file, before any downscale for display. */
  sourceWidth: number;
  sourceHeight: number;
  /** RGBA8 pixels, a view into the received buffer (no copy). */
  pixels: Uint8ClampedArray;
}

export function parseFrame(buffer: ArrayBuffer): Frame {
  if (buffer.byteLength < FRAME_HEADER_LEN) {
    throw new Error("frame is shorter than its header");
  }
  const header = new DataView(buffer, 0, FRAME_HEADER_LEN);
  const width = header.getUint32(0, true);
  const height = header.getUint32(4, true);
  const sourceWidth = header.getUint32(8, true);
  const sourceHeight = header.getUint32(12, true);

  const pixelBytes = width * height * 4;
  if (width === 0 || height === 0) {
    throw new Error("frame has an empty image");
  }
  if (buffer.byteLength !== FRAME_HEADER_LEN + pixelBytes) {
    throw new Error("frame size does not match its header");
  }
  return {
    width,
    height,
    sourceWidth,
    sourceHeight,
    pixels: new Uint8ClampedArray(buffer, FRAME_HEADER_LEN, pixelBytes),
  };
}
