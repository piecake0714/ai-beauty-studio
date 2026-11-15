import { useRef, useState, type MouseEvent as ReactMouseEvent, type PointerEvent as ReactPointerEvent } from "react";
import { cleanCurve, CURVE_POINTS, curveValues, withCurve, withGrade } from "../adjustments";
import { useApp, useAppStore, useI18n } from "../context";
import type { MessageKey } from "../i18n";
import type { CurveChannel, CurvePoint, GradeRangeName } from "../types";

const CHANNELS: CurveChannel[] = ["master", "red", "green", "blue"];
const CHANNEL_COLOURS: Record<CurveChannel, string> = {
  master: "var(--text)",
  red: "#e5484d",
  green: "#30a46c",
  blue: "#3e63dd",
};
const RANGES: GradeRangeName[] = ["shadows", "midtones", "highlights"];
/** Size of the curve box in SVG units. */
const BOX = 200;
/** How close (in box units) a press must be to grab a point. */
const GRAB = 9;

/** RGB curves: drag points, click to add one, double-click to remove one. */
function CurveEditor() {
  const { t } = useI18n();
  const curves = useApp((s) => s.live.curves);
  const setLive = useApp((s) => s.setLive);
  const commitLive = useApp((s) => s.commitLive);
  const store = useAppStore();
  const [channel, setChannel] = useState<CurveChannel>("master");
  const svgRef = useRef<SVGSVGElement>(null);
  const dragRef = useRef<number | null>(null);

  const points = cleanCurve(curves[channel]);
  const shown: CurvePoint[] = points.length >= 2 ? points : [[0, 0], [1, 1]];
  const values = curveValues(shown, 101);
  const path = values.map((y, i) => `${i === 0 ? "M" : "L"}${(i / 100) * BOX},${(1 - y) * BOX}`).join(" ");

  const toCurve = (event: ReactPointerEvent<Element>): CurvePoint | null => {
    const rect = svgRef.current?.getBoundingClientRect();
    if (!rect || rect.width === 0) return null;
    const x = Math.min(Math.max((event.clientX - rect.left) / rect.width, 0), 1);
    const y = Math.min(Math.max(1 - (event.clientY - rect.top) / rect.height, 0), 1);
    return [x, y];
  };
  const nearest = (p: CurvePoint) => {
    let best = -1;
    let distance = Infinity;
    shown.forEach(([x, y], i) => {
      const d = Math.hypot((x - p[0]) * BOX, (y - p[1]) * BOX);
      if (d < distance) {
        distance = d;
        best = i;
      }
    });
    return distance <= GRAB ? best : -1;
  };
  const update = (next: CurvePoint[]) => setLive(withCurve(store.getState().live, channel, next));

  const onPointerDown = (event: ReactPointerEvent<SVGSVGElement>) => {
    const p = toCurve(event);
    if (!p) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    const hit = nearest(p);
    if (hit >= 0) {
      dragRef.current = hit;
      return;
    }
    if (shown.length >= CURVE_POINTS) return;
    // Add a point on the curve under the pointer's x, then drag it.
    const next = cleanCurve([...shown, [p[0], p[1]]]);
    dragRef.current = next.findIndex((q) => q[0] === p[0]);
    update(next);
  };
  const onPointerMove = (event: ReactPointerEvent<SVGSVGElement>) => {
    const index = dragRef.current;
    if (index === null || index < 0) return;
    const p = toCurve(event);
    if (!p) return;
    const current = cleanCurve(store.getState().live.curves[channel]);
    const list: CurvePoint[] = current.length >= 2 ? current : [[0, 0], [1, 1]];
    // Keep the point between its neighbours so the order never changes.
    const lo = index > 0 ? list[index - 1][0] + 0.01 : 0;
    const hi = index < list.length - 1 ? list[index + 1][0] - 0.01 : 1;
    const moved = list.map((q, i) => (i === index ? ([Math.min(Math.max(p[0], lo), hi), p[1]] as CurvePoint) : q));
    update(moved);
  };
  const onPointerUp = () => {
    if (dragRef.current === null) return;
    dragRef.current = null;
    void commitLive();
  };
  const onDoubleClick = (event: ReactMouseEvent<SVGSVGElement>) => {
    const rect = svgRef.current?.getBoundingClientRect();
    if (!rect) return;
    const p: CurvePoint = [(event.clientX - rect.left) / rect.width, 1 - (event.clientY - rect.top) / rect.height];
    const hit = nearest(p);
    // The end points stay; inner points can be removed.
    if (hit > 0 && hit < shown.length - 1) {
      update(shown.filter((_, i) => i !== hit));
      void commitLive();
    }
  };

  return (
    <div className="curve-editor">
      <div className="segmented small" role="tablist" aria-label={t("curves.channel")}>
        {CHANNELS.map((c) => (
          <button
            key={c}
            type="button"
            role="tab"
            aria-selected={channel === c}
            className={channel === c ? "active" : ""}
            onClick={() => setChannel(c)}
          >
            {t(`curves.${c}` as MessageKey)}
          </button>
        ))}
      </div>
      <svg
        ref={svgRef}
        className="curve-box"
        viewBox={`0 0 ${BOX} ${BOX}`}
        role="img"
        aria-label={t("curves.title")}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
        onDoubleClick={onDoubleClick}
      >
        {[0.25, 0.5, 0.75].map((f) => (
          <g key={f} className="curve-grid">
            <line x1={f * BOX} y1={0} x2={f * BOX} y2={BOX} />
            <line x1={0} y1={f * BOX} x2={BOX} y2={f * BOX} />
          </g>
        ))}
        <line className="curve-diagonal" x1={0} y1={BOX} x2={BOX} y2={0} />
        <path d={path} fill="none" stroke={CHANNEL_COLOURS[channel]} strokeWidth={2} />
        {shown.map(([x, y], i) => (
          <circle key={i} cx={x * BOX} cy={(1 - y) * BOX} r={4} className="curve-point" stroke={CHANNEL_COLOURS[channel]} />
        ))}
      </svg>
      <p className="note">{t("curves.hint")}</p>
      <button
        type="button"
        className="ghost small"
        disabled={curves[channel].length === 0}
        onClick={() => {
          update([]);
          void commitLive();
        }}
      >
        {t("curves.reset")}
      </button>
    </div>
  );
}

