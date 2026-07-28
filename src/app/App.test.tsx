import { render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { i18n } from "../i18n";
import { App } from "./App";
import type { PiWorkClient } from "./tauriClient";

const makeClient = () => {
  const listWorks = vi.fn(async () => []);
  const listenToWorkEvents = vi.fn(async () => () => undefined);
  const client: PiWorkClient = {
    createWork: async () => {
      throw new Error("unused");
    },
    listWorks,
    getWork: async () => {
      throw new Error("unused");
    },
    startWork: async () => {
      throw new Error("unused");
    },
    listenToWorkEvents,
  };
  return { client, listWorks, listenToWorkEvents };
};

describe("App", () => {
  it("renders the PiWork product shell and bootstraps Work state", async () => {
    const { client, listWorks, listenToWorkEvents } = makeClient();
    render(<App client={client} />);
    expect(screen.getByRole("heading", { name: "PiWork" })).toBeInTheDocument();
    await waitFor(() => expect(listWorks).toHaveBeenCalledTimes(1));
    expect(listenToWorkEvents).toHaveBeenCalledTimes(1);
  });

  it("renders the localized product shell", async () => {
    const { client } = makeClient();
    await i18n.changeLanguage("zh-CN");
    render(<App client={client} />);
    expect(
      await screen.findByRole("button", { name: "新建 Work" }),
    ).toBeInTheDocument();
    expect(screen.getByTestId("continuous-loop-logo")).toBeInTheDocument();
  });
});
