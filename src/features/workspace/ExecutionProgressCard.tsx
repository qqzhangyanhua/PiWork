import {
  Check,
  ChevronDown,
  CircleAlert,
  LoaderCircle,
} from "lucide-react";
import { useId, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

import type { WorkEventEnvelope } from "../../bindings";
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
  const [expanded, setExpanded] = useState(false);

  const Icon =
    progress.status === "completed"
      ? Check
      : progress.status === "failed"
        ? CircleAlert
        : LoaderCircle;
  const summary = t(summaryKey(progress), { count: progress.toolCount });

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
      <button
        aria-controls={detailsId}
        aria-expanded={expanded}
        aria-label={t(expanded ? "progress.collapse" : "progress.expand")}
        className="execution-progress__summary"
        onClick={() => setExpanded((current) => !current)}
        type="button"
      >
        <span className="execution-progress__icon" data-progress-icon>
          <Icon aria-hidden="true" />
        </span>
        <span className="execution-progress__headline">
          <strong>{summary}</strong>
          {progress.failedToolCount > 0 && progress.status === "completed" ? (
            <small>
              {t("progress.recovered", { count: progress.failedToolCount })}
            </small>
          ) : null}
        </span>
        <ChevronDown
          aria-hidden="true"
          className="execution-progress__chevron"
        />
      </button>
      {expanded && children ? (
        <div className="execution-progress__details" id={detailsId}>
          <div className="execution-progress__activity">{children}</div>
        </div>
      ) : null}
    </article>
  );
}
