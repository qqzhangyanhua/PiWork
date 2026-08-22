import { describe, expect, it } from "vitest";

import type {
  WorkEventEnvelope,
  WorkEventPayload,
} from "../../bindings";
import { buildExecutionProgress } from "./executionProgress";

const event = (
  sequence: number,
  payload: WorkEventPayload,
): WorkEventEnvelope => ({
  version: 1,
  workId: "work-1",
  runId: "run-1",
  sequence,
  occurredAt: `2026-08-02T00:00:${String(sequence).padStart(2, "0")}.000Z`,
  payload,
});

const phase = (
  model: ReturnType<typeof buildExecutionProgress>,
  id: (typeof model.phases)[number]["id"],
) => model.phases.find((candidate) => candidate.id === id);

describe("buildExecutionProgress", () => {
  it("keeps preparation active until the first tool starts", () => {
    const model = buildExecutionProgress([
      event(1, { type: "runStarted", modelLabel: "GPT-5.6" }),
    ]);

    expect(model).toMatchObject({
      status: "preparing",
      currentPhase: "prepare",
      toolCount: 0,
      failedToolCount: 0,
    });
    expect(phase(model, "prepare")?.status).toBe("active");
    expect(phase(model, "analyze")?.status).toBe("pending");
  });

  it("maps tools to phases without regressing after execution begins", () => {
    const model = buildExecutionProgress([
      event(1, { type: "runStarted", modelLabel: "GPT-5.6" }),
      event(2, {
        type: "toolStarted",
        toolCallId: "read-1",
        toolName: "read_file",
        inputSummary: "src/app.tsx",
      }),
      event(3, {
        type: "toolFinished",
        toolCallId: "read-1",
        toolName: "read_file",
        outputSummary: "done",
        success: true,
      }),
      event(4, {
        type: "toolStarted",
        toolCallId: "edit-1",
        toolName: "edit",
        inputSummary: "src/app.tsx",
      }),
      event(5, {
        type: "toolStarted",
        toolCallId: "read-2",
        toolName: "read",
        inputSummary: "src/styles.css",
      }),
    ]);

    expect(model.status).toBe("running");
    expect(model.currentPhase).toBe("execute");
    expect(phase(model, "prepare")?.status).toBe("completed");
    expect(phase(model, "analyze")?.status).toBe("completed");
    expect(phase(model, "execute")?.status).toBe("active");
  });

  it("recognizes validation bash commands", () => {
    const model = buildExecutionProgress([
      event(1, {
        type: "toolStarted",
        toolCallId: "test-1",
        toolName: "bash",
        inputSummary: '{"command":"pnpm test -- --run"}',
      }),
    ]);

    expect(model.currentPhase).toBe("validate");
    expect(phase(model, "validate")).toMatchObject({
      status: "active",
      toolCount: 1,
    });
  });

  it("creates one executing validation tool when progress arrives first", () => {
    const model = buildExecutionProgress([
      event(1, {
        type: "toolProgress",
        toolCallId: "test-1",
        toolName: "bash",
        outputSummary: '{"command":"pnpm test -- --run"}',
      }),
    ]);

    expect(model).toMatchObject({
      status: "running",
      currentPhase: "validate",
      toolCount: 1,
      failedToolCount: 0,
    });
    expect(phase(model, "validate")).toMatchObject({
      status: "active",
      toolCount: 1,
    });
  });

  it("lets authoritative late start input refine a progress-first phase", () => {
    const model = buildExecutionProgress([
      event(1, {
        type: "toolProgress",
        toolCallId: "test-1",
        toolName: "bash",
        outputSummary: "running",
      }),
      event(2, {
        type: "toolStarted",
        toolCallId: "test-1",
        toolName: "bash",
        inputSummary: '{"command":"pnpm test"}',
      }),
    ]);

    expect(model).toMatchObject({
      status: "running",
      currentPhase: "validate",
      toolCount: 1,
      failedToolCount: 0,
    });
    expect(phase(model, "execute")?.toolCount).toBe(0);
    expect(phase(model, "validate")).toMatchObject({
      status: "active",
      toolCount: 1,
    });
  });

  it("refines a terminal tool phase from late input without erasing failure", () => {
    const model = buildExecutionProgress([
      event(1, {
        type: "toolFinished",
        toolCallId: "test-1",
        toolName: "bash",
        outputSummary: "exit 1",
        success: false,
      }),
      event(2, {
        type: "toolStarted",
        toolCallId: "test-1",
        toolName: "bash",
        inputSummary: '{"command":"pnpm test"}',
      }),
      event(3, {
        type: "runCompleted",
        summary: "recovered",
        artifacts: [],
        validation: ["pnpm test"],
        limitations: [],
      }),
    ]);

    expect(model).toMatchObject({
      status: "completed",
      toolCount: 1,
      failedToolCount: 1,
    });
    expect(phase(model, "execute")?.status).toBe("skipped");
    expect(phase(model, "validate")).toMatchObject({
      status: "completed",
      toolCount: 1,
      failedToolCount: 1,
    });
  });

  it("counts pending progress and finish as one completed tool", () => {
    const model = buildExecutionProgress([
      event(1, {
        type: "toolPending",
        toolCallId: "test-1",
        toolName: "bash",
        inputSummary: '{"command":"pnpm test"}',
      }),
      event(2, {
        type: "toolProgress",
        toolCallId: "test-1",
        toolName: "bash",
        outputSummary: "running",
      }),
      event(3, {
        type: "toolFinished",
        toolCallId: "test-1",
        toolName: "bash",
        outputSummary: "passed",
        success: true,
      }),
      event(4, {
        type: "runCompleted",
        summary: "done",
        artifacts: [],
        validation: ["pnpm test"],
        limitations: [],
      }),
    ]);

    expect(model).toMatchObject({
      status: "completed",
      currentPhase: "deliver",
      toolCount: 1,
      failedToolCount: 0,
    });
    expect(phase(model, "validate")).toMatchObject({
      status: "completed",
      toolCount: 1,
    });
  });

  it("does not let late progress erase a terminal tool failure", () => {
    const model = buildExecutionProgress([
      event(1, {
        type: "toolFinished",
        toolCallId: "edit-1",
        toolName: "edit",
        outputSummary: "failed first",
        success: false,
      }),
      event(2, {
        type: "toolProgress",
        toolCallId: "edit-1",
        toolName: "edit",
        outputSummary: "late progress",
      }),
      event(3, {
        type: "runCompleted",
        summary: "recovered",
        artifacts: [],
        validation: [],
        limitations: [],
      }),
    ]);

    expect(model).toMatchObject({
      status: "completed",
      toolCount: 1,
      failedToolCount: 1,
    });
    expect(phase(model, "execute")).toMatchObject({
      status: "completed",
      toolCount: 1,
      failedToolCount: 1,
    });
  });

  it("counts repeated start and finish records as one tool", () => {
    const started = event(1, {
      type: "toolStarted",
      toolCallId: "write-1",
      toolName: "write",
      inputSummary: "src/new.ts",
    });
    const finished = event(2, {
      type: "toolFinished",
      toolCallId: "write-1",
      toolName: "write",
      outputSummary: "created",
      success: true,
    });

    const model = buildExecutionProgress([
      started,
      { ...started, sequence: 3 },
      finished,
      { ...finished, sequence: 4 },
    ]);

    expect(model.toolCount).toBe(1);
    expect(phase(model, "execute")?.toolCount).toBe(1);
  });

  it("preserves a recovered tool failure while completion remains authoritative", () => {
    const model = buildExecutionProgress([
      event(1, { type: "runStarted", modelLabel: "GPT-5.6" }),
      event(2, {
        type: "toolStarted",
        toolCallId: "edit-1",
        toolName: "edit",
        inputSummary: "src/app.tsx",
      }),
      event(3, {
        type: "toolFinished",
        toolCallId: "edit-1",
        toolName: "edit",
        outputSummary: "first attempt failed",
        success: false,
      }),
      event(4, {
        type: "toolStarted",
        toolCallId: "test-1",
        toolName: "bash",
        inputSummary: '{"command":"pnpm test"}',
      }),
      event(5, {
        type: "toolFinished",
        toolCallId: "test-1",
        toolName: "bash",
        outputSummary: "passed",
        success: true,
      }),
      event(6, {
        type: "runCompleted",
        summary: "done",
        artifacts: [],
        validation: ["pnpm test"],
        limitations: [],
      }),
    ]);

    expect(model).toMatchObject({
      status: "completed",
      currentPhase: "deliver",
      toolCount: 2,
      failedToolCount: 1,
      failureMessage: null,
    });
    expect(phase(model, "execute")).toMatchObject({
      status: "completed",
      failedToolCount: 1,
    });
    expect(phase(model, "validate")?.status).toBe("completed");
    expect(phase(model, "analyze")?.status).toBe("skipped");
    expect(phase(model, "deliver")?.status).toBe("completed");
  });

  it("uses runFailed as the final failure even when runStarted is missing", () => {
    const model = buildExecutionProgress([
      event(1, {
        type: "toolStarted",
        toolCallId: "edit-1",
        toolName: "edit",
        inputSummary: "src/app.tsx",
      }),
      event(2, { type: "runFailed", message: "failed safely" }),
    ]);

    expect(model).toMatchObject({
      status: "failed",
      currentPhase: "deliver",
      failureMessage: "failed safely",
    });
    expect(phase(model, "prepare")?.status).toBe("completed");
    expect(phase(model, "execute")?.status).toBe("failed");
    expect(phase(model, "deliver")?.status).toBe("failed");
  });

  it("treats host-tool assignment completion as a terminal Run signal", () => {
    const model = buildExecutionProgress([
      event(1, { type: "runStarted", modelLabel: "GPT-5.6" }),
      event(2, {
        type: "toolStarted",
        toolCallId: "delivery-1",
        toolName: "complete_work_delivery",
        inputSummary: "final delivery",
      }),
      event(3, {
        type: "assignmentCompleted",
        assignmentId: "assignment-1",
        agentInstanceId: "agent-1",
        agentSessionId: "session-1",
        resultSummary: "done",
      }),
    ]);

    expect(model).toMatchObject({
      status: "completed",
      currentPhase: "deliver",
      toolCount: 1,
      failureMessage: null,
    });
    expect(phase(model, "execute")?.status).toBe("completed");
    expect(phase(model, "deliver")?.status).toBe("completed");
  });
});
