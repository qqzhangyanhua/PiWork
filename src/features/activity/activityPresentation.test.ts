/*
 * Regression invariants adapted from block/buzz at
 * 5bf78671f45178f8de02ba18d3d321cbbf19cd1f, Apache-2.0.
 * Original: desktop/src/features/agents/ui/agentSessionToolClassifier.ts.
 * PiWork changes: typed WorkEvent fixtures exercise Pi read/write/bash
 * descriptors without Buzz, Relay, or MCP-specific provider metadata.
 */
import type { WorkEventEnvelope, WorkEventPayload } from "../../bindings";
import { describe, expect, it } from "vitest";

import { projectActivity } from "./activityProjector";
import { describeTool } from "./activityPresentation";

const event = (
  sequence: number,
  payload: WorkEventPayload,
): WorkEventEnvelope => ({
  version: 2,
  eventId: `event-${sequence}`,
  workId: "work-1",
  runId: "run-1",
  turnId: "turn-1",
  sessionId: "session-1",
  sequence,
  occurredAt: `2026-08-09T00:00:${String(sequence).padStart(2, "0")}.000Z`,
  payload,
});

describe("describeTool", () => {
  it("describes a read path", () => {
    expect(describeTool("read", '{"path":"src/app.tsx"}')).toEqual({
      renderClass: "file-read",
      action: "read",
      object: "src/app.tsx",
      preview: null,
      tone: "read",
      groupKey: "read:read",
    });
  });

  it("describes an edit path", () => {
    expect(describeTool("edit", '{"path":"src/app.tsx"}')).toEqual({
      renderClass: "file-edit",
      action: "write",
      object: "src/app.tsx",
      preview: null,
      tone: "write",
      groupKey: "write:edit",
    });
  });

  it("describes a shell command", () => {
    expect(describeTool("bash", '{"command":"pnpm test"}')).toEqual({
      renderClass: "shell",
      action: "execute",
      object: "pnpm test",
      preview: null,
      tone: "admin",
      groupKey: "shell",
    });
  });

  it.each(["grep", "grep_files", "find", "find_files", "ls", "ls_dir"])(
    "classifies the %s read prefix",
    (toolName) => {
      expect(describeTool(toolName, '{"path":"src"}')).toMatchObject({
        renderClass: "file-read",
        action: "read",
        object: "src",
        tone: "read",
        groupKey: `read:${toolName}`,
      });
    },
  );

  it.each(["edit", "edit_file", "write", "write_file"])(
    "classifies the %s write prefix",
    (toolName) => {
      expect(describeTool(toolName, '{"path":"src/app.tsx"}')).toMatchObject({
        renderClass: "file-edit",
        action: "write",
        object: "src/app.tsx",
        tone: "write",
        groupKey: `write:${toolName}`,
      });
    },
  );

  it("accepts file_path and trims string fields", () => {
    expect(
      describeTool("read_file", '{"file_path":"  src/domain/work.ts  "}'),
    ).toMatchObject({
      object: "src/domain/work.ts",
      groupKey: "read:read_file",
    });
  });

  it.each(["not json", '["src/app.tsx"]', "null"])(
    "falls back to the raw summary for %s",
    (summary) => {
      expect(describeTool("read", summary)).toMatchObject({ object: summary });
      expect(describeTool("bash", summary)).toMatchObject({ object: summary });
    },
  );

  it("normalizes tool-name case and whitespace", () => {
    expect(
      describeTool("  READ_FILE  ", '{"path":"src/app.tsx"}'),
    ).toMatchObject({
      renderClass: "file-read",
      object: "src/app.tsx",
      groupKey: "read:read_file",
    });
  });

  it("returns stable generic descriptors for empty and unknown tools", () => {
    expect(describeTool("", "")).toEqual({
      renderClass: "generic",
      action: "invoke",
      object: "",
      preview: null,
      tone: "neutral",
      groupKey: "tool:",
    });
    expect(describeTool("custom_tool", '{"path":"ignored"}')).toEqual({
      renderClass: "generic",
      action: "invoke",
      object: "custom_tool",
      preview: null,
      tone: "neutral",
      groupKey: "tool:custom_tool",
    });
  });
});

describe("projected tool descriptors", () => {
  it("keeps classification tied to input while preview follows progress", () => {
    const items = projectActivity([
      event(1, {
        type: "toolStarted",
        toolCallId: "read-1",
        toolName: "read",
        inputSummary: '{"path":"src/app.tsx"}',
      }),
      event(2, {
        type: "toolProgress",
        toolCallId: "read-1",
        toolName: "read",
        outputSummary: "12 lines",
      }),
    ]);

    expect(items).toMatchObject([
      {
        type: "tool",
        renderClass: "file-read",
        status: "executing",
        result: "12 lines",
        descriptor: {
          renderClass: "file-read",
          action: "read",
          object: "src/app.tsx",
          preview: "12 lines",
          tone: "read",
          groupKey: "read:read",
        },
      },
    ]);
  });

  it("enriches a terminal item descriptor without losing its outcome", () => {
    const items = projectActivity([
      event(1, {
        type: "toolFinished",
        toolCallId: "shell-1",
        toolName: "",
        outputSummary: "done",
        success: true,
      }),
      event(2, {
        type: "toolStarted",
        toolCallId: "shell-1",
        toolName: "bash",
        inputSummary: '{"command":"pnpm test"}',
      }),
    ]);

    expect(items).toMatchObject([
      {
        type: "tool",
        renderClass: "shell",
        status: "completed",
        result: "done",
        isError: false,
        descriptor: {
          renderClass: "shell",
          object: "pnpm test",
          preview: "done",
          groupKey: "shell",
        },
      },
    ]);
  });
});
