import { useState } from "react";
import { LayersSection } from "./LayersSection";
import { LookSection } from "./LookSection";
import {
  BANDS,
  BEGINNER,
  COLOUR,
  LIGHTING,
  fromHex,
  hasHiddenInBeginner,
  isIdentity,
  LIGHT,
  MAX_HEAL_SIZE,
  MIN_HEAL_SIZE,
  PORTRAIT,
  toHex,
  withBand,
  withScalar,
} from "../adjustments";
import { useApp, useAppStore, useI18n } from "../context";
import { errorMessage, type MessageKey } from "../i18n";
import { HSL_BANDS, type BackgroundMode, type BandAdjustment, type LightKind, type ScalarAdjustment } from "../types";

const signed = (digits: number) => (value: number) => {
  const text = Math.abs(value).toFixed(digits);
  return value > 0 ? `+${text}` : value < 0 ? `−${text}` : (0).toFixed(digits);
};

const RANGE: Record<ScalarAdjustment, { min: number; max: number; step: number; format: (v: number) => string }> = {
  exposure: { min: -5, max: 5, step: 0.05, format: (v) => `${signed(2)(v)} EV` },
  brightness: { min: -100, max: 100, step: 1, format: signed(0) },
  contrast: { min: -100, max: 100, step: 1, format: signed(0) },
  highlights: { min: -100, max: 100, step: 1, format: signed(0) },
  shadows: { min: -100, max: 100, step: 1, format: signed(0) },
  whites: { min: -100, max: 100, step: 1, format: signed(0) },
  blacks: { min: -100, max: 100, step: 1, format: signed(0) },
  temperature: { min: -100, max: 100, step: 1, format: signed(0) },
  tint: { min: -100, max: 100, step: 1, format: signed(0) },
  vibrance: { min: -100, max: 100, step: 1, format: signed(0) },
  saturation: { min: -100, max: 100, step: 1, format: signed(0) },
  skinSmoothing: { min: 0, max: 100, step: 1, format: (v) => v.toFixed(0) },
  blemishRemoval: { min: 0, max: 100, step: 1, format: (v) => v.toFixed(0) },
  darkCircles: { min: 0, max: 100, step: 1, format: (v) => v.toFixed(0) },
  eyeEnhance: { min: 0, max: 100, step: 1, format: (v) => v.toFixed(0) },
  teethWhitening: { min: 0, max: 100, step: 1, format: (v) => v.toFixed(0) },
  backgroundDetail: { min: 0, max: 100, step: 1, format: (v) => v.toFixed(0) },
  backgroundBlur: { min: 0, max: 100, step: 1, format: (v) => v.toFixed(0) },
  backgroundBrightness: { min: -100, max: 100, step: 1, format: signed(0) },
  backgroundSaturation: { min: -100, max: 100, step: 1, format: signed(0) },
  backgroundEdge: { min: -100, max: 100, step: 1, format: signed(0) },
  lightX: { min: -1, max: 2, step: 0.01, format: (v) => v.toFixed(2) },
  lightY: { min: -1, max: 2, step: 0.01, format: (v) => v.toFixed(2) },
  lightIntensity: { min: 0, max: 100, step: 1, format: (v) => v.toFixed(0) },
  lightSize: { min: 0, max: 100, step: 1, format: (v) => v.toFixed(0) },
  lightWarmth: { min: -100, max: 100, step: 1, format: signed(0) },
  lightHeight: { min: 0, max: 100, step: 1, format: (v) => v.toFixed(0) },
};

const LIGHT_KINDS: LightKind[] = ["none", "natural", "softbox", "diffuse", "reflector"];

const BACKGROUND_MODES: BackgroundMode[] = ["original", "transparent", "color", "blur", "image"];

/** Quick colour choices for a solid background. */
const SWATCHES: [number, number, number][] = [
  [255, 255, 255],
  [240, 240, 240],
  [128, 128, 128],
  [0, 0, 0],
  [220, 233, 245],
  [40, 90, 170],
  [245, 225, 225],
  [200, 40, 50],
];

/** Swatch colours for the HSL band labels. */
const BAND_COLOURS = ["#e5484d", "#f08c2b", "#e6c12e", "#46a758", "#2bb3c0", "#3e7bdc", "#8e4ec6", "#d6409f"];

interface SliderProps {
  id: string;
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  format: (value: number) => string;
  hint: string;
  swatch?: string;
  className?: string;
  onChange: (value: number) => void;
  onCommit: () => void;
}

/** Dragging shows the change live; releasing (or a key press) records one
 * undoable step; double-click resets to zero. */
