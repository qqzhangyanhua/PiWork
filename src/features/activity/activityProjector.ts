/*
 * Adapted from block/buzz at 5bf78671f45178f8de02ba18d3d321cbbf19cd1f,
 * Apache-2.0. Original:
 * desktop/src/features/agents/ui/agentSessionTranscript.ts.
 * PiWork changes: consumes typed WorkEventPayload values and replaces Buzz
 * Relay, Nostr, Channel and ACP identities with Work/Run/Turn identities.
 */
import type { WorkEventEnvelope, WorkEventPayload } from "../../bindings";

import type {
  ActivityDescriptor,
  ActivityIdentity,
  ActivityItem,
  ToolStatus,
} from "./activityTypes";
import { describeTool } from "./activityPresentation";

export type ActivityProjection = {
  items: ActivityItem[];
  indexById: Map<string, number>;
};

type ActivityDraft = ActivityProjection & {
  mutable: boolean;
};

export const createEmptyActivityProjection = (): ActivityProjection => ({
  items: [],
  indexById: new Map(),
});

const draftFrom = (state: ActivityProjection): ActivityDraft => ({
  items: state.items,
  indexById: state.indexById,
  mutable: false,
});

const ensureMutable = (draft: ActivityDraft): void => {
  if (draft.mutable) {
    return;
  }

  draft.items = [...draft.items];
  draft.indexById = new Map(draft.indexById);
  draft.mutable = true;
};

const replaceItem = (
  draft: ActivityDraft,
  index: number,
  item: ActivityItem,
): void => {
  ensureMutable(draft);
  draft.items[index] = item;
};

const pushItem = (draft: ActivityDraft, item: ActivityItem): void => {
  ensureMutable(draft);
  draft.indexById.set(item.id, draft.items.length);
  draft.items.push(item);
};

const finishDraft = (
  state: ActivityProjection,
  draft: ActivityDraft,
): ActivityProjection =>
  draft.mutable
    ? { items: draft.items, indexById: draft.indexById }
    : state;

const identityFrom = (event: WorkEventEnvelope): ActivityIdentity => ({
  workId: event.workId,
  runId: event.runId,
  turnId: event.turnId ?? null,
  sessionId: event.sessionId ?? null,
  agentId: event.agentId ?? null,
  assignmentId: event.assignmentId ?? null,
});

const baseFrom = (event: WorkEventEnvelope, id: string) => ({
  ...identityFrom(event),
  id,
  timestamp: event.occurredAt,
});

const itemAt = (
  draft: ActivityDraft,
  id: string,
): { index: number; item: ActivityItem } | null => {
  const index = draft.indexById.get(id);
  if (index === undefined) {
    return null;
  }

  const item = draft.items[index];
  return item === undefined ? null : { index, item };
};

const putItem = (draft: ActivityDraft, item: ActivityItem): void => {
  const existing = itemAt(draft, item.id);
  if (existing === null) {
    pushItem(draft, item);
    return;
  }

  replaceItem(draft, existing.index, item);
};

const eventIdentity = (event: WorkEventEnvelope): string =>
  event.runId ??
  event.assignmentId ??
  event.eventId ??
  `${event.workId}:${event.sequence}:${event.payload.type}`;

const turnOrRun = (event: WorkEventEnvelope): string =>
  event.turnId ?? eventIdentity(event);

const toolDescriptor = (
  toolName: string,
  input: string,
  result: string,
): ActivityDescriptor => ({
  ...describeTool(toolName, input),
  preview: result || input || null,
});

type ToolItem = Extract<ActivityItem, { type: "tool" }>;

const isTerminalToolStatus = (status: ToolStatus): boolean =>
  status === "completed" || status === "failed";

const createToolItem = (
  event: WorkEventEnvelope,
  toolCallId: string,
  toolName: string,
  status: ToolStatus,
  input: string,
  result: string,
  isError: boolean,
): ToolItem => {
  const descriptor = toolDescriptor(toolName, input, result);
  return {
    ...baseFrom(event, `tool:${eventIdentity(event)}:${toolCallId}`),
    type: "tool",
    renderClass: descriptor.renderClass,
    toolCallId,
    toolName,
    status,
    input,
    result,
    isError,
    descriptor,
  };
};

