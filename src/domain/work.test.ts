import { describe, expect, it } from "vitest";

import { normalizeAppError } from "./work";

describe("normalizeAppError", () => {
  it("preserves a valid wire error and its structured details", () => {
    const details = { workId: "w1", retryable: false };

    const result = normalizeAppError({
      code: "work_conflict",
      message: "Work changed",
      details,
    });

    expect(result).toEqual({
      code: "work_conflict",
      message: "Work changed",
      details,
    });
    expect(result.details).toBe(details);
  });

  it.each([
    [new Error("engine stopped"), "engine stopped"],
    ["plain failure", "plain failure"],
    [null, "Unknown error"],
    [["private", "values"], "Unknown error"],
    [{ message: "must not leak without a code" }, "Unknown error"],
    [42, "Unknown error"],
  ])("normalizes %p without dumping unknown values", (input, message) => {
    const result = normalizeAppError(input);

    expect(result).toEqual({ code: "unknown", message });
    expect(result.message).not.toBe("[object Object]");
  });

  it("rejects an otherwise shaped wire error with invalid details", () => {
    expect(
      normalizeAppError({
        code: "invalid",
        message: "do not partially trust",
        details: ["not", "a", "record"],
      }),
    ).toEqual({ code: "unknown", message: "Unknown error" });
  });
});
