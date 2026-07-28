import { PanelRightOpen } from "lucide-react";
import type { Ref } from "react";
import { useTranslation } from "react-i18next";

import type { TimelineItem, AppError } from "../../domain/work";
import type { RunSummary, WorkSummary } from "../../bindings";

type WorkHeaderProps = {
  work: WorkSummary;
  timeline: TimelineItem[];
  error: AppError | null;
  latestRun?: RunSummary;
  inspectorOpen: boolean;
  inspectorToggleRef: Ref<HTMLButtonElement>;
  onInspectorToggle(): void;
};

export function WorkHeader({ work, timeline, error, latestRun, inspectorOpen, inspectorToggleRef, onInspectorToggle }: WorkHeaderProps) {
  const { t } = useTranslation();
  const model = [...timeline]
    .reverse()
    .find(({ payload }) => payload.type === "runStarted")?.payload;
  const modelLabel = latestRun?.modelLabel ?? (model?.type === "runStarted" ? model.modelLabel : "Fake model");

  return (
    <div className="work-header-region">
      {error && <div className="workspace-banner" role="status">{error.message}</div>}
      <header className="work-header">
        <div className="work-header__identity">
          <h1>{work.title}</h1>
          <span title={work.rootPath}>{work.rootPath}</span>
        </div>
        <div className="work-header__meta">
          <span className="model-label">{modelLabel}</span>
          <span
            aria-label={t("header.workStatus")}
            className={`status-badge status-badge--${work.status}`}
            role="status"
          >
            {t(`status.${work.status}`)}
          </span>
          <button
            className="icon-button work-header__inspector-toggle"
            type="button"
            aria-label={t(inspectorOpen ? "inspector.close" : "inspector.open")}
            aria-expanded={inspectorOpen}
            onClick={onInspectorToggle}
            ref={inspectorToggleRef}
          >
            <PanelRightOpen aria-hidden="true" size={18} />
          </button>
        </div>
      </header>
    </div>
  );
}
