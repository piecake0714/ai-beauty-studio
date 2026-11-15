import { LAYER_SLIDERS, MAX_LAYERS } from "../adjustments";
import { useApp, useAppStore, useI18n } from "../context";
import type { MessageKey } from "../i18n";
import type { BlendMode, Layer, LayerAdjustments, MaskStart } from "../types";

const BLENDS: BlendMode[] = ["normal", "multiply", "screen", "overlay", "soft-light", "darken", "lighten", "difference"];
const MASK_STARTS: MaskStart[] = ["all", "none", "subject", "background"];

const LAYER_RANGE: Record<keyof LayerAdjustments, { min: number; max: number; step: number }> = {
  exposure: { min: -5, max: 5, step: 0.05 },
  contrast: { min: -100, max: 100, step: 1 },
  highlights: { min: -100, max: 100, step: 1 },
  shadows: { min: -100, max: 100, step: 1 },
  temperature: { min: -100, max: 100, step: 1 },
  tint: { min: -100, max: 100, step: 1 },
  saturation: { min: -100, max: 100, step: 1 },
  vibrance: { min: -100, max: 100, step: 1 },
};

const show = (key: keyof LayerAdjustments, v: number) =>
  key === "exposure" ? `${v > 0 ? "+" : ""}${v.toFixed(2)} EV` : `${v > 0 ? "+" : ""}${v.toFixed(0)}`;

