import { useApp, useI18n } from "../context";
import type { MessageKey } from "../i18n";
import type { Adjustments } from "../types";

interface SliderSpec {
  key: keyof Adjustments;
  label: MessageKey;
  min: number;
  max: number;
  step: number;
  format: (value: number) => string;
}

const signed = (digits: number) => (value: number) => {
  const text = value.toFixed(digits);
  return value > 0 ? `+${text}` : value === 0 ? (0).toFixed(digits) : text;
};

const LIGHT: SliderSpec[] = [
  { key: "exposure", label: "adjust.exposure", min: -5, max: 5, step: 0.05, format: (v) => `${signed(2)(v)} EV` },
  { key: "brightness", label: "adjust.brightness", min: -100, max: 100, step: 1, format: signed(0) },
  { key: "contrast", label: "adjust.contrast", min: -100, max: 100, step: 1, format: signed(0) },
];

/**
 * Adjustment sliders. Dragging shows the change live; releasing the slider
 * (or a key press on it) records one undoable step. Double-click resets a
 * slider to zero.
 */
export function AdjustPanel() {
  const { t } = useI18n();
  const live = useApp((s) => s.live);
  const edited = useApp((s) => s.edit?.edited ?? false);
  const setLive = useApp((s) => s.setLive);
  const commitLive = useApp((s) => s.commitLive);
  const resetAdjustments = useApp((s) => s.resetAdjustments);

  const change = (key: keyof Adjustments, value: number) => setLive({ ...live, [key]: value });

  return (
    <section className="adjust-panel" aria-labelledby="light-title">
      <div className="panel-head">
        <h2 id="light-title">{t("adjust.light")}</h2>
        <button type="button" className="ghost small" onClick={resetAdjustments} disabled={!edited && live.exposure === 0 && live.brightness === 0 && live.contrast === 0}>
          {t("adjust.reset")}
        </button>
      </div>
      {LIGHT.map((spec) => {
        const id = `slider-${spec.key}`;
        return (
          <div className="slider-row" key={spec.key}>
            <div className="slider-label">
              <label htmlFor={id}>{t(spec.label)}</label>
              <output htmlFor={id}>{spec.format(live[spec.key])}</output>
            </div>
            <input
              id={id}
              type="range"
              min={spec.min}
              max={spec.max}
              step={spec.step}
              value={live[spec.key]}
              title={t("adjust.resetHint")}
              onChange={(event) => change(spec.key, Number(event.target.value))}
              onPointerUp={() => void commitLive()}
              onKeyUp={() => void commitLive()}
              onDoubleClick={() => {
                setLive({ ...live, [spec.key]: 0 });
                void commitLive();
              }}
            />
          </div>
        );
      })}
    </section>
  );
}
