import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { i18n } from "../i18n";
import { App } from "./App";

describe("App", () => {
  it("renders the PiWork product shell", () => {
    render(<App />);
    expect(screen.getByRole("heading", { name: "PiWork" })).toBeInTheDocument();
  });

  it("renders the localized product shell", async () => {
    await i18n.changeLanguage("zh-CN");
    render(<App />);
    expect(
      await screen.findByRole("button", { name: "新建 Work" }),
    ).toBeInTheDocument();
    expect(screen.getByTestId("continuous-loop-logo")).toBeInTheDocument();
  });
});
