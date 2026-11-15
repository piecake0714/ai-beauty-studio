import { describe, expect, it, vi } from "vitest";
import type { Api } from "./api";
import { FRAME_HEADER_LEN } from "./frame";
import { createAppStore, defaultSettings } from "./store";
import { NO_ADJUSTMENTS, type Adjustments, type EditState, type LanguagePreference, type Settings, type ThemePreference } from "./types";

function frameBuffer(width: number, height: number): ArrayBuffer {
  const buffer = new ArrayBuffer(FRAME_HEADER_LEN + width * height * 4);
  const view = new DataView(buffer);
  view.setUint32(0, width, true);
  view.setUint32(4, height, true);
  view.setUint32(8, width * 10, true);
  view.setUint32(12, height * 10, true);
  return buffer;
}

/** A fake core that keeps settings in memory like the real one. */
function fakeApi(overrides: Partial<Api> = {}): Api {
  let settings: Settings = { ...defaultSettings, recent: [] };
  // Edit history like the core's: snapshots and a position.
  let states: Adjustments[] = [NO_ADJUSTMENTS];
  let index = 0;
  const editState = (): EditState => ({
    adjustments: states[index],
    canUndo: index > 0,
    canRedo: index < states.length - 1,
    edited: JSON.stringify(states[index]) !== JSON.stringify(NO_ADJUSTMENTS),
  });
  const api: Api = {
    startup: vi.fn(async () => ({ settings, settingsRecovered: false, version: "0.1.0" })),
    getSettings: vi.fn(async () => settings),
    pickImage: vi.fn(async () => "C:\\照片\\a.jpg"),
    openImage: vi.fn(async (path: string) => {
      settings = { ...settings, recent: [{ path, openedAt: 1 }, ...settings.recent] };
      states = [NO_ADJUSTMENTS];
      index = 0;
      return frameBuffer(4, 3);
    }),
    renderView: vi.fn(async () => frameBuffer(4, 3)),
    renderOverview: vi.fn(async () => frameBuffer(4, 3)),
    editState: vi.fn(async () => editState()),
    commitAdjustments: vi.fn(async (adjustments: Adjustments) => {
      if (JSON.stringify(adjustments) !== JSON.stringify(states[index])) {
        states = [...states.slice(0, index + 1), adjustments];
        index = states.length - 1;
      }
      return editState();
    }),
    undo: vi.fn(async () => {
      index = Math.max(0, index - 1);
      return editState();
    }),
    redo: vi.fn(async () => {
      index = Math.min(states.length - 1, index + 1);
      return editState();
    }),
    confirm: vi.fn(async () => true),
    onCloseRequested: vi.fn(async () => () => {}),
    thumbnail: vi.fn(async () => frameBuffer(2, 2)),
    fileInfo: vi.fn(async () => ({ name: "a.jpg", sizeBytes: 1234, format: "JPEG" as const })),
    setTheme: vi.fn(async (theme: ThemePreference) => (settings = { ...settings, theme })),
    setLanguage: vi.fn(async (language: LanguagePreference) => (settings = { ...settings, language })),
    removeRecent: vi.fn(async (path: string) => (settings = { ...settings, recent: settings.recent.filter((r) => r.path !== path) })),
    clearRecent: vi.fn(async () => (settings = { ...settings, recent: [] })),
    thirdPartyNotices: vi.fn(async () => "notices"),
  };
  return { ...api, ...overrides };
}

