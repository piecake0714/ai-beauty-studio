/** Background job updates from the core ("job-update" events). */
export type JobUpdate =
  | { state: "progress"; id: number; fraction: number; stage: string }
  | { state: "finished"; id: number; result: unknown }
  | { state: "failed"; id: number; code: string; detail: string }
  | { state: "cancelled"; id: number };

/** What the UI shows about one job it started. */
export type JobView =
  | { kind: "idle" }
  | { kind: "running"; id: number; fraction: number; stage: string; cancelling: boolean }
  | { kind: "finished"; result: unknown }
  | { kind: "failed"; code: string }
  | { kind: "cancelled" };

/** Apply an update to the view of the job with the same id; others are ignored. */
export function applyJobUpdate(view: JobView, update: JobUpdate): JobView {
  if (view.kind !== "running" || update.id !== view.id) return view;
  switch (update.state) {
    case "progress":
      return { ...view, fraction: Math.max(view.fraction, update.fraction), stage: update.stage };
    case "finished":
      return { kind: "finished", result: update.result };
    case "failed":
      return { kind: "failed", code: update.code };
    case "cancelled":
      return { kind: "cancelled" };
  }
}

export interface SelfTestReport {
  provider: "directml" | "cpu";
  fallbackReason: string | null;
  model: string;
  width: number;
  height: number;
  firstRunMs: number;
  runMs: number;
  maxError: number;
  passed: boolean;
}

export interface Gpu {
  name: string;
  vendor: string;
  isSoftware: boolean;
  dedicatedMemoryMb: number;
}

export interface Hardware {
  gpus: Gpu[];
  runtimeVersion: string;
}
