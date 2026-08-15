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

  it("names persistent teams, honest capability states and task drafting in both locales", () => {
    expect(zhCN.agentCenter.views).toMatchObject({ team: "我的团队", library: "能力库" });
    expect(en.agentCenter.views).toMatchObject({ team: "My Team", library: "Capability Library" });
    expect(zhCN.agentCenter.capability.status).toEqual({
      catalog_only: "目录能力",
      executable: "可装载",
      deprecated: "已停用",
    });
    expect(en.agentCenter.capability.status).toEqual({
      catalog_only: "Catalog entry",
      executable: "Installable",
      deprecated: "Deprecated",
    });
    expect(zhCN.agentCenter.drawer.createDraft).toBe("创建任务草稿");
    expect(en.agentCenter.drawer.createDraft).toBe("Create task draft");
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
