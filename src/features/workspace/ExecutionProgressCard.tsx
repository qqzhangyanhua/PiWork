import {
  Check,
  ChevronDown,
  CircleAlert,
  Clock3,
  LoaderCircle,
} from "lucide-react";
import { useId, useState, type CSSProperties, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

import type { WorkEventEnvelope } from "../../bindings";
import { describeTool } from "../activity/activityPresentation";
import {
  buildExecutionProgress,
  type ExecutionProgressModel,
} from "./executionProgress";

export type ExecutionProgressCardProps = {
  events: WorkEventEnvelope[];
  children?: ReactNode;
};

const summaryKey = (progress: ExecutionProgressModel) => {
  if (progress.status === "preparing") return "progress.preparing";
  if (progress.status === "running") return "progress.running";
  if (progress.status === "waiting") {
    return progress.waitingReason === "waiting_on_assignments"
      ? "progress.waitingOnAssignments"
      : "progress.waiting";
  }
  if (progress.status === "failed") return "progress.failed";
  if (progress.toolCount === 0) return "progress.completedWithoutTools";
  return "progress.completed";
};

export function ExecutionProgressCard({
  events,
  children,
}: ExecutionProgressCardProps) {
  const { t } = useTranslation();
  const progress = buildExecutionProgress(events);
  const detailsId = useId();
  const [expanded, setExpanded] = useState(
    () => progress.status !== "completed",
  );

  const sortedEvents = [...events].sort(
    (left, right) => left.sequence - right.sequence,
  );
  const toolInputById = new Map<string, { name: string; summary: string }>();
  for (const event of sortedEvents) {
    if (
      event.payload.type === "toolPending" ||
      event.payload.type === "toolStarted"
    ) {
      toolInputById.set(event.payload.toolCallId, {
        name: event.payload.toolName,
        summary: event.payload.inputSummary,
      });
    }
  }

  const currentDetail = (() => {
    if (progress.status === "completed") return t("progress.current.completed");
    if (progress.status === "failed") return t("progress.current.failed");
    if (progress.status === "waiting") {
      return t(
        progress.waitingReason === "waiting_on_assignments"
          ? "progress.current.waitingOnAssignments"
          : "progress.current.waiting",
      );
    }
    if (progress.status === "preparing") return t("progress.current.preparing");

    for (let index = sortedEvents.length - 1; index >= 0; index -= 1) {
      const payload = sortedEvents[index]?.payload;
      if (!payload) continue;
      if (payload.type === "toolFinished") {
        return t("progress.current.continuing");
      }
      if (
        payload.type === "toolPending" ||
        payload.type === "toolStarted" ||
        payload.type === "toolProgress"
      ) {
        const knownInput = toolInputById.get(payload.toolCallId);
        const descriptor = describeTool(
          knownInput?.name ?? payload.toolName,
          knownInput?.summary ?? (
            payload.type === "toolProgress"
              ? payload.outputSummary
              : payload.inputSummary
          ),
        );
        return t("progress.current.action", {
          action: t(`activity.actions.${descriptor.action}`),
          object: descriptor.object ?? payload.toolName,
        });
      }
      if (payload.type === "assistantDelta") {
        return t("progress.current.composing");
      }
      if (payload.type === "thoughtDelta") {
        return t("progress.current.thinking");
      }
      if (payload.type === "planChanged" || payload.type === "workPlanUpdated") {
        return t("progress.current.planning");
      }
      if (payload.type === "permissionRequested") {
        return t("progress.current.permission");
      }
      if (payload.type.startsWith("assignment") || payload.type === "leadResumed") {
        return t("progress.current.coordinating");
      }
    }
    return t(`progress.phase.${progress.currentPhase}`);
  })();

  const Icon =
    progress.status === "completed"
      ? Check
      : progress.status === "failed"
        ? CircleAlert
        : progress.status === "waiting"
          ? Clock3
        : LoaderCircle;
  const summary = t(summaryKey(progress), { count: progress.toolCount });
  const summaryContent = (
    <>
      <span className="execution-progress__icon" data-progress-icon>
        <Icon aria-hidden="true" />
      </span>
      <span className="execution-progress__headline">
        <strong>{summary}</strong>
        <small>{currentDetail}</small>
      </span>
      {progress.failedToolCount > 0 && progress.status === "completed" ? (
        <span className="execution-progress__recovered">
          {t("progress.recovered", { count: progress.failedToolCount })}
        </span>
      ) : null}
      {children ? (
        <ChevronDown
          aria-hidden="true"
          className="execution-progress__chevron"
        />
      ) : null}
    </>
  );

  return (
    <article
      aria-label={t("progress.label")}
      aria-live={
        progress.status === "preparing" || progress.status === "running"
          ? "polite"
          : undefined
      }
      className="execution-progress"
      data-status={progress.status}
      role="status"
    >
      {children ? (
        <button
          aria-controls={detailsId}
          aria-expanded={expanded}
          aria-label={t(expanded ? "progress.collapse" : "progress.expand")}
          className="execution-progress__summary"
          onClick={() => setExpanded((current) => !current)}
          type="button"
        >
          {summaryContent}
        </button>
      ) : (
        <div className="execution-progress__summary">
          {summaryContent}
        </div>
      )}
      <div className="execution-progress__overview">
        <ol aria-label={t("progress.phases")} className="execution-progress__phases">
          {progress.phases.map((phase, index) => (
            <li
              className="execution-progress__phase"
              data-current={phase.id === progress.currentPhase ? "true" : undefined}
              data-status={phase.status}
              data-testid={`execution-phase:${phase.id}`}
              key={phase.id}
              style={{ "--phase-index": index } as CSSProperties}
            >
              <span className="execution-progress__phase-marker" aria-hidden="true">
                {phase.status === "completed" ? <Check /> : null}
                {phase.status === "failed" ? <CircleAlert /> : null}
                {phase.status !== "completed" && phase.status !== "failed" ? (
                  <span className="execution-progress__dot" />
                ) : null}
              </span>
              <span className="execution-progress__phase-label">
                {t(`progress.phase.${phase.id}`)}
              </span>
              {phase.toolCount > 0 ? (
                <span
                  aria-label={t("progress.operationCount", { count: phase.toolCount })}
                  className="execution-progress__phase-count"
                >
                  {phase.toolCount}
                </span>
              ) : null}
              <span className="sr-only">{t(`progress.status.${phase.status}`)}</span>
            </li>
          ))}
        </ol>
      </div>
      {expanded && children ? (
        <div className="execution-progress__details" id={detailsId}>
          <div className="execution-progress__activity">{children}</div>
        </div>
      ) : null}
    </article>
  );
}
