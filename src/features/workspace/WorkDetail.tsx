import type { CSSProperties, PointerEvent, Ref } from "react";
import { useTranslation } from "react-i18next";

import type { PickAttachments } from "../../app/attachmentPicker";
import type { ResourceSummary, WorkSummary } from "../../bindings";
import type { AppError, TimelineItem } from "../../domain/work";
import { WorkComposer } from "./WorkComposer";
import { WorkHeader } from "./WorkHeader";
import { WorkInspector, type InspectorTab } from "./WorkInspector";
import { WorkTimeline } from "./WorkTimeline";
import type { DetailExperience } from "./detailExperience";

type WorkDetailProps = {
  composerRef: Ref<HTMLDivElement>;
  error: AppError | null;
  experience: DetailExperience;
  inspectorOpen: boolean;
  inspectorPercent: number;
  inspectorTab: InspectorTab;
  inspectorToggleRef: Ref<HTMLButtonElement>;
  modelLabel: string;
  pickAttachments: PickAttachments;
  resources: ResourceSummary[];
  timeline: TimelineItem[];
  work: WorkSummary;
  workspaceRef: Ref<HTMLElement>;
  onExperienceChange(experience: DetailExperience): void;
  onInspectorClose(): void;
  onInspectorOpenDiagnostics(): void;
  onInspectorResizeReset(): void;
  onInspectorResizeStart(event: PointerEvent<HTMLDivElement>): void;
  onInspectorTabChange(tab: InspectorTab): void;
  onInspectorToggle(): void;
};

export function WorkDetail({
  composerRef,
  error,
  experience,
  inspectorOpen,
  inspectorPercent,
  inspectorTab,
  inspectorToggleRef,
  modelLabel,
  pickAttachments,
  resources,
  timeline,
  work,
  workspaceRef,
  onExperienceChange,
  onInspectorClose,
  onInspectorOpenDiagnostics,
  onInspectorResizeReset,
  onInspectorResizeStart,
  onInspectorTabChange,
  onInspectorToggle,
}: WorkDetailProps) {
  const { t } = useTranslation();

  return (
    <section
      className={`workspace-main${experience === "dashboard" ? " workspace-main--dashboard" : ""}`}
      data-detail-experience={experience}
      data-inspector-open={inspectorOpen}
      ref={workspaceRef}
      style={{ "--inspector-width": `${inspectorPercent}%` } as CSSProperties}
    >
      <WorkHeader
        experience={experience}
        inspectorOpen={inspectorOpen}
        inspectorToggleRef={inspectorToggleRef}
        modelLabel={modelLabel}
        work={work}
        onExperienceChange={onExperienceChange}
        onInspectorToggle={onInspectorToggle}
      />
      <div className="workspace-center">
        <WorkTimeline
          error={error}
          resources={resources}
          timeline={timeline}
          work={work}
          onOpenDiagnostics={onInspectorOpenDiagnostics}
        />
        <WorkComposer
          modelLabel={modelLabel}
          pickAttachments={pickAttachments}
          promptRef={composerRef}
          resources={resources}
          work={work}
        />
      </div>
      <WorkInspector
        active={inspectorTab}
        error={error}
        open={inspectorOpen}
        resources={resources}
        timeline={timeline}
        widthPercent={inspectorPercent}
        onActiveChange={onInspectorTabChange}
        onClose={onInspectorClose}
        onResizeReset={onInspectorResizeReset}
        onResizeStart={onInspectorResizeStart}
      />
      {inspectorOpen && (
        <button
          aria-label={t("inspector.close")}
          className="inspector-scrim"
          onClick={onInspectorClose}
          type="button"
        />
      )}
    </section>
  );
}
