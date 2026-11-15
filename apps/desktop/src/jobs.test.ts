import { describe, expect, it } from "vitest";
import { applyJobUpdate, type JobView } from "./jobs";

const running: JobView = { kind: "running", id: 7, fraction: 0, stage: "", cancelling: false };

describe("applyJobUpdate", () => {
  it("follows progress and ends with the result", () => {
    let view = applyJobUpdate(running, { state: "progress", id: 7, fraction: 0.4, stage: "load" });
    expect(view).toEqual({ ...running, fraction: 0.4, stage: "load" });
    view = applyJobUpdate(view, { state: "finished", id: 7, result: { passed: true } });
    expect(view).toEqual({ kind: "finished", result: { passed: true } });
  });

  it("never moves progress backwards", () => {
    const view = applyJobUpdate({ ...running, fraction: 0.6 }, { state: "progress", id: 7, fraction: 0.5, stage: "run" });
    expect(view.kind === "running" && view.fraction).toBe(0.6);
  });

  it("ignores updates for other jobs and after the job ended", () => {
    expect(applyJobUpdate(running, { state: "cancelled", id: 8 })).toBe(running);
    const done: JobView = { kind: "cancelled" };
    expect(applyJobUpdate(done, { state: "progress", id: 7, fraction: 1, stage: "x" })).toBe(done);
  });

  it("records failures and cancellation", () => {
    expect(applyJobUpdate(running, { state: "failed", id: 7, code: "model_damaged", detail: "" })).toEqual({
      kind: "failed",
      code: "model_damaged",
    });
    expect(applyJobUpdate(running, { state: "cancelled", id: 7 })).toEqual({ kind: "cancelled" });
  });
});
