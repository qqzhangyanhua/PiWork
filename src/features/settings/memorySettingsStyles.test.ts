// @ts-expect-error Vitest executes this visual contract in Node; the app omits global Node typings.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const memoryStyles = readFileSync("src/styles/memory-settings.css", "utf8");
const sharedSettingsStyles = readFileSync("src/styles/web-access-settings.css", "utf8");

function rule(css: string, selector: string) {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
  const match = css.match(new RegExp(`${escaped}\\s*\\{([^}]+)\\}`, "u"));
  expect(match, `Missing CSS rule: ${selector}`).not.toBeNull();
  return match?.[1] ?? "";
}

describe("memory settings layout", () => {
  it("keeps a focused write-back switch from creating horizontal overflow", () => {
    expect(rule(sharedSettingsStyles, ".settings-switch")).toContain("position: relative");
    expect(rule(sharedSettingsStyles, ".settings-switch input")).toContain("clip-path: inset(50%)");

    expect(rule(memoryStyles, ".memory-settings__content")).toContain("overflow-x: hidden");
    expect(rule(memoryStyles, ".memory-runtime-controls")).toContain("repeat(4, minmax(0, 1fr))");
    expect(rule(memoryStyles, ".memory-runtime-controls > label")).toContain("min-width: 0");
  });
});
