import { describe, expect, it } from "vitest";

import en from "./en.json";
import zhCN from "./zh-CN.json";

const leafKeys = (value: object, prefix = ""): string[] =>
  Object.entries(value).flatMap(([key, nested]) => {
    const path = prefix ? `${prefix}.${key}` : key;
    return nested && typeof nested === "object"
      ? leafKeys(nested as object, path)
      : [path];
  });

describe("workspace locale resources", () => {
  it("keeps complete English and Chinese key parity", () => {
    const englishKeys = leafKeys(en).sort();
    const chineseKeys = leafKeys(zhCN).sort();

    expect(englishKeys).toContain("composer.continue");
    expect(chineseKeys).toEqual(englishKeys);
  });

  it("keeps the team, marketplace, connectors, and web settings vocabulary aligned", () => {
    expect(zhCN.agentCenter.team.title).toBe("我的团队");
    expect(en.agentCenter.team.title).toBe("My Team");
    expect(zhCN.sidebar.nav).toMatchObject({ plugins: "插件市场", connectors: "连接器" });
    expect(en.sidebar.nav).toMatchObject({ plugins: "Plugin Marketplace", connectors: "Connectors" });
    expect(zhCN.settings.webAccessNav).toBe("联网搜索");
    expect(en.settings.webAccessNav).toBe("Web access");
    expect(JSON.stringify(zhCN)).not.toContain("MAGIC FACTORY");
    expect(JSON.stringify(en)).not.toContain("MAGIC FACTORY");
  });

  it("does not retain locale keys from the retired summarized log renderer", () => {
    const retiredKeys = [
      "currentRun",
      "allWork",
      "noLogs",
      "logEvents",
      "logOutputSummary",
      "runEvents",
      "toolEvents",
    ];

    for (const key of retiredKeys) {
      expect(en.inspector).not.toHaveProperty(key);
      expect(zhCN.inspector).not.toHaveProperty(key);
    }
  });
});
