import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";

import { isWorkEventTimelineItem, type TimelineItem } from "../../domain/work";

const tabs = ["progress", "changes", "artifacts", "logs"] as const;
type InspectorTab = (typeof tabs)[number];

const compactInspectorQuery = "(max-width: 1099px)";

const useCompactInspector = () => {
  const [compact, setCompact] = useState(
    () => typeof matchMedia === "function" && matchMedia(compactInspectorQuery).matches,
  );
  useEffect(() => {
    if (typeof matchMedia !== "function") return;
    const query = matchMedia(compactInspectorQuery);
    const update = () => setCompact(query.matches);
    update();
    query.addEventListener("change", update);
    return () => query.removeEventListener("change", update);
  }, []);
  return compact;
};

export function WorkInspector({ timeline, open, onClose }: { timeline: TimelineItem[]; open: boolean; onClose(): void }) {
  const { t } = useTranslation();
  const compact = useCompactInspector();
  const [active, setActive] = useState<InspectorTab>("progress");
  const refs = useRef<Array<HTMLButtonElement | null>>([]);
  const events = timeline.filter(isWorkEventTimelineItem);
  const toolCount = events.filter(({ payload }) => payload.type === "toolStarted").length;
  const completed = events.some(({ payload }) => payload.type === "runCompleted");
  const hidden = compact && !open;
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
    if (active === "progress") return timeline.length ? <ul className="inspector-list"><li>{t("inspector.runEvents", { count: events.length })}</li><li>{t("inspector.toolEvents", { count: toolCount })}</li>{completed && <li>{t("status.completed")}</li>}</ul> : <p className="inspector-empty">{t("inspector.noProgress")}</p>;
    const emptyKey = active === "changes"
      ? "inspector.noChanges"
      : active === "artifacts"
        ? "inspector.noArtifacts"
        : "inspector.noLogs";
    return <p className="inspector-empty">{t(emptyKey)}</p>;
  };

  return (
    <aside
      aria-hidden={hidden}
      aria-label={t("inspector.label")}
      className="work-inspector"
      data-open={open}
      inert={hidden}
      onKeyDownCapture={(event) => {
        if (event.key === "Escape" && open) {
          event.preventDefault();
          onClose();
        }
      }}
    >
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