describe("app store", () => {
  it("starts on Home with the core's settings and version", async () => {
    const store = createAppStore(fakeApi());
    await store.getState().init();
    const state = store.getState();
    expect(state.ready).toBe(true);
    expect(state.version).toBe("0.1.0");
    expect(state.view).toBe("home");
    expect(state.document).toBeNull();
  });

  it("opens a picked image, switches to the editor and refreshes the recent list", async () => {
    const api = fakeApi();
    const store = createAppStore(api);
    await store.getState().init();
    await store.getState().pickAndOpen("Images");
    const state = store.getState();
    expect(api.openImage).toHaveBeenCalledWith("C:\\照片\\a.jpg");
    expect(state.view).toBe("editor");
    expect(state.document?.overview.width).toBe(4);
    expect(state.document?.size).toEqual({ width: 40, height: 30 });
    expect(state.document?.info?.format).toBe("JPEG");
    expect(state.settings.recent.map((r) => r.path)).toEqual(["C:\\照片\\a.jpg"]);
    expect(state.opening).toBe(false);
    expect(state.error).toBeNull();
  });

  it("does nothing when the picker is cancelled", async () => {
    const api = fakeApi({ pickImage: vi.fn(async () => null) });
    const store = createAppStore(api);
    await store.getState().pickAndOpen("Images");
    expect(api.openImage).not.toHaveBeenCalled();
    expect(store.getState().view).toBe("home");
  });

  it("keeps the current view and shows the error code when opening fails", async () => {
    const store = createAppStore(fakeApi({ openImage: vi.fn(async () => Promise.reject("not_an_image")) }));
    await store.getState().openPath("C:\\broken.jpg");
    const state = store.getState();
    expect(state.error).toBe("not_an_image");
    expect(state.view).toBe("home");
    expect(state.document).toBeNull();
    expect(state.opening).toBe(false);
  });

  it("opens the image even when its file details cannot be read", async () => {
    const store = createAppStore(fakeApi({ fileInfo: vi.fn(async () => Promise.reject("read_failed")) }));
    await store.getState().openPath("C:\\a.jpg");
    expect(store.getState().document?.info).toBeNull();
    expect(store.getState().error).toBeNull();
  });

  it("treats unexpected failures as internal errors", async () => {
    const store = createAppStore(fakeApi({ openImage: vi.fn(async () => Promise.reject(new Error("boom"))) }));
    await store.getState().openPath("C:\\a.jpg");
    expect(store.getState().error).toBe("internal");
  });

  it("does not switch to the editor without an open image", () => {
    const store = createAppStore(fakeApi());
    store.getState().showView("editor");
    expect(store.getState().view).toBe("home");
  });

  it("saves theme and language through the core", async () => {
    const api = fakeApi();
    const store = createAppStore(api);
    await store.getState().setTheme("dark");
    await store.getState().setLanguage("en");
    expect(api.setTheme).toHaveBeenCalledWith("dark");
    expect(store.getState().settings.theme).toBe("dark");
    expect(store.getState().settings.language).toBe("en");
  });

  it("reports a failed settings save and shows the core's current settings", async () => {
    const api = fakeApi({ setTheme: vi.fn(async () => Promise.reject("settings_save_failed")) });
    const store = createAppStore(api);
    await store.getState().setTheme("light");
    expect(store.getState().error).toBe("settings_save_failed");
    expect(api.getSettings).toHaveBeenCalled();
  });

  it("removes and clears recent files", async () => {
    const store = createAppStore(fakeApi());
    await store.getState().openPath("C:\\a.jpg");
    await store.getState().openPath("C:\\b.jpg");
    await store.getState().removeRecent("C:\\a.jpg");
    expect(store.getState().settings.recent.map((r) => r.path)).toEqual(["C:\\b.jpg"]);
    await store.getState().clearRecent();
    expect(store.getState().settings.recent).toEqual([]);
  });

  it("fits a newly opened image once the canvas size is known", async () => {
    const store = createAppStore(fakeApi());
    await store.getState().openPath("C:\\a.jpg");
    expect(store.getState().camera).toBeNull();
    store.getState().setViewport({ width: 20, height: 30 });
    // Image is 40 x 30: fit is limited by the width.
    expect(store.getState().camera).toEqual({ zoom: 0.5, cx: 20, cy: 15 });
  });

  it("fits immediately when the canvas already has a size", async () => {
    const store = createAppStore(fakeApi());
    store.getState().setViewport({ width: 400, height: 300 });
    await store.getState().openPath("C:\\a.jpg");
    // Small images are not enlarged past 100%.
    expect(store.getState().camera?.zoom).toBe(1);
  });

  it("zooms by steps, to 100% and back to fit", async () => {
    const store = createAppStore(fakeApi());
    store.getState().setViewport({ width: 20, height: 30 });
    await store.getState().openPath("C:\\a.jpg");
    store.getState().zoomStep(1);
    expect(store.getState().camera?.zoom).toBeCloseTo(2 / 3);
    store.getState().zoomTo(1);
    expect(store.getState().camera?.zoom).toBe(1);
    store.getState().fitToView();
    expect(store.getState().camera?.zoom).toBe(0.5);
  });

  it("pans within the image and ignores zoom without an image", async () => {
    const store = createAppStore(fakeApi());
    store.getState().zoomBy(2);
    expect(store.getState().camera).toBeNull();
    store.getState().setViewport({ width: 20, height: 30 });
    await store.getState().openPath("C:\\a.jpg");
    store.getState().zoomTo(1);
    store.getState().pan(-1000, 0);
    // At 100% a 20-pixel-wide view of a 40-pixel image stops at the right edge.
    expect(store.getState().camera?.cx).toBe(30);
  });

  it("loads the edit state when an image opens", async () => {
    const store = createAppStore(fakeApi());
    await store.getState().openPath("C:\\a.jpg");
    expect(store.getState().edit).toEqual({ adjustments: NO_ADJUSTMENTS, canUndo: false, canRedo: false, edited: false });
    expect(store.getState().live).toEqual(NO_ADJUSTMENTS);
  });

  it("shows slider changes live and records them as one step on commit", async () => {
    const api = fakeApi();
    const store = createAppStore(api);
    await store.getState().openPath("C:\\a.jpg");
    const firstOverview = store.getState().document?.overview;
    for (const exposure of [0.1, 0.2, 0.3, 0.5]) store.getState().setLive({ ...NO_ADJUSTMENTS, exposure });
    expect(api.commitAdjustments).not.toHaveBeenCalled();
    await store.getState().commitLive();
    expect(api.commitAdjustments).toHaveBeenCalledTimes(1);
    expect(store.getState().edit?.adjustments.exposure).toBe(0.5);
    expect(store.getState().edit?.canUndo).toBe(true);
    expect(api.renderOverview).toHaveBeenCalledWith({ ...NO_ADJUSTMENTS, exposure: 0.5 });
    expect(store.getState().document?.overview).not.toBe(firstOverview);
  });

  it("undoes and redoes, showing the restored values", async () => {
    const store = createAppStore(fakeApi());
    await store.getState().openPath("C:\\a.jpg");
    store.getState().setLive({ ...NO_ADJUSTMENTS, contrast: 30 });
    await store.getState().commitLive();
    store.getState().setLive({ ...NO_ADJUSTMENTS, contrast: 30, brightness: -10 });
    await store.getState().commitLive();
    await store.getState().undo();
    expect(store.getState().live).toEqual({ ...NO_ADJUSTMENTS, contrast: 30 });
    await store.getState().undo();
    expect(store.getState().live).toEqual(NO_ADJUSTMENTS);
    expect(store.getState().edit?.canUndo).toBe(false);
    await store.getState().redo();
    await store.getState().redo();
    expect(store.getState().live.brightness).toBe(-10);
  });

  it("resets all adjustments as one undoable step", async () => {
    const store = createAppStore(fakeApi());
    await store.getState().openPath("C:\\a.jpg");
    store.getState().setLive({ exposure: 1, brightness: 5, contrast: 5 });
    await store.getState().commitLive();
    await store.getState().resetAdjustments();
    expect(store.getState().edit?.edited).toBe(false);
    await store.getState().undo();
    expect(store.getState().live.exposure).toBe(1);
  });

  it("asks before opening another image over unsaved adjustments", async () => {
    const prompt = { title: "t", message: "m", yes: "y", no: "n" };
    const api = fakeApi({ confirm: vi.fn(async () => false) });
    const store = createAppStore(api);
    await store.getState().openPath("C:\\a.jpg", prompt);
    expect(api.confirm).not.toHaveBeenCalled(); // nothing to lose yet
    store.getState().setLive({ ...NO_ADJUSTMENTS, exposure: 1 });
    await store.getState().commitLive();
    await store.getState().openPath("C:\\b.jpg", prompt);
    expect(api.confirm).toHaveBeenCalledWith("t", "m", "y", "n");
    expect(store.getState().document?.path).toBe("C:\\a.jpg"); // declined: kept
  });

  it("opens over unsaved adjustments when the user agrees, starting fresh", async () => {
    const prompt = { title: "t", message: "m", yes: "y", no: "n" };
    const store = createAppStore(fakeApi());
    await store.getState().openPath("C:\\a.jpg", prompt);
    store.getState().setLive({ ...NO_ADJUSTMENTS, exposure: 1 });
    await store.getState().commitLive();
    await store.getState().openPath("C:\\b.jpg", prompt);
    expect(store.getState().document?.path).toBe("C:\\b.jpg");
    expect(store.getState().live).toEqual(NO_ADJUSTMENTS);
  });

  it("works out which columns show the original", () => {
    const store = createAppStore(fakeApi());
    expect(store.getState().originalColumns(1000)).toBe(0);
    store.getState().setSplitView(true);
    store.getState().setSplit(0.25);
    expect(store.getState().originalColumns(1000)).toBe(250);
    store.getState().setHoldOriginal(true);
    expect(store.getState().originalColumns(1000)).toBe(1000);
    store.getState().setSplit(5);
    expect(store.getState().split).toBe(1);
  });
});
