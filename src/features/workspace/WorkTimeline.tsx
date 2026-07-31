import { Check, CircleAlert, LoaderCircle, Wrench } from "lucide-react";
import { useLayoutEffect, useRef } from "react";
import Markdown from "react-markdown";
import { useTranslation } from "react-i18next";
import remarkGfm from "remark-gfm";

import { appErrorMessageKey, appErrorMessageValues } from "../../domain/appError";
import { isWorkEventTimelineItem, type AppError, type TimelineItem } from "../../domain/work";
import type { MessageSummary, ResourceSummary, WorkEventEnvelope } from "../../bindings";
import { AttachmentChips } from "./AttachmentChips";

type RunGroup = {
  key: string;
  runId: string;
  items: TimelineItem[];
};

const groupByRun = (timeline: TimelineItem[]) => {
  const groups: RunGroup[] = [];
  const byRun = new Map<string, RunGroup>();
  for (const item of timeline) {
    let group = byRun.get(item.runId);
    if (!group) {
      group = { key: `run:${item.runId}`, runId: item.runId, items: [] };
      groups.push(group);
      byRun.set(item.runId, group);
    }
    group.items.push(item);
  }
  return groups;
};

const DetailList = ({ label, values }: { label: string; values: string[] }) =>
  values.length ? (
    <section className="agent-activity__details"><strong>{label}</strong><ul>{values.map((value, index) => <li key={`${value}-${index}`}>{value}</li>)}</ul></section>
  ) : null;

function ActivityRow({ event }: { event: WorkEventEnvelope }) {
  const { t } = useTranslation();
  const payload = event.payload;
  if (payload.type === "runStarted") {
    return <div className="agent-activity__row agent-activity__row--muted"><LoaderCircle aria-hidden="true" /><span>{t("timeline.runStarted")}</span><small>{payload.modelLabel}</small></div>;
  }
  if (payload.type === "toolStarted") {
    return <div className="agent-activity__row"><Wrench aria-hidden="true" /><span><strong>{payload.toolName}</strong><small>{payload.inputSummary}</small></span></div>;
  }
  if (payload.type === "toolFinished") {
    return <div className={`agent-activity__row agent-activity__row--${payload.success ? "success" : "failure"}`}>
      {payload.success ? <Check aria-hidden="true" /> : <CircleAlert aria-hidden="true" />}
      <span><strong>{payload.toolName}</strong><small>{payload.outputSummary}</small></span>
      <em>{t(payload.success ? "timeline.succeeded" : "timeline.failed")}</em>
    </div>;
  }
  if (payload.type === "runFailed") {
    return <div className="agent-activity__row agent-activity__row--failure"><CircleAlert aria-hidden="true" /><span><strong>{t("timeline.runFailed")}</strong><small>{payload.message}</small></span></div>;
  }
  if (payload.type === "runCompleted") {
    return <div className="agent-activity__completion"><p>{payload.summary}</p><DetailList label={t("inspector.artifacts")} values={payload.artifacts} /><DetailList label={t("timeline.validation")} values={payload.validation} /><DetailList label={t("timeline.limitations")} values={payload.limitations} /></div>;
  }
  return null;
}

function AgentActivity({ events }: { events: WorkEventEnvelope[] }) {
  const { t } = useTranslation();
  const tools = new Set(events.flatMap(({ payload }) =>
    payload.type === "toolStarted" || payload.type === "toolFinished" ? [payload.toolCallId] : [],
  ));
  const completed = events.some(({ payload }) => payload.type === "runCompleted");
  const failed = events.some(({ payload }) => payload.type === "runFailed" || (payload.type === "toolFinished" && !payload.success));
  const terminal = completed || failed;
  const label = failed
    ? t("timeline.activityFailed", { count: tools.size })
    : completed
      ? t("timeline.activityCompleted", { count: tools.size })
      : t("timeline.activityRunning");

  return (
    <details className={`agent-activity${failed ? " agent-activity--failure" : ""}`} open={!terminal}>
      <summary><span className="agent-activity__pi" aria-hidden="true">π</span><strong>{label}</strong><span className="agent-activity__toggle">{t("timeline.activityToggle")}</span></summary>
      <div className="agent-activity__body">
        {events.map((event) => <ActivityRow event={event} key={`${event.runId}:${event.sequence}`} />)}
      </div>
    </details>
  );
}

function AssistantMessage({ text }: { text: string }) {
  return (
    <article className="timeline-event--assistant">
      <div className="timeline-markdown">
        <Markdown remarkPlugins={[remarkGfm]}>{text}</Markdown>
      </div>
    </article>
  );
}

function Run({
  group,
  resourcesById,
}: {
  group: RunGroup;
  resourcesById: Map<string, ResourceSummary>;
}) {
  const { t } = useTranslation();
  const messages = group.items.filter(
    (item): item is MessageSummary => !isWorkEventTimelineItem(item),
  );
  const events = group.items.filter(isWorkEventTimelineItem);
  const assistantText = events
    .flatMap(({ payload }) => payload.type === "assistantDelta" ? [payload.text] : [])
    .join("");
  const activity = events.filter(({ payload }) => payload.type !== "assistantDelta");

  return (
    <section className="timeline-run" data-run-id={group.runId}>
      {messages.map((message) => message.role === "assistant" ? (
        <AssistantMessage key={message.id} text={message.content} />
      ) : (
        <article className="timeline-user" data-testid={`message:${message.id}`} key={message.id}>
          <strong className="sr-only">{t("timeline.you")}</strong>
          {message.content && <p>{message.content}</p>}
          <AttachmentChips
            resources={message.resourceIds.flatMap((id) => {
              const resource = resourcesById.get(id);
              return resource ? [resource] : [];
            })}
          />
        </article>
      ))}
      {activity.length > 0 && <AgentActivity events={activity} />}
      {assistantText && <AssistantMessage text={assistantText} />}
    </section>
  );
}

export function WorkTimeline({ timeline, resources, error = null, onOpenDiagnostics }: { timeline: TimelineItem[]; resources: ResourceSummary[]; error?: AppError | null; onOpenDiagnostics?(): void }) {
  const { t } = useTranslation();
  const timelineRef = useRef<HTMLElement>(null);
  const groups = groupByRun(timeline);
  const resourcesById = new Map(resources.map((resource) => [resource.id, resource]));

  useLayoutEffect(() => {
    const element = timelineRef.current;
    if (element) {
      element.scrollTop = element.scrollHeight;
    }
  }, [timeline, resources, error]);

  return (
    <section ref={timelineRef} className="work-timeline" aria-label={t("timeline.label")}>
      {timeline.length === 0 ? (
        <div className="timeline-empty"><span className="timeline-empty__pi" aria-hidden="true">π</span><h2>{t("timeline.emptyTitle")}</h2><p>{t("timeline.emptyBody")}</p></div>
      ) : groups.map((group) => <Run group={group} key={group.key} resourcesById={resourcesById} />)}
      {error && (
        <aside className="agent-activity agent-activity--failure agent-activity__error" role="alert">
          <span className="agent-activity__pi" aria-hidden="true">π</span>
          <div><strong>{t("timeline.runFailed")}</strong><p>{t(appErrorMessageKey(error), appErrorMessageValues(error))}</p></div>
          {onOpenDiagnostics && <button className="button" onClick={onOpenDiagnostics} type="button">{t("diagnostics.open")}</button>}
        </aside>
      )}
    </section>
  );
}
