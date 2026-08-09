import { describe, expect, it } from "vitest";

import type { WorkEventEnvelope } from "../bindings";
import { normalizeAppError, timelineItemKey } from "./work";

const event = (
  overrides: Partial<WorkEventEnvelope> = {},
): WorkEventEnvelope => ({
  version: 2,
  eventId: "event-1",
  workId: "work-1",
  runId: "run-1",
  sequence: 1,
  occurredAt: "2026-07-28T09:00:01.000Z",
  payload: { type: "assistantDelta", text: "hello" },
  ...overrides,
});

describe("timelineItemKey", () => {
  it("uses the stable event ID for Activity Protocol v2 events", () => {
    expect(
      timelineItemKey(
        event({ eventId: "event-1", runId: "run-1", sequence: 1 }),
      ),
    ).toBe("event:event-1");
  });

  it("falls back to run and sequence identity for legacy events", () => {
    const { eventId: _eventId, ...legacyEvent } = event({
      eventId: undefined,
      version: 1,
      runId: "run-1",
      sequence: 1,
    });

    expect(timelineItemKey(legacyEvent)).toBe("event:run-1:1");
  });
});

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
