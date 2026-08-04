import { describe, expect, it } from "vitest";

import { clampInspectorPercent, DEFAULT_INSPECTOR_PERCENT } from "./inspectorLayout";

describe("inspector layout", () => {
  it("uses 36 percent as the default expanded width", () => {
    expect(DEFAULT_INSPECTOR_PERCENT).toBe(36);
  });

  it("constrains dragged inspector widths to 32–60 percent", () => {
    expect(clampInspectorPercent(12)).toBe(32);
    expect(clampInspectorPercent(47.5)).toBe(47.5);
    expect(clampInspectorPercent(81)).toBe(60);
  });
});
