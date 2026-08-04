import { describe, expect, it } from "vitest";

import {
  AGENT_CAPABILITIES,
  AGENT_CAPABILITY_DOMAINS,
  buildCapabilityPrompt,
  filterCapabilities,
} from "./agentCapabilities";

describe("Magic Factory agent capabilities", () => {
  it("contains the complete V2 capability catalog", () => {
    expect(AGENT_CAPABILITIES).toHaveLength(96);
    expect(new Set(AGENT_CAPABILITIES.map(({ id }) => id)).size).toBe(96);
    expect(AGENT_CAPABILITIES.map(({ id }) => id)).toEqual(
      Array.from({ length: 96 }, (_, index) => index + 1),
    );
    expect(AGENT_CAPABILITY_DOMAINS).toHaveLength(9);
    expect(AGENT_CAPABILITIES.filter(({ priority }) => priority === "P0")).toHaveLength(23);
    expect(
      AGENT_CAPABILITY_DOMAINS.map(({ id }) =>
        AGENT_CAPABILITIES.filter(({ domainId }) => domainId === id).length,
      ),
    ).toEqual([8, 7, 9, 12, 10, 14, 16, 8, 12]);
  });

  it("combines full-text search, domain and priority filters", () => {
    expect(
      filterCapabilities(AGENT_CAPABILITIES, {
        query: "报价",
        domainId: "quote-finance",
        priority: "P0",
      }).map(({ name }) => name),
    ).toEqual(["参考报价智能体", "报价预审与版本智能体", "合同一致性与风险智能体"]);
  });

  it("builds an editable task draft from the selected capability", () => {
    const capability = AGENT_CAPABILITIES.find(({ id }) => id === 16)!;
    const prompt = buildCapabilityPrompt(capability);

    expect(prompt).toContain("需求澄清智能体");
    expect(prompt).toContain(capability.coreCapability);
    expect(prompt).toContain(capability.outputs[0]);
    expect(prompt).toContain("业务目标：");
    expect(prompt).toContain("补充约束或人工确认点：");
  });
});
