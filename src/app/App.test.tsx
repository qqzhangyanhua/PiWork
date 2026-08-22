import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SaveAgentAssemblyInput } from "../bindings";
import { i18n } from "../i18n";
import { createMockTauriClient } from "../test/mockTauriClient";
import { App } from "./App";

beforeEach(async () => {
  await i18n.changeLanguage("en");
});

describe("App", () => {
  it("provides the complete agent command contract in the desktop mock", async () => {
    const client = createMockTauriClient();
    const work = await client.createWork({
      title: "Agent contract",
      goal: "Exercise the desktop mock",
      rootPath: "D:/workspace",
      permissionMode: "balanced",
      resourceDraftId: null,
    });

    const instances = await client.listAgentInstances();
    const packs = await client.listCapabilityPacks();
    const team = await client.getWorkTeam(work.summary.id);

    expect(instances).toHaveLength(4);
    expect(packs).toHaveLength(4);
    expect(packs.every(({ status }) => status === "executable")).toBe(true);
    expect(packs.filter(({ status }) => status === "executable")).toMatchObject([
      {
        id: "capability-pack:lead-coordination:v1",
        requiredTools: ["read", "grep", "find", "ls"],
        defaultPermissionScope: "inherit_work",
        compatibleRoleTemplateIds: ["role-template:lead:v1"],
      },
      {
        id: "capability-pack:source-research:v1",
        requiredTools: ["read", "grep", "find", "ls"],
        defaultPermissionScope: "read_only",
        compatibleRoleTemplateIds: ["role-template:researcher:v1"],
      },
      {
        id: "capability-pack:engineering-execution:v1",
        requiredTools: ["read", "grep", "find", "ls", "edit", "write", "bash"],
        defaultPermissionScope: "inherit_work",
        compatibleRoleTemplateIds: ["role-template:engineer:v1"],
      },
      {
        id: "capability-pack:independent-review:v1",
        requiredTools: ["read", "grep", "find", "ls"],
        defaultPermissionScope: "read_only",
        compatibleRoleTemplateIds: ["role-template:reviewer:v1"],
      },
    ]);
    expect(team.workId).toBe(work.summary.id);
    expect(team.lead.roleKind).toBe("lead");
    expect(client.validateAgentAssembly).toBeTypeOf("function");
    expect(client.saveAgentCopy).toBeTypeOf("function");
    expect(client.addWorkMember).toBeTypeOf("function");
  });

  it("returns isolated agent DTOs like the Tauri serialization boundary", async () => {
    const client = createMockTauriClient();
    const work = await client.createWork({
      title: "DTO isolation",
      goal: "Exercise cloning",
      rootPath: "D:/workspace",
      permissionMode: "balanced",
      resourceDraftId: null,
    });
    const firstInstances = await client.listAgentInstances();
    const firstPacks = await client.listCapabilityPacks();
    const firstTeam = await client.getWorkTeam(work.summary.id);
    firstInstances[0]!.displayName = "mutated builtin";
    firstInstances[0]!.definition.capabilityPacks[0]!.name = "mutated nested pack";
    firstPacks[0]!.name = "mutated catalog";
    firstTeam.lead.instance.displayName = "mutated lead";

    expect((await client.listAgentInstances())[0]!.displayName).not.toContain("mutated");
    expect((await client.listCapabilityPacks())[0]!.name).not.toContain("mutated");
    expect((await client.getWorkTeam(work.summary.id)).lead.instance.displayName).not.toContain("mutated");

    const copy = await client.saveAgentCopy({
      sourceInstanceId: "agent-instance:piwork-engineer",
      displayName: "Local engineer",
      capabilityPackIds: ["capability-pack:engineering-execution:v1"],
      engineOverride: null,
      modelConfigurationOverride: null,
      permissionPolicyOverride: null,
      parallelismOverride: null,
    });
    const copyId = copy.id;
    copy.displayName = "mutated copy";
    expect((await client.listAgentInstances()).find(({ id }) => id === copyId)?.displayName)
      .toBe("Local engineer");

    const added = await client.addWorkMember(work.summary.id, copyId);
    added.members.find(({ instance }) => instance.id === copyId)!.instance.displayName = "mutated team";
    expect((await client.getWorkTeam(work.summary.id)).members
      .find(({ instance }) => instance.id === copyId)?.instance.displayName).toBe("Local engineer");
  });

  it("persists a narrowed permission in the copied definition and Work membership", async () => {
    const client = createMockTauriClient();
    const work = await client.createWork({
      title: "Narrowed Agent permission",
      goal: "Preserve resolved assembly authority",
      rootPath: "D:/workspace",
      permissionMode: "balanced",
      resourceDraftId: null,
    });
    const sourceBefore = (await client.listAgentInstances()).find(
      ({ id }) => id === "agent-instance:piwork-engineer",
    )!;

    const copy = await client.saveAgentCopy({
      sourceInstanceId: sourceBefore.id,
      displayName: "Read-only engineer",
      capabilityPackIds: [],
      engineOverride: null,
      modelConfigurationOverride: null,
      permissionPolicyOverride: "read_only",
      parallelismOverride: null,
    });
    const team = await client.addWorkMember(work.summary.id, copy.id);
    const member = team.members.find(({ instance }) => instance.id === copy.id)!;

    expect.soft(copy.definition.defaultPermissionPolicy).toBe("read_only");
    expect.soft(copy.permissionPolicyOverride).toBe("read_only");
    expect.soft(member.permissionPolicy).toBe("read_only");
    expect((await client.listAgentInstances()).find(({ id }) => id === sourceBefore.id))
      .toEqual(sourceBefore);
  });

  it("fails closed when validating or saving invalid agent assemblies", async () => {
    const client = createMockTauriClient();
    const base: SaveAgentAssemblyInput = {
      sourceInstanceId: "agent-instance:piwork-lead",
      displayName: "Local lead",
      capabilityPackIds: ["capability-pack:lead-coordination:v1"],
      engineOverride: null,
      modelConfigurationOverride: null,
      permissionPolicyOverride: null,
      parallelismOverride: null,
    };

    await expect(client.validateAgentAssembly({
      ...base,
      capabilityPackIds: ["capability-pack:source-research:v1"],
    })).resolves.toEqual(expect.arrayContaining([
      expect.objectContaining({
        code: "incompatible_role",
        capabilityPackId: "capability-pack:source-research:v1",
      }),
    ]));
    await expect(client.validateAgentAssembly({
      ...base,
      sourceInstanceId: "agent-instance:piwork-researcher",
      capabilityPackIds: ["capability-pack:source-research:v1"],
      permissionPolicyOverride: "work_write",
    })).resolves.toEqual(expect.arrayContaining([
      expect.objectContaining({ code: "permission_escalation" }),
    ]));
    await expect(client.validateAgentAssembly({
      ...base,
      capabilityPackIds: ["unknown-pack"],
    })).rejects.toThrow("Capability pack does not exist");
    await expect(client.validateAgentAssembly({ ...base, displayName: "  " }))
      .rejects.toThrow("Display name must not be empty");
    await expect(client.validateAgentAssembly({ ...base, parallelismOverride: 0 }))
      .rejects.toThrow("Parallelism must be between 1 and 8");
    await expect(client.validateAgentAssembly({
      ...base,
      capabilityPackIds: [
        "capability-pack:lead-coordination:v1",
        "capability-pack:lead-coordination:v1",
      ],
    })).rejects.toThrow("Capability pack ids must be unique");

    await expect(client.saveAgentCopy({
      ...base,
      capabilityPackIds: ["unknown-pack"],
    })).rejects.toThrow("Capability pack does not exist");
    expect(await client.listAgentInstances()).toHaveLength(4);
  });

  it("rejects missing Works and keeps Work membership unique", async () => {
    const client = createMockTauriClient();

    await expect(client.getWorkTeam("missing-work")).rejects.toThrow(
      "Work not found: missing-work",
    );
    await expect(client.addWorkMember(
      "missing-work",
      "agent-instance:piwork-reviewer",
    )).rejects.toThrow("Work not found: missing-work");

    const work = await client.createWork({
      title: "Team contract",
      goal: "Exercise fail-closed membership",
      rootPath: "D:/workspace",
      permissionMode: "balanced",
      resourceDraftId: null,
    });
    await expect(client.addWorkMember(work.summary.id, "unknown-agent"))
      .rejects.toThrow("Agent instance not found: unknown-agent");
    await client.addWorkMember(work.summary.id, "agent-instance:piwork-reviewer");
    const team = await client.addWorkMember(
      work.summary.id,
      "agent-instance:piwork-reviewer",
    );

    expect(team.members.filter(({ instance }) =>
      instance.id === "agent-instance:piwork-reviewer")).toHaveLength(1);
  });

  it("explains that the Vite URL cannot access the desktop backend", () => {
    render(<App />);

    expect(screen.getByRole("alert").closest("main")).toHaveAttribute(
      "data-motion-state",
      "error",
    );
    expect(
      screen.getByRole("heading", { name: "Open the CoDo desktop app" }),
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

  it("renders the CoDo product shell and bootstraps Work state", async () => {
    const client = createMockTauriClient();
    render(<App client={client} />);
    expect(screen.getByRole("img", { name: "CoDo" })).toBeInTheDocument();
    const newConversation = await screen.findByRole("button", { name: "New conversation" });
    expect(newConversation).toBeInTheDocument();
    expect(newConversation).toHaveTextContent("Ctrl N");
    expect(newConversation).toHaveAttribute("aria-current", "page");
    expect(screen.queryByRole("button", { name: "Home" })).not.toBeInTheDocument();
    expect(screen.getAllByTestId("codo-logo").length).toBeGreaterThan(0);
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
    expect(screen.getAllByTestId("codo-logo").length).toBeGreaterThan(0);
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
    await user.click(screen.getByRole("heading", { name: "Hand it to CoDo." }));
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
