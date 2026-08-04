import { Layers3 } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import type { WorkSummary } from "../../bindings";
import { visibleWorks, workGroup, WorkListRow } from "./WorkList";

const groups = ["active", "open", "attention", "completed"] as const;
const views = ["all", "active", "completed"] as const;
type WorkView = (typeof views)[number];

export function AllWorks({
  works,
  onWorkSelected,
}: {
  works: WorkSummary[];
  onWorkSelected(work: WorkSummary): void;
}) {
  const { t } = useTranslation();
  const sorted = visibleWorks(works);
  const [view, setView] = useState<WorkView>("all");
  const visible = sorted.filter((work) => {
    if (view === "all") return true;
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
                  <WorkListRow key={work.id} onSelect={onWorkSelected} work={work} />
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