const mergeTool = (
  draft: ActivityDraft,
  next: ToolItem,
): void => {
  const existing = itemAt(draft, next.id);
  if (existing === null) {
    pushItem(draft, next);
    return;
  }

  if (existing.item.type !== "tool") {
    replaceItem(draft, existing.index, next);
    return;
  }

  if (isTerminalToolStatus(existing.item.status)) {
    const toolName = existing.item.toolName || next.toolName;
    const input = existing.item.input || next.input;
    if (toolName === existing.item.toolName && input === existing.item.input) {
      return;
    }

    const descriptor = toolDescriptor(
      toolName,
      input,
      existing.item.result,
    );
    replaceItem(draft, existing.index, {
      ...existing.item,
      toolName,
      input,
      renderClass: descriptor.renderClass,
      descriptor,
    });
    return;
  }

  const status =
    existing.item.status === "executing" && next.status === "pending"
      ? "executing"
      : next.status;
  const toolName = next.toolName || existing.item.toolName;
  const input = next.input || existing.item.input;
  const result = next.result || existing.item.result;
  const descriptor = toolDescriptor(toolName, input, result);
  replaceItem(draft, existing.index, {
    ...existing.item,
    toolName,
    status,
    input,
    result,
    isError: next.isError,
    renderClass: descriptor.renderClass,
    descriptor,
  });
};

const lifecycleItem = (
  event: WorkEventEnvelope,
  activityKind: Exclude<
    Extract<ActivityItem, { type: "lifecycle" }>["activityKind"],
    "sessionChanged"
  >,
  renderClass: Extract<ActivityItem, { type: "lifecycle" }>["renderClass"],
  detail: string | null,
): ActivityItem => ({
  ...baseFrom(
    event,
    `lifecycle:${eventIdentity(event)}:${event.payload.type}:${event.sequence}`,
  ),
  type: "lifecycle",
  renderClass,
  activityKind,
  detail,
});

const assertNever = (value: never): never => {
  throw new Error(`Unhandled WorkEventPayload: ${JSON.stringify(value)}`);
};

