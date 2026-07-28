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
});
