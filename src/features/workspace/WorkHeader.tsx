import { Folder, MoreHorizontal, PanelRightOpen } from "lucide-react";
import { type Ref } from "react";
import { useTranslation } from "react-i18next";

import type { WorkSummary } from "../../bindings";

type WorkHeaderProps = {
  work: WorkSummary;
  inspectorOpen: boolean;
  inspectorToggleRef: Ref<HTMLButtonElement>;
  onInspectorToggle(): void;
};

export function WorkHeader({ work, inspectorOpen, inspectorToggleRef, onInspectorToggle }: WorkHeaderProps) {
  const { t } = useTranslation();

  return (
    <div className="work-header-region">
      <header className="work-header">
        <div className="work-header__identity">
          <h1>{work.title}</h1>
          <span className="work-header__project" title={work.rootPath}>
            <Folder aria-hidden="true" size={13} />
            {t("header.project")}
          </span>
        </div>
        <div className="work-header__meta">
          <button className="icon-button" type="button" aria-label={t("header.more")}>
            <MoreHorizontal aria-hidden="true" size={18} />
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
