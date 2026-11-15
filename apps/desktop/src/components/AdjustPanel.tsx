import { useState } from "react";
import { BANDS, BEGINNER, COLOUR, hasHiddenInBeginner, isIdentity, LIGHT, withBand, withScalar } from "../adjustments";
import { useApp, useAppStore, useI18n } from "../context";
import type { MessageKey } from "../i18n";
import { HSL_BANDS, type BandAdjustment, type ScalarAdjustment } from "../types";

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
};

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

export function AdjustPanel() {
  const { t } = useI18n();
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
