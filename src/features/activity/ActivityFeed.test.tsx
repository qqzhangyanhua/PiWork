import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";

import type { WorkEventEnvelope, WorkEventPayload } from "../../bindings";
import { i18n } from "../../i18n";
import { projectActivity } from "./activityProjector";
import type { ActivityItem, ToolStatus } from "./activityTypes";
import { ActivityFeed } from "./ActivityFeed";

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
  correlationId: "run-1",
  sequence,
  occurredAt: `2026-08-09T00:00:${String(sequence).padStart(2, "0")}.000Z`,
  payload,
});

const tool = (
  id: string,
  status: ToolStatus,
  object: string,
  overrides: Partial<Extract<ActivityItem, { type: "tool" }>> = {},
): Extract<ActivityItem, { type: "tool" }> => ({
  id,
  timestamp: "2026-08-09T00:00:00.000Z",
  workId: "work-1",
  runId: "run-1",
  turnId: "turn-1",
  sessionId: "session-1",
  agentId: null,
  assignmentId: null,
  type: "tool",
  renderClass: status === "failed" ? "error" : "file-read",
  toolCallId: id,
  toolName: "read_file",
  status,
  input: object,
  result: status === "completed" ? "done" : "",
  isError: status === "failed",
  descriptor: {
    renderClass: status === "failed" ? "error" : "file-read",
    action: "read",
    object,
    preview: status === "completed" ? "done" : null,
    tone: "read",
    groupKey: null,
  },
  ...overrides,
});

