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
  workspaceId: overrides.workspaceId ?? `workspace-${id}`,
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

  it("renders a focused CoDo conversation entry without dashboard modules", async () => {
    renderHome();

    const home = await screen.findByRole("region", { name: "对话主页" });
    expect(within(home).getByRole("img", { name: "CoDo" })).toBeInTheDocument();
    expect(within(home).getByRole("heading", { name: "有什么事，交给 CoDo。" })).toBeInTheDocument();
    expect(within(home).getByText("Ask less. Get more done.")).toBeInTheDocument();
    expect(within(home).getByTestId("dashboard-composer")).toBeInTheDocument();
    expect(within(home).queryByLabelText("任务起点")).not.toBeInTheDocument();
    expect(screen.queryByRole("complementary", { name: "智能体活动" })).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "环境状态" })).not.toBeInTheDocument();
  });

  it("offers recent projects inside the compact project picker", async () => {
    const user = userEvent.setup();
    const older = work("work-1", "D:\\Projects\\PiTest", "2026-07-28T08:00:00.000Z");
    const newer = work("work-2", "D:\\Projects\\PiTest", "2026-07-28T09:00:00.000Z");
    renderHome({ works: [older, newer] });

    await user.click(screen.getByRole("button", { name: "选择项目" }));
    await user.click(screen.getByRole("button", { name: /PiTest/ }));

    expect(screen.getByRole("button", { name: /当前项目：PiTest/ })).toBeInTheDocument();
  });

  it("shows a project-picker empty state only when the user opens it", async () => {
    const user = userEvent.setup();
    renderHome({ works: [] });

    expect(screen.queryByText("还没有最近项目")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "选择项目" }));
    expect(screen.getByText("还没有最近项目")).toBeInTheDocument();
  });

  it("imports a project directory from the composer", async () => {
    const user = userEvent.setup();
    const pickProjectDirectory = vi.fn().mockResolvedValue("D:\\Projects\\Imported");
    renderHome({ pickProjectDirectory });

    await user.click(screen.getByRole("button", { name: "选择项目" }));
    await user.click(screen.getByRole("button", { name: "选择其他文件夹…" }));

    expect(pickProjectDirectory).toHaveBeenCalledTimes(1);
    expect(await screen.findByRole("button", { name: /当前项目：Imported/ })).toBeInTheDocument();
  });

  it("keeps context, attachment, model and submit as the only composer controls", async () => {
    renderHome();

    const home = await screen.findByRole("region", { name: "对话主页" });
    const composer = within(home).getByTestId("dashboard-composer");
    expect(within(composer).getByTestId("dashboard-composer-context")).toBeInTheDocument();
    expect(within(composer).getByTestId("dashboard-composer-tools")).toBeInTheDocument();
    expect(within(composer).getByTestId("dashboard-composer-submit")).toBeInTheDocument();
    expect(within(composer).getByRole("button", { name: "添加附件" })).toBeInTheDocument();
    expect(within(composer).queryByRole("button", { name: /即将推出/ })).not.toBeInTheDocument();
  });
});
