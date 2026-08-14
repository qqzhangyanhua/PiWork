import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { i18n } from "../i18n";
import { createMockTauriClient } from "../test/mockTauriClient";
import { App } from "./App";

beforeEach(async () => {
  await i18n.changeLanguage("en");
});

describe("App", () => {
  it("provides the complete agent command contract in the desktop mock", async () => {
    const client = createMockTauriClient();

    const instances = await client.listAgentInstances();
    const packs = await client.listCapabilityPacks();
    const team = await client.getWorkTeam("work-1");

    expect(instances).toHaveLength(4);
    expect(packs.filter(({ status }) => status === "catalog_only")).toHaveLength(96);
    expect(team.workId).toBe("work-1");
    expect(team.lead.roleKind).toBe("lead");
    expect(client.validateAgentAssembly).toBeTypeOf("function");
    expect(client.saveAgentCopy).toBeTypeOf("function");
    expect(client.addWorkMember).toBeTypeOf("function");
  });

  it("explains that the Vite URL cannot access the desktop backend", () => {
    render(<App />);

    expect(screen.getByRole("alert").closest("main")).toHaveAttribute(
      "data-motion-state",
      "error",
    );
    expect(
      screen.getByRole("heading", { name: "Open the PiWork desktop app" }),
    ).toBeInTheDocument();
    expect(screen.getByText(/pnpm tauri dev/)).toBeInTheDocument();
  });

  it("requires a verified model configuration before bootstrapping Works", async () => {
    const client = createMockTauriClient();
    client.getModelConfigurationStatus.mockResolvedValue({
      configured: false,
      configuration: null,
    });

    render(<App client={client} />);

    expect(screen.getByRole("status", { name: "Loading model configuration" })).toHaveAttribute(
      "data-motion-state",
      "loading",
    );
    expect(
      await screen.findByRole("heading", { name: "Connect your model" }),
    ).toBeInTheDocument();
    expect(client.getModelConfigurationStatus).toHaveBeenCalledTimes(1);
    expect(client.listWorks).not.toHaveBeenCalled();
    expect(client.listenToWorkEvents).not.toHaveBeenCalled();
  });

  it("shows a retryable product error when model configuration cannot be loaded", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.getModelConfigurationStatus
      .mockRejectedValueOnce(new Error("raw credential failure"))
      .mockResolvedValueOnce({ configured: false, configuration: null });

    render(<App client={client} />);

    const alert = await screen.findByRole("alert");
    expect(alert.closest("main")).toHaveAttribute("data-motion-state", "error");
    expect(alert).toHaveTextContent("Unable to load model configuration");
    expect(alert).not.toHaveTextContent("raw credential failure");
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(
      await screen.findByRole("heading", { name: "Connect your model" }),
    ).toBeInTheDocument();
    expect(client.getModelConfigurationStatus).toHaveBeenCalledTimes(2);
  });

  it("unlocks Works only after connection verification and saving a default model", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.getModelConfigurationStatus.mockResolvedValue({
      configured: false,
      configuration: null,
    });
    client.testModelConnection.mockResolvedValue({
      models: [{ id: "gpt-5.2", label: "GPT-5.2" }],
    });
    client.saveModelConfiguration.mockResolvedValue({
      id: "openai-default",
      provider: "openai",
      baseUrl: "https://api.openai.com/v1",
      modelId: "gpt-5.2",
      active: true,
      credentialConfigured: true,
    });

    render(<App client={client} />);

    await user.type(await screen.findByLabelText("API key"), "sk-test-secret");
    await user.click(screen.getByRole("button", { name: "Test connection" }));
    await user.selectOptions(
      await screen.findByLabelText("Default model"),
      "gpt-5.2",
    );
    await user.click(screen.getByRole("button", { name: "Save and continue" }));

    expect(client.testModelConnection).toHaveBeenCalledWith({
      provider: "openai",
      apiKey: "sk-test-secret",
      baseUrl: "https://api.openai.com/v1",
    });
    expect(client.saveModelConfiguration).toHaveBeenCalledWith({
      provider: "openai",
      apiKey: "sk-test-secret",
      baseUrl: "https://api.openai.com/v1",
      modelId: "gpt-5.2",
    });
    expect(
      await screen.findAllByRole("button", { name: "New conversation" }),
    ).toHaveLength(1);
    expect(client.listWorks).toHaveBeenCalledTimes(1);
  });

  it("distinguishes a verified connection with no available models from a connection failure", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.getModelConfigurationStatus.mockResolvedValue({ configured: false, configuration: null });
    client.testModelConnection.mockResolvedValue({ models: [] });
    render(<App client={client} />);

    expect(await screen.findByText(/Windows Credential Manager/u)).toBeInTheDocument();
    await user.type(screen.getByLabelText("API key"), "sk-test-secret");
    await user.click(screen.getByRole("button", { name: "Test connection" }));

    expect(await screen.findByRole("status")).toHaveTextContent(
      "Connection succeeded, but this provider returned no models.",
    );
    expect(screen.queryByText(/could not be verified/u)).not.toBeInTheDocument();
  });

  it("renders the PiWork product shell and bootstraps Work state", async () => {
    const client = createMockTauriClient();
    render(<App client={client} />);
    expect(screen.getByText("PiWork")).toBeInTheDocument();
    const newConversation = await screen.findByRole("button", { name: "New conversation" });
    expect(newConversation).toBeInTheDocument();
    expect(newConversation).toHaveTextContent("Ctrl N");
    expect(newConversation).toHaveAttribute("aria-current", "page");
    expect(screen.queryByRole("button", { name: "Home" })).not.toBeInTheDocument();
    expect(screen.getByTestId("orb-logo")).toBeInTheDocument();
    await waitFor(() => expect(client.listWorks).toHaveBeenCalledTimes(1));
    expect(client.listenToWorkEvents).toHaveBeenCalledTimes(1);
  });

  it("renders the localized product shell", async () => {
    const client = createMockTauriClient();
    await i18n.changeLanguage("zh-CN");
    render(<App client={client} />);
    expect(
      await screen.findByRole("button", { name: "新对话" }),
    ).toBeInTheDocument();
    expect(screen.getByTestId("orb-logo")).toBeInTheDocument();
  });

  it("opens the local account menu and keeps model configuration inside the settings page", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.getModelConfigurationStatus.mockResolvedValue({
      configured: true,
      configuration: { id: "openai-default", provider: "openai", baseUrl: "https://api.openai.com/v1", modelId: "gpt-5.2", active: true, credentialConfigured: true },
    });
    client.testModelConnection.mockResolvedValue({
      models: [{ id: "deepseek-chat", label: "DeepSeek Chat" }],
    });
    client.saveModelConfiguration.mockResolvedValue({
      id: "deepseek-default",
      provider: "deepseek",
      baseUrl: "https://api.deepseek.com",
      modelId: "deepseek-chat",
      active: true,
      credentialConfigured: true,
    });

    render(<App client={client} />);

    const account = await screen.findByRole("button", { name: /Local user/ });
    expect(account).toHaveTextContent("This device only");
    await user.click(account);
    const menu = screen.getByRole("menu", { name: "Account" });
    expect(within(menu).queryByText(/Sign out|Usage/u)).not.toBeInTheDocument();
    await user.click(within(menu).getByRole("menuitem", { name: "Settings" }));

    const settings = await screen.findByRole("region", { name: "Settings" });
    expect(within(settings).getByRole("heading", { name: "Settings" })).toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: "Model settings" })).not.toBeInTheDocument();
    await user.click(within(settings).getByRole("button", { name: "Add connection" }));
    await user.click(within(settings).getByRole("combobox", { name: "Provider" }));
    await user.click(within(settings).getByRole("option", { name: /DeepSeek/ }));
    await user.type(within(settings).getByLabelText("API key"), "sk-updated-secret");
    await user.click(within(settings).getByRole("button", { name: "Test connection" }));
    await user.click(within(settings).getByRole("button", { name: "Save model configuration" }));

    expect(client.saveModelConfiguration).toHaveBeenCalledWith({
      provider: "deepseek",
      apiKey: "sk-updated-secret",
      baseUrl: "https://api.deepseek.com",
      modelId: "deepseek-chat",
    });
    expect(await within(settings).findByRole("tab", { name: /DeepSeek deepseek-chat Current connection/ })).toBeInTheDocument();
  });

  it("closes the account menu on Escape and outside interaction, returning focus after Escape", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    render(<App client={client} />);

    const trigger = await screen.findByRole("button", { name: /Local user/ });
    await user.click(trigger);
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu", { name: "Account" })).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();

    await user.click(trigger);
    expect(screen.getByRole("menu", { name: "Account" })).toBeInTheDocument();
    await user.click(screen.getByRole("heading", { name: "What should Pi help you get done today?" }));
    expect(screen.queryByRole("menu", { name: "Account" })).not.toBeInTheDocument();
  });

  it("opens the settings page with Ctrl+,", async () => {
    const client = createMockTauriClient();
    render(<App client={client} />);

    await screen.findByRole("button", { name: /Local user/ });
    fireEvent.keyDown(window, { key: ",", ctrlKey: true });

    expect(await screen.findByRole("region", { name: "Settings" })).toBeInTheDocument();
  });

  it("uses a newly selected active model for new conversations", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const current = { id: "openai-default", provider: "openai" as const, baseUrl: "https://api.openai.com/v1", modelId: "gpt-5.2", active: true, credentialConfigured: true };
    const switched = { ...current, modelId: "gpt-5.3" };
    client.getModelConfigurationStatus.mockResolvedValue({ configured: true, configuration: current });
    client.listModelConfigurations.mockResolvedValue([current]);
    client.testSavedModelConfiguration.mockResolvedValue({
      models: [
        { id: "gpt-5.2", label: "GPT-5.2" },
        { id: "gpt-5.3", label: "GPT-5.3" },
      ],
    });
    client.selectModelForConfiguration.mockResolvedValue(switched);

    render(<App client={client} />);
    const account = await screen.findByRole("button", { name: /Local user/ });
    await user.click(account);
    await user.click(within(screen.getByRole("menu", { name: "Account" })).getByRole("menuitem", { name: "Settings" }));
    const settings = await screen.findByRole("region", { name: "Settings" });
    await user.click(within(settings).getByRole("button", { name: "Test connection" }));
    await user.click(await within(settings).findByRole("combobox", { name: "Model" }));
    await user.click(within(settings).getByRole("option", { name: /GPT-5.3/ }));
    await user.click(within(settings).getByRole("button", { name: "Save model configuration" }));
    await user.click(screen.getByRole("button", { name: "New conversation" }));

    expect(await screen.findByTitle("gpt-5.3")).toHaveTextContent("5.3");
  });
});
