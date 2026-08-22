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
    expect(screen.getByRole("button", { name: "收起执行详情" })).toHaveAttribute(
      "aria-expanded",
      "true",
    );
    expect(container.querySelectorAll(".activity-feed__tool")).toHaveLength(1);
    expect(screen.getAllByText("src/app.tsx")).toHaveLength(1);
    expect(screen.getByText("读取完成")).toBeVisible();
    expect(screen.queryByText("queue_update")).not.toBeInTheDocument();
  });

  it("surfaces an unresolved permission alert without opening execution details", () => {
    render(
      <WorkTimeline
        resources={[]}
        timeline={[
          event(1, { type: "runStarted", modelLabel: "GPT-5.6" }),
          event(2, {
            type: "permissionRequested",
            requestId: "permission-1",
            toolCallId: "tool-1",
            title: "Write file",
            detail: "src/app.tsx",
          }),
        ]}
      />,
    );

    const alert = screen.getByRole("alert");
    expect(alert).toBeVisible();
    expect(alert).toHaveTextContent("需要权限");
    expect(alert).toHaveTextContent("Write file");
    expect(alert.closest(".execution-progress")).toBeNull();
    expect(screen.getAllByText("需要权限")).toHaveLength(1);
  });

  it("keeps resolved permissions inside details without an emergency alert role", () => {
    render(
      <WorkTimeline
        resources={[]}
        timeline={[
          event(1, { type: "runStarted", modelLabel: "GPT-5.6" }),
          event(2, {
            type: "permissionRequested",
            requestId: "permission-1",
            title: "Write file",
            detail: "src/app.tsx",
          }),
          event(3, {
            type: "permissionResolved",
            requestId: "permission-1",
            outcome: "allowed_once",
          }),
        ]}
      />,
    );

    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByText("本次允许")).toBeVisible();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it.each([
    {
      label: "raw-only",
      payload: {
        type: "rawEngineEvent",
        kind: "queue_update",
        payloadJson: '{"size":1}',
      } as WorkEventPayload,
    },
    {
      label: "usage-only",
      payload: {
        type: "usageUpdated",
        inputTokens: 10,
        outputTokens: 5,
        cacheReadTokens: 2,
        cacheWriteTokens: 1,
        totalTokens: 18,
      } as WorkEventPayload,
    },
  ])("does not render an empty progress card for a $label journal", ({ payload }) => {
    const { container } = render(
      <WorkTimeline resources={[]} timeline={[event(1, payload)]} />,
    );

    expect(container.querySelector(".execution-progress")).toBeNull();
    expect(
      screen.queryByRole("button", { name: "展开执行详情" }),
    ).not.toBeInTheDocument();
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
    expect(activity).toHaveTextContent("CoDo 已完成 1 个操作");
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

  it("shows preparation immediately after a Run starts", () => {
    render(
      <WorkTimeline
        resources={[]}
        timeline={[event(1, { type: "runStarted", modelLabel: "GPT-5.6" })]}
      />,
    );

    expect(screen.getByRole("status", { name: "执行进度" })).toHaveAttribute(
      "data-status",
      "preparing",
    );
    expect(screen.getByText("正在理解你的要求并准备执行")).toBeVisible();
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

  it("announces a failed Run once while keeping its execution record visible", () => {
    const { container } = render(
      <WorkTimeline
        resources={[]}
        timeline={[
          event(1, { type: "runStarted", modelLabel: "GPT-5.6" }),
          event(2, { type: "runFailed", message: "网络不可用" }),
        ]}
      />,
    );

    expect(
      container.querySelector(
        '.activity-feed__lifecycle[data-kind="runFailed"]',
      ),
    ).toBeVisible();
    const alerts = screen.getAllByRole("alert");
    expect(alerts).toHaveLength(1);
    expect(alerts[0]).toBe(container.querySelector(".timeline-failure"));
  });

  it("keeps an unresolved permission as non-urgent history after a Run fails", () => {
    const { container } = render(
      <WorkTimeline
        resources={[]}
        timeline={[
          event(1, { type: "runStarted", modelLabel: "GPT-5.6" }),
          event(2, {
            type: "permissionRequested",
            requestId: "permission-1",
            title: "Write file",
            detail: "src/app.tsx",
          }),
          event(3, { type: "runFailed", message: "网络不可用" }),
        ]}
      />,
    );

    const permission = container.querySelector(".activity-feed__permission");
    expect(permission).toBeVisible();
    expect(permission).toHaveAttribute("role", "status");
    expect(permission?.closest(".execution-progress")).not.toBeNull();
    const alerts = screen.getAllByRole("alert");
    expect(alerts).toHaveLength(1);
    expect(alerts[0]).toBe(container.querySelector(".timeline-failure"));
  });

  it("keeps the current CoDo activity group open while a Run is active", () => {
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

    expect(container.querySelector(".execution-progress")).toHaveTextContent("CoDo 正在执行");
    expect(container.querySelector(".execution-progress__summary")).toHaveAttribute(
      "aria-expanded",
      "true",
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

  it("keeps a pre-run user question above the answer from its eventual Run", () => {
    const assignmentId = "assignment-weather";
    const withAssignment = (
      envelope: WorkEventEnvelope,
      overrides: Partial<WorkEventEnvelope> = {},
    ): WorkEventEnvelope => ({
      ...envelope,
      assignmentId,
      ...overrides,
    });
    const { container } = render(
      <WorkTimeline
        resources={[]}
        timeline={[
          withAssignment(
            event(0, {
              type: "assignmentQueued",
              assignmentId,
              assignedAgentId: "agent-instance:lead",
              title: "Respond to the user",
              priority: 10,
            }),
            { eventId: "assignment-queued", runId: null },
          ),
          message("user", "今天天气怎么样", {
            id: "weather-question",
            runId: `assignment:${assignmentId}`,
            createdAt: "2026-07-29T00:00:00.500Z",
          }),
          withAssignment(
            event(1, { type: "assistantDelta", text: "今天多云，约 26°C。" }),
            { eventId: "assistant-answer" },
          ),
        ]}
      />,
    );

    const conversation = [...container.querySelectorAll(
      ".timeline-user, .timeline-event--assistant",
    )];
    expect(conversation).toHaveLength(2);
    expect(conversation[0]).toHaveTextContent("今天天气怎么样");
    expect(conversation[1]).toHaveTextContent("今天多云，约 26°C。");
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

  it("groups retry attempts of one Assignment by their real Run", () => {
    const attempt = (
      runId: string,
      text: string,
    ): WorkEventEnvelope => ({
      ...event(1, { type: "assistantDelta", text }),
      version: 2,
      eventId: `event-${runId}`,
      assignmentId: "assignment-1",
      runId,
      turnId: runId,
    });
    const { container } = render(
      <WorkTimeline
        resources={[]}
        timeline={[
          attempt("run-1", "first attempt"),
          attempt("run-2", "retry attempt"),
        ]}
      />,
    );

    const outputs = container.querySelectorAll(".timeline-event--assistant");
    expect(outputs).toHaveLength(2);
    expect(outputs[0]).toHaveTextContent("first attempt");
    expect(outputs[1]).toHaveTextContent("retry attempt");
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

  it("keeps process narration and deduplicates repeated completion summaries", () => {
    const summary = "已检查邮箱，当前没有可读取的新邮件。";
    const processNarration = "搜索完成，正在综合结论并交付。参数格式有误，重新提交。";
    const assignmentId = "assignment-email";
    const withAssignment = (
      envelope: WorkEventEnvelope,
      eventId: string,
    ): WorkEventEnvelope => ({
      ...envelope,
      assignmentId,
      eventId,
    });

    const { container } = render(
      <WorkTimeline
        resources={[]}
        timeline={[
          message("user", "查看最新邮件", {
            id: "email-question",
            runId: `assignment:${assignmentId}`,
          }),
          withAssignment(event(1, { type: "assistantDelta", text: processNarration }), "process-narration"),
          withAssignment(event(2, {
            type: "toolStarted",
            toolCallId: "list-accounts",
            toolName: "list_email_accounts",
            inputSummary: "{}",
          }), "list-started"),
          withAssignment(event(3, {
            type: "toolFinished",
            toolCallId: "list-accounts",
            toolName: "list_email_accounts",
            outputSummary: "1 account",
            success: true,
          }), "list-finished"),
          withAssignment(event(4, {
            type: "workDecisionRecorded",
            decisionId: "decision-1",
            summary: "邮箱元数据搜索结果为空。",
            version: 1,
          }), "decision-recorded"),
          withAssignment(event(5, {
            type: "workDeliveryCompleted",
            summary,
            artifacts: [],
            validation: [],
            limitations: [],
          }), "delivery-completed"),
          withAssignment(event(6, {
            type: "assignmentCompleted",
            assignmentId,
            agentInstanceId: "agent-instance:lead",
            agentSessionId: "session-1",
            resultSummary: summary,
          }), "assignment-completed"),
        ]}
      />,
    );

    expect(container.querySelectorAll(".timeline-event--assistant")).toHaveLength(2);
    expect(screen.getAllByText(summary)).toHaveLength(1);
    expect(screen.getByText(processNarration)).toBeVisible();
    expect(container.querySelector(".timeline-delivery")).toBeNull();
    expect(screen.queryByText("list_email_accounts")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "展开执行详情" }));
    expect(screen.getByText("list_email_accounts")).toBeVisible();
    expect(screen.getAllByText(summary)).toHaveLength(1);
    expect(screen.queryByText("邮箱元数据搜索结果为空。")).not.toBeInTheDocument();
    expect(screen.getByText(processNarration)).toBeVisible();
  });

  it("keeps streamed assistant output visible when a shorter delivery summary arrives", () => {
    const streamedAnswer = [
      "## 21个月宝宝每日陪玩指南",
      "",
      "每天安排大运动、精细动作、语言互动和生活自理练习。",
      "这是一段已经展示给用户的完整回答，任务完成后不能被摘要替换。",
    ].join("\n");

    const { container } = render(
      <WorkTimeline
        resources={[]}
        timeline={[
          event(1, { type: "assistantDelta", text: streamedAnswer }),
          event(2, {
            type: "workDeliveryCompleted",
            summary: "已交付《21个月宝宝每日陪玩指南》。",
            artifacts: [],
            validation: [],
            limitations: [],
          }),
        ]}
      />,
    );

    expect(container.querySelectorAll(".timeline-event--assistant")).toHaveLength(2);
    expect(screen.getByRole("heading", { name: "21个月宝宝每日陪玩指南" })).toBeVisible();
    expect(screen.getByText(/完整回答，任务完成后不能被摘要替换/u)).toBeVisible();
    expect(screen.getByText("已交付《21个月宝宝每日陪玩指南》。")).toBeVisible();
  });

  it("does not render a message for whitespace-only assistant output", () => {
    const { container } = render(
      <WorkTimeline
        resources={[]}
        timeline={[
          event(1, { type: "assistantDelta", text: "\n  \n" }),
        ]}
      />,
    );

    expect(container.querySelector(".timeline-event--assistant")).toBeNull();
  });

  it("merges pre-run assignment activity into its completed Run", () => {
    const assignmentId = "assignment-1";
    const withAssignment = (
      envelope: WorkEventEnvelope,
      eventId: string,
    ): WorkEventEnvelope => ({
      ...envelope,
      assignmentId,
      eventId,
    });

    const { container } = render(
      <WorkTimeline
        resources={[]}
        timeline={[
          withAssignment(
            {
              ...event(1, {
                type: "assignmentQueued",
                assignmentId,
                assignedAgentId: "agent-instance:lead",
                title: "Respond to the user",
                priority: 10,
              }),
              runId: null,
            },
            "assignment-queued",
          ),
          withAssignment(
            event(1, {
              type: "assignmentCompleted",
              assignmentId,
              agentInstanceId: "agent-instance:lead",
              agentSessionId: "session-1",
              resultSummary: "done",
            }),
            "assignment-completed",
          ),
          withAssignment(
            event(2, {
              type: "runCompleted",
              summary: "done",
              artifacts: [],
              validation: [],
              limitations: [],
            }),
            "run-completed",
          ),
        ]}
      />,
    );

    expect(container.querySelectorAll(".execution-progress")).toHaveLength(1);
    expect(screen.queryByText("CoDo 正在准备执行")).not.toBeInTheDocument();
    expect(screen.getByText("CoDo 已完成")).toBeInTheDocument();
  });
});
