import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { WorkEventEnvelope, WorkEventPayload } from "../../bindings";
import { i18n } from "../../i18n";
import { WorkInspector, type InspectorTab } from "./WorkInspector";

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

const events = [
  event(1, { type: "assistantDelta", text: "A" }),
  event(2, { type: "assistantDelta", text: "B" }),
  event(3, {
    type: "rawEngineEvent",
    kind: "queue_update",
    payloadJson: '{"size":1}',
  }),
];

function Harness() {
  const [active, setActive] = useState<InspectorTab>("delivery");
  return (
    <WorkInspector
      active={active}
      error={{
        code: "engine_faulted",
        message: "engine stopped",
        details: { runId: "run-1" },
      }}
      onActiveChange={setActive}
      onClose={vi.fn()}
      onResizeReset={vi.fn()}
      onResizeStart={vi.fn()}
      open
      resources={[]}
      timeline={events}
      widthPercent={36}
    />
  );
}

describe("WorkInspector", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("zh-CN");
  });

  it("shows the unsuppressed journal and diagnostics in the logs tab", async () => {
    const user = userEvent.setup();
    render(<Harness />);

    const rawActivityTab = screen.getByRole("tab", { name: "活动原始记录" });
    expect(rawActivityTab).toHaveAttribute("id", "inspector-tab-logs");
    await user.click(rawActivityTab);

    expect(screen.getByText("rawEngineEvent")).toBeVisible();
    expect(screen.getAllByTestId("raw-activity-event")).toHaveLength(events.length);
    expect(screen.getByText(/engine_faulted/u)).toBeInTheDocument();
  });

  it("keeps ArrowRight and ArrowLeft tab navigation after renaming logs", async () => {
    const user = userEvent.setup();
    render(<Harness />);

    const rawActivityTab = screen.getByRole("tab", { name: "活动原始记录" });
    await user.click(rawActivityTab);
    await user.keyboard("{ArrowRight}");

    const deliveryTab = screen.getByRole("tab", { name: "交付" });
    expect(deliveryTab).toHaveFocus();
    expect(deliveryTab).toHaveAttribute("aria-selected", "true");

    await user.keyboard("{ArrowLeft}");
    expect(rawActivityTab).toHaveFocus();
    expect(rawActivityTab).toHaveAttribute("aria-selected", "true");
  });

  it("labels the retained logs tab as Raw activity in English", async () => {
    await i18n.changeLanguage("en");
    render(<Harness />);

    expect(screen.getByRole("tab", { name: "Raw activity" })).toHaveAttribute(
      "id",
      "inspector-tab-logs",
    );
  });
});
