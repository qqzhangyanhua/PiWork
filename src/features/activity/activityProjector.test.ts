/*
 * Regression invariants adapted from block/buzz at
 * 5bf78671f45178f8de02ba18d3d321cbbf19cd1f, Apache-2.0.
 * Original: desktop/src/features/agents/ui/agentSessionTranscript.test.mjs.
 * PiWork changes: fixtures use typed WorkEventEnvelope payloads and identities.
 */
import type { WorkEventEnvelope, WorkEventPayload } from "../../bindings";
import { describe, expect, it, vi } from "vitest";

import {
  createEmptyActivityProjection,
  processActivityEvent,
  projectActivity,
} from "./activityProjector";

const event = (
  sequence: number,
  payload: WorkEventPayload,
  identity: Partial<WorkEventEnvelope> = {},
): WorkEventEnvelope => ({
  version: 2,
  eventId: `event-${sequence}`,
  workId: "work-1",
  runId: "run-1",
  turnId: "turn-1",
  sessionId: "session-1",
  correlationId: "run-1",
  sequence,
  occurredAt: `2026-08-09T00:00:${String(sequence).padStart(2, "0")}.000Z`,
  payload,
  ...identity,
});

describe("projectActivity", () => {
  it("appends assistant and thought deltas into one item per turn", () => {
    const items = projectActivity([
      event(1, { type: "assistantDelta", text: "A" }),
      event(2, { type: "assistantDelta", text: "B" }),
      event(3, { type: "thoughtDelta", text: "X" }),
      event(4, { type: "thoughtDelta", text: "Y" }),
    ]);

    expect(items.filter((item) => item.type === "message")).toMatchObject([
      { id: "message:run-1:turn-1", text: "AB" },
    ]);
    expect(items.filter((item) => item.type === "thought")).toMatchObject([
      { id: "thought:run-1:turn-1", text: "XY" },
    ]);
  });

  it("merges tool pending start progress and finish into one monotonic row", () => {
    const items = projectActivity([
      event(1, {
        type: "toolPending",
        toolCallId: "t1",
        toolName: "bash",
        inputSummary: "pnpm test",
      }),
      event(2, {
        type: "toolStarted",
        toolCallId: "t1",
        toolName: "bash",
        inputSummary: "pnpm test",
      }),
      event(3, {
        type: "toolProgress",
        toolCallId: "t1",
        toolName: "bash",
        outputSummary: "12/20",
      }),
      event(4, {
        type: "toolFinished",
        toolCallId: "t1",
        toolName: "bash",
        outputSummary: "20/20",
        success: true,
      }),
      event(5, {
        type: "toolProgress",
        toolCallId: "t1",
        toolName: "bash",
        outputSummary: "late",
      }),
    ]);

    expect(items.filter((item) => item.type === "tool")).toMatchObject([
      {
        id: "tool:run-1:t1",
        status: "completed",
        result: "20/20",
        isError: false,
      },
    ]);
  });

  it("creates an executing tool when progress is the first observed event", () => {
    const items = projectActivity([
      event(1, {
        type: "toolProgress",
        toolCallId: "t1",
        toolName: "bash",
        outputSummary: "1/2",
      }),
    ]);

    expect(items).toMatchObject([
      {
        id: "tool:run-1:t1",
        type: "tool",
        status: "executing",
        input: "",
        result: "1/2",
      },
    ]);
  });

  it("enriches a finished tool from a later start without changing its terminal outcome", () => {
    const items = projectActivity([
      event(1, {
        type: "toolFinished",
        toolCallId: "t1",
        toolName: "",
        outputSummary: "done",
        success: true,
      }),
      event(2, {
        type: "toolStarted",
        toolCallId: "t1",
        toolName: "bash",
        inputSummary: "pnpm test",
      }),
    ]);

    expect(items).toMatchObject([
      {
        type: "tool",
        renderClass: "shell",
        toolName: "bash",
        status: "completed",
        input: "pnpm test",
        result: "done",
        isError: false,
        descriptor: {
          renderClass: "shell",
          action: "execute",
          object: "pnpm test",
          preview: "done",
          tone: "admin",
          groupKey: "shell",
        },
      },
    ]);
  });

  it("enriches a failed tool from later pending metadata and preserves the first terminal outcome", () => {
    const items = projectActivity([
      event(1, {
        type: "toolFinished",
        toolCallId: "t1",
        toolName: "",
        outputSummary: "failed first",
        success: false,
      }),
      event(2, {
        type: "toolPending",
        toolCallId: "t1",
        toolName: "bash",
        inputSummary: "pnpm test",
      }),
      event(3, {
        type: "toolFinished",
        toolCallId: "t1",
        toolName: "bash",
        outputSummary: "late success",
        success: true,
      }),
    ]);

    expect(items).toMatchObject([
      {
        type: "tool",
        renderClass: "shell",
        toolName: "bash",
        status: "failed",
        input: "pnpm test",
        result: "failed first",
        isError: true,
        descriptor: {
          renderClass: "shell",
          action: "execute",
          object: "pnpm test",
          preview: "failed first",
          tone: "admin",
          groupKey: "shell",
        },
      },
    ]);
  });

  it("enriches a sparse finish from a richer duplicate without replacing the first terminal outcome", () => {
    const items = projectActivity([
      event(1, {
        type: "toolFinished",
        toolCallId: "t1",
        toolName: "",
        outputSummary: "failed first",
        success: false,
      }),
      event(2, {
        type: "toolFinished",
        toolCallId: "t1",
        toolName: "bash",
        outputSummary: "late success",
        success: true,
      }),
    ]);

    expect(items).toMatchObject([
      {
        type: "tool",
        renderClass: "shell",
        toolName: "bash",
        status: "failed",
        input: "",
        result: "failed first",
        isError: true,
        descriptor: {
          renderClass: "shell",
          action: "execute",
          object: "",
          preview: "failed first",
          tone: "admin",
          groupKey: "shell",
        },
      },
    ]);
  });

  it("replaces a plan only when its revision is newer", () => {
    const items = projectActivity([
      event(1, { type: "planChanged", planId: "main", revision: 2, text: "new" }),
      event(2, { type: "planChanged", planId: "main", revision: 1, text: "stale" }),
      event(3, { type: "planChanged", planId: "main", revision: 2, text: "equal" }),
    ]);

    expect(items.filter((item) => item.type === "plan")).toMatchObject([
      { id: "plan:run-1:main", text: "new", revision: 2 },
    ]);
  });

  it("correlates permission resolution with its request", () => {
    const items = projectActivity([
      event(1, {
        type: "permissionRequested",
        requestId: "p1",
        toolCallId: "t1",
        title: "Write",
        detail: "src/app.tsx",
      }),
      event(2, { type: "permissionResolved", requestId: "p1", outcome: "denied" }),
    ]);

    expect(items.filter((item) => item.type === "permission")).toMatchObject([
      {
        id: "permission:run-1:p1",
        status: "resolved",
        outcome: "denied",
      },
    ]);
  });

  it("keeps a resolution that arrives before its permission request", () => {
    const items = projectActivity([
      event(1, { type: "permissionResolved", requestId: "p1", outcome: "cancelled" }),
      event(2, {
        type: "permissionRequested",
        requestId: "p1",
        title: "Run command",
        detail: "pnpm test",
      }),
    ]);

    expect(items.filter((item) => item.type === "permission")).toMatchObject([
      {
        id: "permission:run-1:p1",
        toolCallId: null,
        title: "Run command",
        detail: "pnpm test",
        status: "resolved",
        outcome: "cancelled",
      },
    ]);
  });

  it("projects lifecycle, waiting, session, artifact, validation, and terminal events", () => {
    const items = projectActivity([
      event(1, { type: "runStarted", modelLabel: "gpt-5" }),
      event(2, { type: "waiting", reason: "Awaiting approval" }),
      event(3, {
        type: "sessionChanged",
        transition: "rotated",
        reason: "context limit",
      }),
      event(4, { type: "artifactProduced", path: "dist/report.md" }),
      event(5, {
        type: "validationProduced",
        command: "pnpm test",
        success: false,
        summary: "1 failed",
      }),
      event(6, { type: "runFailed", message: "engine stopped" }),
      event(7, {
        type: "runCompleted",
        summary: "Delivered",
        artifacts: ["dist/report.md"],
        validation: ["pnpm test"],
        limitations: ["offline"],
      }),
    ]);

    expect(items).toMatchObject([
      {
        id: "lifecycle:run-1:runStarted:1",
        type: "lifecycle",
        renderClass: "status",
        activityKind: "runStarted",
        detail: "Model: gpt-5",
      },
      {
        id: "lifecycle:run-1:waiting:2",
        activityKind: "waiting",
        detail: "Awaiting approval",
      },
      {
        id: "lifecycle:run-1:sessionChanged:3",
        activityKind: "sessionChanged",
        transition: "rotated",
        reason: "context limit",
      },
      {
        id: "lifecycle:run-1:artifactProduced:4",
        renderClass: "status",
        activityKind: "artifactProduced",
        detail: "Artifact: dist/report.md",
      },
      {
        id: "lifecycle:run-1:validationProduced:5",
        renderClass: "status",
        activityKind: "validationProduced",
        detail: "Validation failed (pnpm test): 1 failed",
      },
      {
        id: "lifecycle:run-1:runFailed:6",
        renderClass: "error",
        activityKind: "runFailed",
        detail: "engine stopped",
      },
      {
        id: "lifecycle:run-1:runCompleted:7",
        renderClass: "suppressed",
        activityKind: "runCompleted",
        detail: "Delivered",
      },
    ]);
    expect(items[2]).not.toHaveProperty("detail");
  });

  it("retains raw and alive liveness events while surfacing stalled liveness", () => {
    const items = projectActivity([
      event(1, { type: "rawEngineEvent", kind: "message_end", payloadJson: "{\"ok\":true}" }),
      event(2, { type: "liveness", state: "alive" }),
      event(3, { type: "liveness", state: "stalled" }),
    ]);

    expect(items).toMatchObject([
      {
        id: "raw:event-1",
        type: "raw",
        renderClass: "raw-rail",
        kind: "message_end",
        payloadJson: "{\"ok\":true}",
      },
      {
        id: "lifecycle:run-1:liveness:2",
        type: "lifecycle",
        renderClass: "suppressed",
        activityKind: "liveness",
        detail: "alive",
      },
      {
        id: "lifecycle:run-1:liveness:3",
        type: "lifecycle",
        renderClass: "status",
        activityKind: "liveness",
        detail: "Activity stalled",
      },
    ]);
  });

  it("replaces usage with the latest values per turn", () => {
    const items = projectActivity([
      event(1, {
        type: "usageUpdated",
        inputTokens: 10,
        outputTokens: 5,
        cacheReadTokens: 1,
        cacheWriteTokens: 2,
        totalTokens: 18,
      }),
      event(2, {
        type: "usageUpdated",
        inputTokens: 20,
        outputTokens: 10,
        cacheReadTokens: 4,
        cacheWriteTokens: 6,
        totalTokens: 40,
      }),
      event(
        3,
        {
          type: "usageUpdated",
          inputTokens: 3,
          outputTokens: 2,
          cacheReadTokens: 0,
          cacheWriteTokens: 0,
          totalTokens: 5,
        },
        { turnId: "turn-2" },
      ),
    ]);

    expect(items.filter((item) => item.type === "usage")).toMatchObject([
      { id: "usage:run-1:turn-1", inputTokens: 20, totalTokens: 40 },
      { id: "usage:run-1:turn-2", inputTokens: 3, totalTokens: 5 },
    ]);
  });

  it("separates coalesced identities across runs and turns", () => {
    const items = projectActivity([
      event(1, { type: "assistantDelta", text: "run one / turn one" }),
      event(2, { type: "assistantDelta", text: " / turn two" }, { turnId: "turn-2" }),
      event(3, { type: "assistantDelta", text: "run two" }, { runId: "run-2" }),
      event(4, { type: "thoughtDelta", text: "run-level" }, { turnId: undefined }),
    ]);

    expect(items.map((item) => item.id)).toEqual([
      "message:run-1:turn-1",
      "message:run-1:turn-2",
      "message:run-2:turn-1",
      "thought:run-1:run-1",
    ]);
    expect(items[3]).toMatchObject({ turnId: null, sessionId: "session-1" });
  });

  it("uses a stable run and sequence fallback for raw events without an event id", () => {
    const payload: WorkEventPayload = {
      type: "rawEngineEvent",
      kind: "unknown",
      payloadJson: "{}",
    };

    expect(projectActivity([event(8, payload, { eventId: undefined })])).toMatchObject([
      { id: "raw:run-1:8" },
    ]);
    expect(projectActivity([event(8, payload, { eventId: undefined })])).toMatchObject([
      { id: "raw:run-1:8" },
    ]);
  });

  it("sorts a cloned input and leaves the caller's event order unchanged", () => {
    const events = [
      event(2, { type: "assistantDelta", text: "B" }),
      event(1, { type: "assistantDelta", text: "A" }),
    ];

    expect(projectActivity(events)).toMatchObject([{ text: "AB" }]);
    expect(events.map(({ sequence }) => sequence)).toEqual([2, 1]);
  });

  it("reconstructs persisted events exactly like same-order live increments", () => {
    const events = [
      event(1, { type: "assistantDelta", text: "A" }),
      event(2, { type: "assistantDelta", text: "B" }),
      event(3, {
        type: "toolProgress",
        toolCallId: "t1",
        toolName: "bash",
        outputSummary: "running",
      }),
      event(4, {
        type: "toolFinished",
        toolCallId: "t1",
        toolName: "bash",
        outputSummary: "done",
        success: true,
      }),
      event(5, {
        type: "permissionRequested",
        requestId: "p1",
        title: "Write",
        detail: "report.md",
      }),
      event(6, { type: "permissionResolved", requestId: "p1", outcome: "allowed_once" }),
      event(7, { type: "rawEngineEvent", kind: "turn_end", payloadJson: "{}" }),
    ];
    const incremental = events.reduce(
      processActivityEvent,
      createEmptyActivityProjection(),
    ).items;
    const reconstructed = projectActivity(events);

    expect(reconstructed).toEqual(incremental);
    expect(reconstructed.filter((item) => item.type === "tool")).toMatchObject([
      {
        id: "tool:run-1:t1",
        workId: "work-1",
        runId: "run-1",
        turnId: "turn-1",
        sessionId: "session-1",
        status: "completed",
        result: "done",
      },
    ]);
    expect(reconstructed.filter((item) => item.type === "raw")).toMatchObject([
      {
        id: "raw:event-7",
        workId: "work-1",
        runId: "run-1",
        turnId: "turn-1",
        sessionId: "session-1",
        renderClass: "raw-rail",
        kind: "turn_end",
      },
    ]);
  });

  it("makes projection storage mutable once for a batch", () => {
    const NativeMap = globalThis.Map;
    const nativeArrayIterator = Array.prototype[Symbol.iterator];
    let copiedMapCount = 0;
    let iteratedArrayCount = 0;

    class CopyCountingMap<K, V> extends NativeMap<K, V> {
      constructor(entries?: readonly (readonly [K, V])[] | null) {
        super(entries);
        if (entries !== undefined && entries !== null) {
          copiedMapCount += 1;
        }
      }
    }

    vi.stubGlobal("Map", CopyCountingMap);
    Array.prototype[Symbol.iterator] = function countedIterator() {
      iteratedArrayCount += 1;
      return nativeArrayIterator.call(this);
    };

    try {
      projectActivity(
        Array.from({ length: 100 }, (_, index) =>
          event(index + 1, {
            type: "rawEngineEvent",
            kind: "raw",
            payloadJson: "{}",
          }),
        ),
      );
    } finally {
      Array.prototype[Symbol.iterator] = nativeArrayIterator;
      vi.unstubAllGlobals();
    }

    expect(copiedMapCount).toBeLessThanOrEqual(1);
    expect(iteratedArrayCount).toBeLessThanOrEqual(2);
  });
});

describe("processActivityEvent", () => {
  it("updates with copy-on-write and does not mutate prior projections", () => {
    const empty = createEmptyActivityProjection();
    const first = processActivityEvent(
      empty,
      event(1, { type: "assistantDelta", text: "A" }),
    );
    const second = processActivityEvent(
      first,
      event(2, { type: "assistantDelta", text: "B" }),
    );

    expect(empty.items).toEqual([]);
    expect(empty.indexById.size).toBe(0);
    expect(first.items).toMatchObject([{ text: "A" }]);
    expect(first.indexById.get("message:run-1:turn-1")).toBe(0);
    expect(second.items).toMatchObject([{ text: "AB" }]);
    expect(second.items).not.toBe(first.items);
    expect(second.indexById).not.toBe(first.indexById);
  });
});
