// @ts-expect-error Vitest executes this contract test in Node; the app intentionally omits global Node typings.
import { existsSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

describe("agent center visual contract", () => {
  it("ships a responsive dashboard-aligned surface", () => {
    const path = "src/styles/agent-center.css";
    expect(existsSync(path)).toBe(true);
    if (!existsSync(path)) return;

    const css = readFileSync(path, "utf8");
    expect(css).toContain("var(--pw-dashboard-canvas)");
    expect(css).toContain("grid-template-columns: repeat(3");
    expect(css).toContain("@media (max-width: 1120px)");
    expect(css).toContain("@media (max-width: 760px)");
    expect(css).toContain("@media (prefers-reduced-motion: reduce)");
  });
});
