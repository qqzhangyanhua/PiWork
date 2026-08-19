import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
// @ts-expect-error Vitest executes this regression test in Node; the app intentionally omits global Node typings.
import { readFileSync } from "node:fs";
import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type {
  ResourceSummary,
  RunSummary,
  StartWorkOutput,
  WorkDetail,
  WorkEventEnvelope,
} from "../../bindings";
import { i18n } from "../../i18n";
import {
  assignmentSummary,
  createMockTauriClient,
  runCompletedEvent,
} from "../../test/mockTauriClient";
import { WorkSurface } from "./WorkSurface";

const dashboardStyles = readFileSync("src/styles/dashboard.css", "utf8");

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
  messages: [],
  events: [],
});

const resource = (
  overrides: Partial<ResourceSummary> = {},
): ResourceSummary => ({
  id: "resource-1",
  originalName: "chart.png",
  mediaType: "image/png",
  size: 68n,
  origin: "user_upload",
  status: "ready",
  failureCode: null,
  createdAt: "2026-07-28T08:00:00.000Z",
  ...overrides,
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
  localStorage.clear();
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("WorkSurface", () => {
  it("按项目分组对话，并从项目下打开既有对话", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.seed(seededDetail());
    render(<WorkSurface client={client} initialView="new" />);

    expect(await screen.findByRole("heading", { name: "今天想让 Pi 帮你完成什么？" })).toBeInTheDocument();
    expect(screen.queryByText("构建营收看板", { selector: ".work-header__goal" })).not.toBeInTheDocument();

    const sidebar = screen.getByRole("complementary", { name: "项目与对话" });
    expect(within(sidebar).getByRole("button", { name: "项目 revenue" })).toBeInTheDocument();
    expect(within(sidebar).queryByRole("button", { name: "Work 主页" })).not.toBeInTheDocument();
    expect(within(sidebar).queryByRole("button", { name: "全部 Work" })).not.toBeInTheDocument();

    await user.click(within(sidebar).getByRole("button", { name: /营收看板/ }));
    expect(await screen.findByText("构建营收看板", { selector: ".work-header__goal" })).toBeInTheDocument();
  });

  it("用真实任务建议丰富新会话并将所选建议写入首任务", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    render(<WorkSurface client={client} initialView="new" />);

    expect(await screen.findByLabelText("任务起点")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "探索并理解项目" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "构建新功能" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "审查代码" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "修复问题" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "构建新功能" }));

    const prompt = screen.getByRole("textbox", { name: "首个任务" });
    expect(prompt).toHaveTextContent("在当前项目中构建一个新功能，先梳理实现方案再开始修改。");
    expect(prompt).toHaveFocus();
    expect(screen.getByText("默认空间")).toBeInTheDocument();
    expect(screen.getByText("本地运行")).toBeInTheDocument();
  });

  it("没有手动选择项目时使用真实默认目录创建对话", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const getDefaultProjectDirectory = vi.fn(async () => "D:\\PiWork\\默认空间");
    Object.assign(client, { getDefaultProjectDirectory });

    render(<WorkSurface client={client} initialView="new" />);

    await user.type(await screen.findByRole("textbox", { name: "首个任务" }), "整理今天的研究笔记");
    await user.click(screen.getByRole("button", { name: "发送" }));

    await waitFor(() => expect(client.createWork).toHaveBeenCalledWith(expect.objectContaining({
      rootPath: "D:\\PiWork\\默认空间",
    })));
    expect(getDefaultProjectDirectory).toHaveBeenCalled();
  });

  it("在 StrictMode 下不选择项目也能使用默认目录开始对话", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const getDefaultProjectDirectory = vi.fn(async () => "D:\\PiWork\\默认空间");
    Object.assign(client, { getDefaultProjectDirectory });

    render(
      <StrictMode>
        <WorkSurface client={client} initialView="new" />
      </StrictMode>,
    );

    expect(screen.queryByText("直接描述任务；可使用当前项目，也可以随时切换目录。")).not.toBeInTheDocument();
    expect(screen.queryByText("发送第一条消息时创建对话；未指定项目时使用默认空间")).not.toBeInTheDocument();

    await user.type(await screen.findByRole("textbox", { name: "首个任务" }), "整理临时资料");
    await waitFor(() => expect(screen.getByRole("button", { name: "发送" })).toBeEnabled());
    await user.click(screen.getByRole("button", { name: "发送" }));

    await waitFor(() => expect(client.createWork).toHaveBeenCalledWith(expect.objectContaining({
      rootPath: "D:\\PiWork\\默认空间",
    })));
  });

  it("可以清除已选项目并改用默认目录创建对话", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.seed(seededDetail());
    const getDefaultProjectDirectory = vi.fn(async () => "D:\\PiWork\\默认空间");
    Object.assign(client, { getDefaultProjectDirectory });

    render(<WorkSurface client={client} initialView="new" />);

    await user.click(await screen.findByRole("button", { name: "选择项目" }));
    const picker = screen.getByRole("dialog", { name: "选择项目" });
    await user.click(within(picker).getByRole("button", { name: /revenue/u }));
    expect(screen.getByRole("button", { name: /当前项目：revenue/u })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "不指定项目" }));
    expect(screen.getByRole("button", { name: "选择项目" })).toBeInTheDocument();

    await user.type(screen.getByRole("textbox", { name: "首个任务" }), "分析临时资料");
    await user.click(screen.getByRole("button", { name: "发送" }));

    await waitFor(() => expect(client.createWork).toHaveBeenCalledWith(expect.objectContaining({
      rootPath: "D:\\PiWork\\默认空间",
    })));
  });

  it("uploads multiple images through an injected native picker without showing full paths", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.seed(seededDetail());
    const pickAttachments = vi.fn(async () => [
      "C:/private/chart.png",
      "C:/private/photo.jpg",
    ]);
    render(<WorkSurface client={client} pickAttachments={pickAttachments} />);

    await screen.findByText("营收看板");
    await user.click(screen.getByRole("button", { name: "添加附件" }));
    await user.click(screen.getByRole("button", { name: "上传文件" }));

    expect(pickAttachments).toHaveBeenCalledOnce();
    expect(await screen.findByText("chart.png")).toBeInTheDocument();
    expect(screen.getByText("photo.jpg")).toBeInTheDocument();
    expect(screen.queryByText(/C:\/private/u)).not.toBeInTheDocument();
  });

  it("opens an uploaded image in a keyboard-dismissable preview and restores focus", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.seed(seededDetail());
    render(
      <WorkSurface
        client={client}
        pickAttachments={async () => ["C:/private/chart.png"]}
      />,
    );

    await screen.findByText("营收看板");
    await user.click(screen.getByRole("button", { name: "添加附件" }));
    await user.click(screen.getByRole("button", { name: "上传文件" }));
    const trigger = await screen.findByRole("button", { name: "预览图片 chart.png" });
    await user.click(trigger);

    const dialog = screen.getByRole("dialog", { name: "chart.png" });
    expect(await within(dialog).findByRole("img", { name: "chart.png" })).toBeInTheDocument();
    expect(screen.queryByText(/C:\/private/u)).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "关闭" })).toHaveFocus();

    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "chart.png" })).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
  });

  it("keeps a failed import visible without hiding ready attachments", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.seed(seededDetail());
    client.importResources.mockResolvedValueOnce([
      resource({ id: "ready", originalName: "ready.png" }),
      resource({
        id: "failed",
        originalName: "broken.png",
        status: "failed",
        failureCode: "unsupported_image",
      }),
    ]);
    render(
      <WorkSurface
        client={client}
        pickAttachments={async () => ["C:/private/ready.png", "C:/private/broken.png"]}
      />,
    );

    await screen.findByText("营收看板");
    await user.click(screen.getByRole("button", { name: "添加附件" }));
    await user.click(screen.getByRole("button", { name: "上传文件" }));

    expect(await screen.findByText("ready.png")).toBeInTheDocument();
    expect(screen.getByText("broken.png").closest("[role='alert']")).not.toBeNull();
  });

  it("sends selected image ids separately from text and project references", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.seed(seededDetail());
    render(
      <WorkSurface
        client={client}
        pickAttachments={async () => ["C:/private/chart.png"]}
      />,
    );
    await screen.findByText("营收看板");
    await user.click(screen.getByRole("button", { name: "添加附件" }));
    await user.click(screen.getByRole("button", { name: "上传文件" }));
    await screen.findByText("chart.png");
    await user.type(screen.getByLabelText("给 PiWork 指令"), "解释图表");
    await user.click(screen.getByRole("button", { name: "发送" }));

    expect(client.startWork).toHaveBeenLastCalledWith(
      "work-1",
      "解释图表",
      [],
      [expect.stringMatching(/^resource-/u)],
    );
  });

  it("allows an attachment-only message and can reuse a Work attachment", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.seed(seededDetail());
    client.seedResource(
      "work-1",
      resource({ id: "resource-existing", originalName: "saved.png" }),
    );
    render(<WorkSurface client={client} />);
    await screen.findByText("营收看板");
    await user.click(screen.getByRole("button", { name: "添加附件" }));
    await user.click(screen.getByRole("button", { name: "saved.png" }));

    const send = screen.getByRole("button", { name: "发送" });
    expect(send).toBeEnabled();
    await user.click(send);
    expect(client.startWork).toHaveBeenLastCalledWith(
      "work-1",
      "",
      [],
      ["resource-existing"],
    );
  });

  it("sends a document-only message without requesting an image thumbnail", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.seed(seededDetail());
    client.seedResource(
      "work-1",
      resource({
        id: "document-1",
        originalName: "quarterly.pdf",
        mediaType: "application/pdf",
      }),
    );
    render(<WorkSurface client={client} />);

    await screen.findByText("营收看板");
    await user.click(screen.getByRole("button", { name: "添加附件" }));
    await user.click(screen.getByRole("button", { name: "quarterly.pdf" }));
    await user.click(screen.getByRole("button", { name: "发送" }));

    expect(client.getResourceThumbnail).not.toHaveBeenCalled();
    expect(client.startWork).toHaveBeenLastCalledWith(
      "work-1",
      "",
      [],
      ["document-1"],
    );
  });

  it("adopts new-Work uploads and sends them on the first Run", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    render(
      <WorkSurface
        client={client}
        pickProjectDirectory={async () => "D:/workspace"}
        pickAttachments={async () => ["C:/private/brief.png"]}
      />,
    );
    await screen.findByRole("textbox", { name: "首个任务" });
    await user.click(screen.getByRole("button", { name: "选择项目" }));
    await user.click(screen.getByRole("button", { name: "选择其他文件夹…" }));
    await user.click(screen.getByRole("button", { name: "添加附件" }));
    await user.click(screen.getByRole("button", { name: "上传文件" }));
    await screen.findByText("brief.png");
    await user.type(screen.getByLabelText("首个任务"), "总结图片");
    await user.click(screen.getByRole("button", { name: "发送" }));

    expect(client.createWork).toHaveBeenCalledWith(
      expect.objectContaining({
        resourceDraftId: expect.stringMatching(/^[0-9a-f-]{36}$/u),
      }),
    );
    expect(client.startWork).toHaveBeenCalledWith(
      "work-1",
      "总结图片",
      [],
      [expect.stringMatching(/^resource-/u)],
    );
    expect(await client.listWorkResources("work-1")).toHaveLength(1);
  });

  it("allows a new Work to start from a ready attachment without text", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    render(
      <WorkSurface
        client={client}
        pickProjectDirectory={async () => "D:/workspace"}
        pickAttachments={async () => ["C:/private/brief.png"]}
      />,
    );
    await screen.findByRole("textbox", { name: "首个任务" });
    await user.click(screen.getByRole("button", { name: "选择项目" }));
    await user.click(screen.getByRole("button", { name: "选择其他文件夹…" }));
    await user.click(screen.getByRole("button", { name: "添加附件" }));
    await user.click(screen.getByRole("button", { name: "上传文件" }));

    const start = await screen.findByRole("button", { name: "发送" });
    expect(start).toBeEnabled();
    await user.click(start);
    expect(client.createWork).toHaveBeenCalledWith(
      expect.objectContaining({ title: "brief.png", goal: "Review brief.png" }),
    );
  });

  it("detaches a removed new-Work draft attachment", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    render(
      <WorkSurface
        client={client}
        pickProjectDirectory={async () => "D:/workspace"}
        pickAttachments={async () => ["C:/private/brief.png"]}
      />,
    );
    await screen.findByRole("textbox", { name: "首个任务" });
    await user.click(screen.getByRole("button", { name: "选择项目" }));
    await user.click(screen.getByRole("button", { name: "选择其他文件夹…" }));
    await user.click(screen.getByRole("button", { name: "添加附件" }));
    await user.click(screen.getByRole("button", { name: "上传文件" }));
    await screen.findByText("brief.png");
    await user.click(screen.getByRole("button", { name: "移除附件 brief.png" }));

    expect(client.detachDraftResource).toHaveBeenCalledWith(
      expect.stringMatching(/^[0-9a-f-]{36}$/u),
      expect.stringMatching(/^resource-/u),
    );
    expect(screen.getByRole("button", { name: "发送" })).toBeDisabled();
  });

  it("lists durable Work attachments in a dedicated inspector tab", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.seed(seededDetail());
    client.seedResource(
      "work-1",
      resource({ id: "resource-1", originalName: "retained.png" }),
    );
    render(<WorkSurface client={client} />);

    await screen.findByText("营收看板");
    await user.click(screen.getByRole("button", { name: "打开检查器" }));
    await user.click(screen.getByRole("tab", { name: "附件" }));

    expect(screen.getByText("retained.png")).toBeInTheDocument();
    expect(screen.getByText("1 个附件")).toBeInTheDocument();
  });

  it("never exposes an uploaded attachment source path in product UI", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.seed(seededDetail());
    render(
      <WorkSurface
        client={client}
        pickAttachments={async () => ["C:/Users/Alice/Private/medical.png"]}
      />,
    );

    await screen.findByText("营收看板");
    await user.click(screen.getByRole("button", { name: "添加附件" }));
    await user.click(screen.getByRole("button", { name: "上传文件" }));
    await screen.findByText("medical.png");
    await user.click(screen.getByRole("button", { name: "发送" }));
    await user.click(screen.getByRole("button", { name: "打开检查器" }));
    await user.click(screen.getByRole("tab", { name: "附件" }));

    expect(screen.getAllByText("medical.png").length).toBeGreaterThanOrEqual(2);
    expect(document.body).not.toHaveTextContent(/Alice|Private|C:\\Users/u);
  });

  it("创建 Work、运行、完成后继续同一个 Work", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const pickProjectDirectory = vi.fn().mockResolvedValue("D:\\workspace\\revenue");
    render(<WorkSurface client={client} pickProjectDirectory={pickProjectDirectory} />);

    const firstPrompt = await screen.findByRole("textbox", { name: "首个任务" });
    await user.click(screen.getByRole("button", { name: "选择项目" }));
    await user.click(screen.getByRole("button", { name: "选择其他文件夹…" }));
    await user.type(firstPrompt, "构建营收看板");
    await user.click(screen.getByRole("button", { name: "发送" }));

    expect(client.createWork).toHaveBeenLastCalledWith({
      title: "构建营收看板",
      goal: "构建营收看板",
      rootPath: "D:\\workspace\\revenue",
      permissionMode: "balanced",
      resourceDraftId: expect.stringMatching(/^[0-9a-f-]{36}$/u),
    });
    expect(client.startWork).toHaveBeenLastCalledWith("work-1", "构建营收看板", [], []);

    const composer = await screen.findByRole("textbox", { name: "给 PiWork 指令" });

    client.emit(runCompletedEvent({ runId: "run-1" }));
    expect(await screen.findByText("任务已完成")).toBeInTheDocument();

    await user.type(composer, "再优化一次");
    await user.click(screen.getByRole("button", { name: "发送" }));
    expect(client.startWork).toHaveBeenLastCalledWith("work-1", "再优化一次", [], []);
  });

  it("运行时保留额外输入并将发送操作原位切换为真实停止", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.seed(seededDetail("running"));
    render(<WorkSurface client={client} />);

    const composer = await screen.findByRole("textbox", { name: "给 PiWork 指令" });
    await user.type(composer, "完成后补充单元测试");

    expect(screen.queryByRole("button", { name: "发送" })).not.toBeInTheDocument();
    const queue = screen.getByRole("button", { name: "排在下一步" });
    const stop = screen.getByRole("button", { name: "停止处理" });
    expect(queue).toBeEnabled();
    expect(stop).toBeEnabled();
    expect(composer).toHaveTextContent("完成后补充单元测试");
    expect(client.startWork).not.toHaveBeenCalled();
    expect(screen.getByText("Pi 正在处理上一条指令，你可以排队下一条或停止")).toBeInTheDocument();
    expect(screen.queryByText(/已排队/u)).not.toBeInTheDocument();

    client.stopWork.mockResolvedValueOnce({
      ...seededDetail("stopped"),
      summary: { ...seededDetail("stopped").summary, status: "stopped" },
    });
    await user.click(stop);
    expect(client.stopWork).toHaveBeenCalledWith("work-1");
  });

  it("运行时中断当前任务并用新指令替换", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.seed(seededDetail("running"));
    vi.spyOn(window, "confirm").mockReturnValue(true);
    client.interruptAndReplace.mockResolvedValue({
      assignment: assignmentSummary({ id: "assignment-2", workId: "work-1" }),
      run: {
        id: "run-2",
        workId: "work-1",
        assignmentId: "assignment-2",
        agentInstanceId: "agent-1",
        engineKind: "fake",
        engineSessionId: "session-2",
        modelLabel: "Fake model",
        status: "running",
        createdAt: "2026-07-28T08:05:00.000Z",
        startedAt: "2026-07-28T08:05:00.000Z",
        completedAt: null,
      },
      userMessage: {
        id: "message-2",
        workId: "work-1",
        runId: "run-2",
        assignmentId: "assignment-2",
        role: "user",
        content: "替换为这条指令",
        resourceIds: [],
        createdAt: "2026-07-28T08:05:00.000Z",
      },
    });
    render(<WorkSurface client={client} />);

    const composer = await screen.findByRole("textbox", { name: "给 PiWork 指令" });
    await user.type(composer, "替换为这条指令");

    const interrupt = screen.getByRole("button", { name: "中断并替换" });
    expect(interrupt).toBeEnabled();
    await user.click(interrupt);

    expect(window.confirm).toHaveBeenCalledTimes(1);
    expect(client.interruptAndReplace).toHaveBeenCalledWith("work-1", {
      assignmentId: "",
      replacement: {
        instruction: "替换为这条指令",
        referencedFiles: [],
        resourceIds: [],
      },
    });
  });

  it("连续 Enter 只启动一个 Run", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const pendingRun = deferred<StartWorkOutput>();
    client.seed(seededDetail());
    client.startWork.mockImplementation(() => pendingRun.promise);
    render(<WorkSurface client={client} />);

    const composer = await screen.findByRole("textbox", { name: "给 PiWork 指令" });
    await user.type(composer, "只执行一次");
    await user.keyboard("{Enter}{Enter}");

    expect(client.startWork).toHaveBeenCalledTimes(1);
    pendingRun.resolve({
      assignment: assignmentSummary({ id: "assignment-1", workId: "work-1" }),
      run: {
        id: "run-1",
        workId: "work-1",
        assignmentId: "assignment-1",
        agentInstanceId: "agent-1",
        engineKind: "fake",
        engineSessionId: "session-1",
        modelLabel: "Fake model",
        status: "running",
        createdAt: "2026-07-28T08:00:10.000Z",
        startedAt: "2026-07-28T08:00:10.000Z",
        completedAt: null,
      },
      userMessage: {
        id: "message-1",
        workId: "work-1",
        runId: "run-1",
        assignmentId: "assignment-1",
        role: "user",
        content: "只执行一次",
        resourceIds: [],
        createdAt: "2026-07-28T08:00:10.000Z",
      },
    });
    await waitFor(() => expect(composer).toBeEmptyDOMElement());
  });

  it("启动已持久化后拒绝时刷新权威消息、清空输入且保留产品错误", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const detail = seededDetail();
    const instruction = "故障前已持久化的指令";
    const failedRun: RunSummary = {
      id: "run-1",
      workId: "work-1",
      assignmentId: "assignment-1",
      agentInstanceId: "agent-1",
      engineKind: "fake",
      engineSessionId: null,
      modelLabel: "Fake model",
      status: "failed",
      createdAt: "2026-07-28T08:00:10.000Z",
      startedAt: "2026-07-28T08:00:10.000Z",
      completedAt: "2026-07-28T08:00:11.000Z",
    };
    client.seed(detail);
    client.startWork.mockImplementationOnce(async () => {
      detail.runs.push(failedRun);
      detail.messages.push({
        id: "message-1",
        workId: "work-1",
        runId: "run-1",
        role: "user",
        content: instruction,
        resourceIds: [],
        createdAt: failedRun.createdAt,
      });
      detail.summary.status = "failed";
      detail.summary.updatedAt = failedRun.completedAt!;
      throw {
        code: "engine_start_failed",
        message: "Raw engine startup failure",
        details: { workId: "work-1" },
      };
    });
    render(<WorkSurface client={client} />);

    const composer = await screen.findByRole("textbox", { name: "给 PiWork 指令" });
    await user.type(composer, instruction);
    await user.click(screen.getByRole("button", { name: "发送" }));

    expect(await screen.findByText("无法开始处理，请重试。")).toBeInTheDocument();
    await waitFor(() => expect(composer).toBeEmptyDOMElement());
    expect(screen.getAllByText(instruction)).toHaveLength(1);
    expect(screen.queryByText(/Raw engine startup failure/)).not.toBeInTheDocument();

    client.emit(event(1, {
      type: "runFailed",
      message: "Engine failed during startup",
    }));
    expect(screen.getAllByText(instruction)).toHaveLength(1);
  });

  it("启动拒绝时显示产品错误且不产生未处理 rejection", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.seed(seededDetail());
    client.startWork.mockRejectedValueOnce({
      code: "work_already_running",
      message: "Raw Work already has an active Run",
      details: { workId: "work-1" },
    });
    render(<WorkSurface client={client} />);

    const composer = await screen.findByRole("textbox", { name: "给 PiWork 指令" });
    await user.type(composer, "开始");
    await user.click(screen.getByRole("button", { name: "发送" }));

    expect(await screen.findByText("Pi 正在处理上一条指令。")).toBeInTheDocument();
    expect(screen.getByText("Pi 正在处理上一条指令。").closest(".agent-activity")).not.toBeNull();
    expect(document.querySelector(".workspace-banner")).toBeNull();
    expect(screen.queryByText(/Raw Work already/)).not.toBeInTheDocument();
    expect(screen.queryByText(/work_already_running/)).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "打开诊断" }));
    expect(screen.getByLabelText("对话检查器", { selector: "aside" })).toHaveAttribute("aria-hidden", "false");
    expect(screen.getByRole("tab", { name: "活动原始记录" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByText(/work_already_running/)).toBeInTheDocument();
    expect(client.getWork).toHaveBeenCalledTimes(2);
    expect(composer).toHaveTextContent("开始");
  });

  it("为投影后的高信号事件提供独立的可读渲染", async () => {
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

    await userEvent.setup().click(
      await screen.findByRole("button", { name: "展开执行详情" }),
    );
    expect(screen.queryByText("开始处理")).not.toBeInTheDocument();
    expect(screen.getByText("正在分析收入数据")).toBeInTheDocument();
    expect(screen.getByText("读取 revenue.csv")).toBeInTheDocument();
    expect(screen.getByText("读取 120 行")).toBeInTheDocument();
    expect(screen.getByText("1 项检查失败")).toBeInTheDocument();
    expect(screen.getByText("看板已完成")).toBeInTheDocument();
    expect(screen.getByText("dashboard.html")).toBeInTheDocument();
    expect(screen.getByText("图表检查通过")).toBeInTheDocument();
    expect(screen.getByText("示例数据")).toBeInTheDocument();
    expect(
      screen.getByText("本次执行未完成。确认本地执行环境可用后重试。"),
    ).toBeInTheDocument();
    expect(screen.queryByText("网络不可用")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "打开诊断" })).toBeInTheDocument();
    expect(screen.queryByText(/"type"/)).not.toBeInTheDocument();
  });

  it("重启 hydration 后独立渲染两个 Run 的持久用户指令且不依赖引擎复述", async () => {
    const client = createMockTauriClient();
    const detail = seededDetail("completed");
    detail.runs = [
      {
        id: "run-1",
        workId: "work-1",
        assignmentId: "assignment-1",
        agentInstanceId: "agent-1",
        engineKind: "fake",
        engineSessionId: "session-1",
        modelLabel: "Fake model",
        status: "completed",
        createdAt: "2026-07-28T08:00:01.000Z",
        startedAt: "2026-07-28T08:00:01.000Z",
        completedAt: "2026-07-28T08:00:02.000Z",
      },
      {
        id: "run-2",
        workId: "work-1",
        assignmentId: "assignment-2",
        agentInstanceId: "agent-1",
        engineKind: "fake",
        engineSessionId: "session-2",
        modelLabel: "Fake model",
        status: "completed",
        createdAt: "2026-07-28T08:00:03.000Z",
        startedAt: "2026-07-28T08:00:03.000Z",
        completedAt: "2026-07-28T08:00:04.000Z",
      },
    ];
    detail.messages = [
      {
        id: "message-2",
        workId: "work-1",
        runId: "run-2",
        role: "user",
        content: "第二次用户指令",
        resourceIds: [],
        createdAt: "2026-07-28T08:00:03.000Z",
      },
      {
        id: "message-1",
        workId: "work-1",
        runId: "run-1",
        role: "user",
        content: "第一次用户指令",
        resourceIds: [],
        createdAt: "2026-07-28T08:00:01.000Z",
      },
    ];
    detail.events = [
      event(1, { type: "runStarted", modelLabel: "Fake model" }),
      {
        ...event(1, { type: "runStarted", modelLabel: "Fake model" }),
        runId: "run-2",
        occurredAt: "2026-07-28T08:00:04.000Z",
      },
    ];
    client.seed(detail);

    render(<WorkSurface client={client} />);

    const first = await screen.findByText("第一次用户指令");
    const second = screen.getByText("第二次用户指令");
    expect(first.compareDocumentPosition(second) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(within(first.closest("article")!).getByText("你")).toBeInTheDocument();
    expect(screen.queryByText("引擎复述 prompt")).not.toBeInTheDocument();
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

    await screen.findByText("交付完成");
    await user.click(await screen.findByRole("button", { name: "打开检查器" }));
    const tabs = await screen.findByRole("tablist", { name: "对话检查器" });
    expect(within(tabs).getAllByRole("tab")).toHaveLength(8);
    expect(within(tabs).queryByRole("tab", { name: "变更" })).not.toBeInTheDocument();
    const delivery = within(tabs).getByRole("tab", { name: "交付" });
    await user.click(delivery);
    expect(delivery).toHaveAttribute("aria-selected", "true");
    const artifactPanel = screen.getByRole("tabpanel", { name: "交付" });
    expect(within(artifactPanel).getByText("inspector-output.html")).toBeInTheDocument();
    expect(within(artifactPanel).queryByText("交付完成")).not.toBeInTheDocument();
    expect(within(artifactPanel).queryByText("还没有产物")).not.toBeInTheDocument();
    await user.click(within(tabs).getByRole("tab", { name: "验证" }));
    await user.keyboard("{ArrowRight}");
    expect(within(tabs).getByRole("tab", { name: "活动原始记录" })).toHaveFocus();
  });

  it("交付检查器在已完成 Run 没有产物时提供不重复摘要的下一步", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const detail = seededDetail("completed");
    detail.events = [
      event(1, {
        type: "runCompleted",
        summary: "仅在时间线显示的交付摘要",
        artifacts: [],
        validation: [],
        limitations: [],
      }),
    ];
    client.seed(detail);
    render(<WorkSurface client={client} />);

    await screen.findByText("仅在时间线显示的交付摘要");
    await user.click(await screen.findByRole("button", { name: "打开检查器" }));
    const artifactPanel = screen.getByRole("tabpanel", { name: "交付" });

    expect(within(artifactPanel).queryByText("仅在时间线显示的交付摘要")).not.toBeInTheDocument();
    expect(within(artifactPanel).getByText("最新一次执行没有产物。继续对话并生成文件后，这里会列出可查看的产物。")).toBeInTheDocument();
  });

  it("交付检查器在尚无 Run 时引导用户先发送指令", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.seed(seededDetail("draft"));
    render(<WorkSurface client={client} />);

    await user.click(await screen.findByRole("button", { name: "打开检查器" }));
    const artifactPanel = screen.getByRole("tabpanel", { name: "交付" });

    expect(within(artifactPanel).getByText("生成文件后，产物会显示在这里。先在下方输入一条指令。")).toBeInTheDocument();
  });

  it("活动原始记录逐条显示 journal event discriminator", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const detail = seededDetail("completed");
    detail.events = [
      event(1, { type: "runStarted", modelLabel: "Fake model" }),
      event(2, { type: "assistantDelta", text: "正在分析" }),
      event(3, { type: "assistantDelta", text: "收入" }),
      event(4, { type: "toolStarted", toolCallId: "tool-1", toolName: "read_file", inputSummary: "读取文件" }),
      event(5, { type: "toolFinished", toolCallId: "tool-1", toolName: "read_file", outputSummary: "读取完成", success: true }),
      event(6, { type: "runCompleted", summary: "执行完成", artifacts: [], validation: [], limitations: [] }),
      event(7, { type: "runFailed", message: "诊断信息" }),
    ];
    client.seed(detail);
    render(<WorkSurface client={client} />);

    await user.click(await screen.findByRole("button", { name: "打开检查器" }));
    await user.click(screen.getByRole("tab", { name: "活动原始记录" }));
    const logs = screen.getByRole("tabpanel", { name: "活动原始记录" });

    expect(within(logs).getAllByTestId("raw-activity-event")).toHaveLength(
      detail.events.length,
    );
    for (const discriminator of ["runStarted", "assistantDelta", "toolStarted", "toolFinished", "runCompleted", "runFailed"]) {
      expect(within(logs).getAllByText(discriminator)).toHaveLength(
        discriminator === "assistantDelta" ? 2 : 1,
      );
    }
  });

  it("检查器展示整个 Work 的结构化结果，不提供执行批次切换", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const detail = seededDetail("completed");
    detail.events = [
      event(1, {
        type: "runCompleted",
        summary: "第一次交付",
        artifacts: ["old-report.md"],
        validation: [],
        limitations: [],
      }),
      {
        ...event(1, {
          type: "runCompleted",
          summary: "最新交付",
          artifacts: ["latest-report.md"],
          validation: [],
          limitations: [],
        }),
        runId: "run-2",
        occurredAt: "2026-07-28T08:00:02.000Z",
      },
    ];
    client.seed(detail);
    render(<WorkSurface client={client} />);

    await user.click(await screen.findByRole("button", { name: "打开检查器" }));
    const inspector = screen.getByLabelText("对话检查器", { selector: "aside" });
    expect(within(inspector).getByText("latest-report.md")).toBeInTheDocument();
    expect(within(inspector).getByText("old-report.md")).toBeInTheDocument();
    expect(within(inspector).queryByRole("heading", { name: /第 .* 次执行/u })).not.toBeInTheDocument();

    expect(within(inspector).queryByRole("button", { name: "最新结果" })).not.toBeInTheDocument();
    expect(within(inspector).queryByRole("button", { name: "全部记录" })).not.toBeInTheDocument();
  });

  it("显示加载、错误诊断与可重试入口", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    let rejectList!: (error: unknown) => void;
    client.listWorks.mockImplementationOnce(
      () => new Promise((_, reject) => { rejectList = reject; }),
    );
    render(<WorkSurface client={client} />);
    const loadingState = screen.getByRole("status", { name: "正在加载对话" });
    expect(loadingState).toHaveAttribute("data-motion-state", "loading");
    expect(loadingState.querySelectorAll("[data-motion-line]")).toHaveLength(2);
    rejectList({
      code: "database_error",
      message: "Database operation failed",
      details: { path: "C:\\Users\\private\\piwork.sqlite3", token: "secret-token" },
    });

    const pageAlert = await screen.findByRole("alert");
    expect(pageAlert).toHaveAttribute("data-motion-state", "error");
    expect(pageAlert).toHaveTextContent("本地数据库暂时不可用，请重试。");
    expect(screen.getByRole("alert")).not.toHaveTextContent("Database operation failed");
    expect(screen.getByRole("alert")).not.toHaveTextContent("database_error");
    await user.click(screen.getByRole("button", { name: "打开诊断" }));
    expect(screen.getByText(/database_error/)).toBeInTheDocument();
    expect(screen.getByText(/Database operation failed/)).toBeInTheDocument();
    expect(screen.queryByText(/private\\piwork/)).not.toBeInTheDocument();
    expect(screen.queryByText(/secret-token/)).not.toBeInTheDocument();
    expect(screen.getByText(/\[redacted\]/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "重试" }));
    await waitFor(() => expect(client.listWorks).toHaveBeenCalledTimes(2));
  });

  it("初始 Work 详情加载失败时显示完整错误页", async () => {
    const client = createMockTauriClient();
    client.seed(seededDetail());
    client.getWork.mockRejectedValueOnce({
      code: "database_error",
      message: "Raw detail database failure",
    });

    render(<WorkSurface client={client} />);

    const errorPage = await screen.findByRole("alert");
    expect(errorPage).toHaveTextContent("本地数据库暂时不可用，请重试。");
    expect(errorPage).not.toHaveTextContent("Raw detail database failure");
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

  it("无 Work 时直接显示首任务输入并在未选择项目时禁止开始", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    render(<WorkSurface client={client} />);
    expect(await screen.findByRole("heading", { name: "今天想让 Pi 帮你完成什么？" })).toBeInTheDocument();
    const prompt = screen.getByRole("textbox", { name: "首个任务" });
    await user.type(prompt, "分析当前项目");
    expect(screen.getByRole("button", { name: "发送" })).toBeDisabled();
    expect(client.createWork).not.toHaveBeenCalled();
  });

  it("可以通过系统目录选择器选定项目且不在界面暴露完整路径", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const pickProjectDirectory = vi.fn().mockResolvedValue("D:\\workspace\\picked-project");
    render(<WorkSurface client={client} pickProjectDirectory={pickProjectDirectory} />);

    await user.click(await screen.findByRole("button", { name: "选择项目" }));
    await user.click(screen.getByRole("button", { name: "选择其他文件夹…" }));

    expect(pickProjectDirectory).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: /picked-project/ })).toHaveAttribute(
      "title",
      "D:\\workspace\\picked-project",
    );
    expect(screen.queryByText("D:\\workspace\\picked-project")).not.toBeInTheDocument();
  });

  it("通过搜索最近项目选择目录且不提供手动路径输入", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.seed(seededDetail());
    render(<WorkSurface client={client} />);

    await user.click(await screen.findByRole("button", { name: "新对话" }));
    await user.click(screen.getByRole("button", { name: /当前项目：revenue/u }));

    const search = screen.getByRole("searchbox", { name: "搜索项目" });
    await user.type(search, "revenue");
    const picker = screen.getByRole("dialog", { name: "选择项目" });
    expect(picker.closest(".project-chip-control")).not.toBeNull();
    await user.click(within(picker).getByRole("button", { name: /revenue/u }));

    expect(screen.getByRole("button", { name: /当前项目：revenue/u })).toHaveAttribute(
      "title",
      "D:\\workspace\\revenue",
    );
    expect(screen.queryByRole("textbox", { name: "工作目录" })).not.toBeInTheDocument();
  });

  it("点击项目浮层外部时关闭浮层", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.seed(seededDetail());
    render(<WorkSurface client={client} />);

    await user.click(await screen.findByRole("button", { name: "新对话" }));
    await user.click(screen.getByRole("button", { name: /当前项目：revenue/u }));

    const search = screen.getByRole("searchbox", { name: "搜索项目" });
    await user.click(search);
    expect(search).toBeInTheDocument();

    await user.click(screen.getByRole("textbox", { name: "首个任务" }));
    expect(screen.queryByRole("searchbox", { name: "搜索项目" })).not.toBeInTheDocument();
  });

  it("最近项目显示父路径，并可用 Escape 关闭后归还焦点", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const first = seededDetail();
    first.summary.rootPath = "D:\\workspace\\client\\app";
    const second = seededDetail();
    second.summary.id = "work-2";
    second.summary.title = "另一个 Work";
    second.summary.rootPath = "D:\\archive\\app";
    client.seed(first);
    client.seed(second);
    render(<WorkSurface client={client} />);

    const trigger = await screen.findByRole("button", { name: "新对话" });
    await user.click(trigger);
    const projectTrigger = screen.getByRole("button", { name: /当前项目：app/u });
    await user.click(projectTrigger);

    expect(screen.getByText("D:\\workspace\\client")).toBeInTheDocument();
    expect(screen.getByText("D:\\archive")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("searchbox", { name: "搜索项目" })).not.toBeInTheDocument();
    expect(projectTrigger).toHaveFocus();
  });

  it("新版项目选择器使用固定网格列对齐项目名与父路径", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const first = seededDetail();
    first.summary.rootPath = "D:\\dev\\创新研究项目\\PiTest";
    const second = seededDetail();
    second.summary.id = "work-2";
    second.summary.title = "另一个 Work";
    second.summary.rootPath = "D:\\dev\\创新研究项目\\启元";
    client.seed(first);
    client.seed(second);
    render(<WorkSurface client={client} initialView="new" />);

    await user.click(await screen.findByRole("button", { name: "选择项目" }));
    const picker = screen.getByRole("dialog", { name: "选择项目" });
    const piTest = within(picker).getByRole("button", { name: /PiTest/u });
    const qiYuan = within(picker).getByRole("button", { name: /启元/u });

    expect(within(piTest).getByText("D:\\dev\\创新研究项目")).toHaveClass("project-picker__path-context");
    expect(within(qiYuan).getByText("D:\\dev\\创新研究项目")).toHaveClass("project-picker__path-context");
    expect(dashboardStyles).toMatch(/\.new-work-start--dashboard \.project-picker__recent button\s*\{[^}]*display:\s*grid;[^}]*grid-template-columns:/su);
    expect(dashboardStyles).toMatch(/\.new-work-start--dashboard \.project-picker__path-context\s*\{[^}]*grid-column:\s*3;/su);
  });

  it("项目分组以标题为主，并将状态放在可访问名称而不是堆叠元信息", async () => {
    const client = createMockTauriClient();
    client.seed(seededDetail());
    const rendered = render(<WorkSurface client={client} />);

    const sidebar = await screen.findByRole("complementary", { name: "项目与对话" });
    expect(within(sidebar).getByRole("button", { name: "项目 revenue" })).toBeInTheDocument();
    const item = within(sidebar).getByRole("button", { name: "营收看板, 草稿" });
    expect(within(item).getByText("营收看板")).toBeInTheDocument();
    expect(within(item).queryByText("草稿")).not.toBeInTheDocument();
    expect(rendered.container.querySelector(".work-sidebar__time")).not.toBeInTheDocument();
  });

  it("不展示没有实时健康契约支撑的本地连接状态", async () => {
    const client = createMockTauriClient();
    client.seed(seededDetail());
    render(<WorkSurface client={client} />);

    await screen.findByRole("complementary", { name: "项目与对话" });
    expect(screen.queryByText("本地执行已连接")).not.toBeInTheDocument();
  });

  it("Work 标题栏只显示真实可用的检查器操作", async () => {
    const client = createMockTauriClient();
    client.seed(seededDetail());
    render(<WorkSurface client={client} />);

    await screen.findByRole("heading", { name: "营收看板" });
    expect(screen.queryByRole("button", { name: "更多操作" })).not.toBeInTheDocument();
    expect(screen.getByText("revenue", { selector: ".work-header__breadcrumb-project" })).toBeInTheDocument();
    expect(screen.getByText("revenue", { selector: ".work-header__project-name" })).toBeInTheDocument();
    expect(screen.getByText("Pi", { selector: ".work-header__model" })).toBeInTheDocument();
  });

  it("默认使用主页延展型详情并可在标题栏切回经典版", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.seed(seededDetail());
    render(<WorkSurface client={client} />);

    await screen.findByRole("heading", { name: "营收看板" });
    const main = screen.getByRole("main");
    expect(main.querySelector('[data-detail-experience="dashboard"]')).not.toBeNull();

    await user.click(screen.getByRole("button", { name: "经典版" }));

    expect(main.querySelector('[data-detail-experience="classic"]')).not.toBeNull();
    expect(localStorage.getItem("piwork.detailExperience")).toBe("classic");
  });

  it("新版详情只保留蓝色用户气泡，不绘制经典版灰色外层", async () => {
    const client = createMockTauriClient();
    const detail = seededDetail();
    detail.messages = [{
      id: "message-1",
      workId: "work-1",
      runId: "run-1",
      role: "user",
      content: "2222",
      resourceIds: [],
      createdAt: "2026-07-28T08:00:01.000Z",
    }];
    client.seed(detail);
    render(<WorkSurface client={client} />);

    const message = (await screen.findByText("2222")).closest("article");
    expect(message).not.toBeNull();
    expect(message).toHaveClass("timeline-user");
    expect(dashboardStyles).toMatch(/\.workspace-main--dashboard \.timeline-user\s*\{[^}]*padding:\s*0;[^}]*border:\s*0;[^}]*background:\s*transparent;/su);
    expect(dashboardStyles).toMatch(/\.workspace-main--dashboard \.timeline-user p\s*\{[^}]*background:\s*var\(--pw-dashboard-user-message\);/su);
  });

  it("Work 标题栏以次要上下文显示持久目标", async () => {
    const client = createMockTauriClient();
    client.seed(seededDetail());
    const rendered = render(<WorkSurface client={client} />);

    await screen.findByRole("heading", { name: "营收看板" });
    const goal = rendered.container.querySelector(".work-header__goal");
    expect(goal).toHaveTextContent("构建营收看板");
    expect(goal).toHaveAttribute("title", "构建营收看板");
  });

  it("在已有 Work 中把 @ 文件作为结构化引用发送", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.seed(seededDetail());
    client.listProjectFiles.mockResolvedValue([
      { relativePath: "src/features/WorkComposer.tsx" },
    ]);
    render(<WorkSurface client={client} />);

    const composer = await screen.findByRole("textbox", { name: "给 PiWork 指令" });
    await user.click(composer);
    await user.type(composer, "参考 @composer");
    await user.keyboard("{Enter}");
    await user.click(screen.getByRole("button", { name: "发送" }));

    expect(client.startWork).toHaveBeenLastCalledWith(
      "work-1",
      "参考 @{src/features/WorkComposer.tsx}",
      ["src/features/WorkComposer.tsx"],
      [],
    );
  });

  it("在新建 Work 的首任务中发送 @ 文件引用", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const pickProjectDirectory = vi.fn().mockResolvedValue("D:\\workspace\\new-project");
    client.listProjectFiles.mockResolvedValue([{ relativePath: "README.md" }]);
    render(
      <WorkSurface
        client={client}
        pickProjectDirectory={pickProjectDirectory}
      />,
    );

    await user.click(await screen.findByRole("button", { name: "选择项目" }));
    await user.click(screen.getByRole("button", { name: "选择其他文件夹…" }));
    const prompt = screen.getByRole("textbox", { name: "首个任务" });
    await user.click(prompt);
    await user.type(prompt, "根据 @readme");
    await screen.findByRole("option", { name: "README.md" });
    await user.keyboard("{Enter}");
    await user.click(screen.getByRole("button", { name: "发送" }));

    expect(client.startWork).toHaveBeenLastCalledWith(
      "work-1",
      "根据 @{README.md}",
      ["README.md"],
      [],
    );
  });

  it("切换项目时移除旧文件引用并保留周围草稿", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.listProjectFiles.mockResolvedValue([{ relativePath: "README.md" }]);
    const pickProjectDirectory = vi.fn()
      .mockResolvedValueOnce("D:\\workspace\\revenue")
      .mockResolvedValueOnce("D:\\workspace\\new-project");
    render(
      <WorkSurface
        client={client}
        pickProjectDirectory={pickProjectDirectory}
      />,
    );

    await user.click(await screen.findByRole("button", { name: "选择项目" }));
    await user.click(screen.getByRole("button", { name: "选择其他文件夹…" }));

    const prompt = screen.getByRole("textbox", { name: "首个任务" });
    await user.click(prompt);
    await user.type(prompt, "比较 @readme");
    await screen.findByRole("option", { name: "README.md" });
    await user.keyboard("{Enter}");
    await user.keyboard(" 和当前实现");

    await user.click(screen.getByRole("button", { name: /revenue/ }));
    await user.click(screen.getByRole("button", { name: "选择其他文件夹…" }));

    await waitFor(() => {
      expect(prompt).toHaveTextContent("比较 和当前实现");
      expect(prompt).not.toHaveTextContent("README.md");
    });
  });

  it("标题栏隐藏完整目录，只通过项目芯片的悬浮提示提供路径", async () => {
    const client = createMockTauriClient();
    client.seed(seededDetail());
    render(<WorkSurface client={client} />);

    await screen.findByRole("heading", { name: "营收看板" });
    expect(screen.queryByText("D:\\workspace\\revenue")).not.toBeInTheDocument();
    expect(screen.getByText("revenue", { selector: ".work-header__project-name" }).closest("[title]")).toHaveAttribute("title", "D:\\workspace\\revenue");
  });

  it("模型在标题栏作为次要上下文，并保留输入框操作区指示", async () => {
    const client = createMockTauriClient();
    const detail = seededDetail("completed");
    detail.runs = [{
      id: "run-1",
      workId: "work-1",
      assignmentId: "assignment-1",
      agentInstanceId: "agent-1",
      engineKind: "codex",
      engineSessionId: "session-1",
      modelLabel: "gpt-5.6-sol",
      status: "completed",
      createdAt: "2026-07-28T08:00:01.000Z",
      startedAt: "2026-07-28T08:00:01.000Z",
      completedAt: "2026-07-28T08:00:02.000Z",
    }];
    client.seed(detail);
    const rendered = render(<WorkSurface client={client} modelLabel="gpt-5.6-sol" />);

    await screen.findByRole("heading", { name: "营收看板" });
    const header = rendered.container.querySelector<HTMLElement>(".work-header");
    expect(header).not.toBeNull();
    expect(within(header!).getByText("gpt-5.6-sol")).toHaveClass("work-header__model");

    const model = screen.getByText("5.6 Sol");
    expect(model.closest(".work-composer")).not.toBeNull();
    expect(model.closest("[title]")).toHaveAttribute("title", "gpt-5.6-sol");
    const actions = model.closest(".work-composer__actions");
    expect(actions?.querySelector(".lucide-arrow-up")).toBeInTheDocument();
    const submit = within(actions as HTMLElement).getByRole("button", { name: "发送" });
    expect(submit.childNodes).toHaveLength(1);
  });

  it("模型入口显示在新 Work 输入框操作区", async () => {
    const client = createMockTauriClient();
    render(<WorkSurface client={client} modelLabel="gpt-5.6-sol" />);

    const model = await screen.findByText("5.6 Sol");
    expect(model.closest(".new-work-start__actions")).not.toBeNull();
    expect(screen.queryByText("Agent · 自动")).not.toBeInTheDocument();
  });

  it("已有 Work 时点击新建进入空白工作台，选择原 Work 可取消", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.seed(seededDetail());
    render(<WorkSurface client={client} />);
    const sidebar = screen.getByRole("complementary", { name: "项目与对话" });
    const trigger = within(sidebar).getByRole("button", { name: "新对话" });

    await user.click(trigger);
    expect(await screen.findByRole("heading", { name: "今天想让 Pi 帮你完成什么？" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "首个任务" })).toHaveFocus();
    expect(screen.queryByRole("dialog", { name: "要在 PiWork 中完成什么？" })).not.toBeInTheDocument();
    expect(trigger).toHaveAttribute("aria-current", "page");
    expect(client.createWork).not.toHaveBeenCalled();
    expect(client.startWork).not.toHaveBeenCalled();

    await user.click(within(sidebar).getByRole("button", { name: /营收看板/ }));
    expect(await screen.findByRole("heading", { name: "营收看板" })).toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: "首个任务" })).not.toBeInTheDocument();
  });

  it("创建 pending 时同步单飞并禁用首任务控件", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const pendingCreate = deferred<WorkDetail>();
    client.createWork.mockImplementationOnce(() => pendingCreate.promise);
    const pickProjectDirectory = vi.fn().mockResolvedValue("D:\\workspace\\single-flight");
    render(<WorkSurface client={client} pickProjectDirectory={pickProjectDirectory} />);

    const prompt = await screen.findByRole("textbox", { name: "首个任务" });
    await user.click(screen.getByRole("button", { name: "选择项目" }));
    await user.click(screen.getByRole("button", { name: "选择其他文件夹…" }));
    await user.type(prompt, "只创建一次");
    const start = screen.getByRole("button", { name: "发送" });
    fireEvent.click(start);
    fireEvent.click(start);

    expect(client.createWork).toHaveBeenCalledTimes(1);
    expect(prompt).toHaveAttribute("aria-disabled", "true");
    expect(screen.getByRole("button", { name: /single-flight/ })).toBeDisabled();
    expect(start).toBeDisabled();

    pendingCreate.resolve(seededDetail());
    await waitFor(() =>
      expect(screen.queryByRole("textbox", { name: "首个任务" })).not.toBeInTheDocument(),
    );
  });

  it("创建失败后解除 pending、保留输入与项目并聚焦首任务", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const pendingCreate = deferred<WorkDetail>();
    client.createWork.mockImplementationOnce(() => pendingCreate.promise);
    const pickProjectDirectory = vi.fn().mockResolvedValue("D:\\workspace\\keep-input");
    render(<WorkSurface client={client} pickProjectDirectory={pickProjectDirectory} />);

    const prompt = await screen.findByRole("textbox", { name: "首个任务" });
    await user.click(screen.getByRole("button", { name: "选择项目" }));
    await user.click(screen.getByRole("button", { name: "选择其他文件夹…" }));
    await user.type(prompt, "保留这条目标");
    await user.click(screen.getByRole("button", { name: "发送" }));

    pendingCreate.reject({
      code: "database_error",
      message: "raw database failure must stay hidden",
    });

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "本地数据库暂时不可用，请重试。",
    );
    expect(screen.queryByText("raw database failure must stay hidden")).not.toBeInTheDocument();
    expect(prompt).toHaveTextContent("保留这条目标");
    expect(screen.getByRole("button", { name: /keep-input/ })).toBeEnabled();
    expect(prompt).toHaveAttribute("aria-disabled", "false");
    expect(screen.getByRole("button", { name: "发送" })).toBeEnabled();
    await waitFor(() => expect(prompt).toHaveFocus());
  });

  it.each([
    ["zh-CN", "所选工作目录不存在或无法访问。"],
    ["en", "The selected working directory does not exist or cannot be accessed."],
  ] as const)("%s 创建不存在路径时只显示本地化产品错误", async (language, expected) => {
    await i18n.changeLanguage(language);
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.createWork.mockRejectedValueOnce({
      code: "path_resolution_error",
      message: "Raw workspace path could not be resolved",
      details: {
        field: "rootPath",
        path: "C:\\Users\\private\\missing-workspace",
      },
    });
    const pickProjectDirectory = vi.fn().mockResolvedValue("C:\\missing");
    render(<WorkSurface client={client} pickProjectDirectory={pickProjectDirectory} />);

    const prompt = await screen.findByRole("textbox", { name: language === "en" ? "First task" : "首个任务" });
    await user.click(screen.getByRole("button", { name: language === "en" ? "Select project" : "选择项目" }));
    await user.click(screen.getByRole("button", { name: language === "en" ? "Choose another folder…" : "选择其他文件夹…" }));
    await user.type(prompt, "Test path error");
    await user.click(screen.getByRole("button", { name: language === "en" ? "Send" : "发送" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(expected);
    expect(screen.queryByText("Raw workspace path could not be resolved")).not.toBeInTheDocument();
    expect(screen.queryByText(/private\\missing-workspace/)).not.toBeInTheDocument();
  });

  it("首任务启动成功后将焦点移入 Work composer", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const pickProjectDirectory = vi.fn().mockResolvedValue("D:\\workspace\\empty-focus");
    render(<WorkSurface client={client} pickProjectDirectory={pickProjectDirectory} />);
    const prompt = await screen.findByRole("textbox", { name: "首个任务" });
    await user.click(screen.getByRole("button", { name: "选择项目" }));
    await user.click(screen.getByRole("button", { name: "选择其他文件夹…" }));
    await user.type(prompt, "中央空态焦点测试");
    await user.click(screen.getByRole("button", { name: "发送" }));

    const composer = await screen.findByRole("textbox", { name: "给 PiWork 指令" });
    expect(composer).toHaveFocus();
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

  it("检查器默认隐藏，打开后成为可拖拽三段式并记住宽度", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.seed(seededDetail());
    const rendered = render(<WorkSurface client={client} />);
    const toggle = await screen.findByRole("button", { name: "打开检查器" });
    const inspector = rendered.container.querySelector(".work-inspector");
    expect(inspector).not.toBeNull();
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(inspector).toHaveAttribute("aria-hidden", "true");
    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(inspector).toHaveAttribute("aria-hidden", "false");
    expect(inspector).toHaveAttribute("data-open", "true");
    const divider = screen.getByRole("separator", { name: "调整检查器宽度" });
    fireEvent.doubleClick(divider);
    expect(localStorage.getItem("piwork.inspectorWidthPercent")).toBe("36");
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
    const inspector = screen.getByLabelText("对话检查器", { selector: "aside" });
    expect(inspector).toHaveAttribute("aria-hidden", "true");
    expect(inspector).toHaveAttribute("inert");

    await user.click(toggle);
    expect(inspector).toHaveAttribute("aria-hidden", "false");
    expect(inspector).not.toHaveAttribute("inert");
    expect(inspector).toHaveAttribute("role", "dialog");
    expect(inspector).toHaveAttribute("aria-modal", "true");
    await waitFor(() => expect(within(inspector).getByRole("button", { name: "关闭检查器" })).toHaveFocus());
    toggle.focus();
    fireEvent.keyDown(window, { key: "Escape" });

    expect(inspector).toHaveAttribute("aria-hidden", "true");
    expect(toggle).toHaveFocus();
  });

  it("从侧栏进入智能体中心，同时保留未落地模块的禁用状态", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    render(<WorkSurface client={client} initialView="home" />);

    const sidebar = await screen.findByRole("complementary", { name: "项目与对话" });
    expect(within(sidebar).getByRole("button", { name: "新对话" })).toHaveAttribute("aria-current", "page");
    expect(within(sidebar).queryByRole("button", { name: "首页" })).not.toBeInTheDocument();
    const agents = within(sidebar).getByRole("button", { name: "智能体中心" });
    expect(agents).toBeEnabled();
    await user.click(agents);
    expect(await screen.findByRole("heading", { name: "智能体中心" })).toBeInTheDocument();
    expect(agents).toHaveAttribute("aria-current", "page");
    expect(within(sidebar).getByRole("button", { name: "知识库（即将推出）" })).toBeDisabled();
    expect(within(sidebar).getByRole("button", { name: "数据源（即将推出）" })).toBeDisabled();
  });

  it("shows_catalog_badges_and_keeps_prompt_drafting", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    render(<WorkSurface client={client} initialView="home" />);

    const sidebar = await screen.findByRole("complementary", { name: "项目与对话" });
    await user.click(within(sidebar).getByRole("button", { name: "智能体中心" }));
    await user.click(await screen.findByRole("tab", { name: "能力库" }));
    expect(screen.getAllByText("目录能力").length).toBeGreaterThan(0);
    await user.click(await screen.findByRole("button", { name: "查看需求澄清智能体详情" }));
    const capabilityDialog = screen.getByRole("dialog", { name: "需求澄清智能体" });
    expect(within(capabilityDialog).getByText("目录能力")).toBeInTheDocument();
    await user.click(within(capabilityDialog).getByRole("button", { name: "创建任务草稿" }));

    const home = await screen.findByRole("region", { name: "对话主页" });
    const editor = within(home).getByLabelText("首个任务");
    expect(editor).toHaveTextContent("需求澄清智能体");
    expect(editor).toHaveTextContent("业务目标：");
    expect(client.createWork).not.toHaveBeenCalled();
    expect(client.startWork).not.toHaveBeenCalled();
  });

  it("侧栏设置导航项打开设置页", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    render(<WorkSurface client={client} />);

    const sidebar = await screen.findByRole("complementary", { name: "项目与对话" });
    await user.click(within(sidebar).getByRole("button", { name: "设置" }));

    expect(await screen.findByRole("region", { name: "设置" })).toBeInTheDocument();
  });

  it("仅保留具体项目的快捷新建并在首页预选该项目", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.seed(seededDetail());
    render(<WorkSurface client={client} />);

    const sidebar = await screen.findByRole("complementary", { name: "项目与对话" });
    expect(within(sidebar).queryByRole("button", { name: "新建项目" })).not.toBeInTheDocument();
    await user.click(within(sidebar).getByRole("button", { name: "在 revenue 中新建对话" }));

    expect(await screen.findByRole("region", { name: "对话主页" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /当前项目：revenue/u })).toHaveAttribute(
      "title",
      "D:\\workspace\\revenue",
    );
  });
});
