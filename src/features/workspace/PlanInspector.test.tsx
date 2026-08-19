import { describe, expect, it } from "vitest";

import type { WorkEventEnvelope } from "../../bindings";
import { summarizePlan } from "./PlanInspector";

const event = (
  sequence: number,
  payload: WorkEventEnvelope["payload"],
): WorkEventEnvelope => ({
  version: 2,
  eventId: `event-${sequence}`,
  workId: "work-1",
  runId: null,
  sequence,
  occurredAt: "2026-08-16T00:00:00Z",
  payload,
});

describe("summarizePlan", () => {
  it("keeps the latest plan revision and decision version", () => {
    const view = summarizePlan([
      event(1, { type: "workPlanUpdated", planId: "plan", revision: 1, text: "Step A" }),
      event(2, { type: "workPlanUpdated", planId: "plan", revision: 2, text: "Step B" }),
      event(3, { type: "workDecisionRecorded", decisionId: "d1", summary: "Adopt", version: 1 }),
      event(4, { type: "workDecisionRecorded", decisionId: "d1", summary: "Adopt v2", version: 2 }),
    ]);

    expect(view.plans).toEqual(["Step B"]);
    expect(view.decisions).toEqual(["Adopt v2"]);
  });
});