function GradeSlider(props: {
  id: string;
  label: string;
  value: number;
  min: number;
  max: number;
  className?: string;
  onChange: (value: number) => void;
  onCommit: () => void;
}) {
  return (
    <div className={props.className ? `slider-row ${props.className}` : "slider-row"}>
      <div className="slider-label">
        <label htmlFor={props.id}>{props.label}</label>
        <output htmlFor={props.id}>{Math.round(props.value)}</output>
      </div>
      <input
        id={props.id}
        type="range"
        min={props.min}
        max={props.max}
        step={1}
        value={props.value}
        onChange={(event) => props.onChange(Number(event.target.value))}
        onPointerUp={props.onCommit}
        onKeyUp={props.onCommit}
      />
    </div>
  );
}

/** Colour grading: a tint and brightness for shadows, midtones and
 * highlights, and where the ranges meet. */
function GradingEditor() {
  const { t } = useI18n();
  const grading = useApp((s) => s.live.grading);
  const setLive = useApp((s) => s.setLive);
  const commitLive = useApp((s) => s.commitLive);
  const store = useAppStore();
  const [range, setRange] = useState<GradeRangeName>("shadows");
  const commit = () => void commitLive();
  const values = grading[range];
  return (
    <div className="grading">
      <div className="segmented small" role="tablist" aria-label={t("grading.title")}>
        {RANGES.map((r) => (
          <button
            key={r}
            type="button"
            role="tab"
            aria-selected={range === r}
            className={range === r ? "active" : ""}
            onClick={() => setRange(r)}
          >
            {t(`grading.${r}` as MessageKey)}
            {grading[r].saturation > 0 && (
              <span className="grade-dot" style={{ background: `hsl(${grading[r].hue} 80% 55%)` }} aria-hidden="true" />
            )}
          </button>
        ))}
      </div>
      <GradeSlider
        id={`grade-${range}-hue`}
        label={t("grading.hue")}
        value={values.hue}
        min={0}
        max={360}
        className="hue"
        onChange={(v) => setLive(withGrade(store.getState().live, range, { hue: v }))}
        onCommit={commit}
      />
      <GradeSlider
        id={`grade-${range}-saturation`}
        label={t("grading.saturation")}
        value={values.saturation}
        min={0}
        max={100}
        onChange={(v) => setLive(withGrade(store.getState().live, range, { saturation: v }))}
        onCommit={commit}
      />
      <GradeSlider
        id={`grade-${range}-luminance`}
        label={t("grading.luminance")}
        value={values.luminance}
        min={-100}
        max={100}
        onChange={(v) => setLive(withGrade(store.getState().live, range, { luminance: v }))}
        onCommit={commit}
      />
      <GradeSlider
        id="grade-balance"
        label={t("grading.balance")}
        value={grading.balance}
        min={-100}
        max={100}
        onChange={(v) => {
          const live = store.getState().live;
          setLive({ ...live, grading: { ...live.grading, balance: v } });
        }}
        onCommit={commit}
      />
    </div>
  );
}

