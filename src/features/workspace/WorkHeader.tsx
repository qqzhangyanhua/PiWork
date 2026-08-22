import { Archive, ChevronRight, CircleAlert, CircleCheck, CircleDashed, Folder, LoaderCircle, PanelRightOpen } from "lucide-react";
import { type Ref } from "react";
import { useTranslation } from "react-i18next";

import type { WorkSummary } from "../../bindings";
import { CoDoLogo } from "../../components/brand/CoDoLogo";
import type { DetailExperience } from "./detailExperience";

type WorkHeaderProps = {
  work: WorkSummary;
  modelLabel: string;
  inspectorOpen: boolean;
  inspectorToggleRef: Ref<HTMLButtonElement>;
  experience: DetailExperience;
  onExperienceChange(experience: DetailExperience): void;
  onArchive(): void;
  onInspectorToggle(): void;
};

export function WorkHeader({ work, modelLabel, inspectorOpen, inspectorToggleRef, experience, onExperienceChange, onArchive, onInspectorToggle }: WorkHeaderProps) {
  const { t } = useTranslation();
  const projectName = work.rootPath.split(/[\\/]/).filter(Boolean).at(-1) ?? work.rootPath;
  const StatusIcon = work.status === "running" || work.status === "queued"
    ? LoaderCircle
    : work.status === "completed" || work.status === "idle"
      ? CircleCheck
      : work.status === "failed" || work.status === "interrupted"
        ? CircleAlert
        : CircleDashed;

  return (
    <div className="work-header-region">
      <header className="work-header">
        <div className="work-header__identity">
          <div className="work-header__title-row">
            <span className="work-header__workspace-mark"><CoDoLogo size={16} /></span>
            <span className="work-header__workspace-name">CoDo</span>
            <ChevronRight aria-hidden="true" className="work-header__breadcrumb-chevron" size={13} />
            <span className="work-header__breadcrumb-project" title={work.rootPath}>{projectName}</span>
            <ChevronRight aria-hidden="true" className="work-header__breadcrumb-chevron" size={13} />
            <h1 className="work-header__title">{work.title}</h1>
          </div>
          <div className="work-header__context">
            <span className={`work-header__status work-header__status--${work.status}`}>
              <StatusIcon aria-hidden="true" size={14} />
              {t(`status.${work.status}`)}
            </span>
            <span className="work-header__project" title={work.rootPath}>
              <Folder aria-hidden="true" size={13} />
              <span className="work-header__project-name">{projectName}</span>
            </span>
            <span className="work-header__model">{modelLabel}</span>
            <span className="work-header__goal" title={work.goal}>{work.goal}</span>
          </div>
        </div>
        <div className="work-header__meta">
          <fieldset className="work-header__experience">
            <legend className="sr-only">{t("workspace.detailExperience")}</legend>
            {(["classic", "dashboard"] as const).map((value) => (
              <button
                aria-pressed={experience === value}
                key={value}
                onClick={() => onExperienceChange(value)}
                type="button"
              >
                {t(`workspace.detailExperienceOptions.${value}`)}
              </button>
            ))}
          </fieldset>
          <button
            aria-label={t("conversation.archive")}
            className="icon-button work-header__archive"
            disabled={["queued", "running", "waiting"].includes(work.status)}
            onClick={onArchive}
            title={t("conversation.archive")}
            type="button"
          >
            <Archive aria-hidden="true" size={17} />
          </button>
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
