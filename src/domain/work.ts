import type { MessageSummary, WorkEventEnvelope } from "../bindings";

export type AppError = {
  code: string;
  message: string;
  details?: Record<string, unknown>;
};

export type TimelineItem = MessageSummary | WorkEventEnvelope;

export const isWorkEventTimelineItem = (
  item: TimelineItem,
): item is WorkEventEnvelope => "payload" in item;

export const workEventSequenceKey = (event: WorkEventEnvelope): string => {
  if (event.assignmentId) return `assignment:${event.assignmentId}`;
  if (event.runId) return event.runId;
  if (event.eventId) return `event:${event.eventId}`;
  return `work:${event.workId}:sequence:${event.sequence}:type:${event.payload.type}`;
};

export const timelineItemKey = (item: TimelineItem) =>
  isWorkEventTimelineItem(item)
    ? item.eventId
      ? `event:${item.eventId}`
      : item.assignmentId
        ? `event:assignment:${item.assignmentId}:${item.sequence}`
        : item.runId
          ? `event:${item.runId}:${item.sequence}`
          : `event:${workEventSequenceKey(item)}`
    : `message:${item.id}`;

const unknownError = (): AppError => ({
  code: "unknown",
  message: "Unknown error",
});

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

export const normalizeAppError = (value: unknown): AppError => {
  if (value instanceof Error) {
    return {
      code: "unknown",
      message: value.message || "Unknown error",
    };
  }
  if (typeof value === "string") {
    return { code: "unknown", message: value || "Unknown error" };
  }
  if (!isRecord(value)) {
    return unknownError();
  }

  try {
    const { code, details, message } = value;
    if (
      typeof code !== "string" ||
      typeof message !== "string" ||
      (details !== undefined && !isRecord(details))
    ) {
      return unknownError();
    }
    return details === undefined
      ? { code, message }
      : { code, message, details };
  } catch {
    return unknownError();
  }
};
