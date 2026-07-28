import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { RunSummary, WorkDetail, WorkEventEnvelope } from "../../bindings";
import { i18n } from "../../i18n";
import {
  createMockTauriClient,
  runCompletedEvent,
} from "../../test/mockTauriClient";
import { WorkSurface } from "./WorkSurface";

const deferred = <T,>() => {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, reject, resolve };
};

const seededDetail = (status: WorkDetail["summary"]["status"] = "draft"): WorkDetail => ({
  summary: {
    id: "work-1",
    title: "营收看板",
    goal: "构建营收看板",
    rootPath: "D:\\workspace\\revenue",
    permissionMode: "balanced",
    status,
    createdAt: "2026-07-28T08:00:00.000Z",
    updatedAt: "2026-07-28T08:00:00.000Z",
  },
  runs: [],
  events: [],
});

const event = (
  sequence: number,
  payload: WorkEventEnvelope["payload"],
): WorkEventEnvelope => ({
  version: 1,
  workId: "work-1",
  runId: "run-1",
  sequence,
  occurredAt: `2026-07-28T08:00:${String(sequence).padStart(2, "0")}.000Z`,
  payload,
});

beforeEach(async () => {
  await i18n.changeLanguage("zh-CN");
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("WorkSurface", () => {
  it("创建 Work、运行、完成后继续同一个 Work", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    render(<WorkSurface client={client} />);

    await user.click(await screen.findByRole("button", { name: "新建 Work" }));
    const dialog = screen.getByRole("dialog", { name: "新建 Work" });
    await user.type(within(dialog).getByLabelText("目标"), "构建营收看板");
    await user.type(
      within(dialog).getByLabelText("工作目录"),
      "D:\\workspace\\revenue",
    );
    await user.click(within(dialog).getByRole("button", { name: "创建" }));

    const composer = await screen.findByRole("textbox", {
      name: "给 PiWork 指令",
    });
    await user.type(composer, "开始执行");
    await user.click(screen.getByRole("button", { name: "发送" }));
    expect(client.startWork).toHaveBeenLastCalledWith("work-1", "开始执行");

    client.emit(runCompletedEvent({ runId: "run-1" }));
    expect(await screen.findByText("Work 已完成")).toBeInTheDocument();
    expect(
      screen.getByRole("status", { name: "Work 状态" }),
    ).toHaveTextContent("已完成");

    await user.type(composer, "再优化一次");
    await user.click(screen.getByRole("button", { name: "继续 Work" }));
    expect(client.startWork).toHaveBeenLastCalledWith("work-1", "再优化一次");
  });

  it("运行时额外输入只排队，不启动新 Run", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.seed(seededDetail("running"));
    render(<WorkSurface client={client} />);

    const composer = await screen.findByRole("textbox", { name: "给 PiWork 指令" });
    await user.type(composer, "完成后补充单元测试");
    await user.click(screen.getByRole("button", { name: "发送" }));

    expect(client.startWork).not.toHaveBeenCalled();
    expect(screen.getByText("已排队 1 条指令")).toBeInTheDocument();
  });

  it("连续 Enter 只启动一个 Run", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const pendingRun = deferred<RunSummary>();
    client.seed(seededDetail());
    client.startWork.mockImplementation(() => pendingRun.promise);
    render(<WorkSurface client={client} />);

    const composer = await screen.findByRole("textbox", { name: "给 PiWork 指令" });
    await user.type(composer, "只执行一次");
    await user.keyboard("{Enter}{Enter}");

    expect(client.startWork).toHaveBeenCalledTimes(1);
    pendingRun.resolve({
      id: "run-1",
      workId: "work-1",
      engineKind: "fake",
      engineSessionId: "session-1",
      modelLabel: "Fake model",
      status: "running",
      createdAt: "2026-07-28T08:00:10.000Z",
      startedAt: "2026-07-28T08:00:10.000Z",
      completedAt: null,
    });
    await waitFor(() => expect(composer).toHaveValue(""));
  });

  it("启动拒绝时显示产品错误且不产生未处理 rejection", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.seed(seededDetail());
    client.startWork.mockRejectedValueOnce({
      code: "engine_unavailable",
      message: "无法启动 Run",
    });
    render(<WorkSurface client={client} />);

    const composer = await screen.findByRole("textbox", { name: "给 PiWork 指令" });
    await user.type(composer, "开始");
    await user.click(screen.getByRole("button", { name: "发送" }));

    expect(await screen.findByText("无法启动 Run")).toBeInTheDocument();
    expect(composer).toHaveValue("开始");
  });

  it("为六类事件提供独立的可读渲染", async () => {
    const client = createMockTauriClient();
    client.seed(seededDetail("running"));
    render(<WorkSurface client={client} />);
    await screen.findByRole("heading", { name: "营收看板" });

    client.emit(event(1, { type: "runStarted", modelLabel: "Fake model" }));
    client.emit(event(2, { type: "assistantDelta", text: "正在分析收入数据" }));
    client.emit(event(3, {
      type: "toolStarted",
      toolCallId: "tool-1",
      toolName: "read_file",
      inputSummary: "读取 revenue.csv",
    }));
    client.emit(event(4, {
      type: "toolFinished",
      toolCallId: "tool-1",
      toolName: "read_file",
      outputSummary: "读取 120 行",
      success: true,
    }));
    client.emit(event(5, {
      type: "toolFinished",
      toolCallId: "tool-2",
      toolName: "run_tests",
      outputSummary: "1 项检查失败",
      success: false,
    }));
    client.emit(event(6, {
      type: "runCompleted",
      summary: "看板已完成",
      artifacts: ["dashboard.html"],
      validation: ["图表检查通过"],
      limitations: ["示例数据"],
    }));
    client.emit(event(7, { type: "runFailed", message: "网络不可用" }));

    expect(await screen.findByText("Run 已开始")).toBeInTheDocument();
    expect(screen.getByText("正在分析收入数据")).toBeInTheDocument();
    expect(screen.getByText("读取 revenue.csv")).toBeInTheDocument();
    expect(screen.getByText("读取 120 行")).toBeInTheDocument();
    expect(screen.getByText("1 项检查失败")).toBeInTheDocument();
    expect(screen.getByText("看板已完成")).toBeInTheDocument();
    expect(screen.getByText("dashboard.html")).toBeInTheDocument();
    expect(screen.getByText("图表检查通过")).toBeInTheDocument();
    expect(screen.getByText("示例数据")).toBeInTheDocument();
    expect(screen.getByText("网络不可用")).toBeInTheDocument();
    expect(screen.queryByText(/"type"/)).not.toBeInTheDocument();
  });

  it("提供可访问的检查器标签页与真实空状态", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const detail = seededDetail("completed");
    detail.events = [
      event(1, {
        type: "runCompleted",
        summary: "交付完成",
        artifacts: ["inspector-output.html"],
        validation: [],
        limitations: [],
      }),
    ];
    client.seed(detail);
    render(<WorkSurface client={client} />);

    const tabs = await screen.findByRole("tablist", { name: "Work 检查器" });
    const artifacts = within(tabs).getByRole("tab", { name: "产物" });
    await user.click(artifacts);
    expect(artifacts).toHaveAttribute("aria-selected", "true");
    const artifactPanel = screen.getByRole("tabpanel", { name: "产物" });
    expect(within(artifactPanel).getByText("还没有产物")).toBeInTheDocument();
    expect(within(artifactPanel).queryByText("inspector-output.html")).not.toBeInTheDocument();
    await user.keyboard("{ArrowRight}");
    expect(within(tabs).getByRole("tab", { name: "日志" })).toHaveFocus();
  });

  it("显示加载、错误诊断与可重试入口", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    let rejectList!: (error: unknown) => void;
    client.listWorks.mockImplementationOnce(
      () => new Promise((_, reject) => { rejectList = reject; }),
    );
    render(<WorkSurface client={client} />);
    expect(screen.getByRole("status", { name: "正在加载 Work" })).toBeInTheDocument();
    rejectList({ code: "db_unavailable", message: "数据库不可用", details: { path: "safe.db" } });

    expect(await screen.findByRole("alert")).toHaveTextContent("数据库不可用");
    await user.click(screen.getByRole("button", { name: "打开诊断" }));
    expect(screen.getByText(/db_unavailable/)).toBeInTheDocument();
    expect(screen.getByText(/safe.db/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "重试" }));
    await waitFor(() => expect(client.listWorks).toHaveBeenCalledTimes(2));
  });

  it("初始 Work 详情加载失败时显示完整错误页", async () => {
    const client = createMockTauriClient();
    client.seed(seededDetail());
    client.getWork.mockRejectedValueOnce({
      code: "detail_unavailable",
      message: "Work 详情不可用",
    });

    render(<WorkSurface client={client} />);

    const errorPage = await screen.findByRole("alert");
    expect(errorPage).toHaveTextContent("Work 详情不可用");
    expect(within(errorPage).getByRole("button", { name: "重试" })).toBeInTheDocument();
    expect(within(errorPage).getByRole("button", { name: "打开诊断" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "营收看板" })).not.toBeInTheDocument();
  });

  it("初始详情失败后选择另一个 Work 可恢复工作区", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const first = seededDetail();
    first.summary.title = "首要 Work";
    first.summary.updatedAt = "2026-07-28T08:01:00.000Z";
    const second = seededDetail();
    second.summary.id = "work-2";
    second.summary.title = "备用 Work";
    second.summary.updatedAt = "2026-07-28T08:00:00.000Z";
    client.seed(first);
    client.seed(second);
    client.getWork.mockRejectedValueOnce({
      code: "detail_unavailable",
      message: "首要详情不可用",
    });
    render(<WorkSurface client={client} />);

    await screen.findByRole("alert");
    await user.click(screen.getByRole("button", { name: /备用 Work/ }));

    expect(
      await screen.findByRole("heading", { name: "备用 Work" }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("无 Work 时显示产品空状态并校验创建表单", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    render(<WorkSurface client={client} />);
    expect(await screen.findByText("创建第一个 Work")).toBeInTheDocument();
    await user.click(screen.getAllByRole("button", { name: "新建 Work" })[0]!);
    await user.click(screen.getByRole("button", { name: "创建" }));
    expect(screen.getByText("请输入目标")).toBeInTheDocument();
    expect(screen.getByText("请输入工作目录")).toBeInTheDocument();
    expect(client.createWork).not.toHaveBeenCalled();
  });

  it("创建 dialog 限制焦点并在 Escape、关闭和成功后归还触发按钮", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    render(<WorkSurface client={client} />);
    const sidebar = screen.getByRole("complementary", { name: "Work 导航" });
    const trigger = within(sidebar).getByRole("button", { name: "新建 Work" });

    await user.click(trigger);
    let dialog = screen.getByRole("dialog", { name: "新建 Work" });
    const goal = within(dialog).getByLabelText("目标");
    const close = within(dialog).getByRole("button", { name: "关闭" });
    const create = within(dialog).getByRole("button", { name: "创建" });
    expect(goal).toHaveFocus();
    await user.keyboard("{Shift>}{Tab}{/Shift}");
    expect(close).toHaveFocus();
    await user.keyboard("{Shift>}{Tab}{/Shift}");
    expect(create).toHaveFocus();
    await user.keyboard("{Tab}");
    expect(close).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "新建 Work" })).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();

    await user.click(trigger);
    dialog = screen.getByRole("dialog", { name: "新建 Work" });
    await user.click(within(dialog).getByRole("button", { name: "关闭" }));
    expect(trigger).toHaveFocus();

    await user.click(trigger);
    dialog = screen.getByRole("dialog", { name: "新建 Work" });
    await user.type(within(dialog).getByLabelText("目标"), "恢复焦点测试");
    await user.type(within(dialog).getByLabelText("工作目录"), "D:\\workspace\\focus");
    await user.click(within(dialog).getByRole("button", { name: "创建" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "新建 Work" })).not.toBeInTheDocument());
    expect(trigger).toHaveFocus();
  });

  it("诊断详情不可序列化时仍显示安全文本", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const details: Record<string, unknown> = {};
    details.self = details;
    client.listWorks.mockRejectedValueOnce({
      code: "circular",
      message: "加载失败",
      details,
    });
    render(<WorkSurface client={client} />);

    await screen.findByRole("alert");
    await user.click(screen.getByRole("button", { name: "打开诊断" }));

    expect(screen.getByText(/诊断详情无法安全显示/)).toBeInTheDocument();
  });

  it("可切换窄窗口检查器抽屉并在卸载时取消订阅", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.seed(seededDetail());
    const rendered = render(<WorkSurface client={client} />);
    const toggle = await screen.findByRole("button", { name: "打开检查器" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("complementary", { name: "Work 检查器" })).toHaveAttribute("data-open", "true");
    rendered.unmount();
    expect(client.unlisten).toHaveBeenCalledTimes(1);
  });

  it("窄屏关闭检查器时隐藏语义并在 Escape 后归还焦点", async () => {
    const user = userEvent.setup();
    vi.stubGlobal("matchMedia", vi.fn(() => ({
      matches: true,
      media: "(max-width: 1099px)",
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })));
    const client = createMockTauriClient();
    client.seed(seededDetail());
    render(<WorkSurface client={client} />);

    const toggle = await screen.findByRole("button", { name: "打开检查器" });
    const inspector = screen.getByLabelText("Work 检查器", { selector: "aside" });
    expect(inspector).toHaveAttribute("aria-hidden", "true");
    expect(inspector).toHaveAttribute("inert");

    await user.click(toggle);
    expect(inspector).toHaveAttribute("aria-hidden", "false");
    expect(inspector).not.toHaveAttribute("inert");
    within(inspector).getByRole("tab", { name: "进度" }).focus();
    await user.keyboard("{Escape}");

    expect(inspector).toHaveAttribute("aria-hidden", "true");
    expect(toggle).toHaveFocus();
  });
});
