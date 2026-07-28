import { useRef, useState, type KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";

import type { TimelineItem } from "../../domain/work";

const tabs = ["progress", "changes", "artifacts", "logs"] as const;
type InspectorTab = (typeof tabs)[number];

export function WorkInspector({ timeline, open }: { timeline: TimelineItem[]; open: boolean }) {
  const { t } = useTranslation();
  const [active, setActive] = useState<InspectorTab>("progress");
  const refs = useRef<Array<HTMLButtonElement | null>>([]);
  const toolCount = timeline.filter(({ payload }) => payload.type === "toolStarted").length;
  const completed = timeline.some(({ payload }) => payload.type === "runCompleted");
  const activateRelative = (event: KeyboardEvent, index: number) => {
    if (event.key !== "ArrowRight" && event.key !== "ArrowLeft") return;
    event.preventDefault();
    const direction = event.key === "ArrowRight" ? 1 : -1;
    const next = (index + direction + tabs.length) % tabs.length;
    const nextTab = tabs[next];
    if (!nextTab) return;
    setActive(nextTab);
    refs.current[next]?.focus();
  };

  const content = () => {
    if (active === "progress") return timeline.length ? <ul className="inspector-list"><li>{t("inspector.runEvents", { count: timeline.length })}</li><li>{t("inspector.toolEvents", { count: toolCount })}</li>{completed && <li>{t("status.completed")}</li>}</ul> : <p className="inspector-empty">{t("inspector.noProgress")}</p>;
    const emptyKey = active === "changes"
      ? "inspector.noChanges"
      : active === "artifacts"
        ? "inspector.noArtifacts"
        : "inspector.noLogs";
    return <p className="inspector-empty">{t(emptyKey)}</p>;
  };

  return (
    <aside className="work-inspector" data-open={open} aria-label={t("inspector.label")}>
      <div className="work-inspector__tabs" role="tablist" aria-label={t("inspector.label")}>
        {tabs.map((tab, index) => (
          <button
            aria-controls={`inspector-panel-${tab}`}
            aria-selected={active === tab}
            id={`inspector-tab-${tab}`}
            key={tab}
            onClick={() => setActive(tab)}
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
      <div className="work-inspector__panel" role="tabpanel" id={`inspector-panel-${active}`} aria-labelledby={`inspector-tab-${active}`}>{content()}</div>
    </aside>
  );
}