/** LUT presets: built-in looks and imported .cube files, with strength. */
function LutPicker() {
  const { t } = useI18n();
  const luts = useApp((s) => s.luts);
  const lut = useApp((s) => s.live.lut);
  const chooseLut = useApp((s) => s.chooseLut);
  const importLut = useApp((s) => s.importLut);
  const setLive = useApp((s) => s.setLive);
  const commitLive = useApp((s) => s.commitLive);
  const store = useAppStore();
  const known = lut.id === "" || luts.some((l) => l.id === lut.id);
  const label = (name: string, builtin: boolean) => (builtin ? t(`lut.builtin.${name}` as MessageKey) : name);
  return (
    <div className="lut-picker">
      <label className="field">
        {t("lut.preset")}
        <select value={lut.id} onChange={(event) => void chooseLut(event.target.value)}>
          <option value="">{t("lut.none")}</option>
          {luts.map((l) => (
            <option key={l.id} value={l.id}>
              {label(l.name, l.builtin)}
            </option>
          ))}
          {!known && <option value={lut.id}>{t("lut.missing")}</option>}
        </select>
      </label>
      {!known && <p className="note warning">{t("lut.missingNote")}</p>}
      {lut.id !== "" && (
        <GradeSlider
          id="lut-amount"
          label={t("lut.amount")}
          value={lut.amount}
          min={0}
          max={100}
          onChange={(v) => {
            const live = store.getState().live;
            setLive({ ...live, lut: { ...live.lut, amount: v } });
          }}
          onCommit={() => void commitLive()}
        />
      )}
      <button type="button" className="ghost small" onClick={() => void importLut(t("lut.files"))}>
        {t("lut.import")}
      </button>
    </div>
  );
}

/** The look: LUT presets for everyone; curves and grading in advanced mode. */
export function LookSection({ advanced }: { advanced: boolean }) {
  const { t } = useI18n();
  return (
    <>
      <section aria-labelledby="lut-title">
        <h2 id="lut-title">{t("lut.title")}</h2>
        <LutPicker />
      </section>
      {advanced && (
        <>
          <section aria-labelledby="curves-title">
            <h2 id="curves-title">{t("curves.title")}</h2>
            <CurveEditor />
          </section>
          <section aria-labelledby="grading-title">
            <h2 id="grading-title">{t("grading.title")}</h2>
            <GradingEditor />
          </section>
        </>
      )}
    </>
  );
}
