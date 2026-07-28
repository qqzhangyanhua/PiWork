import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { i18n } from "../i18n";
import { createMockTauriClient } from "../test/mockTauriClient";
import { App } from "./App";

beforeEach(async () => {
  await i18n.changeLanguage("en");
});

describe("App", () => {
  it("renders the PiWork product shell and bootstraps Work state", async () => {
    const client = createMockTauriClient();
    render(<App client={client} />);
    expect(screen.getByText("PiWork")).toBeInTheDocument();
    expect(screen.getByTestId("continuous-loop-logo")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "New Work" })).toBeInTheDocument();
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
