import { useEffect, useRef } from "react";
import type { Frame } from "../frame";

/** Draws a frame received from the core. The UI only displays pixels. */
export function FrameCanvas({ frame, className, label }: { frame: Frame; className?: string; label?: string }) {
  const ref = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = ref.current;
    if (!canvas) return;
    canvas.width = frame.width;
    canvas.height = frame.height;
    canvas.getContext("2d")?.putImageData(new ImageData(frame.pixels, frame.width, frame.height), 0, 0);
  }, [frame]);

  return <canvas ref={ref} className={className} role="img" aria-label={label} />;
}
