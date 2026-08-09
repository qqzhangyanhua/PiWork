import { render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";

import type { WorkEventEnvelope, WorkEventPayload } from "../../bindings";
import { i18n } from "../../i18n";
import { RawActivityRail } from "./RawActivityRail";

const event = (
  sequence: number,
  payload: WorkEventPayload,
  identity: Partial<WorkEventEnvelope> = {},
): WorkEventEnvelope => ({
  version: 2,
  eventId: `event-${sequence}`,
  workId: "work-1",
  runId: "run-1",
  turnId: "turn-1",
  sessionId: "session-1",
  correlationId: "run-1",
  sequence,
  occurredAt: `2026-08-09T00:00:${String(sequence).padStart(2, "0")}.000Z`,
  payload,
  ...identity,
});

describe("RawActivityRail", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("zh-CN");
  });

  it("renders every journal event in tuple order and formats raw JSON", () => {
    render(
      <RawActivityRail
        events={[
          event(2, {
            type: "rawEngineEvent",
            kind: "queue_update",
            payloadJson: '{"size":1}',
          }),
          event(1, { type: "assistantDelta", text: "answer" }),
        ]}
      />,
    );

    const rows = screen.getAllByTestId("raw-activity-event");
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent("assistantDelta");
    expect(rows[1]).toHaveTextContent("queue_update");
    expect(within(rows[1]!).getByText(/"size": 1/u)).toBeInTheDocument();
  });

  it("marks identity-poor v1 events as legacy without inventing identifiers", () => {
    render(
      <RawActivityRail
        events={[
          event(
            1,
            { type: "assistantDelta", text: "legacy" },
            {
              version: 1,
              turnId: undefined,
              sessionId: undefined,
              correlationId: undefined,
            },
          ),
        ]}
      />,
    );

    const row = screen.getByTestId("raw-activity-event");
    expect(within(row).getByText("旧版事件")).toBeInTheDocument();
    expect(row).not.toHaveTextContent("session-unknown");
    expect(row).not.toHaveTextContent("turn-unknown");
    expect(row).not.toHaveTextContent("correlation-unknown");
  });

  it("renders malformed raw payload text safely", () => {
    const malformed = '<img src=x onerror="alert(1)">';
    const { container } = render(
      <RawActivityRail
        events={[
          event(1, {
            type: "rawEngineEvent",
            kind: "malformed",
            payloadJson: malformed,
          }),
        ]}
      />,
    );

    expect(container.querySelector("pre")?.textContent).toContain(
      JSON.stringify(malformed),
    );
    expect(container.querySelector("img")).toBeNull();
  });

  it("sorts multiple runs by occurredAt, runId, sequence, and eventId", () => {
    const { container } = render(
      <RawActivityRail
        events={[
          event(1, { type: "assistantDelta", text: "run-b" }, {
            eventId: "event-b",
            occurredAt: "2026-08-09T00:00:01.000Z",
            runId: "run-b",
          }),
          event(9, { type: "assistantDelta", text: "run-a-z" }, {
            eventId: "event-z",
            occurredAt: "2026-08-09T00:00:01.000Z",
            runId: "run-a",
          }),
          event(99, { type: "assistantDelta", text: "earliest" }, {
            eventId: "event-early",
            occurredAt: "2026-08-09T00:00:00.000Z",
            runId: "run-z",
          }),
          event(9, { type: "assistantDelta", text: "run-a-a" }, {
            eventId: "event-a",
            occurredAt: "2026-08-09T00:00:01.000Z",
            runId: "run-a",
          }),
        ]}
      />,
    );

    const rows = Array.from(
      container.querySelectorAll<HTMLElement>("[data-testid='raw-activity-event']"),
    );
    expect(rows.map((row) => row.textContent)).toEqual([
      expect.stringContaining("earliest"),
      expect.stringContaining("run-a-a"),
      expect.stringContaining("run-a-z"),
      expect.stringContaining("run-b"),
    ]);
  });
});