function Slider(props: SliderProps) {
  return (
    <div className={props.className ? `slider-row ${props.className}` : "slider-row"}>
      <div className="slider-label">
        <label htmlFor={props.id}>
          {props.swatch && <span className="swatch" style={{ background: props.swatch }} aria-hidden="true" />}
          {props.label}
        </label>
        <output htmlFor={props.id}>{props.format(props.value)}</output>
      </div>
      <input
        id={props.id}
        type="range"
        min={props.min}
        max={props.max}
        step={props.step}
        value={props.value}
        title={props.hint}
        onChange={(event) => props.onChange(Number(event.target.value))}
        onPointerUp={props.onCommit}
        onKeyUp={props.onCommit}
        onDoubleClick={() => {
          props.onChange(0);
          props.onCommit();
        }}
      />
    </div>
  );
}

function FaceNote() {
  const { t, language } = useI18n();
  const faces = useApp((s) => s.faces);
  if (!faces) return null;
  const text =
    faces.state === "detecting"
      ? t("faces.detecting")
      : faces.state === "unavailable"
        ? t("faces.unavailable", { reason: errorMessage(language, faces.code) })
        : faces.count === 0
          ? t("faces.none")
          : t("faces.found", { count: faces.count });
  return (
    <p className="note" role="status">
      {text}
    </p>
  );
}

/** Separation status and the button that starts it. */
function SeparationStatus() {
  const { t, language } = useI18n();
  const background = useApp((s) => s.background);
  const start = useApp((s) => s.startSeparation);
  const cancel = useApp((s) => s.cancelSeparation);
  if (!background) return null;
  switch (background.state) {
    case "none":
      return (
        <div className="separation">
          <p className="note">{t("background.intro")}</p>
          <button type="button" onClick={() => void start()}>
            {t("background.separate")}
          </button>
        </div>
      );
    case "running":
      return (
        <div className="separation" role="status">
          <p className="note">{t("background.running", { percent: Math.round(background.fraction * 100) })}</p>
          <progress max={1} value={background.fraction} />
          <button type="button" className="ghost small" onClick={() => void cancel()} disabled={background.cancelling}>
            {t("ai.cancel")}
          </button>
        </div>
      );
    case "failed":
      return (
        <div className="separation">
          <p className="error-text">{errorMessage(language, background.code)}</p>
          <button type="button" onClick={() => void start()}>
            {t("background.retry")}
          </button>
        </div>
      );
    case "ready":
      return <OutdatedNote result="matte" onRedo={start} />;
  }
}

/** A saved AI result made by another model version can be redone. */
function OutdatedNote({ result, onRedo }: { result: string; onRedo: () => Promise<void> }) {
  const { t } = useI18n();
  const outdated = useApp((s) => s.outdated.includes(result));
  if (!outdated) return null;
  return (
    <div className="separation">
      <p className="note warning">{t("models.outdated")}</p>
      <button type="button" className="ghost small" onClick={() => void onRedo()}>
        {t("models.redo")}
      </button>
    </div>
  );
}

/** 3D shape estimation status for lighting. */
function ShapeStatus() {
  const { t, language } = useI18n();
  const shape = useApp((s) => s.shape);
  const start = useApp((s) => s.startShapeEstimation);
  const cancel = useApp((s) => s.cancelShapeEstimation);
  if (!shape) return null;
  switch (shape.state) {
    case "none":
      return (
        <div className="separation">
          <p className="note">{t("light.flatNote")}</p>
          <button type="button" className="ghost small" onClick={() => void start()}>
            {t("light.analyse")}
          </button>
        </div>
      );
    case "running":
      return (
        <div className="separation" role="status">
          <p className="note">{t("light.running", { percent: Math.round(shape.fraction * 100) })}</p>
          <progress max={1} value={shape.fraction} />
          <button type="button" className="ghost small" onClick={() => void cancel()} disabled={shape.cancelling}>
            {t("ai.cancel")}
          </button>
        </div>
      );
    case "failed":
      return (
        <div className="separation">
          <p className="error-text">{errorMessage(language, shape.code)}</p>
          <button type="button" className="ghost small" onClick={() => void start()}>
            {t("background.retry")}
          </button>
        </div>
      );
    case "ready":
      return (
        <>
          <p className="note">{t("light.ready")}</p>
          <OutdatedNote result="shape" onRedo={start} />
        </>
      );
  }
}

/** Healing brush: click a spot on the photo to replace it with matching
 * texture from nearby. */
