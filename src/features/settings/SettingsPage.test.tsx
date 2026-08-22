import "@testing-library/jest-dom/vitest";

import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { ModelConfigurationSummary } from "../../app/tauriClient";
import { i18n } from "../../i18n";
import { createMockTauriClient } from "../../test/mockTauriClient";
import { SettingsPage } from "./SettingsPage";

const openai: ModelConfigurationSummary = {
  id: "openai-main",
  provider: "openai",
  baseUrl: "https://api.openai.com/v1",
  modelId: "gpt-5.2",
  active: true,
  credentialConfigured: true,
};

const deepseek: ModelConfigurationSummary = {
  id: "deepseek-code",
  provider: "deepseek",
  baseUrl: "https://api.deepseek.com",
  modelId: "deepseek-chat",
  active: false,
  credentialConfigured: true,
};

beforeEach(async () => {
  await i18n.changeLanguage("zh-CN");
});

describe("SettingsPage screenshot layout", () => {
  it("展示设置二级导航、连接标签和单个连接编辑器", async () => {
    const client = createMockTauriClient();
    client.listModelConfigurations.mockResolvedValue([openai, deepseek]);

    render(<SettingsPage client={client} configuration={openai} onModelConfigured={vi.fn()} />);

    expect(await screen.findByRole("navigation", { name: "设置导航" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "模型与本地运行" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "联网搜索" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "工作区记忆" })).toBeEnabled();
    ["通用设置", "快捷键", "本地资源管理", "数据保存", "隐私与安全", "导出与备份", "关于 PiWork", "检查更新"].forEach((label) => {
      expect(screen.queryByRole("button", { name: label })).not.toBeInTheDocument();
    });
    expect(screen.getByRole("heading", { name: "模型与本地运行设置" })).toBeInTheDocument();
    const tabs = screen.getByRole("tablist", { name: "模型连接" });
    expect(within(tabs).getByRole("tab", { name: /OpenAI/ })).toHaveAttribute("aria-selected", "true");
    expect(within(tabs).getByRole("tab", { name: /DeepSeek/ })).toHaveAttribute("aria-selected", "false");
    expect(screen.getAllByRole("combobox", { name: "Provider" })).toHaveLength(1);
  });

  it("切换编辑标签不会隐式激活连接", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.listModelConfigurations.mockResolvedValue([openai, deepseek]);

    render(<SettingsPage client={client} configuration={openai} onModelConfigured={vi.fn()} />);
    const tabs = await screen.findByRole("tablist", { name: "模型连接" });
    await user.click(within(tabs).getByRole("tab", { name: /DeepSeek/ }));

    expect(client.activateModelConfiguration).not.toHaveBeenCalled();
    expect(within(screen.getByRole("combobox", { name: "Provider" })).getByText("DeepSeek")).toBeInTheDocument();
    expect(within(tabs).getByRole("tab", { name: /DeepSeek/ })).toHaveAttribute("aria-selected", "true");
  });

  it("添加连接在同一编辑区域打开空白表单", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.listModelConfigurations.mockResolvedValue([openai]);

    render(<SettingsPage client={client} configuration={openai} onModelConfigured={vi.fn()} />);
    await screen.findByRole("tablist", { name: "模型连接" });
    expect(screen.queryByLabelText("API Key")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "添加连接" }));

    expect(screen.getByLabelText("API Key")).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Provider" })).toBeInTheDocument();
  });

  it("激活连接后更新标签状态并同步应用级模型", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const activated = { ...deepseek, active: true };
    client.listModelConfigurations.mockResolvedValue([openai, deepseek]);
    client.activateModelConfiguration.mockResolvedValue(activated);
    const onModelConfigured = vi.fn();

    render(<SettingsPage client={client} configuration={openai} onModelConfigured={onModelConfigured} />);
    const tabs = await screen.findByRole("tablist", { name: "模型连接" });
    await user.click(within(tabs).getByRole("tab", { name: /DeepSeek/ }));
    await user.click(screen.getByRole("button", { name: "设为当前连接" }));

    expect(onModelConfigured).toHaveBeenCalledWith(activated);
    await waitFor(() => expect(within(tabs).getByRole("tab", { name: /DeepSeek/ })).toHaveTextContent("当前连接"));
  });

  it("读取失败时提供重试并隐藏底层错误", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.listModelConfigurations
      .mockRejectedValueOnce(new Error("raw database path"))
      .mockResolvedValueOnce([openai]);

    render(<SettingsPage client={client} configuration={openai} onModelConfigured={vi.fn()} />);

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("无法读取已保存的模型配置。");
    expect(alert).not.toHaveTextContent("raw database path");
    await user.click(screen.getByRole("button", { name: "重试" }));

    expect(await screen.findByRole("tab", { name: /OpenAI/ })).toBeInTheDocument();
  });

  it("保存共享 Team 连接和工作区 Task 绑定", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.listModelConfigurations.mockResolvedValue([openai]);
    client.getMemorySettings = vi.fn().mockResolvedValue({
      enabled: false,
      hubEndpoint: "",
      endpoint: "",
      authMode: "gatewayBearer",
      authUsername: "",
      allowInsecureHttp: false,
      serviceId: "",
      teamId: "",
      userId: "codo-local-user",
      requestTimeoutMs: 5000,
      recallTimeoutMs: 1500,
      maxRecallItems: 8,
      maxRecallChars: 6000,
      captureEnabled: true,
      recallEnabled: true,
      apiKeyConfigured: false,
      userKeyConfigured: false,
    });
    client.listWorkspaceMemoryBindings = vi.fn().mockResolvedValue([{
      rootPath: "D:\\dev\\agent\\PiWork",
      taskId: "task-piwork",
      enabled: false,
      captureEnabled: true,
      recallEnabled: true,
      pendingCaptureCount: 0,
    }]);
    client.saveMemorySettings = vi.fn(async (input) => ({
      ...input,
      apiKeyConfigured: Boolean(input.apiKey),
      userKeyConfigured: Boolean(input.userKey),
    }));
    client.saveWorkspaceMemoryBinding = vi.fn(async (input) => ({
      ...input,
      taskId: "task-piwork",
      pendingCaptureCount: 0,
    }));

    render(<SettingsPage client={client} configuration={openai} onModelConfigured={vi.fn()} />);
    await user.click(await screen.findByRole("button", { name: "工作区记忆" }));

    expect(await screen.findByRole("heading", { name: "工作区记忆" })).toBeInTheDocument();
    await user.click(screen.getByRole("radio", { name: "反向代理 Basic Auth" }));
    await user.type(screen.getByLabelText("Basic 用户名"), "tdai");
    await user.type(screen.getByLabelText("Memory API 地址"), "https://memory.example.com");
    await user.type(screen.getByLabelText("Service ID"), "service-1");
    await user.type(screen.getByLabelText("反向代理密码"), "proxy-secret");
    await user.type(screen.getByLabelText("Team ID"), "team-codo");
    await user.click(screen.getAllByRole("checkbox", { name: "已关闭" })[0]!);
    await user.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() => expect(client.saveMemorySettings).toHaveBeenCalled());
    expect(client.saveMemorySettings).toHaveBeenCalledWith(expect.objectContaining({
      enabled: true,
      endpoint: "https://memory.example.com",
      authMode: "basic",
      authUsername: "tdai",
      serviceId: "service-1",
      teamId: "team-codo",
      apiKey: "proxy-secret",
    }));
    expect(client.saveWorkspaceMemoryBinding).toHaveBeenCalledWith(expect.objectContaining({
      rootPath: "D:\\dev\\agent\\PiWork",
    }));
    expect(client.saveWorkspaceMemoryBinding).not.toHaveBeenCalledWith(expect.objectContaining({
      teamId: expect.anything(),
    }));
  });

  it("总开关关闭时仍可测试连接，并自动纠正 User Key 记录 ID", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.listModelConfigurations.mockResolvedValue([openai]);
    client.getMemorySettings = vi.fn().mockResolvedValue({
      enabled: false,
      hubEndpoint: "http://memory.example.com",
      endpoint: "http://memory.example.com/mem",
      authMode: "basic",
      authUsername: "tdai",
      allowInsecureHttp: true,
      serviceId: "default",
      teamId: "team-codo",
      userId: "uky-record-id",
      requestTimeoutMs: 5000,
      recallTimeoutMs: 1500,
      maxRecallItems: 8,
      maxRecallChars: 6000,
      captureEnabled: true,
      recallEnabled: true,
      apiKeyConfigured: true,
      userKeyConfigured: true,
    });
    client.listWorkspaceMemoryBindings = vi.fn().mockResolvedValue([]);
    client.saveMemorySettings = vi.fn(async (input) => ({
      ...input,
      apiKeyConfigured: true,
      userKeyConfigured: true,
    }));
    client.saveWorkspaceMemoryBinding = vi.fn();
    client.testMemoryConnection = vi.fn().mockResolvedValue({
      healthy: true,
      authenticated: false,
      latencyMs: 36,
      failureCode: "gatewayBearerRequired",
      resolvedUserId: "usr-verified",
    });

    render(<SettingsPage client={client} configuration={openai} onModelConfigured={vi.fn()} />);
    await user.click(await screen.findByRole("button", { name: "工作区记忆" }));
    await user.click(await screen.findByRole("button", { name: "测试连接" }));

    await waitFor(() => expect(client.testMemoryConnection).toHaveBeenCalledOnce());
    expect(client.saveMemorySettings).toHaveBeenNthCalledWith(1, expect.objectContaining({
      enabled: false,
      userId: "uky-record-id",
    }));
    await waitFor(() => expect(screen.getByLabelText("User ID")).toHaveValue("usr-verified"));
    expect(client.saveMemorySettings).toHaveBeenLastCalledWith(expect.objectContaining({
      enabled: false,
      userId: "usr-verified",
      apiKey: null,
      userKey: null,
    }));
    expect(screen.getByText(/Memory Core 数据接口要求 Gateway Bearer/)).toBeVisible();

    await user.type(screen.getByLabelText("Service ID"), "-next");
    expect(screen.queryByText(/Memory Core 数据接口要求 Gateway Bearer/)).not.toBeInTheDocument();
  });
});
