import "@testing-library/jest-dom/vitest";

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { ModelConfigurationSummary } from "../../app/tauriClient";
import { i18n } from "../../i18n";
import { createMockTauriClient } from "../../test/mockTauriClient";
import { ModelConnectionEditor } from "./ModelConnectionEditor";

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

describe("ModelConnectionEditor saved connection", () => {
  it("使用安全存储中的凭据测试连接并保存模型切换", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const switched = { ...deepseek, modelId: "deepseek-reasoner" };
    client.testSavedModelConfiguration.mockResolvedValue({
      models: [
        { id: "deepseek-chat", label: "DeepSeek Chat" },
        { id: "deepseek-reasoner", label: "DeepSeek Reasoner" },
      ],
    });
    client.selectModelForConfiguration.mockResolvedValue(switched);
    const onSaved = vi.fn();

    render(
      <ModelConnectionEditor
        client={client}
        configuration={deepseek}
        onActivated={vi.fn()}
        onSaved={onSaved}
      />,
    );

    expect(screen.getByText("••••••••••••••••••••••••")).toBeInTheDocument();
    expect(screen.queryByLabelText("API Key")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "测试连接" }));
    expect(client.testSavedModelConfiguration).toHaveBeenCalledWith("deepseek-code");
    expect(await screen.findByText("连接测试成功")).toBeInTheDocument();

    await user.click(screen.getByRole("combobox", { name: "模型" }));
    await user.click(screen.getByRole("option", { name: /DeepSeek Reasoner/ }));
    await user.click(screen.getByRole("button", { name: "保存模型配置" }));

    expect(client.selectModelForConfiguration).toHaveBeenCalledWith({
      configurationId: "deepseek-code",
      modelId: "deepseek-reasoner",
    });
    expect(onSaved).toHaveBeenCalledWith(switched);
  });

  it("只有显式操作才激活当前编辑的连接", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const activated = { ...deepseek, active: true };
    client.activateModelConfiguration.mockResolvedValue(activated);
    const onActivated = vi.fn();

    render(
      <ModelConnectionEditor
        client={client}
        configuration={deepseek}
        onActivated={onActivated}
        onSaved={vi.fn()}
      />,
    );
    await user.click(screen.getByRole("button", { name: "设为当前连接" }));

    expect(client.activateModelConfiguration).toHaveBeenCalledWith("deepseek-code");
    expect(onActivated).toHaveBeenCalledWith(activated);
  });

  it("连接失败时保留编辑状态且不暴露底层错误", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.testSavedModelConfiguration.mockRejectedValue(new Error("raw credential payload"));

    render(
      <ModelConnectionEditor
        client={client}
        configuration={deepseek}
        onActivated={vi.fn()}
        onSaved={vi.fn()}
      />,
    );
    await user.click(screen.getByRole("button", { name: "测试连接" }));

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("无法验证 Provider，请检查配置后重试。");
    expect(alert).not.toHaveTextContent("raw credential payload");
    expect(screen.getByText("deepseek-chat")).toBeInTheDocument();
  });
});

describe("ModelConnectionEditor editable credential", () => {
  it("新增连接通过测试后保存 Provider、凭据和模型", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const saved: ModelConfigurationSummary = {
      ...deepseek,
      id: "deepseek-new",
      modelId: "deepseek-reasoner",
      active: true,
    };
    client.testModelConnection.mockResolvedValue({
      models: [{ id: "deepseek-reasoner", label: "DeepSeek Reasoner" }],
    });
    client.saveModelConfiguration.mockResolvedValue(saved);
    const onSaved = vi.fn();

    render(
      <ModelConnectionEditor
        client={client}
        configuration={null}
        onActivated={vi.fn()}
        onSaved={onSaved}
      />,
    );

    await user.type(screen.getByLabelText("API Key"), "new-secret");
    await user.click(screen.getByRole("button", { name: "测试连接" }));
    await user.click(screen.getByRole("button", { name: "保存模型配置" }));

    expect(client.testModelConnection).toHaveBeenCalledWith({
      provider: "deepseek",
      apiKey: "new-secret",
      baseUrl: "https://api.deepseek.com",
    });
    expect(client.saveModelConfiguration).toHaveBeenCalledWith({
      provider: "deepseek",
      apiKey: "new-secret",
      baseUrl: "https://api.deepseek.com",
      modelId: "deepseek-reasoner",
    });
    expect(onSaved).toHaveBeenCalledWith(saved);
  });

  it("重置已保存凭据后使用原配置 ID 覆盖保存", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const updated = { ...deepseek, modelId: "deepseek-reasoner" };
    client.testModelConnection.mockResolvedValue({
      models: [{ id: "deepseek-reasoner", label: "DeepSeek Reasoner" }],
    });
    client.saveModelConfiguration.mockResolvedValue(updated);

    render(
      <ModelConnectionEditor
        client={client}
        configuration={deepseek}
        onActivated={vi.fn()}
        onSaved={vi.fn()}
      />,
    );

    await user.click(screen.getByRole("button", { name: "重置 API Key" }));
    await user.type(screen.getByLabelText("API Key"), "replacement-key");
    await user.click(screen.getByRole("button", { name: "测试连接" }));
    await user.click(screen.getByRole("button", { name: "保存模型配置" }));

    expect(client.saveModelConfiguration).toHaveBeenCalledWith({
      id: "deepseek-code",
      provider: "deepseek",
      apiKey: "replacement-key",
      baseUrl: "https://api.deepseek.com",
      modelId: "deepseek-reasoner",
    });
  });

  it("更换 Provider 会要求输入新凭据并使用对应 Base URL", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.testModelConnection.mockResolvedValue({ models: [{ id: "gpt-5", label: "GPT-5" }] });

    render(
      <ModelConnectionEditor
        client={client}
        configuration={deepseek}
        onActivated={vi.fn()}
        onSaved={vi.fn()}
      />,
    );

    await user.click(screen.getByRole("combobox", { name: "Provider" }));
    await user.click(screen.getByRole("option", { name: "OpenAI" }));
    await user.type(screen.getByLabelText("API Key"), "openai-key");
    await user.click(screen.getByRole("button", { name: "测试连接" }));

    expect(client.testModelConnection).toHaveBeenCalledWith({
      provider: "openai",
      apiKey: "openai-key",
      baseUrl: "https://api.openai.com/v1",
    });
  });
});