function HealTool() {
  const { t } = useI18n();
  const active = useApp((s) => s.tool === "heal");
  const healSize = useApp((s) => s.healSize);
  const count = useApp((s) => s.live.spots.length);
  const setTool = useApp((s) => s.setTool);
  const setHealSize = useApp((s) => s.setHealSize);
  const clearSpots = useApp((s) => s.clearSpots);
  return (
    <div className="heal-tool">
      <div className="button-row">
        <button
          type="button"
          className={active ? "ghost small pressed" : "ghost small"}
          aria-pressed={active}
          onClick={() => setTool(active ? "none" : "heal")}
        >
          {t("heal.tool")}
        </button>
        <button type="button" className="ghost small" disabled={count === 0} onClick={() => void clearSpots()}>
          {t("heal.clear", { count })}
        </button>
      </div>
      {active && (
        <>
          <p className="note">{t("heal.hint")}</p>
          <div className="slider-row">
            <div className="slider-label">
              <label htmlFor="heal-size">{t("heal.size")}</label>
              <output htmlFor="heal-size">{(healSize * 100).toFixed(1)}%</output>
            </div>
            <input
              id="heal-size"
              type="range"
              min={MIN_HEAL_SIZE * 100}
              max={MAX_HEAL_SIZE * 100}
              step={0.1}
              value={healSize * 100}
              onChange={(event) => setHealSize(Number(event.target.value) / 100)}
            />
          </div>
        </>
      )}
    </div>
  );
}

