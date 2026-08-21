import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { AgentInstanceSummary, AssemblyDiagnostic } from "../../bindings";
import { i18n } from "../../i18n";
import { createMockTauriClient } from "../../test/mockTauriClient";
import { AgentCenterPage } from "./AgentCenterPage";
import { MemberAssembler } from "./MemberAssembler";

const DIAGNOSTICS: AssemblyDiagnostic[] = [
  { code: "not_executable", capabilityPackId: "capability-pack:unavailable", message: "工具权限不可执行" },
  { code: "incompatible_role", capabilityPackId: "capability-pack:other", message: "能力包与角色不兼容" },
  { code: "missing_tool", capabilityPackId: "capability-pack:tool", message: "缺少必要工具" },
  { code: "missing_engine_capability", capabilityPackId: "capability-pack:engine", message: "Engine 能力不足" },
  { code: "permission_escalation", capabilityPackId: null, message: "请求扩大权限" },
  { code: "capability_conflict", capabilityPackId: "capability-pack:conflict", message: "能力包互相冲突" },
  { code: "context_budget_exceeded", capabilityPackId: null, message: "上下文预算超限" },
];

describe("MemberAssembler", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("zh-CN");
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("customizes_a_builtin_member_as_a_copy", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    render(
      <AgentCenterPage
        client={client}
      />,
    );

    await user.click(await screen.findByRole("button", { name: "查看researcher详情" }));
    const dialog = screen.getByRole("dialog", { name: "researcher" });
    const nameInput = within(dialog).getByRole("textbox", { name: "成员名称" });
    expect(nameInput).toBeDisabled();
    expect(within(dialog).getByText("内置成员为只读；自定义会创建本地副本。")).toBeInTheDocument();

    await user.click(within(dialog).getByRole("button", { name: "自定义副本" }));
    expect(nameInput).toBeEnabled();
    await user.clear(nameInput);
    await user.type(nameInput, "本地研究员");

    await waitFor(() => expect(client.validateAgentAssembly).toHaveBeenLastCalledWith(
      expect.objectContaining({
        sourceInstanceId: "agent-instance:piwork-researcher",
        displayName: "本地研究员",
        capabilityPackIds: ["capability-pack:source-research:v1"],
      }),
    ));
    const save = within(dialog).getByRole("button", { name: "保存长期成员" });
    await waitFor(() => expect(save).toBeEnabled());
    await user.click(save);

    expect(client.saveAgentCopy).toHaveBeenCalledWith(expect.objectContaining({
      sourceInstanceId: "agent-instance:piwork-researcher",
      displayName: "本地研究员",
      capabilityPackIds: ["capability-pack:source-research:v1"],
    }));
  });

  it("locks_all_mutable_controls_while_a_save_is_pending", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const source = {
      ...(await client.listAgentInstances()).find(
        ({ definition }) => definition.roleKind === "researcher",
      )!,
      builtin: false,
    };
    const capabilityPacks = await client.listCapabilityPacks();
    let resolveSave!: (member: AgentInstanceSummary) => void;
    client.saveAgentCopy.mockReturnValue(new Promise((resolve) => {
      resolveSave = resolve;
    }));
    render(
      <MemberAssembler
        capabilityPacks={capabilityPacks}
        client={client}
        onSaved={vi.fn()}
        source={source}
      />,
    );
    const save = screen.getByRole("button", { name: "保存长期成员" });
    await waitFor(() => expect(save).toBeEnabled());

    await user.click(save);

    expect(screen.getByRole("textbox", { name: "成员名称" })).toBeDisabled();
    expect(screen.getByRole("textbox", { name: "Engine 覆盖" })).toBeDisabled();
    expect(screen.getByRole("textbox", { name: "Model 覆盖" })).toBeDisabled();
    expect(screen.getByRole("combobox", { name: "权限策略" })).toBeDisabled();
    expect(screen.getByRole("spinbutton", { name: "并行上限" })).toBeDisabled();

    await act(async () => resolveSave(source));
  });

  it("blocks_save_for_every_server_diagnostic", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.validateAgentAssembly.mockResolvedValue(DIAGNOSTICS);
    render(
      <AgentCenterPage
        client={client}
      />,
    );

    await user.click(await screen.findByRole("button", { name: "查看engineer详情" }));
    const dialog = screen.getByRole("dialog", { name: "engineer" });
    await user.click(within(dialog).getByRole("button", { name: "自定义副本" }));

    for (const diagnostic of DIAGNOSTICS) {
      expect(await within(dialog).findByText(diagnostic.code)).toBeInTheDocument();
      expect(within(dialog).getByText(diagnostic.message)).toBeInTheDocument();
    }
    expect(within(dialog).getAllByText("修复建议")).toHaveLength(DIAGNOSTICS.length);
    expect(within(dialog).getByRole("button", { name: "保存长期成员" })).toBeDisabled();
  });

  it("associates_live_validation_feedback_with_assembly_fields", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.validateAgentAssembly.mockRejectedValue({
      code: "database_error",
      message: "sqlite busy",
    });
    render(
      <AgentCenterPage
        client={client}
      />,
    );

    await user.click(await screen.findByRole("button", { name: "查看engineer详情" }));
    const dialog = screen.getByRole("dialog", { name: "engineer" });
    await user.click(within(dialog).getByRole("button", { name: "自定义副本" }));
    const validation = within(dialog).getByLabelText("装配校验");
    const displayName = within(dialog).getByRole("textbox", { name: "成员名称" });
    const save = within(dialog).getByRole("button", { name: "保存长期成员" });

    expect(validation).toHaveAttribute("aria-live", "polite");
    expect(validation).toHaveAttribute("id");
    expect(displayName).toHaveAttribute("aria-describedby", validation.id);
    expect(save).toHaveAttribute("aria-describedby", validation.id);
    expect(await within(validation).findByRole("alert")).toHaveTextContent(
      "本地数据库暂时不可用，请重试。",
    );
    expect(within(validation).getByRole("alert")).not.toHaveTextContent("[object Object]");

    await user.clear(displayName);
    expect(await within(validation).findByText("请输入成员名称后再保存。")).toBeInTheDocument();
    expect(save).toBeDisabled();
  });

  it("localizes_structured_save_errors", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.saveAgentCopy.mockRejectedValue({
      code: "database_error",
      message: "sqlite busy",
    });
    render(
      <AgentCenterPage
        client={client}
      />,
    );

    await user.click(await screen.findByRole("button", { name: "查看engineer详情" }));
    const dialog = screen.getByRole("dialog", { name: "engineer" });
    await user.click(within(dialog).getByRole("button", { name: "自定义副本" }));
    const save = within(dialog).getByRole("button", { name: "保存长期成员" });
    await waitFor(() => expect(save).toBeEnabled());
    await user.click(save);

    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "本地数据库暂时不可用，请重试。",
    );
    expect(within(dialog).getByRole("alert")).not.toHaveTextContent("[object Object]");
  });

  it("ignores_a_save_that_resolves_after_opening_another_member", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const researcher = (await client.listAgentInstances()).find(
      ({ definition }) => definition.roleKind === "researcher",
    )!;
    const lateSavedMember: AgentInstanceSummary = {
      ...researcher,
      id: "agent-instance:local:late-researcher",
      displayName: "迟到的研究员副本",
      builtin: false,
      definition: {
        ...researcher.definition,
        id: "agent-definition:local:late-researcher:v1",
        builtin: false,
      },
    };
    let resolveSave!: (member: AgentInstanceSummary) => void;
    client.saveAgentCopy.mockReturnValue(new Promise((resolve) => {
      resolveSave = resolve;
    }));
    render(
      <AgentCenterPage
        client={client}
      />,
    );

    await user.click(await screen.findByRole("button", { name: "查看researcher详情" }));
    let dialog = screen.getByRole("dialog", { name: "researcher" });
    await user.click(within(dialog).getByRole("button", { name: "自定义副本" }));
    const save = within(dialog).getByRole("button", { name: "保存长期成员" });
    await waitFor(() => expect(save).toBeEnabled());
    await user.click(save);
    await waitFor(() => expect(client.saveAgentCopy).toHaveBeenCalledOnce());

    await user.click(within(dialog).getByRole("button", { name: "关闭成员详情" }));
    await user.click(screen.getByRole("button", { name: "查看engineer详情" }));
    dialog = screen.getByRole("dialog", { name: "engineer" });
    expect(dialog).toBeInTheDocument();

    await act(async () => resolveSave(lateSavedMember));

    expect(screen.getByRole("dialog", { name: "engineer" })).toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: "迟到的研究员副本" })).not.toBeInTheDocument();
  });

  it("clears_pending_validation_when_the_member_name_becomes_empty", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.validateAgentAssembly.mockReturnValue(new Promise(() => undefined));
    render(
      <AgentCenterPage
        client={client}
      />,
    );

    await user.click(await screen.findByRole("button", { name: "查看engineer详情" }));
    const dialog = screen.getByRole("dialog", { name: "engineer" });
    await user.click(within(dialog).getByRole("button", { name: "自定义副本" }));
    const validation = within(dialog).getByLabelText("装配校验");
    const displayName = within(dialog).getByRole("textbox", { name: "成员名称" });
    await within(validation).findByText("正在校验装配…");

    await user.clear(displayName);

    expect(await within(validation).findByText("请输入成员名称后再保存。")).toBeInTheDocument();
    expect(validation).toHaveAttribute("aria-busy", "false");
  });

  it("debounces_validation_and_ignores_stale_or_cleared_results", async () => {
    const client = createMockTauriClient();
    const source = {
      ...(await client.listAgentInstances()).find(
        ({ definition }) => definition.roleKind === "engineer",
      )!,
      builtin: false,
      displayName: "",
    };
    const capabilityPacks = await client.listCapabilityPacks();
    let resolveFirst!: (diagnostics: AssemblyDiagnostic[]) => void;
    let resolveSecond!: (diagnostics: AssemblyDiagnostic[]) => void;
    const first = new Promise<AssemblyDiagnostic[]>((resolve) => {
      resolveFirst = resolve;
    });
    const second = new Promise<AssemblyDiagnostic[]>((resolve) => {
      resolveSecond = resolve;
    });
    client.validateAgentAssembly.mockReset();
    client.validateAgentAssembly
      .mockReturnValueOnce(first)
      .mockReturnValueOnce(second)
      .mockReturnValueOnce(new Promise(() => undefined));
    vi.useFakeTimers();
    render(
      <MemberAssembler
        capabilityPacks={capabilityPacks}
        client={client}
        onSaved={vi.fn()}
        source={source}
      />,
    );

    const validation = screen.getByLabelText("装配校验");
    const displayName = screen.getByRole("textbox", { name: "成员名称" });
    for (const value of ["A", "Al", "Alp", "Alph", "Alpha"]) {
      fireEvent.change(displayName, { target: { value } });
    }
    await act(async () => vi.advanceTimersByTime(199));
    expect(client.validateAgentAssembly).not.toHaveBeenCalled();
    await act(async () => vi.advanceTimersByTime(1));
    expect(client.validateAgentAssembly).toHaveBeenCalledOnce();
    expect(client.validateAgentAssembly).toHaveBeenLastCalledWith(
      expect.objectContaining({ displayName: "Alpha" }),
    );
    expect(validation).toHaveAttribute("aria-busy", "true");

    fireEvent.change(displayName, { target: { value: "" } });
    fireEvent.change(displayName, { target: { value: "Beta" } });
    await act(async () => vi.advanceTimersByTime(200));
    expect(client.validateAgentAssembly).toHaveBeenCalledTimes(2);
    expect(client.validateAgentAssembly).toHaveBeenLastCalledWith(
      expect.objectContaining({ displayName: "Beta" }),
    );

    await act(async () => resolveSecond([
      { code: "missing_tool", capabilityPackId: null, message: "最新校验" },
    ]));
    expect(within(validation).getByText("最新校验")).toBeInTheDocument();
    expect(validation).toHaveAttribute("aria-busy", "false");
    await act(async () => resolveFirst([
      { code: "capability_conflict", capabilityPackId: null, message: "过期校验" },
    ]));
    expect(within(validation).queryByText("过期校验")).not.toBeInTheDocument();
    expect(within(validation).getByText("最新校验")).toBeInTheDocument();

    fireEvent.change(displayName, { target: { value: "Beta!" } });
    await act(async () => vi.advanceTimersByTime(200));
    expect(client.validateAgentAssembly).toHaveBeenCalledTimes(3);
    expect(validation).toHaveAttribute("aria-busy", "true");
    fireEvent.change(displayName, { target: { value: "" } });
    expect(validation).toHaveAttribute("aria-busy", "false");
    expect(within(validation).getByText("请输入成员名称后再保存。")).toBeInTheDocument();
    await act(async () => vi.advanceTimersByTime(200));
    expect(client.validateAgentAssembly).toHaveBeenCalledTimes(3);
  });

  it("keeps fractional parallelism invalid instead of truncating it", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    render(
      <AgentCenterPage
        client={client}
      />,
    );

    await user.click(await screen.findByRole("button", { name: "查看engineer详情" }));
    const dialog = screen.getByRole("dialog", { name: "engineer" });
    await user.click(within(dialog).getByRole("button", { name: "自定义副本" }));
    const parallelism = within(dialog).getByRole("spinbutton", { name: "并行上限" });
    await user.type(parallelism, "1.5");

    await waitFor(() => expect(client.validateAgentAssembly).toHaveBeenLastCalledWith(
      expect.objectContaining({ parallelismOverride: 1.5 }),
    ));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "无法完成装配校验: 操作失败，请重试。",
    );
    expect(within(dialog).getByRole("button", { name: "保存长期成员" })).toBeDisabled();
  });
});
