import { GripVertical, X } from "lucide-react";
import { useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { useTranslation } from "react-i18next";

import { formatAppErrorDiagnostics } from "../../domain/appError";
import { isWorkEventTimelineItem, type AppError, type TimelineItem } from "../../domain/work";
import type { ResourceSummary } from "../../bindings";
import { AttachmentChips } from "./AttachmentChips";

const tabs = ["preview", "attachments", "changes", "validation", "logs"] as const;
export type InspectorTab = (typeof tabs)[number];
type InspectorScope = "current" | "all";

export function WorkInspector({
  timeline,
  resources,
  open,
  widthPercent,
  active,
  error,
  onActiveChange,
  onClose,
  onResizeReset,
  onResizeStart,
}: {
  timeline: TimelineItem[];
  resources: ResourceSummary[];
  open: boolean;
  widthPercent: number;
  active: InspectorTab;
  error: AppError | null;
  onActiveChange(tab: InspectorTab): void;
  onClose(): void;
  onResizeReset(): void;
  onResizeStart(event: PointerEvent<HTMLDivElement>): void;
}) {
  const { t } = useTranslation();
  const [scope, setScope] = useState<InspectorScope>("current");
  const refs = useRef<Array<HTMLButtonElement | null>>([]);
  const latestRunId = timeline.at(-1)?.runId;
  const allEvents = timeline.filter(isWorkEventTimelineItem);
  const events = scope === "all"
    ? allEvents
    : allEvents.filter((event) => !latestRunId || event.runId === latestRunId);
  const completions = events.filter(
    (event): event is typeof event & { payload: Extract<typeof event.payload, { type: "runCompleted" }> } =>
      event.payload.type === "runCompleted",
  );
  const hidden = !open;
  const activateRelative = (event: KeyboardEvent, index: number) => {
    if (event.key !== "ArrowRight" && event.key !== "ArrowLeft") return;
    event.preventDefault();
    const direction = event.key === "ArrowRight" ? 1 : -1;
    const next = (index + direction + tabs.length) % tabs.length;
    const nextTab = tabs[next];
    if (!nextTab) return;
    onActiveChange(nextTab);
    refs.current[next]?.focus();
  };

  const content = () => {
    if (active === "preview") {
      if (!completions.length) return <p className="inspector-empty">{t("inspector.noArtifacts")}</p>;
      return <div className="inspector-previews">{completions.map(({ payload, runId, sequence }) => <article className="inspector-preview" key={`${runId}:${sequence}`}><h2>{payload.summary}</h2>{payload.artifacts.length > 0 ? <ul className="inspector-files">{payload.artifacts.map((artifact) => <li key={artifact}>{artifact}</li>)}</ul> : <p className="inspector-empty">{t("inspector.noArtifacts")}</p>}</article>)}</div>;
    }
    if (active === "attachments") {
      if (!resources.length) return <p className="inspector-empty">{t("inspector.noAttachments")}</p>;
      return <section className="inspector-attachments"><strong>{t("attachments.count", { count: resources.length })}</strong><AttachmentChips resources={resources} /></section>;
    }
    if (active === "validation") {
      const validations = completions.flatMap(({ payload, runId }) => payload.validation.map((item) => ({ item, runId })));
      return validations.length ? <ul className="inspector-list">{validations.map(({ item, runId }, index) => <li key={`${runId}:${index}`}>{item}</li>)}</ul> : <p className="inspector-empty">{t("inspector.noValidation")}</p>;
    }
    if (active === "logs") {
      return <div className="inspector-logs">{error && <pre className="diagnostics">{formatAppErrorDiagnostics(error, t("diagnostics.unavailable"))}</pre>}{events.length ? <ol className="inspector-log">{events.map(({ payload, sequence, runId }) => <li key={`${runId}:${sequence}`}><span>{payload.type}</span>{payload.type === "toolStarted" ? payload.inputSummary : payload.type === "toolFinished" ? payload.outputSummary : payload.type === "runFailed" ? payload.message : payload.type === "runCompleted" ? payload.summary : payload.type === "runStarted" ? payload.modelLabel : payload.text}</li>)}</ol> : !error && <p className="inspector-empty">{t("inspector.noLogs")}</p>}</div>;
    }
    return <p className="inspector-empty">{t("inspector.noChanges")}</p>;
  };

  return (
    <aside
      aria-hidden={hidden}
      aria-label={t("inspector.label")}
      className="work-inspector"
      data-open={open}
      inert={hidden}
      onKeyDownCapture={(event) => {
        if (event.key === "Escape" && open) {
          event.preventDefault();
          onClose();
        }
      }}
    >
      <div
        aria-label={t("inspector.resize")}
        aria-orientation="vertical"
        aria-valuemax={60}
        aria-valuemin={32}
        aria-valuenow={Math.round(widthPercent)}
        className="work-inspector__resize"
        onDoubleClick={onResizeReset}
        onPointerDown={onResizeStart}
        role="separator"
        tabIndex={open ? 0 : -1}
        title={t("inspector.resizeHint")}
      >
        <GripVertical aria-hidden="true" size={14} />
      </div>
      <header className="work-inspector__header">
        <div className="work-inspector__heading"><strong>{t("inspector.title")}</strong><div className="work-inspector__scope"><button aria-pressed={scope === "current"} onClick={() => setScope("current")} type="button">{t("inspector.currentRun")}</button><button aria-pressed={scope === "all"} onClick={() => setScope("all")} type="button">{t("inspector.allWork")}</button></div></div>
        <button className="icon-button" type="button" aria-label={t("inspector.close")} onClick={onClose}>
          <X aria-hidden="true" size={17} />
        </button>
      </header>
      <div className="work-inspector__tabs" role="tablist" aria-label={t("inspector.label")}>
        {tabs.map((tab, index) => (
          <button
            aria-controls={`inspector-panel-${tab}`}
            aria-selected={active === tab}
            id={`inspector-tab-${tab}`}
            key={tab}
            onClick={() => onActiveChange(tab)}
            onKeyDown={(event) => activateRelative(event, index)}
            ref={(node) => { refs.current[index] = node; }}
            role="tab"
            tabIndex={active === tab ? 0 : -1}
            type="button"
          >
            {t(`inspector.${tab}`)}
          </button>
        ))}
      </div>
      <div className="work-inspector__panel" role="tabpanel" id={`inspector-panel-${active}`} aria-labelledby={`inspector-tab-${active}`}>{open ? content() : null}</div>
    </aside>
  );
}
