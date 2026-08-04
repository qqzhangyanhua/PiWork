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
});