/** Layers over the photo: add, order, show or hide, blend, mask by brush. */
export function LayersSection() {
  const { t } = useI18n();
  const layers = useApp((s) => s.live.layers);
  const selected = useApp((s) => s.selectedLayer);
  const tool = useApp((s) => s.tool);
  const brush = useApp((s) => s.brush);
  const separated = useApp((s) => s.background?.state === "ready");
  const addLayer = useApp((s) => s.addLayer);
  const addImageLayer = useApp((s) => s.addImageLayer);
  const updateLayer = useApp((s) => s.updateLayer);
  const removeLayer = useApp((s) => s.removeLayer);
  const moveLayer = useApp((s) => s.moveLayer);
  const selectLayer = useApp((s) => s.selectLayer);
  const setTool = useApp((s) => s.setTool);
  const setBrush = useApp((s) => s.setBrush);
  const store = useAppStore();

  const full = layers.length >= MAX_LAYERS;
  const current = layers.find((l) => l.id === selected) ?? null;
  const change = (id: number, patch: Partial<Layer>, commit = true) =>
    void updateLayer(id, (l) => ({ ...l, ...patch }), commit);
  const commitCurrent = () => {
    const id = store.getState().selectedLayer;
    if (id !== null) void updateLayer(id, (l) => l, true);
  };
  const nextName = (kind: "adjustment" | "image") =>
    `${t(kind === "adjustment" ? "layers.adjustmentName" : "layers.imageName")} ${layers.length + 1}`;

  return (
    <section aria-labelledby="layers-title">
      <h2 id="layers-title">{t("layers.title")}</h2>
      <div className="button-row">
        <button type="button" className="ghost small" disabled={full} onClick={() => void addLayer("adjustment", nextName("adjustment"))}>
          {t("layers.addAdjustment")}
        </button>
        <button
          type="button"
          className="ghost small"
          disabled={full}
          onClick={() => void addImageLayer(t("imageFiles"), nextName("image"))}
        >
          {t("layers.addImage")}
        </button>
      </div>
      {full && <p className="note">{t("layers.full")}</p>}
      {layers.length === 0 && <p className="note">{t("layers.empty")}</p>}
      <ul className="layer-list" aria-label={t("layers.title")}>
        {[...layers].reverse().map((layer, index) => (
          <li key={layer.id} className={layer.id === selected ? "layer-row active" : "layer-row"}>
            <input
              type="checkbox"
              checked={layer.visible}
              aria-label={t("layers.visible")}
              title={t("layers.visible")}
              onChange={(event) => change(layer.id, { visible: event.target.checked })}
            />
            <button
              type="button"
              className="layer-name"
              aria-pressed={layer.id === selected}
              onClick={() => selectLayer(layer.id === selected ? null : layer.id)}
            >
              {layer.name}
              <span className="layer-kind">{t(`layers.kind.${layer.kind}` as MessageKey)}</span>
            </button>
            <button type="button" className="icon" aria-label={t("layers.up")} title={t("layers.up")} disabled={index === 0} onClick={() => void moveLayer(layer.id, 1)}>
              ↑
            </button>
            <button
              type="button"
              className="icon"
              aria-label={t("layers.down")}
              title={t("layers.down")}
              disabled={index === layers.length - 1}
              onClick={() => void moveLayer(layer.id, -1)}
            >
              ↓
            </button>
            <button type="button" className="icon" aria-label={t("layers.delete")} title={t("layers.delete")} onClick={() => void removeLayer(layer.id)}>
              ×
            </button>
          </li>
        ))}
      </ul>

      {current && (
        <div className="layer-detail">
          <label className="field">
            {t("layers.name")}
            <input
              type="text"
              value={current.name}
              maxLength={60}
              onChange={(event) => change(current.id, { name: event.target.value }, false)}
              onBlur={commitCurrent}
            />
          </label>
          <div className="slider-row">
            <div className="slider-label">
              <label htmlFor="layer-opacity">{t("layers.opacity")}</label>
              <output htmlFor="layer-opacity">{current.opacity.toFixed(0)}%</output>
            </div>
            <input
              id="layer-opacity"
              type="range"
              min={0}
              max={100}
              step={1}
              value={current.opacity}
              onChange={(event) => change(current.id, { opacity: Number(event.target.value) }, false)}
              onPointerUp={commitCurrent}
              onKeyUp={commitCurrent}
            />
          </div>
          <label className="field">
            {t("layers.blend")}
            <select value={current.blend} onChange={(event) => change(current.id, { blend: event.target.value as BlendMode })}>
              {BLENDS.map((blend) => (
                <option key={blend} value={blend}>
                  {t(`blend.${blend}` as MessageKey)}
                </option>
              ))}
            </select>
          </label>

          <h3>{t("layers.mask")}</h3>
          <label className="field">
            {t("layers.maskStart")}
            <select
              value={current.mask.start}
              onChange={(event) => change(current.id, { mask: { ...current.mask, start: event.target.value as MaskStart } })}
            >
              {MASK_STARTS.map((start) => (
                <option key={start} value={start} disabled={(start === "subject" || start === "background") && !separated}>
                  {t(`mask.${start}` as MessageKey)}
                </option>
              ))}
            </select>
          </label>
          {(current.mask.start === "subject" || current.mask.start === "background") && !separated && (
            <p className="note">{t("mask.needsSeparation")}</p>
          )}
          <label className="check-row">
            <input
              type="checkbox"
              checked={current.mask.invert}
              onChange={(event) => change(current.id, { mask: { ...current.mask, invert: event.target.checked } })}
            />
            {t("layers.invert")}
          </label>
          <div className="segmented small" role="radiogroup" aria-label={t("brush.tool")}>
            {(["none", "brush", "eraser"] as const).map((value) => (
              <button
                key={value}
                type="button"
                role="radio"
                aria-checked={tool === value || (value === "none" && tool === "crop")}
                className={tool === value ? "active" : ""}
                onClick={() => setTool(value)}
              >
                {t(`brush.${value}` as MessageKey)}
              </button>
            ))}
          </div>
          {(tool === "brush" || tool === "eraser") && (
            <>
              <p className="note">{t("brush.hint")}</p>
              <BrushSlider id="brush-size" label={t("brush.size")} value={Math.round(brush.size * 1000) / 10} min={0.5} max={40} step={0.5} unit="%" onChange={(v) => setBrush({ size: v / 100 })} />
              <BrushSlider id="brush-hardness" label={t("brush.hardness")} value={brush.hardness} min={0} max={100} step={1} unit="" onChange={(v) => setBrush({ hardness: v })} />
              <BrushSlider id="brush-opacity" label={t("brush.opacity")} value={brush.opacity} min={1} max={100} step={1} unit="%" onChange={(v) => setBrush({ opacity: v })} />
            </>
          )}
          <button
            type="button"
            className="ghost small"
            disabled={current.mask.strokes.length === 0}
            onClick={() => change(current.id, { mask: { ...current.mask, strokes: [] } })}
          >
            {t("layers.clearStrokes")}
          </button>

          {current.kind === "adjustment" && (
            <>
              <h3>{t("layers.adjustments")}</h3>
              {LAYER_SLIDERS.map((key) => (
                <div className="slider-row" key={key}>
                  <div className="slider-label">
                    <label htmlFor={`layer-${key}`}>{t(`adjust.${key}` as MessageKey)}</label>
                    <output htmlFor={`layer-${key}`}>{show(key, current.adjustments[key])}</output>
                  </div>
                  <input
                    id={`layer-${key}`}
                    type="range"
                    {...LAYER_RANGE[key]}
                    value={current.adjustments[key]}
                    title={t("adjust.resetHint")}
                    onChange={(event) =>
                      void updateLayer(current.id, (l) => ({ ...l, adjustments: { ...l.adjustments, [key]: Number(event.target.value) } }), false)
                    }
                    onPointerUp={commitCurrent}
                    onKeyUp={commitCurrent}
                    onDoubleClick={() => void updateLayer(current.id, (l) => ({ ...l, adjustments: { ...l.adjustments, [key]: 0 } }), true)}
                  />
                </div>
              ))}
            </>
          )}
        </div>
      )}
    </section>
  );
}

function BrushSlider(props: {
  id: string;
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  unit: string;
  onChange: (value: number) => void;
}) {
  return (
    <div className="slider-row">
      <div className="slider-label">
        <label htmlFor={props.id}>{props.label}</label>
        <output htmlFor={props.id}>
          {props.value}
          {props.unit}
        </output>
      </div>
      <input
        id={props.id}
        type="range"
        min={props.min}
        max={props.max}
        step={props.step}
        value={props.value}
        onChange={(event) => props.onChange(Number(event.target.value))}
      />
    </div>
  );
}
