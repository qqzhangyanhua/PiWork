import { Bot, CheckCircle2, CircleAlert, Play, UserRound, Wrench } from "lucide-react";
import { useTranslation } from "react-i18next";

import { isWorkEventTimelineItem, timelineItemKey, type TimelineItem } from "../../domain/work";

const DetailList = ({ label, values }: { label: string; values: string[] }) =>
  values.length ? (
    <section className="timeline-details"><strong>{label}</strong><ul>{values.map((value, index) => <li key={`${value}-${index}`}>{value}</li>)}</ul></section>
  ) : null;

function TimelineEvent({ item }: { item: TimelineItem }) {
  const { t } = useTranslation();
  if (!isWorkEventTimelineItem(item)) {
    return <article className="timeline-event timeline-event--user"><UserRound aria-hidden="true" /><div><strong>{t("timeline.you")}</strong><p>{item.content}</p></div></article>;
  }
  const payload = item.payload;
  if (payload.type === "runStarted") return <article className="timeline-event"><Play aria-hidden="true" /><div><strong>{t("timeline.runStarted")}</strong><p>{payload.modelLabel}</p></div></article>;
  if (payload.type === "assistantDelta") return <article className="timeline-event timeline-event--assistant"><Bot aria-hidden="true" /><p>{payload.text}</p></article>;
  if (payload.type === "toolStarted") return <article className="timeline-event"><Wrench aria-hidden="true" /><div><strong>{payload.toolName}</strong><p>{payload.inputSummary}</p></div></article>;
  if (payload.type === "toolFinished") return <article className={`timeline-event timeline-event--${payload.success ? "success" : "failure"}`}><Wrench aria-hidden="true" /><div><strong>{payload.toolName} · {t(payload.success ? "timeline.succeeded" : "timeline.failed")}</strong><p>{payload.outputSummary}</p></div></article>;
  if (payload.type === "runFailed") return <article className="timeline-event timeline-event--failure"><CircleAlert aria-hidden="true" /><div><strong>{t("timeline.runFailed")}</strong><p>{payload.message}</p></div></article>;
  return (
    <article className="timeline-event timeline-event--completed">
      <CheckCircle2 aria-hidden="true" />
      <div><strong>{t("timeline.completed")}</strong><p>{payload.summary}</p>
        <DetailList label={t("inspector.artifacts")} values={payload.artifacts} />
        <DetailList label={t("timeline.validation")} values={payload.validation} />
        <DetailList label={t("timeline.limitations")} values={payload.limitations} />
      </div>
    </article>
  );
}

export function WorkTimeline({ timeline }: { timeline: TimelineItem[] }) {
  const { t } = useTranslation();
  return (
    <section className="work-timeline" aria-label={t("timeline.label")}>
      {timeline.length === 0 ? (
        <div className="timeline-empty"><Bot aria-hidden="true" size={24} /><h2>{t("timeline.emptyTitle")}</h2><p>{t("timeline.emptyBody")}</p></div>
      ) : timeline.map((item) => <TimelineEvent item={item} key={timelineItemKey(item)} />)}
    </section>
  );
}