const processActivityEventIntoDraft = (
  draft: ActivityDraft,
  event: WorkEventEnvelope,
): void => {
  const payload = event.payload;

  switch (payload.type) {
    case "assistantDelta": {
      const id = `message:${eventIdentity(event)}:${turnOrRun(event)}`;
      const existing = itemAt(draft, id);
      if (existing?.item.type === "message") {
        replaceItem(draft, existing.index, {
          ...existing.item,
          text: `${existing.item.text}${payload.text}`,
        });
      } else {
        putItem(draft, {
          ...baseFrom(event, id),
          type: "message",
          renderClass: "message",
          text: payload.text,
        });
      }
      return;
    }

    case "thoughtDelta": {
      const id = `thought:${eventIdentity(event)}:${turnOrRun(event)}`;
      const existing = itemAt(draft, id);
      if (existing?.item.type === "thought") {
        replaceItem(draft, existing.index, {
          ...existing.item,
          text: `${existing.item.text}${payload.text}`,
        });
      } else {
        putItem(draft, {
          ...baseFrom(event, id),
          type: "thought",
          renderClass: "thought",
          text: payload.text,
        });
      }
      return;
    }

    case "planChanged": {
      const id = `plan:${eventIdentity(event)}:${payload.planId}`;
      const existing = itemAt(draft, id);
      if (
        existing?.item.type === "plan" &&
        payload.revision <= existing.item.revision
      ) {
        return;
      }

      const next: ActivityItem = {
        ...baseFrom(event, id),
        type: "plan",
        renderClass: "plan",
        planId: payload.planId,
        revision: payload.revision,
        text: payload.text,
      };
      if (existing?.item.type === "plan") {
        replaceItem(draft, existing.index, {
          ...existing.item,
          revision: payload.revision,
          text: payload.text,
        });
      } else {
        putItem(draft, next);
      }
      return;
    }

    case "toolPending":
      mergeTool(
        draft,
        createToolItem(
          event,
          payload.toolCallId,
          payload.toolName,
          "pending",
          payload.inputSummary,
          "",
          false,
        ),
      );
      return;

    case "toolStarted":
      mergeTool(
        draft,
        createToolItem(
          event,
          payload.toolCallId,
          payload.toolName,
          "executing",
          payload.inputSummary,
          "",
          false,
        ),
      );
      return;

    case "toolProgress":
      mergeTool(
        draft,
        createToolItem(
          event,
          payload.toolCallId,
          payload.toolName,
          "executing",
          "",
          payload.outputSummary,
          false,
        ),
      );
      return;

    case "toolFinished":
      mergeTool(
        draft,
        createToolItem(
          event,
          payload.toolCallId,
          payload.toolName,
          payload.success ? "completed" : "failed",
          "",
          payload.outputSummary,
          !payload.success,
        ),
      );
      return;

    case "permissionRequested": {
      const id = `permission:${eventIdentity(event)}:${payload.requestId}`;
      const existing = itemAt(draft, id);
      if (existing?.item.type === "permission") {
        replaceItem(draft, existing.index, {
          ...existing.item,
          toolCallId: payload.toolCallId ?? null,
          title: payload.title,
          detail: payload.detail,
        });
      } else {
        putItem(draft, {
          ...baseFrom(event, id),
          type: "permission",
          renderClass: "permission",
          requestId: payload.requestId,
          toolCallId: payload.toolCallId ?? null,
          title: payload.title,
          detail: payload.detail,
          status: "requested",
          outcome: null,
        });
      }
      return;
    }

    case "permissionResolved": {
      const id = `permission:${eventIdentity(event)}:${payload.requestId}`;
      const existing = itemAt(draft, id);
      if (existing?.item.type === "permission") {
        replaceItem(draft, existing.index, {
          ...existing.item,
          status: "resolved",
          outcome: payload.outcome,
        });
      } else {
        putItem(draft, {
          ...baseFrom(event, id),
          type: "permission",
          renderClass: "permission",
          requestId: payload.requestId,
          toolCallId: null,
          title: "Permission request",
          detail: "",
          status: "resolved",
          outcome: payload.outcome,
        });
      }
      return;
    }

    case "runStarted":
      putItem(
        draft,
        lifecycleItem(event, "runStarted", "status", `Model: ${payload.modelLabel}`),
      );
      return;

    case "waiting":
      putItem(draft, lifecycleItem(event, "waiting", "status", payload.reason));
      return;

    case "liveness":
      putItem(
        draft,
        lifecycleItem(
          event,
          "liveness",
          payload.state === "alive" ? "suppressed" : "status",
          payload.state === "alive" ? "alive" : "Activity stalled",
        ),
      );
      return;

    case "sessionChanged":
      putItem(draft, {
        ...baseFrom(
          event,
          `lifecycle:${eventIdentity(event)}:${event.payload.type}:${event.sequence}`,
        ),
        type: "lifecycle",
        renderClass: "status",
        activityKind: "sessionChanged",
        transition: payload.transition,
        reason: payload.reason ?? null,
      });
      return;

    case "artifactProduced":
      putItem(
        draft,
        lifecycleItem(event, "artifactProduced", "status", `Artifact: ${payload.path}`),
      );
      return;

    case "validationProduced":
      putItem(
        draft,
        lifecycleItem(
          event,
          "validationProduced",
          "status",
          `Validation ${payload.success ? "passed" : "failed"} (${payload.command}): ${payload.summary}`,
        ),
      );
      return;

    case "runCompleted":
      putItem(
        draft,
        lifecycleItem(event, "runCompleted", "suppressed", payload.summary),
      );
      return;

    case "runFailed":
      putItem(draft, lifecycleItem(event, "runFailed", "error", payload.message));
      return;

    case "usageUpdated": {
      const id = `usage:${eventIdentity(event)}:${turnOrRun(event)}`;
      const existing = itemAt(draft, id);
      const next: ActivityItem = {
        ...baseFrom(event, id),
        type: "usage",
        renderClass: "suppressed",
        inputTokens: payload.inputTokens,
        outputTokens: payload.outputTokens,
        cacheReadTokens: payload.cacheReadTokens,
        cacheWriteTokens: payload.cacheWriteTokens,
        totalTokens: payload.totalTokens,
      };
      if (existing?.item.type === "usage") {
        replaceItem(draft, existing.index, {
          ...existing.item,
          inputTokens: payload.inputTokens,
          outputTokens: payload.outputTokens,
          cacheReadTokens: payload.cacheReadTokens,
          cacheWriteTokens: payload.cacheWriteTokens,
          totalTokens: payload.totalTokens,
        });
      } else {
        putItem(draft, next);
      }
      return;
    }

    case "rawEngineEvent":
      putItem(draft, {
        ...baseFrom(
          event,
          `raw:${event.eventId ?? `${eventIdentity(event)}:${event.sequence}`}`,
        ),
        type: "raw",
        renderClass: "raw-rail",
        kind: payload.kind,
        payloadJson: payload.payloadJson,
      });
      return;

    case "assignmentQueued":
    case "assignmentClaimed":
    case "assignmentStarted":
    case "assignmentWaiting":
    case "assignmentRetryScheduled":
    case "assignmentCompleted":
    case "assignmentCancelled":
    case "assignmentFailed":
    case "assignmentInterrupted":
    case "assignmentDeadLettered":
    case "assignmentRecoveryRequired":
    case "queueControlApplied":
      return;
  }

  return assertNever(payload);
};

export function processActivityEvent(
  state: ActivityProjection,
  event: WorkEventEnvelope,
): ActivityProjection {
  const draft = draftFrom(state);
  processActivityEventIntoDraft(draft, event);
  return finishDraft(state, draft);
}

/**
 * Projects the sequence-ordered journal for one Run. Task 9 supplies events
 * from a single Run; callers combining Runs must project each journal first.
 */
export function projectActivity(events: WorkEventEnvelope[]): ActivityItem[] {
  const sortedEvents = [...events].sort(
    (left, right) => left.sequence - right.sequence,
  );
  const draft = draftFrom(createEmptyActivityProjection());
  ensureMutable(draft);

  for (let index = 0; index < sortedEvents.length; index += 1) {
    const event = sortedEvents[index];
    if (event !== undefined) {
      processActivityEventIntoDraft(draft, event);
    }
  }

  return draft.items;
}
