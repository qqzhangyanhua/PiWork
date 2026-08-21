import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";

import type { AgentInstanceSummary, WorkAgentSummary, WorkTeamSummary } from "../../bindings";
import { i18n } from "../../i18n";
import { createMockTauriClient } from "../../test/mockTauriClient";
import { AgentCenterPage } from "./AgentCenterPage";

const workAgent = (workId: string, instance: AgentInstanceSummary): WorkAgentSummary => ({
  workId,
  instance,
  roleKind: instance.definition.roleKind,
  status: "joined",
  permissionPolicy: instance.permissionPolicyOverride ?? instance.definition.defaultPermissionPolicy,
  joinedAt: "2026-08-15T00:00:00.000Z",
  updatedAt: "2026-08-15T00:00:00.000Z",
});

const workTeam = (
  workId: string,
  leadInstance: AgentInstanceSummary,
  memberInstances: AgentInstanceSummary[],
): WorkTeamSummary => {
  const lead = workAgent(workId, leadInstance);
  return { workId, lead, members: [lead, ...memberInstances.map((member) => workAgent(workId, member))] };
};

describe("AgentCenterPage", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("zh-CN");
  });

  it("shows only the persistent team and executable tool permissions", async () => {
    const client = createMockTauriClient();
    render(<AgentCenterPage client={client} />);

    expect(await screen.findByRole("heading", { name: "我的团队", level: 1 })).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /查看.*详情/u })).toHaveLength(4);
    expect(screen.queryByText("能力库")).not.toBeInTheDocument();
    expect(screen.queryByText(/MAGIC FACTORY/u)).not.toBeInTheDocument();
    expect(await client.listCapabilityPacks()).toHaveLength(4);
  });

  it("opens a member drawer and restores focus when it closes", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    render(<AgentCenterPage client={client} />);

    const opener = await screen.findByRole("button", { name: "查看researcher详情" });
    await user.click(opener);
    const dialog = screen.getByRole("dialog", { name: "researcher" });
    expect(within(dialog).getByRole("heading", { level: 3, name: "工具权限" })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "researcher" })).not.toBeInTheDocument();
    await waitFor(() => expect(opener).toHaveFocus());
  });

  it("adds a member to the selected Work", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const instances = await client.listAgentInstances();
    const lead = instances.find(({ definition }) => definition.roleKind === "lead")!;
    const researcher = instances.find(({ definition }) => definition.roleKind === "researcher")!;
    client.getWorkTeam
      .mockResolvedValueOnce(workTeam("work-1", lead, []))
      .mockResolvedValueOnce(workTeam("work-1", lead, [researcher]));
    client.addWorkMember.mockResolvedValue(workTeam("work-1", lead, [researcher]));
    render(<AgentCenterPage client={client} currentWorkId="work-1" />);

    const opener = await screen.findByRole("button", { name: "查看researcher详情" });
    await user.click(opener);
    await user.click(within(screen.getByRole("dialog", { name: "researcher" }))
      .getByRole("button", { name: "加入当前 Work" }));

    await waitFor(() => expect(client.addWorkMember).toHaveBeenCalledWith("work-1", researcher.id));
    expect(await within(opener).findByText("已加入当前 Work")).toBeInTheDocument();
  });

  it("localizes structured loading failures", async () => {
    const client = createMockTauriClient();
    client.listAgentInstances.mockRejectedValue({ code: "database_error", message: "sqlite busy" });
    render(<AgentCenterPage client={client} />);

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("无法加载团队");
    expect(alert).toHaveTextContent("本地数据库暂时不可用，请重试。");
  });
});
