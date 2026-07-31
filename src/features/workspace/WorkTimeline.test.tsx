import { render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";

import type {
  MessageSummary,
  ResourceSummary,
  WorkEventEnvelope,
  WorkEventPayload,
} from "../../bindings";
import { i18n } from "../../i18n";
import { WorkTimeline } from "./WorkTimeline";

const event = (sequence: number, payload: WorkEventPayload): WorkEventEnvelope => ({
  version: 1,
  workId: "work-1",
  runId: "run-1",
  sequence,
  occurredAt: `2026-07-29T00:00:0${sequence}.000Z`,
  payload,
});

const message = (
  role: MessageSummary["role"],
  content: string,
  overrides: Partial<MessageSummary> = {},
): MessageSummary => ({
  id: `${role}-message`,
  workId: "work-1",
  runId: "run-1",
  role,
  content,
  resourceIds: [],
  createdAt: "2026-07-29T00:00:00.000Z",
  ...overrides,
});

const resource = (
  overrides: Partial<ResourceSummary> = {},
): ResourceSummary => ({
  id: "resource-1",
  originalName: "diagram.png",
  mediaType: "image/png",
  size: 68n,
  origin: "user_upload",
  status: "ready",
  failureCode: null,
  createdAt: "2026-07-29T00:00:00.000Z",
  ...overrides,
});

describe("WorkTimeline", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("zh-CN");
  });
  it("combines consecutive assistant deltas and renders their Markdown once", () => {
    const { container } = render(
      <WorkTimeline
        resources={[]}
        timeline={[
          event(1, { type: "assistantDelta", text: "可以使用 **" }),
          event(2, { type: "assistantDelta", text: "read" }),
          event(3, { type: "assistantDelta", text: "** 工具" }),
        ]}
      />,
    );

    const assistantMessages = container.querySelectorAll(
      ".timeline-event--assistant",
    );
    expect(assistantMessages).toHaveLength(1);
    expect(assistantMessages[0]).toHaveTextContent("可以使用 read 工具");
    expect(
      within(assistantMessages[0] as HTMLElement).getByText("read").tagName,
    ).toBe("STRONG");
  });

  it("groups one Run's tool activity into a compact expandable Pi summary", () => {
    const { container } = render(
      <WorkTimeline
        resources={[]}
        timeline={[
          event(1, { type: "runStarted", modelLabel: "GPT-5.6" }),
          event(2, {
            type: "toolStarted",
            toolCallId: "tool-1",
            toolName: "read_file",
            inputSummary: "读取 src/app.tsx",
          }),
          event(3, {
            type: "toolFinished",
            toolCallId: "tool-1",
            toolName: "read_file",
            outputSummary: "读取完成",
            success: true,
          }),
          event(4, {
            type: "runCompleted",
            summary: "检查完成",
            artifacts: ["report.md"],
            validation: ["类型检查通过"],
            limitations: [],
          }),
        ]}
      />,
    );

    const activity = container.querySelector(".agent-activity");
    expect(activity).not.toBeNull();
    expect(container.querySelectorAll(".agent-activity")).toHaveLength(1);
    expect(activity).toHaveTextContent("Pi 已完成 1 个操作");
    expect(activity).toHaveTextContent("读取 src/app.tsx");
    expect(activity).toHaveTextContent("类型检查通过");
    expect(activity).not.toHaveAttribute("open");
    expect(container.querySelectorAll(".timeline-event")).toHaveLength(0);
  });

  it("keeps the current Pi activity group expanded while a Run is active", () => {
    const { container } = render(
      <WorkTimeline
        resources={[]}
        timeline={[
          event(1, { type: "runStarted", modelLabel: "GPT-5.6" }),
          event(2, {
            type: "toolStarted",
            toolCallId: "tool-1",
            toolName: "search",
            inputSummary: "搜索认证模块",
          }),
        ]}
      />,
    );

    expect(container.querySelector(".agent-activity")).toHaveAttribute("open");
    expect(container.querySelector(".agent-activity")).toHaveTextContent("Pi 正在执行");
  });

  it("renders persisted assistant messages as assistant output instead of user bubbles", () => {
    const { container } = render(
      <WorkTimeline resources={[]} timeline={[message("user", "帮我检查项目"), message("assistant", "检查完成")]} />,
    );

    expect(container.querySelector(".timeline-user")).toHaveTextContent("帮我检查项目");
    expect(container.querySelector(".timeline-event--assistant")).toHaveTextContent("检查完成");
    expect(container.querySelector(".timeline-user")).not.toHaveTextContent("检查完成");
  });

  it("keeps following the latest content while messages and assistant deltas arrive", () => {
    const firstMessage = message("user", "继续执行");
    const rendered = render(<WorkTimeline resources={[]} timeline={[firstMessage]} />);
    const timeline = rendered.container.querySelector<HTMLElement>(".work-timeline");
    expect(timeline).not.toBeNull();
    Object.defineProperty(timeline, "scrollHeight", { configurable: true, value: 480 });
    timeline!.scrollTop = 0;

    rendered.rerender(
      <WorkTimeline resources={[]} timeline={[firstMessage, event(1, { type: "assistantDelta", text: "正在处理" })]} />,
    );
    expect(timeline!.scrollTop).toBe(480);

    Object.defineProperty(timeline, "scrollHeight", { configurable: true, value: 720 });
    rendered.rerender(
      <WorkTimeline timeline={[
        firstMessage,
        event(1, { type: "assistantDelta", text: "正在处理" }),
        event(2, { type: "assistantDelta", text: "，即将完成" }),
      ]} resources={[]} />,
    );
    expect(timeline!.scrollTop).toBe(720);
  });

  it("renders message attachments under their authoritative user message", () => {
    render(
      <WorkTimeline
        timeline={[
          message("user", "", {
            id: "message-1",
            resourceIds: ["resource-1"],
          }),
        ]}
        resources={[resource()]}
      />,
    );

    expect(screen.getByText("diagram.png")).toBeInTheDocument();
    expect(screen.queryByText("C:/private/diagram.png")).not.toBeInTheDocument();
    expect(screen.getByTestId("message:message-1")).toContainElement(
      screen.getByText("diagram.png"),
    );
  });
});
