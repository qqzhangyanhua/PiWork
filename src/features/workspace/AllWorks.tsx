import { Layers3, RotateCcw } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import type { WorkSummary } from "../../bindings";
import { visibleWorks, workGroup, WorkListRow } from "./WorkList";

const groups = ["active", "open", "attention", "completed", "archived"] as const;
const views = ["all", "active", "completed", "archived"] as const;
type WorkView = (typeof views)[number];

export function AllWorks({
  works,
  onWorkSelected,
  onWorkRestore,
}: {
  works: WorkSummary[];
  onWorkSelected(work: WorkSummary): void;
  onWorkRestore(work: WorkSummary): void;
}) {
  const { t } = useTranslation();
  const [view, setView] = useState<WorkView>("all");
  const sorted = view === "archived"
    ? [...works].filter((work) => work.status === "archived").sort((left, right) => right.updatedAt.localeCompare(left.updatedAt))
    : visibleWorks(works);
  const visible = sorted.filter((work) => {
    if (view === "all") return true;
    if (view === "archived") return work.status === "archived";
    if (view === "completed") return workGroup(work.status) === "completed";
    return workGroup(work.status) === "active";
  });

  return (
    <section aria-labelledby="all-works-heading" className="all-works">
      <header className="all-works__header">
        <Layers3 aria-hidden="true" size={16} />
        <h1 id="all-works-heading">{t("workspace.allWorks")}</h1>
      </header>
      <div aria-label={t("workspace.views.label")} className="all-works__views" role="tablist">
        {views.map((item) => (
          <button
            aria-selected={view === item}
            key={item}
            onClick={() => setView(item)}
            role="tab"
            tabIndex={view === item ? 0 : -1}
            type="button"
          >
            {t(`workspace.views.${item}`)}
          </button>
        ))}
        <span className="all-works__count">{t("workspace.workCount", { count: visible.length })}</span>
      </div>
      <div className="all-works__groups">
        {groups.map((group) => {
          const groupWorks = visible.filter((work) => workGroup(work.status) === group);
          if (!groupWorks.length) return null;
          return (
            <section aria-labelledby={`work-group-${group}`} className="work-group" key={group}>
              <header className="work-group__header">
                <h2 id={`work-group-${group}`}>{t(`workspace.groups.${group}`)}</h2>
                <span>{groupWorks.length}</span>
              </header>
              <div className="work-list">
                {groupWorks.map((work) => (
                  <div className="work-list-item" key={work.id}>
                    <WorkListRow onSelect={onWorkSelected} work={work} />
                    {work.status === "archived" && (
                      <button
                        aria-label={t("conversation.restore", { title: work.title })}
                        className="icon-button work-list-item__restore"
                        onClick={() => onWorkRestore(work)}
                        title={t("conversation.restoreAction")}
                        type="button"
                      ><RotateCcw aria-hidden="true" size={15} /></button>
                    )}
                  </div>
                ))}
              </div>
            </section>
          );
        })}
        {!visible.length && <p className="work-collection-empty">{t("workspace.noWorks")}</p>}
      </div>
    </section>
  );
}
