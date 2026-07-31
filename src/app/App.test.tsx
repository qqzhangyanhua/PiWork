import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { i18n } from "../i18n";
import { createMockTauriClient } from "../test/mockTauriClient";
import { App } from "./App";

beforeEach(async () => {
  await i18n.changeLanguage("en");
});

describe("App", () => {
  it("explains that the Vite URL cannot access the desktop backend", () => {
    render(<App />);

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
      provider: "openai",
      modelId: "gpt-5.2",
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
      await screen.findAllByRole("button", { name: "New Work" }),
    ).toHaveLength(1);
    expect(client.listWorks).toHaveBeenCalledTimes(1);
  });

  it("renders the PiWork product shell and bootstraps Work state", async () => {
    const client = createMockTauriClient();
    render(<App client={client} />);
    expect(screen.getByText("PiWork")).toBeInTheDocument();
    expect(screen.getByTestId("continuous-loop-logo")).toBeInTheDocument();
    expect(
      await screen.findByRole("button", { name: "New Work" }),
    ).toBeInTheDocument();
    await waitFor(() => expect(client.listWorks).toHaveBeenCalledTimes(1));
    expect(client.listenToWorkEvents).toHaveBeenCalledTimes(1);
  });

  it("renders the localized product shell", async () => {
    const client = createMockTauriClient();
    await i18n.changeLanguage("zh-CN");
    render(<App client={client} />);
    expect(
      await screen.findByRole("button", { name: "新建 Work" }),
    ).toBeInTheDocument();
    expect(screen.getByTestId("continuous-loop-logo")).toBeInTheDocument();
  });
});
