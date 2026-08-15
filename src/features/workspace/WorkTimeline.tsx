import { ArrowDown, CircleAlert, CircleCheck } from "lucide-react";
import { useLayoutEffect, useRef, useState } from "react";
import Markdown from "react-markdown";
import { useTranslation } from "react-i18next";
import remarkGfm from "remark-gfm";

import { appErrorMessageKey, appErrorMessageValues } from "../../domain/appError";
import {
  isWorkEventTimelineItem,
  workEventSequenceKey,
  type AppError,
  type TimelineItem,
} from "../../domain/work";
import type { MessageSummary, ResourceSummary, WorkEventEnvelope, WorkSummary } from "../../bindings";
import {
  ActivityFeed,
  isActivityFeedItem,
} from "../activity/ActivityFeed";
import { projectActivity } from "../activity/activityProjector";
import type { ActivityItem } from "../activity/activityTypes";
import { AttachmentChips } from "./AttachmentChips";
import { ExecutionProgressCard } from "./ExecutionProgressCard";

type ConversationTurnGroup = {
  key: string;
  items: TimelineItem[];
};

const itemTimestamp = (item: TimelineItem) =>
  isWorkEventTimelineItem(item) ? item.occurredAt : item.createdAt;

const formatTurnTimestamp = (value: string, locale: string) => {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  const now = new Date();
  const sameDay = date.getFullYear() === now.getFullYear()
    && date.getMonth() === now.getMonth()
    && date.getDate() === now.getDate();
  const sameYear = date.getFullYear() === now.getFullYear();
  return new Intl.DateTimeFormat(locale, sameDay
    ? { hour: "2-digit", minute: "2-digit" }
    : {
        ...(sameYear ? {} : { year: "numeric" as const }),
        month: "short",
        day: "numeric",
        hour: "2-digit",
        minute: "2-digit",
      }).format(date);
};

const groupByRun = (timeline: TimelineItem[]) => {
  const groups: ConversationTurnGroup[] = [];
  const byRun = new Map<string, ConversationTurnGroup>();
  for (const item of timeline) {
    const identity = isWorkEventTimelineItem(item)
      ? workEventSequenceKey(item)
      : item.runId;
    let group = byRun.get(identity);
    if (!group) {
      group = { key: `run:${identity}`, items: [] };
      groups.push(group);
      byRun.set(identity, group);
    }
    group.items.push(item);
  }
  return groups;
};

const DetailList = ({ label, values }: { label: string; values: string[] }) =>
  values.length ? (
    <section className="timeline-delivery__details"><strong>{label}</strong><ul>{values.map((value, index) => <li key={`${value}-${index}`}>{value}</li>)}</ul></section>
  ) : null;

const presentableCompletionSummary = (summary: string) => {
  const value = summary.trim();
  return /^Pi completed this Run with \d+ tool calls?$/iu.test(value) ? "" : value;
};

const isUrgentPermission = (
  item: ActivityItem,
): item is Extract<ActivityItem, { type: "permission" }> =>
  item.type === "permission" && item.status === "requested";

function AssistantMessage({ text }: { text: string }) {
  return (
    <article className="timeline-event--assistant">
      <div className="timeline-markdown">
        <Markdown remarkPlugins={[remarkGfm]}>{text}</Markdown>
      </div>
    </article>
  );
}

