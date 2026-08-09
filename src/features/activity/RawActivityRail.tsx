import { ChevronRight } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { WorkEventEnvelope } from "../../bindings";

export type RawActivityRailProps = {
  events: WorkEventEnvelope[];
};

const compareText = (left: string, right: string): number =>
  left < right ? -1 : left > right ? 1 : 0;

const compareEvents = (
  left: WorkEventEnvelope,
  right: WorkEventEnvelope,
): number =>
  compareText(left.occurredAt, right.occurredAt) ||
  compareText(left.runId, right.runId) ||
  left.sequence - right.sequence ||
  compareText(left.eventId ?? "", right.eventId ?? "");

const presentPayload = (event: WorkEventEnvelope): unknown => {
  if (event.payload.type !== "rawEngineEvent") return event.payload;
  try {
    return {
      ...event.payload,
      payload: JSON.parse(event.payload.payloadJson) as unknown,
    };
  } catch {
    return event.payload;
  }
};

const metadata = (event: WorkEventEnvelope) => [
  ["event", event.eventId],
  ["work", event.workId],
  ["run", event.runId],
  ["turn", event.turnId],
  ["session", event.sessionId],
  ["agent", event.agentId],
  ["assignment", event.assignmentId],
  ["causation", event.causationId],
  ["correlation", event.correlationId],
] as const;

const isLegacyEvent = (event: WorkEventEnvelope): boolean =>
  event.version < 2 ||
  !event.eventId ||
  !event.turnId ||
  !event.sessionId ||
  !event.correlationId;

export function RawActivityRail({ events }: RawActivityRailProps) {
  const { t } = useTranslation();
  const orderedEvents = [...events].sort(compareEvents);

  if (orderedEvents.length === 0) {
    return <p className="inspector-empty">{t("rawActivity.empty")}</p>;
  }

  return (
    <ol aria-label={t("rawActivity.label")} className="raw-activity-rail">
      {orderedEvents.map((event, index) => (
        <li
          className="raw-activity-rail__item"
          data-testid="raw-activity-event"
          key={event.eventId ?? `${event.occurredAt}:${event.runId}:${event.sequence}:${index}`}
        >
          <details className="raw-activity-event">
            <summary>
              <ChevronRight aria-hidden="true" size={13} />
              <span className="raw-activity-event__sequence">
                {t("rawActivity.sequence", { sequence: event.sequence })}
              </span>
              <strong>{event.payload.type}</strong>
              <time dateTime={event.occurredAt}>{event.occurredAt}</time>
            </summary>
            <div className="raw-activity-event__body">
              <div className="raw-activity-event__identity">
                {isLegacyEvent(event) ? (
                  <span className="raw-activity-event__legacy">
                    {t("rawActivity.legacyEvent")}
                  </span>
                ) : null}
                <dl>
                  {metadata(event).map(([label, value]) =>
                    value ? (
                      <div key={label}>
                        <dt>{label}</dt>
                        <dd>{value}</dd>
                      </div>
                    ) : null,
                  )}
                </dl>
              </div>
              <pre>{JSON.stringify(presentPayload(event), null, 2)}</pre>
            </div>
          </details>
        </li>
      ))}
    </ol>
  );
}
