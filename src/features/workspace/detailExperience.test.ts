import { beforeEach, describe, expect, it } from "vitest";

import {
  DETAIL_EXPERIENCE_STORAGE_KEY,
  persistDetailExperience,
  readDetailExperience,
} from "./detailExperience";

describe("detailExperience", () => {
  beforeEach(() => localStorage.clear());

  it("defaults to dashboard and persists classic", () => {
    expect(readDetailExperience()).toBe("dashboard");
    expect(persistDetailExperience("classic")).toBe("classic");
    expect(readDetailExperience()).toBe("classic");
  });

  it("ignores invalid persisted values", () => {
    localStorage.setItem(DETAIL_EXPERIENCE_STORAGE_KEY, "broken");
    expect(readDetailExperience()).toBe("dashboard");
  });
});
