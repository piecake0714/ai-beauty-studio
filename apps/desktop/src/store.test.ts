import { describe, expect, it, vi } from "vitest";
import type { Api } from "./api";
import { FRAME_HEADER_LEN } from "./frame";
import { createAppStore, defaultSettings, documentKey, fileStem, type FileLabels } from "./store";
import { NO_ADJUSTMENTS, type Adjustments, type EditorMode, type EditState, type LanguagePreference, type Settings, type ThemePreference } from "./types";

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
  let serial = 0;
  let openedPath = "";
  let projectPath: string | null = null;
  const openProjectLike = (path: string, original?: string | null) => {
    if (path.includes("missing") && !original) return Promise.reject("original_missing");
    openedPath = original ?? "C:\\照片\\a.jpg";
    projectPath = path;
    states = [NO_ADJUSTMENTS, { ...NO_ADJUSTMENTS, exposure: 1 }];
    index = 1;
    serial += 1;
    return Promise.resolve(frameBuffer(4, 3));
  };
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
      serial += 1;
      openedPath = path;
      projectPath = null;
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
    pickToOpen: vi.fn(async () => "C:\\照片\\a.jpg"),
    pickSavePath: vi.fn(async (_name: string, extension: string) => `C:\\out\\saved.${extension}`),
    openProject: vi.fn(openProjectLike),
    inspectProject: vi.fn(async () => ({ originalPath: "D:\\old\\a.jpg", savedAt: 1 })),
    documentStatus: vi.fn(async () => ({ serial, originalPath: openedPath, projectPath, missingBackgrounds: 0, outdated: [] as string[] })),
    modelOverview: vi.fn(async () => []),
    verifyModels: vi.fn(async () => 11),
    unloadModels: vi.fn(async () => 2),
    backgroundStatus: vi.fn(async () => ({ serial, ready: false, shapeReady: false })),
    saveProject: vi.fn(async (path: string | null) => {
      if (!path && !projectPath) return Promise.reject("needs_path");
      projectPath = path ?? projectPath;
      return projectPath as string;
    }),
    autosave: vi.fn(async () => undefined),
    setTitle: vi.fn(async () => undefined),
    recoveryStatus: vi.fn(async () => null),
    restoreRecovery: vi.fn(async (original?: string | null) => {
      await openProjectLike("C:\\recovery.aibs", original ?? "C:\\照片\\a.jpg");
      projectPath = null;
      return frameBuffer(4, 3);
    }),
    discardRecovery: vi.fn(async () => undefined),
    exportImage: vi.fn(async () => 9),
    startBackgroundSeparation: vi.fn(async () => 7),
    startShapeEstimation: vi.fn(async () => 8),
    addBackgroundImage: vi.fn(async () => 1),
    facesStatus: vi.fn(async () => ({ serial, status: { state: "detecting" as const } })),
    onFaces: vi.fn(async () => () => {}),
    autoEnhance: vi.fn(async () => {
      states = [...states.slice(0, index + 1), { ...states[index], exposure: 0.5, vibrance: 10 }];
      index = states.length - 1;
      return editState();
    }),
    hardwareInfo: vi.fn(async () => ({ gpus: [], runtimeVersion: "1.24.4" })),
    startAiSelfTest: vi.fn(async () => 1),
    cancelJob: vi.fn(async () => true),
    onJobUpdate: vi.fn(async () => () => {}),
    confirm: vi.fn(async () => true),
    onCloseRequested: vi.fn(async () => () => {}),
    thumbnail: vi.fn(async () => frameBuffer(2, 2)),
    fileInfo: vi.fn(async () => ({ name: "a.jpg", sizeBytes: 1234, format: "JPEG" as const })),
    setTheme: vi.fn(async (theme: ThemePreference) => (settings = { ...settings, theme })),
    setLanguage: vi.fn(async (language: LanguagePreference) => (settings = { ...settings, language })),
    setEditorMode: vi.fn(async (editorMode: EditorMode) => (settings = { ...settings, editorMode })),
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
    store.getState().setLive({ ...NO_ADJUSTMENTS, exposure: 1, brightness: 5, contrast: 5 });
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

  it("switches between beginner and advanced mode without touching adjustments", async () => {
    const api = fakeApi();
    const store = createAppStore(api);
    await store.getState().init();
    expect(store.getState().settings.editorMode).toBe("beginner");
    await store.getState().openPath("C:\\a.jpg");
    store.getState().setLive({ ...NO_ADJUSTMENTS, vibrance: 40 });
    await store.getState().commitLive();
    await store.getState().setEditorMode("advanced");
    expect(api.setEditorMode).toHaveBeenCalledWith("advanced");
    expect(store.getState().settings.editorMode).toBe("advanced");
    expect(store.getState().live.vibrance).toBe(40);
  });

  it("shows face detection for the newest image only", async () => {
    const store = createAppStore(fakeApi());
    expect(store.getState().faces).toBeNull();
    await store.getState().openPath("C:\\a.jpg");
    expect(store.getState().faces).toEqual({ state: "detecting" });
    store.getState().receiveFaces({ serial: 1, status: { state: "ready", count: 2 } });
    expect(store.getState().faces).toEqual({ state: "ready", count: 2 });
    // A late "detecting" reply for the same image does not undo the result.
    store.getState().receiveFaces({ serial: 1, status: { state: "detecting" } });
    expect(store.getState().faces).toEqual({ state: "ready", count: 2 });
    await store.getState().openPath("C:\\b.jpg");
    expect(store.getState().faces).toEqual({ state: "detecting" });
    // The result for the first image arriving late is ignored.
    store.getState().receiveFaces({ serial: 1, status: { state: "ready", count: 5 } });
    expect(store.getState().faces).toEqual({ state: "detecting" });
    store.getState().receiveFaces({ serial: 2, status: { state: "ready", count: 0 } });
    expect(store.getState().faces).toEqual({ state: "ready", count: 0 });
  });

  it("applies one-click enhancement as one undoable step", async () => {
    const api = fakeApi();
    const store = createAppStore(api);
    await store.getState().openPath("C:\\a.jpg");
    await store.getState().autoEnhance();
    expect(api.autoEnhance).toHaveBeenCalledTimes(1);
    expect(store.getState().live.exposure).toBe(0.5);
    expect(store.getState().edit?.canUndo).toBe(true);
    expect(store.getState().enhancing).toBe(false);
    await store.getState().undo();
    expect(store.getState().live).toEqual(NO_ADJUSTMENTS);
  });

  it("does not enhance without an open image", async () => {
    const api = fakeApi();
    const store = createAppStore(api);
    await store.getState().autoEnhance();
    expect(api.autoEnhance).not.toHaveBeenCalled();
  });

  it("tracks background separation through job updates of its own job only", async () => {
    const api = fakeApi();
    const store = createAppStore(api);
    expect(store.getState().background).toBeNull();
    await store.getState().openPath("C:\\a.jpg");
    expect(store.getState().background).toEqual({ state: "none" });
    await store.getState().startSeparation();
    expect(store.getState().background).toMatchObject({ state: "running", jobId: 7 });
    store.getState().receiveJobUpdate({ state: "progress", id: 99, fraction: 0.9, stage: "run" });
    expect(store.getState().background).toMatchObject({ fraction: 0 });
    store.getState().receiveJobUpdate({ state: "progress", id: 7, fraction: 0.4, stage: "run" });
    expect(store.getState().background).toMatchObject({ fraction: 0.4 });
    store.getState().receiveJobUpdate({ state: "finished", id: 7, result: { provider: "directml" } });
    expect(store.getState().background).toEqual({ state: "ready", provider: "directml" });
    // Opening another image starts over.
    await store.getState().openPath("C:\\b.jpg");
    expect(store.getState().background).toEqual({ state: "none" });
  });

  it("reports a failed or cancelled separation", async () => {
    const api = fakeApi();
    const store = createAppStore(api);
    await store.getState().openPath("C:\\a.jpg");
    await store.getState().startSeparation();
    await store.getState().cancelSeparation();
    expect(api.cancelJob).toHaveBeenCalledWith(7);
    expect(store.getState().background).toMatchObject({ cancelling: true });
    store.getState().receiveJobUpdate({ state: "cancelled", id: 7 });
    expect(store.getState().background).toEqual({ state: "none" });
    await store.getState().startSeparation();
    store.getState().receiveJobUpdate({ state: "failed", id: 7, code: "model_missing", detail: "" });
    expect(store.getState().background).toEqual({ state: "failed", code: "model_missing" });
  });

  it("changes the background as one undoable step and adds background images", async () => {
    const api = fakeApi();
    const store = createAppStore(api);
    await store.getState().openPath("C:\\a.jpg");
    await store.getState().chooseBackground({ backgroundMode: "color", backgroundColor: [255, 255, 255] });
    expect(store.getState().live.backgroundMode).toBe("color");
    expect(store.getState().edit?.canUndo).toBe(true);
    await store.getState().pickBackgroundImage("Images");
    expect(api.addBackgroundImage).toHaveBeenCalledWith("C:\\照片\\a.jpg");
    expect(store.getState().live).toMatchObject({ backgroundMode: "image", backgroundImage: 1 });
    await store.getState().undo();
    expect(store.getState().live.backgroundMode).toBe("color");
  });

  it("turns a light on with defaults and starts the 3D shape analysis once", async () => {
    const api = fakeApi();
    const store = createAppStore(api);
    await store.getState().openPath("C:\\a.jpg");
    expect(store.getState().shape).toEqual({ state: "none" });
    await store.getState().chooseLight("softbox");
    expect(store.getState().live).toMatchObject({ lightKind: "softbox", lightIntensity: 50, lightX: 0.3 });
    expect(api.startShapeEstimation).toHaveBeenCalledTimes(1);
    expect(store.getState().shape).toMatchObject({ state: "running", jobId: 8 });
    // Shape job updates do not touch the separation state.
    store.getState().receiveJobUpdate({ state: "finished", id: 8, result: { provider: "cpu" } });
    expect(store.getState().shape).toEqual({ state: "ready", provider: "cpu" });
    expect(store.getState().background).toEqual({ state: "none" });
    // Switching kind keeps the light's position and does not analyse again.
    store.getState().setLive({ ...store.getState().live, lightX: 0.8 });
    await store.getState().commitLive();
    await store.getState().chooseLight("reflector");
    expect(store.getState().live).toMatchObject({ lightKind: "reflector", lightX: 0.8 });
    expect(api.startShapeEstimation).toHaveBeenCalledTimes(1);
    await store.getState().setLightSubjectOnly(true);
    expect(store.getState().live.lightSubjectOnly).toBe(true);
    await store.getState().undo();
    expect(store.getState().live.lightSubjectOnly).toBe(false);
  });

  const labels: FileLabels = { supported: "S", images: "I", projects: "P", jpeg: "J", png: "N" };

  it("tracks unsaved changes and saves a project, asking where only the first time", async () => {
    const api = fakeApi();
    const store = createAppStore(api);
    await store.getState().openPath("C:\\照片\\a.jpg");
    expect(store.getState().isDirty()).toBe(false);
    store.getState().setLive({ ...store.getState().live, contrast: 20 });
    await store.getState().commitLive();
    expect(store.getState().isDirty()).toBe(true);

    expect(await store.getState().saveProject(labels)).toBe(true);
    expect(api.pickSavePath).toHaveBeenCalledWith("P", "aibs", "C:\\照片\\a.aibs");
    expect(api.saveProject).toHaveBeenCalledWith("C:\\out\\saved.aibs");
    expect(store.getState().projectPath).toBe("C:\\out\\saved.aibs");
    expect(store.getState().isDirty()).toBe(false);

    await store.getState().undo();
    expect(store.getState().isDirty()).toBe(true);
    await store.getState().redo();
    expect(store.getState().isDirty()).toBe(false); // back to what was saved

    store.getState().setLive({ ...store.getState().live, contrast: 40 });
    await store.getState().commitLive();
    await store.getState().saveProject(labels);
    expect(api.pickSavePath).toHaveBeenCalledTimes(1);
    expect(api.saveProject).toHaveBeenLastCalledWith(null);
    // "Save as" asks again.
    await store.getState().saveProject(labels, true);
    expect(api.pickSavePath).toHaveBeenCalledTimes(2);
  });

  it("asks before discarding only when there are unsaved changes", async () => {
    const api = fakeApi();
    const store = createAppStore(api);
    const prompt = { title: "t", message: "m", yes: "y", no: "n" };
    await store.getState().openPath("C:\\照片\\a.jpg");
    store.getState().setLive({ ...store.getState().live, contrast: 20 });
    await store.getState().commitLive();
    await store.getState().saveProject(labels);
    expect(await store.getState().confirmDiscard(prompt)).toBe(true);
    expect(api.confirm).not.toHaveBeenCalled(); // saved: nothing to lose
    store.getState().setLive({ ...store.getState().live, contrast: 30 });
    await store.getState().commitLive();
    await store.getState().confirmDiscard(prompt);
    expect(api.confirm).toHaveBeenCalledTimes(1);
  });

  it("opens a project with its photo, edits and no unsaved changes", async () => {
    const api = fakeApi();
    const store = createAppStore(api);
    await store.getState().openPath("C:\\work\\portrait.aibs");
    const state = store.getState();
    expect(api.openProject).toHaveBeenCalledWith("C:\\work\\portrait.aibs", null);
    expect(api.openImage).not.toHaveBeenCalled();
    expect(state.document?.path).toBe("C:\\照片\\a.jpg");
    expect(state.projectPath).toBe("C:\\work\\portrait.aibs");
    expect(state.live.exposure).toBe(1);
    expect(state.isDirty()).toBe(false);
    expect(api.renderOverview).toHaveBeenCalled(); // the overview shows the edits
  });

  it("asks for a project's missing photo and opens it once pointed to", async () => {
    const api = fakeApi({ pickImage: vi.fn(async () => "E:\\moved\\a.jpg") });
    const store = createAppStore(api);
    await store.getState().openPath("C:\\work\\missing.aibs");
    expect(store.getState().document).toBeNull();
    expect(store.getState().error).toBeNull();
    expect(store.getState().pendingLocate).toEqual({
      projectPath: "C:\\work\\missing.aibs",
      expected: "D:\\old\\a.jpg",
      reason: "original_missing",
    });
    await store.getState().locateOriginal("Images");
    expect(api.openProject).toHaveBeenLastCalledWith("C:\\work\\missing.aibs", "E:\\moved\\a.jpg");
    expect(store.getState().pendingLocate).toBeNull();
    expect(store.getState().document?.path).toBe("E:\\moved\\a.jpg");

    await store.getState().openPath("C:\\work\\missing.aibs");
    store.getState().cancelLocate();
    expect(store.getState().pendingLocate).toBeNull();
  });

  it("offers recovered work at startup and restores it as unsaved", async () => {
    const info = { savedAt: 100, projectPath: null, originalPath: "C:\\照片\\a.jpg" };
    const api = fakeApi({ recoveryStatus: vi.fn(async () => info) });
    const store = createAppStore(api);
    await store.getState().init();
    expect(store.getState().recovery).toEqual(info);
    await store.getState().restoreRecovery();
    expect(store.getState().document?.path).toBe("C:\\照片\\a.jpg");
    expect(store.getState().recovery).toBeNull();
    expect(store.getState().isDirty()).toBe(true);

    const store2 = createAppStore(fakeApi({ recoveryStatus: vi.fn(async () => info) }));
    await store2.getState().init();
    await store2.getState().discardRecovery();
    expect(store2.getState().recovery).toBeNull();
  });

  it("saves for recovery only when there are unsaved changes", async () => {
    const api = fakeApi();
    const store = createAppStore(api);
    await store.getState().openPath("C:\\照片\\a.jpg");
    await store.getState().autosave();
    expect(api.autosave).not.toHaveBeenCalled();
    store.getState().setLive({ ...store.getState().live, contrast: 20 });
    await store.getState().commitLive();
    await store.getState().autosave();
    expect(api.autosave).toHaveBeenCalledTimes(1);
  });

  it("exports through a job and reports the result", async () => {
    const api = fakeApi();
    const store = createAppStore(api);
    await store.getState().openPath("C:\\照片\\a.jpg");
    const options = { format: "jpeg" as const, quality: 92, longSide: null };
    await store.getState().exportImage(options, labels);
    expect(api.pickSavePath).toHaveBeenCalledWith("J", "jpg", "C:\\照片\\a-edited.jpg");
    expect(api.exportImage).toHaveBeenCalledWith("C:\\out\\saved.jpg", options);
    expect(store.getState().exporting).toMatchObject({ state: "running", jobId: 9 });
    store.getState().receiveJobUpdate({ state: "progress", id: 9, fraction: 0.6, stage: "encode" });
    expect(store.getState().exporting).toMatchObject({ fraction: 0.6 });
    store.getState().receiveJobUpdate({
      state: "finished",
      id: 9,
      result: { path: "C:\\out\\saved.jpg", width: 40, height: 30 },
    });
    expect(store.getState().exporting).toEqual({ state: "done", path: "C:\\out\\saved.jpg", width: 40, height: 30 });
    store.getState().dismissExport();
    expect(store.getState().exporting).toBeNull();

    const refusing = createAppStore(fakeApi({ exportImage: vi.fn(async () => Promise.reject("export_would_overwrite_original")) }));
    await refusing.getState().openPath("C:\\照片\\a.jpg");
    await refusing.getState().exportImage(options, labels);
    expect(refusing.getState().exporting).toEqual({ state: "failed", code: "export_would_overwrite_original" });
  });

  it("names files from paths", () => {
    expect(fileStem("C:\\照片\\my.photo.jpg")).toBe("my.photo");
    expect(fileStem("/home/a/b")).toBe("b");
    expect(documentKey({ edit: null, background: null, shape: null })).toContain("false");
  });

  it("opens the file the app was started with", async () => {
    const api = fakeApi({
      startup: vi.fn(async () => ({
        settings: defaultSettings,
        settingsRecovered: false,
        version: "0.9.0",
        launchPath: "C:\\work\\portrait.aibs",
      })),
    });
    const store = createAppStore(api);
    await store.getState().init();
    expect(api.openProject).toHaveBeenCalledWith("C:\\work\\portrait.aibs", null);
    expect(store.getState().view).toBe("editor");
  });

  it("marks AI results made by another model version until they are redone", async () => {
    const api = fakeApi();
    api.documentStatus = vi.fn(async () => ({
      serial: 1,
      originalPath: "C:\\照片\\a.jpg",
      projectPath: "C:\\work\\portrait.aibs",
      missingBackgrounds: 0,
      outdated: ["matte", "shape"],
    }));
    const store = createAppStore(api);
    await store.getState().openPath("C:\\work\\portrait.aibs");
    expect(store.getState().outdated).toEqual(["matte", "shape"]);
    await store.getState().startSeparation();
    store.getState().receiveJobUpdate({ state: "finished", id: 7, result: { provider: "cpu" } });
    expect(store.getState().outdated).toEqual(["shape"]);
  });
});
