import { describe, expect, it } from "vitest";

import type { WorkEventEnvelope } from "../../bindings";
import { summarizeAssignments } from "./AssignmentInspector";

const event = (
  sequence: number,
  payload: WorkEventEnvelope["payload"],
): WorkEventEnvelope => ({
  version: 2,
  eventId: `event-${sequence}`,
  workId: "work-1",
  runId: null,
  turnId: null,
  sessionId: null,
  agentId: null,
  assignmentId: "assignmentId" in payload ? payload.assignmentId : null,
  causationId: null,
  correlationId: null,
  sequence,
  occurredAt: "2026-08-16T00:00:00Z",
  payload,
});

describe("summarizeAssignments", () => {
  it("keeps the latest event per assignment and derives a readable detail", () => {
    const rows = summarizeAssignments([
      event(1, {
        type: "assignmentQueued",
        assignmentId: "a1",
        assignedAgentId: "lead",
        title: "Research",
        priority: 10,
      }),
      event(2, {
        type: "assignmentStarted",
        assignmentId: "a1",
        agentInstanceId: "lead",
        agentSessionId: "s1",
        runId: "run-1",
      }),
      event(3, {
        type: "assignmentCompleted",
        assignmentId: "a1",
        agentInstanceId: "lead",
        agentSessionId: "s1",
        resultSummary: "done",
      }),
      event(4, {
        type: "assignmentDelegated",
        assignmentId: "a2",
        parentAssignmentId: "a1",
        assignedAgentId: "researcher",
        title: "Investigate",
      }),
    ]);

    expect(rows).toHaveLength(2);
    expect(rows.find((row) => row.id === "a1")).toMatchObject({
      status: "assignmentCompleted",
      detail: "done",
    });
    expect(rows.find((row) => row.id === "a2")).toMatchObject({
      status: "assignmentDelegated",
      detail: "Delegated: Investigate",
    });
  });

  it("ignores work-scoped events without an assignment id", () => {
    const rows = summarizeAssignments([
      event(1, {
        type: "workDeliveryCompleted",
        summary: "Delivered",
        artifacts: [],
        validation: [],
        limitations: [],
      }),
    ]);
    expect(rows).toHaveLength(0);
  });
});
