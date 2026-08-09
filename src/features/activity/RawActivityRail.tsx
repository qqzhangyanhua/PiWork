import { ChevronRight } from "lucide-react";
import { memo, useMemo, useState } from "react";
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

const isLegacyEvent = (event: WorkEventEnvelope): boolean => event.version < 2;

const eventKey = (event: WorkEventEnvelope): string =>
  `${encodeURIComponent(event.workId)}:${encodeURIComponent(event.runId)}:${event.sequence}:${event.eventId ? `event:${encodeURIComponent(event.eventId)}` : "legacy"}`;

type RawActivityRowModel = {
  event: WorkEventEnvelope;
  key: string;
};

const RawActivityEventRow = memo(function RawActivityEventRow({
  event,
}: {
  event: WorkEventEnvelope;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const payload = useMemo(
    () => open ? JSON.stringify(presentPayload(event), null, 2) : null,
    [event, open],
  );

  return (
    <li className="raw-activity-rail__item" data-testid="raw-activity-event">
      <details
        className="raw-activity-event"
        onToggle={(toggleEvent) => setOpen(toggleEvent.currentTarget.open)}
        open={open}
      >
        <summary>
          <ChevronRight aria-hidden="true" size={13} />
          <span className="raw-activity-event__sequence">
            {t("rawActivity.sequence", { sequence: event.sequence })}
          </span>
          <strong>{event.payload.type}</strong>
          <time dateTime={event.occurredAt}>{event.occurredAt}</time>
        </summary>
        {open ? (
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
            <pre>{payload}</pre>
          </div>
        ) : null}
      </details>
    </li>
  );
});

export function RawActivityRail({ events }: RawActivityRailProps) {
  const { t } = useTranslation();
  const rows = useMemo<RawActivityRowModel[]>(
    () => [...events]
      .sort(compareEvents)
      .map((event) => ({ event, key: eventKey(event) })),
    [events],
  );

  if (rows.length === 0) {
    return <p className="inspector-empty">{t("rawActivity.empty")}</p>;
  }

  return (
    <ol aria-label={t("rawActivity.label")} className="raw-activity-rail">
      {rows.map((row) => (
        <RawActivityEventRow event={row.event} key={row.key} />
      ))}
    </ol>
  );
}
