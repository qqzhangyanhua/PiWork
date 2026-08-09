import { fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

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

  it("projects each Run into one assistant message and one consolidated activity feed", () => {
    const { container } = render(
      <WorkTimeline
        resources={[]}
        timeline={[
          event(1, { type: "runStarted", modelLabel: "GPT-5.6" }),
          event(2, { type: "assistantDelta", text: "正在**检查" }),
          event(3, {
            type: "toolStarted",
            toolCallId: "tool-1",
            toolName: "read_file",
            inputSummary: '{"path":"src/app.tsx"}',
          }),
          event(4, {
            type: "toolProgress",
            toolCallId: "tool-1",
            toolName: "read_file",
            outputSummary: "读取一半",
          }),
          event(5, {
            type: "toolFinished",
            toolCallId: "tool-1",
            toolName: "read_file",
            outputSummary: "读取完成",
            success: true,
          }),
          event(6, { type: "assistantDelta", text: "完成**" }),
          event(7, {
            type: "rawEngineEvent",
            kind: "queue_update",
            payloadJson: '{"size":1}',
          }),
        ]}
      />,
    );

    expect(container.querySelectorAll(".timeline-event--assistant")).toHaveLength(1);
    expect(screen.getByText("检查完成").tagName).toBe("STRONG");
    fireEvent.click(screen.getByRole("button", { name: "展开执行详情" }));
    expect(container.querySelectorAll(".activity-feed__tool")).toHaveLength(1);
    expect(screen.getAllByText("src/app.tsx")).toHaveLength(1);
    expect(screen.getByText("读取完成")).toBeVisible();
    expect(screen.queryByText("queue_update")).not.toBeInTheDocument();
  });

  it("separates process details from delivery without exposing Run containers", () => {
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

    expect(container.querySelector("details.timeline-run")).toBeNull();
    expect(screen.queryByText(/^Run\s+\d+/u)).not.toBeInTheDocument();
    const activity = container.querySelector(".execution-progress");
    const delivery = container.querySelector(".timeline-delivery");
    expect(activity).not.toBeNull();
    expect(container.querySelectorAll(".execution-progress")).toHaveLength(1);
    expect(activity).toHaveTextContent("Pi 已完成 1 个操作");
    expect(activity).not.toHaveTextContent("读取 src/app.tsx");
    fireEvent.click(screen.getByRole("button", { name: "展开执行详情" }));
    expect(activity).toHaveTextContent("读取 src/app.tsx");
    expect(activity).not.toHaveTextContent("类型检查通过");
    expect(delivery).toHaveTextContent("检查完成");
    expect(delivery).toHaveTextContent("report.md");
    expect(delivery).toHaveTextContent("类型检查通过");
  });

  it("shows the authoritative turn time without naming the internal Run", () => {
    const { container } = render(
      <WorkTimeline
        resources={[]}
        timeline={[event(1, { type: "runStarted", modelLabel: "GPT-5.6" })]}
      />,
    );

    const turnTime = container.querySelector<HTMLTimeElement>(".conversation-turn__time");
    expect(turnTime).not.toBeNull();
    expect(turnTime).toHaveAttribute("dateTime", "2026-07-29T00:00:01.000Z");
    expect(turnTime).not.toBeEmptyDOMElement();
    expect(screen.queryByText(/Run/u)).not.toBeInTheDocument();
  });

  it("renders a failed Run as a safe failure node with diagnostics", () => {
    const onOpenDiagnostics = vi.fn();
    const { container } = render(
      <WorkTimeline
        onOpenDiagnostics={onOpenDiagnostics}
        resources={[]}
        timeline={[
          event(1, { type: "runStarted", modelLabel: "GPT-5.6" }),
          event(2, { type: "runFailed", message: "网络不可用" }),
        ]}
      />,
    );

    expect(container.querySelector(".timeline-failure")).toHaveTextContent("本次执行未完成。确认本地执行环境可用后重试。");
    expect(container.querySelector(".timeline-failure")).not.toHaveTextContent("网络不可用");
    expect(container.querySelector(".execution-progress")).not.toHaveTextContent("网络不可用");
    fireEvent.click(screen.getByRole("button", { name: "打开诊断" }));
    expect(onOpenDiagnostics).toHaveBeenCalledTimes(1);
  });

  it("keeps the current Pi activity group collapsed while a Run is active", () => {
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

    expect(container.querySelector(".execution-progress")).toHaveTextContent("Pi 正在执行");
    expect(container.querySelector(".execution-progress__summary")).toHaveAttribute(
      "aria-expanded",
      "false",
    );
  });

  it("renders persisted assistant messages as assistant output instead of user bubbles", () => {
    const { container } = render(
      <WorkTimeline resources={[]} timeline={[message("user", "帮我检查项目"), message("assistant", "检查完成")]} />,
    );

    expect(container.querySelector(".timeline-user")).toHaveTextContent("帮我检查项目");
    expect(container.querySelector(".timeline-event--assistant")).toHaveTextContent("检查完成");
    expect(container.querySelector(".timeline-user")).not.toHaveTextContent("检查完成");
  });

  it("follows new output near the bottom but preserves an active reading position", () => {
    const firstMessage = message("user", "继续执行");
    const rendered = render(<WorkTimeline resources={[]} timeline={[firstMessage]} />);
    const timeline = rendered.container.querySelector<HTMLElement>(".work-timeline");
    expect(timeline).not.toBeNull();
    Object.defineProperty(timeline, "clientHeight", { configurable: true, value: 200 });
    Object.defineProperty(timeline, "scrollHeight", { configurable: true, value: 480 });
    timeline!.scrollTop = 260;
    fireEvent.scroll(timeline!);

    rendered.rerender(
      <WorkTimeline resources={[]} timeline={[firstMessage, event(1, { type: "assistantDelta", text: "正在处理" })]} />,
    );
    expect(timeline!.scrollTop).toBe(480);

    Object.defineProperty(timeline, "scrollHeight", { configurable: true, value: 720 });
    timeline!.scrollTop = 100;
    fireEvent.scroll(timeline!);
    rendered.rerender(
      <WorkTimeline timeline={[
        firstMessage,
        event(1, { type: "assistantDelta", text: "正在处理" }),
        event(2, { type: "assistantDelta", text: "，即将完成" }),
      ]} resources={[]} />,
    );
    expect(timeline!.scrollTop).toBe(100);
    expect(screen.getByRole("button", { name: "查看新输出" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "查看新输出" }));
    expect(timeline!.scrollTop).toBe(720);
    expect(screen.queryByRole("button", { name: "查看新输出" })).not.toBeInTheDocument();
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

  it("renders every historical interaction as one continuous visible conversation", () => {
    const { container } = render(
      <WorkTimeline
        resources={[]}
        timeline={[
          message("user", "第一次指令", { id: "message-1", runId: "run-1" }),
          event(1, {
            type: "runCompleted",
            summary: "第一次完成",
            artifacts: [],
            validation: [],
            limitations: [],
          }),
          message("user", "第二次指令", {
            id: "message-2",
            runId: "run-2",
            createdAt: "2026-07-29T01:00:00.000Z",
          }),
          {
            ...event(2, { type: "runStarted", modelLabel: "GPT-5.6" }),
            runId: "run-2",
            occurredAt: "2026-07-29T01:00:01.000Z",
          },
        ]}
      />,
    );

    expect(container.querySelectorAll(".conversation-thread")).toHaveLength(1);
    expect(container.querySelector(".conversation-turn")).toBeNull();
    expect(container.querySelector("details.timeline-run")).toBeNull();
    expect(screen.getByText("第一次指令")).toBeVisible();
    expect(screen.getByText("第一次完成")).toBeVisible();
    expect(screen.getByText("第二次指令")).toBeVisible();
    expect(screen.queryByText(/^Run\s+\d+/u)).not.toBeInTheDocument();
  });

  it("does not surface legacy Run boilerplate as a duplicate delivery", () => {
    const { container } = render(
      <WorkTimeline
        resources={[]}
        timeline={[
          message("user", "已经安装好了"),
          event(1, { type: "assistantDelta", text: "请重启 PiWork 后继续扫描。" }),
          event(2, {
            type: "runCompleted",
            summary: "Pi completed this Run with 1 tool call",
            artifacts: [],
            validation: [],
            limitations: [],
          }),
        ]}
      />,
    );

    expect(screen.getByText("请重启 PiWork 后继续扫描。")).toBeVisible();
    expect(screen.queryByText(/completed this Run/u)).not.toBeInTheDocument();
    expect(container.querySelector(".timeline-delivery")).toBeNull();
  });
});
