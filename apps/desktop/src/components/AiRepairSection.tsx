import { useState } from "react";
import { useApp, useI18n } from "../context";
import { errorMessage } from "../i18n";

/** Progress of the running AI edit, or why it failed. */
function TaskStatus({ kind }: { kind: "remove" | "repair" }) {
  const { t, language } = useI18n();
  const task = useApp((s) => (s.aiTask?.kind === kind ? s.aiTask : null));
  const cancelAiTask = useApp((s) => s.cancelAiTask);
  if (!task) return null;
  if (task.state === "failed") {
    return (
      <p className="note warning" role="alert">
        {errorMessage(language, task.code)}
      </p>
    );
  }
  return (
    <div className="separation" role="status">
      <p className="note">{t("ai.working", { percent: Math.round(task.fraction * 100) })}</p>
      <progress max={1} value={task.fraction} />
      <button type="button" className="ghost small" onClick={() => void cancelAiTask()} disabled={task.cancelling}>
        {t("ai.cancel")}
      </button>
    </div>
  );
}

/** Object removal: paint over what to remove, then let the AI fill it. */
function RemoveTool() {
  const { t } = useI18n();
  const active = useApp((s) => s.tool === "remove");
  const painted = useApp((s) => s.removeStrokes.length);
  const removals = useApp((s) => s.live.removals.length);
  const running = useApp((s) => s.aiTask?.state === "running");
  const brush = useApp((s) => s.brush);
  const setTool = useApp((s) => s.setTool);
  const setBrush = useApp((s) => s.setBrush);
  const startRemoval = useApp((s) => s.startRemoval);
  const clearRemoveStrokes = useApp((s) => s.clearRemoveStrokes);
  const setLive = useApp((s) => s.setLive);
  const commitLive = useApp((s) => s.commitLive);
  const live = useApp((s) => s.live);
  return (
    <div className="ai-tool">
      <h3>{t("remove.title")}</h3>
      <div className="button-row">
        <button
          type="button"
          className={active ? "ghost small pressed" : "ghost small"}
          aria-pressed={active}
          onClick={() => setTool(active ? "none" : "remove")}
        >
          {t("remove.paint")}
        </button>
        <button type="button" className="primary small" disabled={painted === 0 || running} onClick={() => void startRemoval()}>
          {t("remove.run")}
        </button>
        <button type="button" className="ghost small" disabled={painted === 0 || running} onClick={clearRemoveStrokes}>
          {t("remove.clear")}
        </button>
      </div>
      {active && (
        <>
          <p className="note">{t("remove.hint")}</p>
          <div className="slider-row">
            <div className="slider-label">
              <label htmlFor="remove-size">{t("brush.size")}</label>
              <output htmlFor="remove-size">{(brush.size * 100).toFixed(1)}%</output>
            </div>
            <input
              id="remove-size"
              type="range"
              min={0.5}
              max={20}
              step={0.5}
              value={brush.size * 100}
              onChange={(event) => setBrush({ size: Number(event.target.value) / 100 })}
            />
          </div>
        </>
      )}
      <TaskStatus kind="remove" />
      {removals > 0 && (
        <button
          type="button"
          className="ghost small"
          onClick={() => {
            setLive({ ...live, removals: [] });
            void commitLive();
          }}
        >
          {t("remove.undoAll", { count: removals })}
        </button>
      )}
    </div>
  );
}

/** Old-photo repair: dust and scratches filled, faded colour restored. */
function RepairTool() {
  const { t } = useI18n();
  const running = useApp((s) => s.aiTask?.state === "running");
  const startRepair = useApp((s) => s.startRepair);
  const [strength, setStrength] = useState(50);
  return (
    <div className="ai-tool">
      <h3>{t("repair.title")}</h3>
      <p className="note">{t("repair.hint")}</p>
      <div className="slider-row">
        <div className="slider-label">
          <label htmlFor="repair-strength">{t("repair.strength")}</label>
          <output htmlFor="repair-strength">{strength}</output>
        </div>
        <input
          id="repair-strength"
          type="range"
          min={0}
          max={100}
          step={1}
          value={strength}
          onChange={(event) => setStrength(Number(event.target.value))}
        />
      </div>
      <button type="button" className="primary small" disabled={running} onClick={() => void startRepair(strength)}>
        {t("repair.run")}
      </button>
      <TaskStatus kind="repair" />
    </div>
  );
}

/** AI repair tools: object removal and old-photo repair. */
export function AiRepairSection() {
  const { t } = useI18n();
  return (
    <section aria-labelledby="ai-repair-title">
      <h2 id="ai-repair-title">{t("aiRepair.title")}</h2>
      <RemoveTool />
      <RepairTool />
    </section>
  );
}
