import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { WorkSummary } from "../../bindings";
import type { PickProjectDirectory } from "../../app/projectDirectory";
import { i18n } from "../../i18n";
import { createMockTauriClient } from "../../test/mockTauriClient";
import { WorkStoreProvider } from "../works/WorkStoreProvider";
import { WorkHome } from "./WorkHome";

const work = (
  id: string,
  rootPath: string,
  updatedAt: string,
  overrides: Partial<WorkSummary> = {},
): WorkSummary => ({
  id,
  title: `${id}-title`,
  goal: `${id}-goal`,
  rootPath,
  permissionMode: "balanced",
  status: "idle",
  createdAt: updatedAt,
  updatedAt,
  ...overrides,
});

const renderHome = ({
  works = [],
  pickProjectDirectory = vi.fn().mockResolvedValue(null),
  onWorkSelected = vi.fn(),
  onAllWorks = vi.fn(),
  onAgentsRequest = vi.fn(),
  client = createMockTauriClient(),
}: {
  works?: WorkSummary[];
  pickProjectDirectory?: PickProjectDirectory;
  onWorkSelected?: (work: WorkSummary) => void;
  onAllWorks?: () => void;
  onAgentsRequest?: () => void;
  client?: ReturnType<typeof createMockTauriClient>;
} = {}) => {
  render(
    <WorkStoreProvider client={client}>
      <WorkHome
        modelLabel="Pi"
        onAgentsRequest={onAgentsRequest}
        onAllWorks={onAllWorks}
        onStarted={vi.fn()}
        onWorkSelected={onWorkSelected}
        pickAttachments={async () => []}
        pickProjectDirectory={pickProjectDirectory}
        works={works}
      />
    </WorkStoreProvider>,
  );
};

describe("WorkHome", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("zh-CN");
  });

  it("renders the greeting, all five task starters and the home-only activity rail", async () => {
    renderHome();

    const home = await screen.findByRole("region", { name: "对话主页" });
    expect(within(home).getByTestId("dashboard-orbit-art")).toHaveAttribute("aria-hidden", "true");
    expect(within(home).getByRole("heading", { name: "今天想让 Pi 帮你完成什么？" })).toBeInTheDocument();
    expect(within(home).getAllByText("Pi", { selector: ".dashboard-greeting__pi" })).toHaveLength(2);
    expect(within(home).getByTestId("dashboard-panels-row")).toHaveClass("dashboard-panels-row");
    expect(within(home).getByLabelText("任务起点").children).toHaveLength(5);
    for (const title of ["探索并理解项目", "构建新功能", "审查代码", "修复问题", "生成文档"]) {
      expect(screen.getByRole("button", { name: title })).toBeInTheDocument();
    }
    expect(screen.getByRole("complementary", { name: "智能体活动" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "智能体活动" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "今日摘要" })).toBeInTheDocument();
  });

  it("groups recent projects from real Work data and opens the most recently updated conversation", async () => {
    const user = userEvent.setup();
    const onWorkSelected = vi.fn();
    const older = work("work-1", "D:\\Projects\\PiTest", "2026-07-28T08:00:00.000Z");
    const newer = work("work-2", "D:\\Projects\\PiTest", "2026-07-28T09:00:00.000Z");
    renderHome({ works: [older, newer], onWorkSelected });

    const item = screen.getByRole("button", { name: /PiTest/ });
    await user.click(item);

    expect(onWorkSelected).toHaveBeenCalledWith(newer);
  });

  it("shows an empty state when there are no recent projects", () => {
    renderHome({ works: [] });
    expect(screen.getByText("还没有最近项目，创建第一个对话后会显示在这里。")).toBeInTheDocument();
  });

  it("imports a project directory from the header and applies it to the embedded composer", async () => {
    const user = userEvent.setup();
    const pickProjectDirectory = vi.fn().mockResolvedValue("D:\\Projects\\Imported");
    renderHome({ pickProjectDirectory });

    await user.click(screen.getByRole("button", { name: "导入项目" }));

    expect(pickProjectDirectory).toHaveBeenCalledTimes(1);
    expect(await screen.findByRole("button", { name: /Imported/ })).toBeInTheDocument();
  });

  it("reports real runtime detection results in the environment status panel", async () => {
    const client = createMockTauriClient();
    client.getRuntimeStatus.mockResolvedValue({
      python: { available: true, version: "3.11.6" },
      node: { available: false, version: null },
      git: { available: true, version: null },
    });
    renderHome({ client });

    const panel = screen.getByRole("heading", { name: "环境状态" }).closest("section")!;
    expect(await within(panel).findByText("3.11.6")).toBeInTheDocument();
    expect(within(panel).getByText("未检测到")).toBeInTheDocument();
    expect(within(panel).getByText("已就绪")).toBeInTheDocument();
  });

  it("opens the live agent center from both homepage discovery entries", async () => {
    const user = userEvent.setup();
    const onAgentsRequest = vi.fn();
    renderHome({ onAgentsRequest });

    await user.click(screen.getByRole("button", { name: "探索智能体" }));
    await user.click(screen.getByRole("button", { name: "更多技能" }));
    expect(onAgentsRequest).toHaveBeenCalledTimes(2);
  });

  it("keeps not-yet-available modules as disabled controls rather than misleading live features", () => {
    renderHome();

    expect(screen.getByRole("button", { name: "通知（即将推出）" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "需求分析师（即将推出）" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "智能体（即将推出）" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "知识库（即将推出）" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "联网搜索（即将推出）" })).toBeDisabled();
  });

  it("separates dashboard composer context, tools and submit controls", async () => {
    renderHome();

    const home = await screen.findByRole("region", { name: "对话主页" });
    const composer = within(home).getByTestId("dashboard-composer");
    expect(within(composer).getByTestId("dashboard-composer-context")).toBeInTheDocument();
    expect(within(composer).getByTestId("dashboard-composer-tools")).toBeInTheDocument();
    expect(within(composer).getByTestId("dashboard-composer-submit")).toBeInTheDocument();
    expect(within(composer).queryByText("本地运行环境")).not.toBeInTheDocument();
  });
});
