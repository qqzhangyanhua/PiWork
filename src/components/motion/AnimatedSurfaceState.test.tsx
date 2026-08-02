import { render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AnimatedSurfaceState } from "./AnimatedSurfaceState";

const stubMotionPreference = (reduceMotion: boolean) => {
  vi.stubGlobal(
    "matchMedia",
    vi.fn((query: string) => ({
      matches: query.includes("no-preference")
        ? !reduceMotion
        : query.includes(": reduce")
          ? reduceMotion
          : false,
      media: query,
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })),
  );
};

describe("AnimatedSurfaceState", () => {
  beforeEach(() => stubMotionPreference(true));
  afterEach(() => vi.unstubAllGlobals());

  it("preserves main loading semantics when motion is reduced", () => {
    render(
      <AnimatedSurfaceState
        aria-label="正在加载"
        as="main"
        className="loading-state"
        role="status"
        variant="loading"
      >
        <div data-motion-line>加载线</div>
      </AnimatedSurfaceState>,
    );

    const state = screen.getByRole("status", { name: "正在加载" });
    expect(state.tagName).toBe("MAIN");
    expect(state).toHaveClass("loading-state");
    expect(screen.getByText("加载线")).not.toHaveAttribute("style");
  });

  it("preserves section error semantics and recovery controls", () => {
    render(
      <AnimatedSurfaceState as="section" role="alert" variant="error">
        <h1>加载失败</h1>
        <button type="button">重试</button>
      </AnimatedSurfaceState>,
    );

    const state = screen.getByRole("alert");
    expect(state.tagName).toBe("SECTION");
    expect(screen.getByRole("button", { name: "重试" })).toBeEnabled();
  });

  it("reverts scoped inline animation styles on unmount", async () => {
    stubMotionPreference(false);
    const rendered = render(
      <AnimatedSurfaceState
        aria-label="正在加载"
        as="main"
        role="status"
        variant="loading"
      >
        <div data-motion-line>加载线</div>
      </AnimatedSurfaceState>,
    );
    const root = rendered.container.querySelector<HTMLElement>("main");
    const line = screen.getByText("加载线");

    await waitFor(() =>
      expect(root).not.toHaveStyle({ visibility: "hidden" }),
    );
    expect(line).toHaveAttribute("style");

    rendered.unmount();

    expect(root?.style.length).toBe(0);
    expect(line.style.cssText).toBe("");
  });
});
