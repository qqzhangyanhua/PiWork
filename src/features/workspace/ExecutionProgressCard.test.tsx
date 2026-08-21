import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type {
  WorkEventEnvelope,
  WorkEventPayload,
} from "../../bindings";
import { i18n } from "../../i18n";
import { ExecutionProgressCard } from "./ExecutionProgressCard";

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

const runningEvents = () => [
  event(1, { type: "runStarted", modelLabel: "GPT-5.6" }),
  event(2, {
    type: "toolStarted",
    toolCallId: "read-1",
    toolName: "read",
    inputSummary: "src/app.tsx",
  }),
];

describe("ExecutionProgressCard", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("zh-CN");
    vi.stubGlobal(
      "matchMedia",
      vi.fn((query: string) => ({
        matches: query.includes("no-preference"),
        media: query,
        onchange: null,
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
        addListener: vi.fn(),
        removeListener: vi.fn(),
        dispatchEvent: vi.fn(),
      })),
    );
  });

  it("keeps live execution details collapsed and never invents workflow phases", async () => {
    render(
      <ExecutionProgressCard events={runningEvents()}>
        <div>工具活动详情</div>
      </ExecutionProgressCard>,
    );

    expect(
      await screen.findByRole("status", { name: "执行进度" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "展开执行详情" }),
    ).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText("工具活动详情")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "展开执行详情" }));

    expect(screen.getByText("工具活动详情")).toBeVisible();
    expect(screen.queryByTestId("execution-phase:analyze")).not.toBeInTheDocument();
    expect(screen.queryByText("未经过")).not.toBeInTheDocument();
  });

  it("starts a hydrated successful execution collapsed and remains inspectable", () => {
    const events = [
      ...runningEvents(),
      event(3, {
        type: "toolFinished",
        toolCallId: "read-1",
        toolName: "read",
        outputSummary: "done",
        success: true,
      }),
      event(4, {
        type: "runCompleted",
        summary: "done",
        artifacts: [],
        validation: [],
        limitations: [],
      }),
    ];
    render(
      <ExecutionProgressCard events={events}>
        <div>读取 src/app.tsx</div>
      </ExecutionProgressCard>,
    );

    const toggle = screen.getByRole("button", { name: "展开执行详情" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByText("Pi 已完成 1 个操作")).toBeInTheDocument();
    expect(screen.queryByText("读取 src/app.tsx")).not.toBeInTheDocument();

    fireEvent.click(toggle);

    expect(
      screen.getByRole("button", { name: "收起执行详情" }),
    ).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText("读取 src/app.tsx")).toBeVisible();
  });

  it("does not describe a text-only answer as zero tool actions", () => {
    render(
      <ExecutionProgressCard events={[
        event(1, { type: "runStarted", modelLabel: "GPT-5.6" }),
        event(2, { type: "assistantDelta", text: "answer" }),
        event(3, { type: "runCompleted", summary: "done", artifacts: [], validation: [], limitations: [] }),
      ]} />,
    );

    expect(screen.getByText("Pi 已完成")).toBeInTheDocument();
    expect(screen.queryByText(/0 个操作/)).not.toBeInTheDocument();
  });

  it("keeps terminal failure activity collapsed without exposing raw engine text", async () => {
    render(
      <ExecutionProgressCard
        events={[
          ...runningEvents(),
          event(3, { type: "runFailed", message: "private engine failure" }),
        ]}
      >
        <div>最后一个安全活动</div>
      </ExecutionProgressCard>,
    );

    expect(
      await screen.findByRole("status", { name: "执行进度" }),
    ).toBeInTheDocument();
    expect(screen.getByText("执行未完成")).toBeInTheDocument();
    expect(screen.queryByText("private engine failure")).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "展开执行详情" }),
    ).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText("最后一个安全活动")).not.toBeInTheDocument();
  });

  it("renders a delegated Lead turn as stable waiting instead of endless loading", () => {
    const { container } = render(
      <ExecutionProgressCard
        events={[
          event(1, { type: "runStarted", modelLabel: "GPT-5.6" }),
          event(2, { type: "waiting", reason: "waiting_on_assignments" }),
        ]}
      >
        <div>等待成员活动</div>
      </ExecutionProgressCard>,
    );

    expect(screen.getByText("Pi 正在等待成员结果")).toBeInTheDocument();
    expect(screen.getByRole("status", { name: "执行进度" })).toHaveAttribute(
      "data-status",
      "waiting",
    );
    expect(
      container
        .querySelector("[data-progress-icon] svg")
        ?.getAttribute("class"),
    ).toContain("clock");
    expect(container.querySelector(".lucide-loader-circle")).toBeNull();
  });

  it("reports recovered tool failures without overriding final success", () => {
    render(
      <ExecutionProgressCard
        events={[
          event(1, {
            type: "toolStarted",
            toolCallId: "edit-1",
            toolName: "edit",
            inputSummary: "src/app.tsx",
          }),
          event(2, {
            type: "toolFinished",
            toolCallId: "edit-1",
            toolName: "edit",
            outputSummary: "failed",
            success: false,
          }),
          event(3, {
            type: "runCompleted",
            summary: "recovered",
            artifacts: [],
            validation: [],
            limitations: [],
          }),
        ]}
      >
        <div>失败工具仍可查看</div>
      </ExecutionProgressCard>,
    );

    expect(screen.getByText("Pi 已完成 1 个操作")).toBeInTheDocument();
    expect(screen.getByText("其中 1 个操作曾遇到问题")).toBeInTheDocument();
    expect(screen.queryByText("失败工具仍可查看")).not.toBeInTheDocument();
  });
});
