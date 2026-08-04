import { GripVertical, X } from "lucide-react";
import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { useTranslation } from "react-i18next";

import { formatAppErrorDiagnostics } from "../../domain/appError";
import { isWorkEventTimelineItem, type AppError, type TimelineItem } from "../../domain/work";
import type { ResourceSummary } from "../../bindings";
import { AttachmentChips } from "./AttachmentChips";

const tabs = ["delivery", "attachments", "validation", "logs"] as const;
export type InspectorTab = (typeof tabs)[number];

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
  const refs = useRef<Array<HTMLButtonElement | null>>([]);
  const inspectorRef = useRef<HTMLElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  const [modal, setModal] = useState(() =>
    typeof window !== "undefined" && typeof window.matchMedia === "function"
      ? window.matchMedia("(max-width: 1150px)").matches
      : false,
  );
  const events = timeline.filter(isWorkEventTimelineItem);
  const completions = events.filter(
    (event): event is typeof event & { payload: Extract<typeof event.payload, { type: "runCompleted" }> } =>
      event.payload.type === "runCompleted",
  );
  const hidden = !open;
  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const query = window.matchMedia("(max-width: 1150px)");
    const update = () => setModal(query.matches);
    update();
    query.addEventListener?.("change", update);
    return () => query.removeEventListener?.("change", update);
  }, []);
  useEffect(() => {
    if (!open || !modal) return;
    queueMicrotask(() => closeRef.current?.focus());
  }, [modal, open]);
  useEffect(() => {
    if (!open || !modal) return;
    const closeOnEscape = (event: globalThis.KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      onClose();
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [modal, onClose, open]);
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
    if (active === "delivery") {
      if (!completions.length) return <p className="inspector-empty">{t("inspector.noDeliveries")}</p>;
      const artifacts = Array.from(new Set(completions.flatMap(({ payload }) => payload.artifacts)));
      if (!artifacts.length) return <p className="inspector-empty">{t("inspector.noArtifacts")}</p>;
      return <section aria-label={t("inspector.artifacts")} className="inspector-preview"><ul className="inspector-files">{artifacts.map((artifact) => <li key={artifact}>{artifact}</li>)}</ul></section>;
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
      const assistantRuns = new Set<string>();
      const logEvents = events.filter(({ payload, runId }) => {
        if (payload.type !== "assistantDelta") return true;
        if (assistantRuns.has(runId)) return false;
        assistantRuns.add(runId);
        return true;
      });
      return <div className="inspector-logs">{error && <pre className="diagnostics">{formatAppErrorDiagnostics(error, t("diagnostics.unavailable"))}</pre>}{logEvents.length ? <ol className="inspector-log">{logEvents.map(({ payload, sequence, runId }) => {
        const description = payload.type === "toolStarted"
          ? payload.inputSummary
          : payload.type === "toolFinished"
            ? payload.outputSummary
            : payload.type === "runFailed"
              ? payload.message
              : payload.type === "runCompleted"
                ? payload.summary
                : payload.type === "runStarted"
                  ? payload.modelLabel
                  : t("inspector.logOutputSummary");
        return <li key={`${runId}:${sequence}`}><span>{t(`inspector.logEvents.${payload.type}`)}</span><p>{description}</p></li>;
      })}</ol> : !error && <p className="inspector-empty">{t("inspector.noLogs")}</p>}</div>;
    }
    return null;
  };

  return (
    <aside
      aria-hidden={hidden}
      aria-label={t("inspector.label")}
      aria-modal={modal ? true : undefined}
      className="work-inspector"
      data-open={open}
      inert={hidden}
      ref={inspectorRef}
      role={modal ? "dialog" : undefined}
      onKeyDownCapture={(event) => {
        if (event.key === "Escape" && open) {
          event.preventDefault();
          onClose();
          return;
        }
        if (event.key === "Tab" && open && modal) {
          const focusable = Array.from(inspectorRef.current?.querySelectorAll<HTMLElement>(
            'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
          ) ?? []);
          const first = focusable[0];
          const last = focusable.at(-1);
          if (!first || !last) return;
          const active = document.activeElement;
          if (event.shiftKey && (active === first || !inspectorRef.current?.contains(active))) {
            event.preventDefault();
            last.focus();
          } else if (!event.shiftKey && (active === last || !inspectorRef.current?.contains(active))) {
            event.preventDefault();
            first.focus();
          }
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
        <div className="work-inspector__heading"><strong>{t("inspector.title")}</strong></div>
        <button className="icon-button" ref={closeRef} type="button" aria-label={t("inspector.close")} onClick={onClose}>
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