function ConversationSegment({
  group,
  resourcesById,
  onOpenDiagnostics,
}: {
  group: ConversationTurnGroup;
  resourcesById: Map<string, ResourceSummary>;
  onOpenDiagnostics?(): void;
}) {
  const { i18n, t } = useTranslation();
  const messages = group.items.filter(
    (item): item is MessageSummary => !isWorkEventTimelineItem(item),
  );
  const events = group.items.filter(isWorkEventTimelineItem);
  const projected = projectActivity(events);
  const assistantText = projected
    .filter(
      (item): item is Extract<ActivityItem, { type: "message" }> =>
        item.type === "message",
    )
    .map((item) => item.text)
    .join("");
  const hasTerminalEvent = events.some(
    (event) =>
      event.payload.type === "runCompleted" || event.payload.type === "runFailed",
  );
  const urgentPermissionItems = hasTerminalEvent
    ? []
    : projected.filter(isUrgentPermission);
  const activityItems = projected.filter(
    (item) =>
      item.type !== "message" &&
      (hasTerminalEvent || !isUrgentPermission(item)),
  );
  const hasProgressActivity = activityItems.some(isActivityFeedItem);
  const completion = events.find(
    (event): event is WorkEventEnvelope & { payload: Extract<WorkEventEnvelope["payload"], { type: "runCompleted" }> } =>
      event.payload.type === "runCompleted",
  );
  const failure = events.find(
    (event): event is WorkEventEnvelope & { payload: Extract<WorkEventEnvelope["payload"], { type: "runFailed" }> } =>
      event.payload.type === "runFailed",
  );
  const completionSummary = completion
    ? presentableCompletionSummary(completion.payload.summary)
    : "";
  const hasDelivery = Boolean(completion && (
    completionSummary
    || completion.payload.artifacts.length
    || completion.payload.validation.length
    || completion.payload.limitations.length
  ));
  const startedAt = group.items
    .map(itemTimestamp)
    .reduce((earliest, current) => current < earliest ? current : earliest);
  return (
    <>
      <time className="conversation-turn__time" dateTime={startedAt}>
        {formatTurnTimestamp(startedAt, i18n.resolvedLanguage ?? i18n.language)}
      </time>
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
      {urgentPermissionItems.length > 0 ? (
        <ActivityFeed items={urgentPermissionItems} />
      ) : null}
      {hasProgressActivity && (
        <ExecutionProgressCard events={events}>
          <ActivityFeed
            items={activityItems}
            permissionRequestRole="status"
          />
        </ExecutionProgressCard>
      )}
      {assistantText && <AssistantMessage text={assistantText} />}
      {completion && hasDelivery && (
        <article className="timeline-delivery">
          <header><CircleCheck aria-hidden="true" /><strong>{t("timeline.delivery")}</strong></header>
          {completionSummary && <p>{completionSummary}</p>}
          <DetailList label={t("inspector.artifacts")} values={completion.payload.artifacts} />
          <DetailList label={t("timeline.validation")} values={completion.payload.validation} />
          <DetailList label={t("timeline.limitations")} values={completion.payload.limitations} />
        </article>
      )}
      {failure && (
        <article className="timeline-failure" role="alert">
          <CircleAlert aria-hidden="true" />
          <div>
            <strong>{t("timeline.runFailed")}</strong>
            <p>{t("timeline.runFailedBody")}</p>
            {onOpenDiagnostics && <button className="button" onClick={onOpenDiagnostics} type="button">{t("diagnostics.open")}</button>}
          </div>
        </article>
      )}
    </>
  );
}

export function WorkTimeline({ timeline, resources, work, error = null, onOpenDiagnostics }: { timeline: TimelineItem[]; resources: ResourceSummary[]; work?: WorkSummary; error?: AppError | null; onOpenDiagnostics?(): void }) {
  const { t } = useTranslation();
  const timelineRef = useRef<HTMLElement>(null);
  const followingRef = useRef(true);
  const [hasNewOutput, setHasNewOutput] = useState(false);
  const groups = groupByRun(timeline);
  const resourcesById = new Map(resources.map((resource) => [resource.id, resource]));

  useLayoutEffect(() => {
    followingRef.current = true;
    setHasNewOutput(false);
    const element = timelineRef.current;
    if (element) element.scrollTop = element.scrollHeight;
  }, [work?.id]);

  useLayoutEffect(() => {
    const element = timelineRef.current;
    if (element && followingRef.current) {
      element.scrollTop = element.scrollHeight;
      setHasNewOutput(false);
    } else if (element) {
      setHasNewOutput(true);
    }
  }, [timeline, resources, error]);

  const scrollToLatest = () => {
    const element = timelineRef.current;
    if (!element) return;
    followingRef.current = true;
    element.scrollTop = element.scrollHeight;
    setHasNewOutput(false);
  };

  return (
    <div className="work-timeline-shell">
      <section
        ref={timelineRef}
        className="work-timeline"
        aria-label={t("timeline.label")}
        onScroll={(event) => {
          const element = event.currentTarget;
          const nearBottom = element.scrollHeight - element.scrollTop - element.clientHeight <= 120;
          followingRef.current = nearBottom;
          if (nearBottom) setHasNewOutput(false);
        }}
      >
        {timeline.length === 0 ? (
          <div className="timeline-empty"><h2>{t("timeline.emptyTitle")}</h2><p>{t("timeline.emptyBody")}</p></div>
        ) : (
          <section className="conversation-thread">
            {groups.map((group) => <ConversationSegment group={group} key={group.key} onOpenDiagnostics={onOpenDiagnostics} resourcesById={resourcesById} />)}
          </section>
        )}
        {error && (
          <aside className="agent-activity agent-activity--failure agent-activity__error" role="alert">
            <span className="agent-activity__pi" aria-hidden="true">π</span>
            <div><strong>{t("timeline.runFailed")}</strong><p>{t(appErrorMessageKey(error), appErrorMessageValues(error))}</p></div>
            {onOpenDiagnostics && <button className="button" onClick={onOpenDiagnostics} type="button">{t("diagnostics.open")}</button>}
          </aside>
        )}
      </section>
      {hasNewOutput && (
        <button className="timeline-new-output" onClick={scrollToLatest} type="button">
          <ArrowDown aria-hidden="true" size={15} />
          {t("timeline.newOutput")}
        </button>
      )}
    </div>
  );
}
