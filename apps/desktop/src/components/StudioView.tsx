import { useState } from "react";
import { useApp, useDiscardPrompt, useI18n } from "../context";
import { errorMessage, type MessageKey } from "../i18n";
import type { Expansion } from "../types";

const EXPANSIONS: { key: string; value: Expansion }[] = [
  { key: "sides", value: { left: 0.25, top: 0, right: 0.25, bottom: 0 } },
  { key: "topBottom", value: { left: 0, top: 0.25, right: 0, bottom: 0.25 } },
  { key: "all", value: { left: 0.2, top: 0.2, right: 0.2, bottom: 0.2 } },
];

/** One AI task: what it does and a button that does it. */
function Task(props: { title: string; text: string; action: string; disabled?: boolean; onRun: () => void }) {
  return (
    <div className="studio-card">
      <h2>{props.title}</h2>
      <p className="note">{props.text}</p>
      <button type="button" className="primary small" disabled={props.disabled} onClick={props.onRun}>
        {props.action}
      </button>
    </div>
  );
}

/** AI Studio: one-click AI tasks for the open photo, and batch editing. */
export function StudioView() {
  const { t, language } = useI18n();
  const document = useApp((s) => s.document);
  const aiTask = useApp((s) => s.aiTask);
  const pickAndOpen = useApp((s) => s.pickAndOpen);
  const portraitPreset = useApp((s) => s.portraitPreset);
  const startCutout = useApp((s) => s.startCutout);
  const startRepair = useApp((s) => s.startRepair);
  const startExpand = useApp((s) => s.startExpand);
  const requestExport = useApp((s) => s.requestExport);
  const showView = useApp((s) => s.showView);
  const setTool = useApp((s) => s.setTool);
  const cancelAiTask = useApp((s) => s.cancelAiTask);
  const discard = useDiscardPrompt("open");
  const [expansion, setExpansion] = useState("sides");
  const running = aiTask?.state === "running";
  const none = !document;

  return (
    <main className="studio">
      <div className="studio-head">
        <h1>{t("studio.title")}</h1>
        <p className="note">{document ? t("studio.photo", { name: document.info?.name ?? "" }) : t("studio.noPhoto")}</p>
        <button type="button" className="ghost small" onClick={() => void pickAndOpen(t("imageFiles"), discard).then(() => showView("studio"))}>
          {t("studio.pick")}
        </button>
      </div>
      {aiTask?.kind === "expand" && aiTask.state === "running" && (
        <div className="separation" role="status">
          <p className="note">{t("ai.working", { percent: Math.round(aiTask.fraction * 100) })}</p>
          <progress max={1} value={aiTask.fraction} />
          <button type="button" className="ghost small" onClick={() => void cancelAiTask()} disabled={aiTask.cancelling}>
            {t("ai.cancel")}
          </button>
        </div>
      )}
      {aiTask?.kind === "expand" && aiTask.state === "failed" && (
        <p className="note warning" role="alert">
          {errorMessage(language, aiTask.code)}
        </p>
      )}
      <div className="studio-grid">
        <Task
          title={t("studio.portrait")}
          text={t("studio.portraitText")}
          action={t("studio.run")}
          disabled={none}
          onRun={() => void portraitPreset()}
        />
        <Task
          title={t("studio.cutout")}
          text={t("studio.cutoutText")}
          action={t("studio.run")}
          disabled={none}
          onRun={() => void startCutout()}
        />
        <Task
          title={t("studio.remove")}
          text={t("studio.removeText")}
          action={t("studio.open")}
          disabled={none}
          onRun={() => {
            showView("editor");
            setTool("remove");
          }}
        />
        <Task
          title={t("studio.repair")}
          text={t("studio.repairText")}
          action={t("studio.run")}
          disabled={none || running}
          onRun={() => {
            showView("editor");
            void startRepair(60);
          }}
        />
        <div className="studio-card">
          <h2>{t("studio.expand")}</h2>
          <p className="note">{t("studio.expandText")}</p>
          <div className="segmented small" role="radiogroup" aria-label={t("studio.expand")}>
            {EXPANSIONS.map(({ key }) => (
              <button
                key={key}
                type="button"
                role="radio"
                aria-checked={expansion === key}
                className={expansion === key ? "active" : ""}
                onClick={() => setExpansion(key)}
              >
                {t(`studio.expand.${key}` as MessageKey)}
              </button>
            ))}
          </div>
          <button
            type="button"
            className="primary small"
            disabled={none || running}
            onClick={() => void startExpand(EXPANSIONS.find((e) => e.key === expansion)!.value, t("studio.pngFiles"))}
          >
            {t("studio.run")}
          </button>
        </div>
        <Task
          title={t("studio.upscale")}
          text={t("studio.upscaleText")}
          action={t("studio.open")}
          disabled={none}
          onRun={() => {
            showView("editor");
            requestExport({ upscale: 2, longSide: null });
          }}
        />
        <Task
          title={t("studio.batch")}
          text={t("studio.batchText")}
          action={t("studio.open")}
          onRun={() => showView("batch")}
        />
      </div>
    </main>
  );
}
