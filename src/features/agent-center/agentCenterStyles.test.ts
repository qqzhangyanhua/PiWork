// @ts-expect-error Vitest executes this contract test in Node; the app intentionally omits global Node typings.
import { existsSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const relativeLuminance = (hex: string) => {
  const channels = hex.match(/[\da-f]{2}/gi)?.map((channel) => {
    const value = Number.parseInt(channel, 16) / 255;
    return value <= 0.04045
      ? value / 12.92
      : ((value + 0.055) / 1.055) ** 2.4;
  });
  if (!channels || channels.length !== 3) throw new Error(`Invalid hex color: ${hex}`);
  return channels[0]! * 0.2126 + channels[1]! * 0.7152 + channels[2]! * 0.0722;
};

const contrastRatio = (first: string, second: string) => {
  const ordered = [relativeLuminance(first), relativeLuminance(second)]
    .sort((left, right) => right - left);
  return (ordered[0]! + 0.05) / (ordered[1]! + 0.05);
};

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

  it("defines explicit dark contrast for key agent center surfaces", () => {
    const path = "src/styles/agent-center.css";
    const css = readFileSync(path, "utf8");
    const darkStart = css.indexOf("@media (prefers-color-scheme: dark)");
    const darkEnd = css.indexOf("@media (prefers-reduced-motion: reduce)", darkStart);
    const darkCss = css.slice(darkStart, darkEnd);

    for (const selector of [
      ".agent-center-stat",
      ".agent-center-stat__value",
      ".capability-path[aria-pressed=\"true\"]",
      ".capability-drawer__footer",
      ".capability-drawer__footer p",
      ".member-assembler__diagnostics code",
      ".member-assembler__diagnostics p",
      ".member-assembler__diagnostics span",
    ]) {
      expect(darkCss).toContain(selector);
    }
    expect(darkCss).toContain("var(--pw-dashboard-border)");
    expect(darkCss).toContain("var(--pw-dashboard-panel-solid)");
    expect(darkCss).toContain("var(--pw-text-primary)");
    expect(darkCss).toContain("var(--pw-text-secondary)");
    for (const rule of [
      ".capability-empty { border-color: var(--pw-dashboard-border); background: #192231; color: var(--pw-text-secondary); }",
      ".capability-empty h2 { color: var(--pw-text-primary); }",
      ".capability-card__body > span { color: var(--pw-text-secondary); }",
      ".team-member-card__description { color: var(--pw-text-secondary); }",
      ".member-assembler__validation > p { color: #ffd58a; }",
      ".member-assembler__validation > p.is-valid { color: #82d9b1; }",
      ".member-assembler__save-error { color: #ffb4bd !important; }",
      ".capability-drawer__availability { border-color: var(--pw-dashboard-border) !important; background: #1d293a; }",
      ".capability-drawer__availability.is-deprecated { border-color: #80515c !important; background: #35242a; }",
      ".team-member-card__packs > span { color: #9fc2ff; }",
      ".member-pack-option.is-fixed { background: #202b3b; color: var(--pw-text-secondary); }",
    ]) {
      expect(darkCss).toContain(rule);
    }
  });

  it("keeps dark surface text combinations at WCAG AA contrast", () => {
    const tokens = readFileSync("src/styles/tokens.css", "utf8");
    const darkTokens = tokens.slice(tokens.indexOf("@media (prefers-color-scheme: dark)"));
    const tokenColor = (name: string) => {
      const color = darkTokens.match(new RegExp(`${name}:\\s*(#[\\da-f]{6})`, "i"))?.[1];
      expect(color).toBeDefined();
      return color!;
    };
    const primary = tokenColor("--pw-text-primary");
    const secondary = tokenColor("--pw-text-secondary");
    const panel = tokenColor("--pw-dashboard-panel-solid");

    for (const [background, foreground] of [
      ["#192231", primary],
      ["#192231", secondary],
      ["#192231", "#ffd58a"],
      ["#192231", "#82d9b1"],
      ["#192231", "#ffb4bd"],
      ["#22334d", "#9fc2ff"],
      ["#35242a", "#ffc7cb"],
      ["#35242a", "#e6aeb4"],
      ["#4b2d36", "#ffb4bd"],
      ["#1d293a", primary],
      ["#1d293a", secondary],
      ["#1d293a", "#9fc2ff"],
      ["#35242a", primary],
      ["#35242a", secondary],
      [panel, secondary],
    ]) {
      expect(contrastRatio(background, foreground)).toBeGreaterThanOrEqual(4.5);
    }
  });
});
