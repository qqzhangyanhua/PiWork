import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type {
  AgentInstanceSummary,
  CapabilityPackSummary,
  WorkAgentSummary,
  WorkTeamSummary,
} from "../../bindings";
import { i18n } from "../../i18n";
import { createMockTauriClient, type MockTauriClient } from "../../test/mockTauriClient";
import { AgentCenterPage } from "./AgentCenterPage";

const DISPLAY_NAMES = {
  lead: "PiWork 主理人",
  researcher: "研究员",
  engineer: "工程师",
  reviewer: "审阅者",
} as const;

const PACK_NAMES = {
  lead: "统筹与综合",
  researcher: "来源研究",
  engineer: "工程执行",
  reviewer: "独立审阅",
} as const;

const configureAgentCenterClient = async () => {
  const client = createMockTauriClient();
  const instances: AgentInstanceSummary[] = (await client.listAgentInstances()).map((instance) => {
    const role = instance.definition.roleKind;
    const capabilityPacks = instance.definition.capabilityPacks.map((pack) => ({
      ...pack,
      name: PACK_NAMES[role],
    }));
    return {
      ...instance,
      displayName: DISPLAY_NAMES[role],
      definition: {
        ...instance.definition,
        name: DISPLAY_NAMES[role],
        description: `${DISPLAY_NAMES[role]}的长期职责`,
        responsibilities: [`${role}-responsibility`],
        nonResponsibilities: [`${role}-boundary`],
        capabilityPacks,
      },
    } satisfies AgentInstanceSummary;
  });
  const packNameById = new Map(
    instances.flatMap(({ definition }) => definition.capabilityPacks)
      .map((pack) => [pack.id, pack.name]),
  );
  const packs = (await client.listCapabilityPacks()).map((pack) => ({
    ...pack,
    name: packNameById.get(pack.id) ?? pack.name,
  } satisfies CapabilityPackSummary));
  client.listAgentInstances.mockClear();
  client.listCapabilityPacks.mockClear();
  client.listAgentInstances.mockResolvedValue(instances);
  client.listCapabilityPacks.mockResolvedValue(packs);
  return { client, instances, packs };
};

const workAgent = (workId: string, instance: AgentInstanceSummary): WorkAgentSummary => ({
  workId,
  instance,
  roleKind: instance.definition.roleKind,
  status: "joined",
  permissionPolicy:
    instance.permissionPolicyOverride ?? instance.definition.defaultPermissionPolicy,
  joinedAt: "2026-08-15T00:00:00.000Z",
  updatedAt: "2026-08-15T00:00:00.000Z",
});

const workTeam = (
  workId: string,
  leadInstance: AgentInstanceSummary,
  memberInstances: AgentInstanceSummary[],
): WorkTeamSummary => {
  const lead = workAgent(workId, leadInstance);
  return {
    workId,
    lead,
    members: [lead, ...memberInstances.map((instance) => workAgent(workId, instance))],
  };
};

const renderAgentCenter = (
  client: MockTauriClient,
  currentWorkId?: string,
  onStartCatalogCapability = vi.fn(),
) => render(
  <AgentCenterPage
    client={client}
    currentWorkId={currentWorkId}
    onStartCatalogCapability={onStartCatalogCapability}
  />,
);