export function AdjustPanel() {
  const { t } = useI18n();
  const chooseLight = useApp((s) => s.chooseLight);
  const setLightSubjectOnly = useApp((s) => s.setLightSubjectOnly);
  const background = useApp((s) => s.background);
  const chooseBackground = useApp((s) => s.chooseBackground);
  const pickBackgroundImage = useApp((s) => s.pickBackgroundImage);
  const live = useApp((s) => s.live);
  const edited = useApp((s) => s.edit?.edited ?? false);
  const mode = useApp((s) => s.settings.editorMode);
  const setEditorMode = useApp((s) => s.setEditorMode);
  const setLive = useApp((s) => s.setLive);
  const commitLive = useApp((s) => s.commitLive);
  const resetAdjustments = useApp((s) => s.resetAdjustments);
  const [bandProperty, setBandProperty] = useState<BandAdjustment>("hslHue");

  const advanced = mode === "advanced";
  const commit = () => void commitLive();
  // Read the newest values at event time: several events can arrive
  // between renders while a slider is dragged.
  const store = useAppStore();
  const latest = () => store.getState().live;

  const scalar = (key: ScalarAdjustment) => {
    const range = RANGE[key];
    return (
      <Slider
        key={key}
        id={`slider-${key}`}
        label={t(`adjust.${key}` as MessageKey)}
        value={live[key]}
        {...range}
        hint={t("adjust.resetHint")}
        className={key === "temperature" || key === "tint" ? key : undefined}
        onChange={(value) => setLive(withScalar(latest(), key, value))}
        onCommit={commit}
      />
    );
  };

  const visible = (keys: ScalarAdjustment[]) => (advanced ? keys : keys.filter((key) => BEGINNER.includes(key)));

  return (
    <div className="adjust-panel">
      <div className="panel-head">
        <div className="segmented" role="radiogroup" aria-label={t("mode.label")}>
          {(["beginner", "advanced"] as const).map((value) => (
            <button
              key={value}
              type="button"
              role="radio"
              aria-checked={mode === value}
              className={mode === value ? "active" : ""}
              onClick={() => void setEditorMode(value)}
            >
              {t(`mode.${value}`)}
            </button>
          ))}
        </div>
        <button type="button" className="ghost small" onClick={() => void resetAdjustments()} disabled={!edited && isIdentity(live)}>
          {t("adjust.reset")}
        </button>
      </div>

      {!advanced && hasHiddenInBeginner(live) && <p className="note">{t("mode.hidden")}</p>}

      <section aria-labelledby="light-title">
        <h2 id="light-title">{t("adjust.light")}</h2>
        {visible(LIGHT).map(scalar)}
      </section>

      <section aria-labelledby="colour-title">
        <h2 id="colour-title">{t("adjust.colour")}</h2>
        {visible(COLOUR).map(scalar)}
      </section>

      <section aria-labelledby="portrait-title">
        <h2 id="portrait-title">{t("adjust.portrait")}</h2>
        <FaceNote />
        {visible(PORTRAIT).map(scalar)}
        <HealTool />
      </section>

      <section aria-labelledby="background-title">
        <h2 id="background-title">{t("background.title")}</h2>
        <SeparationStatus />
        {background?.state === "ready" && (
          <>
            <div className="segmented small wrap" role="radiogroup" aria-label={t("background.title")}>
              {BACKGROUND_MODES.map((mode) => (
                <button
                  key={mode}
                  type="button"
                  role="radio"
                  aria-checked={live.backgroundMode === mode}
                  className={live.backgroundMode === mode ? "active" : ""}
                  onClick={() =>
                    mode === "image" && live.backgroundImage === 0
                      ? void pickBackgroundImage(t("imageFiles"))
                      : void chooseBackground(
                          // A first colour choice starts from white rather than black.
                          mode === "color" && live.backgroundColor.every((v) => v === 0)
                            ? { backgroundMode: mode, backgroundColor: [255, 255, 255] }
                            : { backgroundMode: mode },
                        )
                  }
                >
                  {t(`background.${mode}` as MessageKey)}
                </button>
              ))}
            </div>
            {live.backgroundMode === "color" && (
              <div className="swatches">
                {SWATCHES.map((rgb) => (
                  <button
                    key={rgb.join(",")}
                    type="button"
                    className={toHex(rgb) === toHex(live.backgroundColor) ? "swatch-button active" : "swatch-button"}
                    style={{ background: toHex(rgb) }}
                    aria-label={toHex(rgb)}
                    onClick={() => void chooseBackground({ backgroundColor: rgb })}
                  />
                ))}
                <label className="colour-pick">
                  {t("background.custom")}
                  <input
                    type="color"
                    value={toHex(live.backgroundColor)}
                    onChange={(event) => setLive({ ...latest(), backgroundColor: fromHex(event.target.value) })}
                    onBlur={commit}
                  />
                </label>
              </div>
            )}
            {live.backgroundMode === "image" && (
              <button type="button" className="ghost small" onClick={() => void pickBackgroundImage(t("imageFiles"))}>
                {t("background.pickImage")}
              </button>
            )}
            {live.backgroundMode === "transparent" && <p className="note">{t("background.transparentNote")}</p>}
            {live.backgroundMode !== "original" && scalar("backgroundEdge")}
            {live.backgroundMode !== "original" && scalar("backgroundDetail")}
            {live.backgroundMode === "blur" && scalar("backgroundBlur")}
            {live.backgroundMode !== "transparent" && live.backgroundMode !== "color" && (
              <>
                {scalar("backgroundBrightness")}
                {scalar("backgroundSaturation")}
              </>
            )}
          </>
        )}
      </section>

      <section aria-labelledby="light-section-title">
        <h2 id="light-section-title">{t("light.title")}</h2>
        <div className="segmented small wrap" role="radiogroup" aria-label={t("light.title")}>
          {LIGHT_KINDS.map((kind) => (
            <button
              key={kind}
              type="button"
              role="radio"
              aria-checked={live.lightKind === kind}
              className={live.lightKind === kind ? "active" : ""}
              onClick={() => void chooseLight(kind)}
            >
              {t(`light.${kind}` as MessageKey)}
            </button>
          ))}
        </div>
        {live.lightKind !== "none" && (
          <>
            <ShapeStatus />
            <p className="note">{t("light.dragHint")}</p>
            {LIGHTING.map(scalar)}
            <label className="check-row" title={background?.state === "ready" ? undefined : t("light.subjectNeedsSeparation")}>
              <input
                type="checkbox"
                checked={live.lightSubjectOnly}
                disabled={background?.state !== "ready"}
                onChange={(event) => void setLightSubjectOnly(event.target.checked)}
              />
              {t("light.subjectOnly")}
            </label>
          </>
        )}
      </section>

      <LookSection advanced={advanced} />

      <LayersSection />

      {advanced && (
        <section aria-labelledby="hsl-title">
          <h2 id="hsl-title">{t("hsl.title")}</h2>
          <div className="segmented small" role="tablist" aria-label={t("hsl.title")}>
            {BANDS.map((key) => (
              <button
                key={key}
                type="button"
                role="tab"
                aria-selected={bandProperty === key}
                className={bandProperty === key ? "active" : ""}
                onClick={() => setBandProperty(key)}
              >
                {t(`hsl.${key}` as MessageKey)}
              </button>
            ))}
          </div>
          {Array.from({ length: HSL_BANDS }, (_, band) => (
            <Slider
              key={`${bandProperty}-${band}`}
              id={`slider-${bandProperty}-${band}`}
              label={t(`band.${band}` as MessageKey)}
              swatch={BAND_COLOURS[band]}
              value={live[bandProperty][band]}
              min={-100}
              max={100}
              step={1}
              format={signed(0)}
              hint={t("adjust.resetHint")}
              onChange={(value) => setLive(withBand(latest(), bandProperty, band, value))}
              onCommit={commit}
            />
          ))}
        </section>
      )}
    </div>
  );
}