describe("ActivityFeed", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("zh-CN");
  });

  it("shows one consolidated tool row and hides raw events from the primary feed", () => {
    render(
      <ActivityFeed
        items={projectActivity([
          event(1, {
            type: "toolStarted",
            toolCallId: "t1",
            toolName: "read",
            inputSummary: '{"path":"src/app.tsx"}',
          }),
          event(2, {
            type: "toolProgress",
            toolCallId: "t1",
            toolName: "read",
            outputSummary: "half",
          }),
          event(3, {
            type: "toolFinished",
            toolCallId: "t1",
            toolName: "read",
            outputSummary: "done",
            success: true,
          }),
          event(4, {
            type: "rawEngineEvent",
            kind: "queue_update",
            payloadJson: "{}",
          }),
        ])}
      />,
    );

    expect(screen.getAllByText("src/app.tsx")).toHaveLength(1);
    expect(screen.getByText("done")).toBeVisible();
    expect(screen.queryByText("queue_update")).not.toBeInTheDocument();
  });

  it("keeps thought and plan collapsed but makes permission requests prominent", async () => {
    const user = userEvent.setup();
    const items = projectActivity([
      event(1, { type: "thoughtDelta", text: "private reasoning detail" }),
      event(2, {
        type: "planChanged",
        planId: "main",
        revision: 1,
        text: "- verify",
      }),
      event(3, {
        type: "permissionRequested",
        requestId: "permission-1",
        toolCallId: "tool-1",
        title: "Write file",
        detail: "src/app.tsx",
      }),
    ]);

    render(<ActivityFeed items={items} />);

    expect(screen.getByText("需要权限")).toBeVisible();
    expect(screen.getByRole("alert")).toHaveTextContent("Write file");
    expect(screen.queryByText("private reasoning detail")).not.toBeVisible();
    expect(screen.queryByText("- verify")).not.toBeVisible();

    const thoughtSummary = screen.getByRole("button", { name: "展开思考" });
    expect(thoughtSummary).toHaveAttribute("aria-expanded", "false");
    await user.click(thoughtSummary);

    expect(screen.getByText("private reasoning detail")).toBeVisible();
    expect(screen.getByRole("button", { name: "收起思考" })).toHaveAttribute(
      "aria-expanded",
      "true",
    );
    expect(screen.getByRole("button", { name: "展开计划" })).toBeVisible();
  });

  it("omits message, usage, raw, suppressed, and duplicate delivery activity", () => {
    render(
      <ActivityFeed
        items={projectActivity([
          event(1, { type: "runStarted", modelLabel: "private-model" }),
          event(2, { type: "assistantDelta", text: "assistant body" }),
          event(3, {
            type: "usageUpdated",
            inputTokens: 11,
            outputTokens: 7,
            cacheReadTokens: 3,
            cacheWriteTokens: 2,
            totalTokens: 23,
          }),
          event(4, { type: "liveness", state: "alive" }),
          event(5, {
            type: "rawEngineEvent",
            kind: "hidden_raw_kind",
            payloadJson: '{"secret":true}',
          }),
          event(6, { type: "artifactProduced", path: "report.md" }),
          event(7, {
            type: "validationProduced",
            command: "pnpm test",
            success: true,
            summary: "23 tests passed",
          }),
          event(8, {
            type: "runCompleted",
            summary: "delivery summary",
            artifacts: [],
            validation: [],
            limitations: [],
          }),
        ])}
      />,
    );

    for (const hiddenText of [
      "private-model",
      "assistant body",
      "23",
      "alive",
      "hidden_raw_kind",
      "report.md",
      "23 tests passed",
      "delivery summary",
    ]) {
      expect(screen.queryByText(hiddenText)).not.toBeInTheDocument();
    }
  });

  it("pairs every tool state with visible text and a non-color icon", () => {
    const states: Array<[ToolStatus, string, string]> = [
      ["pending", "等待执行", "pending.ts"],
      ["executing", "正在执行", "running.ts"],
      ["completed", "已完成", "completed.ts"],
      ["failed", "失败", "failed.ts"],
    ];

    const { container } = render(
      <ActivityFeed
        items={states.map(([status, , object]) =>
          tool(`tool-${status}`, status, object),
        )}
      />,
    );

    for (const [status, label, object] of states) {
      const row = screen.getByText(object).closest(".activity-feed__tool");
      expect(row).not.toBeNull();
      const marker = row!.querySelector(`[data-status="${status}"]`);
      expect(marker).toHaveTextContent(label);
      expect(marker?.querySelector("svg")).not.toBeNull();
    }
  });

  it("summarizes successful tool bursts and exposes their children in details", async () => {
    const user = userEvent.setup();
    const groupKey = "read:read_file";
    render(
      <ActivityFeed
        items={[
          tool("read-1", "completed", "src/one.ts", {
            descriptor: {
              renderClass: "file-read",
              action: "read",
              object: "src/one.ts",
              preview: "done",
              tone: "read",
              groupKey,
            },
          }),
          tool("read-2", "completed", "src/two.ts", {
            descriptor: {
              renderClass: "file-read",
              action: "read",
              object: "src/two.ts",
              preview: "done",
              tone: "read",
              groupKey,
            },
          }),
        ]}
      />,
    );

    const summary = screen.getByRole("button", { name: "读取了 2 项" });
    const details = summary.closest("details");
    expect(details).not.toBeNull();
    expect(summary).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByText("src/one.ts")).not.toBeVisible();
    expect(screen.getByText("src/two.ts")).not.toBeVisible();

    await user.click(summary);

    expect(details).toHaveAttribute("open");
    expect(summary).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText("src/one.ts")).toBeVisible();
    expect(screen.getByText("src/two.ts")).toBeVisible();
  });

  it("never folds failed tools into a successful burst", () => {
    const groupKey = "read:read_file";
    render(
      <ActivityFeed
        items={[
          tool("failed-1", "failed", "src/one.ts", {
            descriptor: {
              renderClass: "error",
              action: "read",
              object: "src/one.ts",
              preview: "denied",
              tone: "read",
              groupKey,
            },
          }),
          tool("failed-2", "failed", "src/two.ts", {
            descriptor: {
              renderClass: "error",
              action: "read",
              object: "src/two.ts",
              preview: "denied",
              tone: "read",
              groupKey,
            },
          }),
        ]}
      />,
    );

    expect(screen.queryByRole("button", { name: "读取了 2 项" })).not.toBeInTheDocument();
    expect(screen.getAllByText("失败")).toHaveLength(2);
    expect(screen.getByText("src/one.ts")).toBeVisible();
    expect(screen.getByText("src/two.ts")).toBeVisible();
  });

  it("announces resolved permission outcomes without losing request context", () => {
    render(
      <ActivityFeed
        items={projectActivity([
          event(1, {
            type: "permissionRequested",
            requestId: "permission-1",
            title: "Write file",
            detail: "src/app.tsx",
          }),
          event(2, {
            type: "permissionResolved",
            requestId: "permission-1",
            outcome: "allowed_once",
          }),
        ])}
      />,
    );

    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Write file");
    expect(alert).toHaveTextContent("src/app.tsx");
    expect(alert).toHaveTextContent("本次允许");
    expect(screen.queryByText("需要权限")).not.toBeInTheDocument();
  });

  it("shows user-relevant lifecycle transitions while hiding routine and duplicate events", () => {
    render(
      <ActivityFeed
        items={projectActivity([
          event(1, { type: "runStarted", modelLabel: "private-model" }),
          event(2, { type: "liveness", state: "stalled" }),
          event(3, { type: "waiting", reason: "等待网络恢复" }),
          event(4, {
            type: "sessionChanged",
            transition: "rotated",
            reason: "上下文已满",
          }),
          event(5, { type: "runFailed", message: "sensitive stack trace" }),
          event(6, {
            type: "runCompleted",
            summary: "routine completion",
            artifacts: [],
            validation: [],
            limitations: [],
          }),
        ])}
      />,
    );

    expect(screen.getByText("等待继续")).toBeVisible();
    expect(screen.getByText("等待网络恢复")).toBeVisible();
    expect(screen.getByText("会话已切换")).toBeVisible();
    expect(screen.getByText(/上下文已满/u)).toBeVisible();
    expect(screen.getByText("执行未完成")).toBeVisible();
    expect(screen.queryByText("private-model")).not.toBeInTheDocument();
    expect(screen.queryByText("Activity stalled")).not.toBeInTheDocument();
    expect(screen.queryByText("sensitive stack trace")).not.toBeInTheDocument();
    expect(screen.queryByText("routine completion")).not.toBeInTheDocument();
  });

  it("gives every disclosure an accessible summary and permission a live alert", () => {
    const { container } = render(
      <ActivityFeed
        items={[
          ...projectActivity([
            event(1, { type: "thoughtDelta", text: "reasoning" }),
            event(2, {
              type: "planChanged",
              planId: "main",
              revision: 1,
              text: "plan",
            }),
            event(3, {
              type: "permissionRequested",
              requestId: "permission-1",
              title: "Write file",
              detail: "src/app.tsx",
            }),
          ]),
          tool("read-1", "completed", "one", {
            descriptor: {
              renderClass: "file-read",
              action: "read",
              object: "one",
              preview: null,
              tone: "read",
              groupKey: "read:read_file",
            },
          }),
          tool("read-2", "completed", "two", {
            descriptor: {
              renderClass: "file-read",
              action: "read",
              object: "two",
              preview: null,
              tone: "read",
              groupKey: "read:read_file",
            },
          }),
        ]}
      />,
    );

    expect(screen.getByRole("list", { name: "活动记录" })).toBeVisible();
    expect(screen.getByRole("button", { name: "展开思考" })).toBeVisible();
    expect(screen.getByRole("button", { name: "展开计划" })).toBeVisible();
    expect(screen.getByRole("button", { name: "读取了 2 项" })).toBeVisible();
    expect(container.querySelectorAll("details > summary")).toHaveLength(3);
    expect(screen.getByRole("alert")).toHaveTextContent("需要权限");
  });
});