describe("AgentCenterPage", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("zh-CN");
  });

  it("opens_on_my_team_with_four_builtin_members", async () => {
    const { client } = await configureAgentCenterClient();
    renderAgentCenter(client);

    expect(await screen.findByRole("tab", { name: "我的团队" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    for (const name of Object.values(DISPLAY_NAMES)) {
      expect(screen.getByRole("button", { name: `查看${name}详情` })).toBeInTheDocument();
    }
    expect(screen.getAllByText("主理人", { selector: ".team-member-card__lead" })).toHaveLength(1);
    for (const packName of Object.values(PACK_NAMES)) {
      expect(screen.getByText(packName)).toBeInTheDocument();
    }
    expect(within(screen.getByRole("button", { name: "查看研究员详情" }))
      .getByText("researcher-responsibility")).toBeInTheDocument();
    expect(screen.queryByText("运行中")).not.toBeInTheDocument();
  });

  it("does_not_offer_install_for_catalog_only_capabilities", async () => {
    const user = userEvent.setup();
    const { client } = await configureAgentCenterClient();
    const onStartCatalogCapability = vi.fn();
    renderAgentCenter(client, undefined, onStartCatalogCapability);

    await user.click(await screen.findByRole("tab", { name: "能力库" }));
    const opener = await screen.findByRole("button", { name: "查看需求澄清智能体详情" });
    expect(within(opener).getByText("目录能力")).toBeInTheDocument();
    await user.click(opener);

    const dialog = screen.getByRole("dialog", { name: "需求澄清智能体" });
    expect(within(dialog).getByText("目录能力")).toBeInTheDocument();
    expect(within(dialog).getByText("该能力仍是目录说明，尚未具备可装载所需的指令、工具与验证合同。")).toBeInTheDocument();
    expect(within(dialog).queryByRole("button", { name: /安装/u })).not.toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "创建任务草稿" }));
    expect(onStartCatalogCapability).toHaveBeenCalledOnce();
  });

  it("carries_an_executable_capability_into_member_assembly", async () => {
    const user = userEvent.setup();
    const { client, packs } = await configureAgentCenterClient();
    const executablePack = {
      ...packs.find(({ id }) => id === "catalog-capability:016")!,
      compatibleRoleTemplateIds: ["role-template:researcher:v1"],
      name: "需求澄清能力包",
      status: "executable" as const,
    };
    client.listCapabilityPacks.mockResolvedValue(
      packs.map((pack) => pack.id === executablePack.id ? executablePack : pack),
    );
    renderAgentCenter(client);

    await user.click(await screen.findByRole("tab", { name: "能力库" }));
    await user.click(await screen.findByRole("button", { name: "查看需求澄清智能体详情" }));
    await user.click(within(screen.getByRole("dialog", { name: "需求澄清智能体" }))
      .getByRole("button", { name: "选择成员装配" }));
    await user.click(screen.getByRole("button", { name: "查看研究员详情" }));
    const memberDialog = screen.getByRole("dialog", { name: "研究员" });
    await user.click(within(memberDialog).getByRole("button", { name: "自定义副本" }));

    const selectedPack = within(memberDialog).getByRole("checkbox", { name: /需求澄清能力包/u });
    expect(selectedPack).toBeChecked();
    await waitFor(() => expect(client.validateAgentAssembly).toHaveBeenLastCalledWith(
      expect.objectContaining({
        capabilityPackIds: expect.arrayContaining(["catalog-capability:016"]),
        sourceInstanceId: "agent-instance:piwork-researcher",
      }),
    ));
  });

  it("shows_loaded_business_packs_in_member_details", async () => {
    const user = userEvent.setup();
    const { client, instances, packs } = await configureAgentCenterClient();
    const researcher = instances.find(({ definition }) => definition.roleKind === "researcher")!;
    const businessPack = {
      ...packs.find(({ id }) => id === "catalog-capability:001")!,
      name: "已装载业务能力",
      status: "executable" as const,
    };
    researcher.definition.capabilityPacks.push(businessPack);
    renderAgentCenter(client);

    await user.click(await screen.findByRole("button", { name: "查看研究员详情" }));

    expect(within(screen.getByRole("dialog", { name: "研究员" }))
      .getByText("已装载业务能力")).toBeInTheDocument();
  });

  it("adds_a_member_to_the_selected_work", async () => {
    const user = userEvent.setup();
    const { client, instances } = await configureAgentCenterClient();
    const lead = instances.find(({ definition }) => definition.roleKind === "lead")!;
    const researcher = instances.find(({ definition }) => definition.roleKind === "researcher")!;
    client.getWorkTeam
      .mockResolvedValueOnce(workTeam("work-1", lead, []))
      .mockResolvedValueOnce(workTeam("work-1", lead, [researcher]));
    client.addWorkMember.mockResolvedValue(workTeam("work-1", lead, [researcher]));
    renderAgentCenter(client, "work-1");

    const opener = await screen.findByRole("button", { name: "查看研究员详情" });
    await user.click(opener);
    await user.click(
      within(screen.getByRole("dialog", { name: "研究员" }))
        .getByRole("button", { name: "加入当前 Work" }),
    );

    await waitFor(() => expect(client.addWorkMember).toHaveBeenCalledWith(
      "work-1",
      researcher.id,
    ));
    await waitFor(() => expect(client.getWorkTeam).toHaveBeenCalledTimes(2));
    expect(within(opener).getByText("已加入当前 Work")).toBeInTheDocument();
  });

  it("ignores_stale_member_refresh_when_selected_work_changes", async () => {
    const user = userEvent.setup();
    const { client, instances } = await configureAgentCenterClient();
    const lead = instances.find(({ definition }) => definition.roleKind === "lead")!;
    const researcher = instances.find(({ definition }) => definition.roleKind === "researcher")!;
    let resolveAdd!: (team: WorkTeamSummary) => void;
    client.getWorkTeam.mockImplementation(async (workId) => workTeam(workId, lead, []));
    client.addWorkMember.mockReturnValue(new Promise((resolve) => {
      resolveAdd = resolve;
    }));
    const view = renderAgentCenter(client, "work-1");

    await user.click(await screen.findByRole("button", { name: "查看研究员详情" }));
    await user.click(within(screen.getByRole("dialog", { name: "研究员" }))
      .getByRole("button", { name: "加入当前 Work" }));
    await waitFor(() => expect(client.addWorkMember).toHaveBeenCalledWith("work-1", researcher.id));

    view.rerender(
      <AgentCenterPage
        client={client}
        currentWorkId="work-2"
        onStartCatalogCapability={vi.fn()}
      />,
    );
    await waitFor(() => expect(client.getWorkTeam).toHaveBeenCalledWith("work-2"));
    await act(async () => resolveAdd(workTeam("work-1", lead, [researcher])));

    await waitFor(() => expect(client.getWorkTeam).toHaveBeenCalledTimes(2));
    expect(client.getWorkTeam.mock.calls).toEqual([["work-1"], ["work-2"]]);
  });

  it("clears_the_previous_work_team_and_member_drawer_when_a_new_work_fails_to_load", async () => {
    const user = userEvent.setup();
    const { client, instances } = await configureAgentCenterClient();
    const lead = instances.find(({ definition }) => definition.roleKind === "lead")!;
    const researcher = instances.find(({ definition }) => definition.roleKind === "researcher")!;
    client.getWorkTeam
      .mockResolvedValueOnce(workTeam("work-1", lead, [researcher]))
      .mockRejectedValueOnce({ code: "database_error", message: "work-2 unavailable" });
    const view = renderAgentCenter(client, "work-1");

    await user.click(await screen.findByRole("button", { name: "查看研究员详情" }));
    expect(within(screen.getByRole("dialog", { name: "研究员" }))
      .getByRole("button", { name: "已加入当前 Work" })).toBeDisabled();

    view.rerender(
      <AgentCenterPage
        client={client}
        currentWorkId="work-2"
        onStartCatalogCapability={vi.fn()}
      />,
    );
    expect((await screen.findAllByText("无法加载团队和能力库")).length).toBeGreaterThan(0);

    expect(screen.queryByRole("dialog", { name: "研究员" })).not.toBeInTheDocument();
    expect(screen.queryByText("已加入当前 Work")).not.toBeInTheDocument();
  });

  it("ignores_an_add_result_after_the_drawer_switches_to_a_saved_copy", async () => {
    const user = userEvent.setup();
    const { client, instances } = await configureAgentCenterClient();
    const lead = instances.find(({ definition }) => definition.roleKind === "lead")!;
    const researcher = instances.find(({ definition }) => definition.roleKind === "researcher")!;
    const savedCopy: AgentInstanceSummary = {
      ...researcher,
      id: "agent-instance:local:saved-researcher",
      displayName: "本地研究副本",
      builtin: false,
      definition: {
        ...researcher.definition,
        id: "agent-definition:local:saved-researcher:v1",
        builtin: false,
      },
    };
    let rejectAdd!: (error: unknown) => void;
    client.getWorkTeam.mockResolvedValue(workTeam("work-1", lead, []));
    client.addWorkMember.mockReturnValue(new Promise((_, reject) => {
      rejectAdd = reject;
    }));
    client.saveAgentCopy.mockResolvedValue(savedCopy);
    renderAgentCenter(client, "work-1");

    await user.click(await screen.findByRole("button", { name: "查看研究员详情" }));
    let dialog = screen.getByRole("dialog", { name: "研究员" });
    await user.click(within(dialog).getByRole("button", { name: "加入当前 Work" }));
    await waitFor(() => expect(client.addWorkMember).toHaveBeenCalledWith("work-1", researcher.id));
    await user.click(within(dialog).getByRole("button", { name: "自定义副本" }));
    const save = within(dialog).getByRole("button", { name: "保存长期成员" });
    await waitFor(() => expect(save).toBeEnabled());
    await user.click(save);
    dialog = await screen.findByRole("dialog", { name: "本地研究副本" });

    await act(async () => rejectAdd({ code: "database_error", message: "late failure" }));

    expect(within(dialog).queryByRole("alert")).not.toBeInTheDocument();
    expect(within(dialog).queryByRole("status")).not.toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "加入当前 Work" })).toBeEnabled();
  });

  it("localizes_structured_add_member_errors", async () => {
    const user = userEvent.setup();
    const { client, instances } = await configureAgentCenterClient();
    const lead = instances.find(({ definition }) => definition.roleKind === "lead")!;
    client.getWorkTeam.mockResolvedValue(workTeam("work-1", lead, []));
    client.addWorkMember.mockRejectedValue({
      code: "database_error",
      message: "sqlite busy",
    });
    renderAgentCenter(client, "work-1");

    await user.click(await screen.findByRole("button", { name: "查看研究员详情" }));
    const dialog = screen.getByRole("dialog", { name: "研究员" });
    await user.click(within(dialog).getByRole("button", { name: "加入当前 Work" }));

    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "本地数据库暂时不可用，请重试。",
    );
    expect(within(dialog).getByRole("alert")).not.toHaveTextContent("[object Object]");
  });

  it("keeps_a_successful_add_when_the_followup_team_refresh_fails", async () => {
    const user = userEvent.setup();
    const { client, instances } = await configureAgentCenterClient();
    const lead = instances.find(({ definition }) => definition.roleKind === "lead")!;
    const researcher = instances.find(({ definition }) => definition.roleKind === "researcher")!;
    client.getWorkTeam
      .mockResolvedValueOnce(workTeam("work-1", lead, []))
      .mockRejectedValueOnce({ code: "database_error", message: "refresh failed" });
    client.addWorkMember.mockResolvedValue(workTeam("work-1", lead, [researcher]));
    renderAgentCenter(client, "work-1");

    const opener = await screen.findByRole("button", { name: "查看研究员详情" });
    await user.click(opener);
    const dialog = screen.getByRole("dialog", { name: "研究员" });
    await user.click(within(dialog).getByRole("button", { name: "加入当前 Work" }));

    expect(await within(dialog).findByRole("status")).toHaveTextContent(
      "成员已加入，但团队刷新失败。当前成员状态已保留。",
    );
    expect(within(dialog).queryByRole("alert")).not.toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "已加入当前 Work" })).toBeDisabled();
    expect(within(opener).getByText("已加入当前 Work")).toBeInTheDocument();
    expect(client.addWorkMember).toHaveBeenCalledOnce();
    expect(client.getWorkTeam).toHaveBeenCalledTimes(2);
  });

  it("keeps_capability_drawer_focus_trapped_across_parent_rerenders", async () => {
    const user = userEvent.setup();
    const { client } = await configureAgentCenterClient();
    const onStartCatalogCapability = vi.fn();
    const view = renderAgentCenter(client, undefined, onStartCatalogCapability);

    await user.click(await screen.findByRole("tab", { name: "能力库" }));
    const opener = await screen.findByRole("button", { name: "查看需求澄清智能体详情" });
    await user.click(opener);
    const dialog = screen.getByRole("dialog", { name: "需求澄清智能体" });
    const close = within(dialog).getByRole("button", { name: "关闭详情" });
    const last = within(dialog).getByRole("button", { name: "创建任务草稿" });
    expect(close).toHaveFocus();

    view.rerender(
      <AgentCenterPage
        client={client}
        onStartCatalogCapability={onStartCatalogCapability}
      />,
    );
    await act(async () => Promise.resolve());
    expect(close).toHaveFocus();
    expect(opener).not.toHaveFocus();

    await user.tab({ shift: true });
    expect(last).toHaveFocus();
    await user.tab();
    expect(close).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "需求澄清智能体" })).not.toBeInTheDocument();
    await waitFor(() => expect(opener).toHaveFocus());
  });

  it("preserves_tabs_drawer_focus_and_error_states", async () => {
    const user = userEvent.setup();
    const { client } = await configureAgentCenterClient();
    const view = renderAgentCenter(client);

    const teamTab = await screen.findByRole("tab", { name: "我的团队" });
    const libraryTab = screen.getByRole("tab", { name: "能力库" });
    expect(document.getElementById("agent-center-panel-team")).toBeInTheDocument();
    expect(document.getElementById("agent-center-panel-library")).toBeInTheDocument();
    teamTab.focus();
    await user.keyboard("{ArrowRight}");
    expect(libraryTab).toHaveFocus();
    expect(libraryTab).toHaveAttribute("aria-selected", "true");
    await user.keyboard("{ArrowLeft}");
    expect(teamTab).toHaveFocus();
    expect(teamTab).toHaveAttribute("aria-selected", "true");

    const opener = screen.getByRole("button", { name: "查看研究员详情" });
    await user.click(opener);
    expect(screen.getByRole("dialog", { name: "研究员" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "关闭成员详情" })).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "研究员" })).not.toBeInTheDocument();
    expect(opener).toHaveFocus();

    view.unmount();
    const failingClient = createMockTauriClient();
    failingClient.listAgentInstances.mockRejectedValue({
      code: "database_error",
      message: "agent inventory unavailable",
    });
    renderAgentCenter(failingClient);
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("无法加载团队和能力库");
    expect(alert).toHaveTextContent("本地数据库暂时不可用，请重试。");
    expect(alert).not.toHaveTextContent("[object Object]");
  });

  it("filters the complete catalog by text, domain and priority", async () => {
    const user = userEvent.setup();
    const { client } = await configureAgentCenterClient();
    renderAgentCenter(client);

    await user.click(await screen.findByRole("tab", { name: "能力库" }));
    expect(screen.getByRole("group", { name: "能力库浏览方式" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "全部能力" }));
    expect(screen.getByRole("group", { name: "能力筛选" })).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /查看.*详情/u })).toHaveLength(96);

    await user.type(screen.getByRole("searchbox", { name: "搜索能力" }), "报价");
    await user.selectOptions(screen.getByLabelText("能力域"), "quote-finance");
    await user.selectOptions(screen.getByLabelText("优先级"), "P0");

    expect(screen.getByText("找到 3 项能力")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "查看参考报价智能体详情" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "查看报价解释与谈判辅助智能体详情" })).not.toBeInTheDocument();
  });

  it("clears an empty catalog search state", async () => {
    const user = userEvent.setup();
    const { client } = await configureAgentCenterClient();
    renderAgentCenter(client);

    await user.click(await screen.findByRole("tab", { name: "能力库" }));
    await user.click(screen.getByRole("button", { name: "全部能力" }));
    await user.type(screen.getByRole("searchbox", { name: "搜索能力" }), "不存在的能力");
    expect(screen.getByRole("heading", { name: "没有匹配的能力" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "清除筛选" }));
    expect(screen.getAllByRole("button", { name: /查看.*详情/u })).toHaveLength(96);
  });
});
