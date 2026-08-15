import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
// @ts-expect-error Vitest runs this visual contract in Node; the app omits global Node typings.
import { readFileSync } from "node:fs";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { WorkEventEnvelope, WorkEventPayload } from "../../bindings";
import { i18n } from "../../i18n";
import { RawActivityRail } from "./RawActivityRail";

const workspaceStyles = readFileSync("src/styles/workspace.css", "utf8");
const tokenStyles = readFileSync("src/styles/tokens.css", "utf8");

const tokenHex = (source: string, token: string): string => {
  const match = source.match(new RegExp(`${token}:\\s*(#[0-9a-f]{6})`, "iu"));
  if (!match) throw new Error(`Missing color token ${token}`);
  return match[1]!;
};

const relativeLuminance = (hex: string): number => {
  const channel = (start: number): number =>
    Number.parseInt(hex.slice(start, start + 2), 16) / 255;
  const [red, green, blue] = [channel(1), channel(3), channel(5)].map((value) =>
    value <= 0.04045
      ? value / 12.92
      : ((value + 0.055) / 1.055) ** 2.4,
  );
  return 0.2126 * red! + 0.7152 * green! + 0.0722 * blue!;
};

const contrastRatio = (foreground: string, background: string): number => {
  const lighter = Math.max(
    relativeLuminance(foreground),
    relativeLuminance(background),
  );
  const darker = Math.min(
    relativeLuminance(foreground),
    relativeLuminance(background),
  );
  return (lighter + 0.05) / (darker + 0.05);
};

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

const journal = (
  count: number,
  payload: WorkEventPayload = { type: "assistantDelta", text: "entry" },
): WorkEventEnvelope[] =>
  Array.from({ length: count }, (_, index) => {
    const sequence = index + 1;
    return event(sequence, payload, {
      occurredAt: new Date(Date.UTC(2026, 7, 9, 0, 0, 0, sequence)).toISOString(),
    });
  });

describe("RawActivityRail", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("zh-CN");
  });

  it("renders every journal event in tuple order and formats raw JSON", async () => {
    const user = userEvent.setup();
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
    await user.click(rows[1]!.querySelector("summary")!);
    expect(rows[1]).toHaveTextContent("queue_update");
    expect(within(rows[1]!).getByText(/"size": 1/u)).toBeInTheDocument();
  });

  it("marks v1 events with database event ids as legacy without inventing identifiers", async () => {
    const user = userEvent.setup();
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
    await user.click(row.querySelector("summary")!);
    expect(within(row).getByText("旧版事件")).toBeInTheDocument();
    expect(row).not.toHaveTextContent("session-unknown");
    expect(row).not.toHaveTextContent("turn-unknown");
    expect(row).not.toHaveTextContent("correlation-unknown");
  });

  it("does not mark a v2 event as legacy when one optional identity is absent", async () => {
    const user = userEvent.setup();
    render(
      <RawActivityRail
        events={[
          event(1, { type: "assistantDelta", text: "current" }, {
            sessionId: undefined,
          }),
        ]}
      />,
    );

    await user.click(screen.getByText("#1").closest("summary")!);
    expect(screen.queryByText("旧版事件")).not.toBeInTheDocument();
  });

  it("renders malformed raw payload text safely", async () => {
    const user = userEvent.setup();
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

    await user.click(container.querySelector("summary")!);
    expect(container.querySelector("pre")?.textContent).toContain(
      JSON.stringify(malformed),
    );
    expect(container.querySelector("img")).toBeNull();
  });

  it("sorts multiple runs by occurredAt, runId, sequence, and eventId", async () => {
    const user = userEvent.setup();
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
    for (const row of rows) await user.click(row.querySelector("summary")!);
    expect(rows.map((row) => row.textContent)).toEqual([
      expect.stringContaining("earliest"),
      expect.stringContaining("run-a-a"),
      expect.stringContaining("run-a-z"),
      expect.stringContaining("run-b"),
    ]);
  });

  it("defers raw payload materialization and reuses an expanded row", async () => {
    const user = userEvent.setup();
    const journal = [
      event(1, {
        type: "rawEngineEvent",
        kind: "first",
        payloadJson: '{"first":1}',
      }),
      event(2, {
        type: "rawEngineEvent",
        kind: "second",
        payloadJson: '{"second":2}',
      }),
    ];
    const parse = vi.spyOn(JSON, "parse");
    const { container, rerender } = render(<RawActivityRail events={journal} />);

    expect(parse).not.toHaveBeenCalled();
    expect(container.querySelectorAll(".raw-activity-event pre")).toHaveLength(0);

    const summaries = Array.from(container.querySelectorAll("summary"));
    await user.click(summaries[1]!);
    expect(parse).toHaveBeenCalledTimes(1);
    expect(parse).toHaveBeenCalledWith('{"second":2}');
    expect(container.querySelectorAll(".raw-activity-event pre")).toHaveLength(1);

    rerender(<RawActivityRail events={journal} />);
    expect(parse).toHaveBeenCalledTimes(1);
  });

  it("keeps an expanded legacy row open when an older event is prepended", async () => {
    const user = userEvent.setup();
    const legacy = event(2, { type: "assistantDelta", text: "kept-open" }, {
      eventId: undefined,
      version: 1,
    });
    const { rerender } = render(<RawActivityRail events={[legacy]} />);

    await user.click(screen.getByText("#2").closest("summary")!);
    expect(screen.getByText("#2").closest("details")).toHaveAttribute("open");

    rerender(
      <RawActivityRail
        events={[
          event(1, { type: "assistantDelta", text: "older" }),
          legacy,
        ]}
      />,
    );

    expect(screen.getByText("#2").closest("details")).toHaveAttribute("open");
  });

  it("does not collide when separate runs reuse an event id", () => {
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => undefined);
    render(
      <RawActivityRail
        events={[
          event(1, { type: "assistantDelta", text: "run a" }, {
            assignmentId: "assignment-1",
            eventId: "shared-event",
            runId: "run-a",
          }),
          event(1, { type: "assistantDelta", text: "run b" }, {
            assignmentId: "assignment-1",
            eventId: "shared-event",
            runId: "run-b",
          }),
        ]}
      />,
    );

    expect(consoleError.mock.calls.flat().join(" ")).not.toContain(
      "Encountered two children with the same key",
    );
    consoleError.mockRestore();
  });

  it("does not mutate its event input while sorting", () => {
    const journal = [
      event(2, { type: "assistantDelta", text: "second" }),
      event(1, { type: "assistantDelta", text: "first" }),
    ];

    render(<RawActivityRail events={journal} />);

    expect(journal.map(({ sequence }) => sequence)).toEqual([2, 1]);
  });

  it("mounts only one 200-event page while preserving access to all journal events", async () => {
    const user = userEvent.setup();
    const events = journal(450).reverse();

    render(<RawActivityRail events={events} />);

    expect(screen.getAllByTestId("raw-activity-event")).toHaveLength(200);
    expect(screen.getByText("#1")).toBeVisible();
    expect(screen.getByText("#200")).toBeVisible();
    expect(screen.queryByText("#201")).not.toBeInTheDocument();
    expect(screen.getByText("1–200 / 450")).toBeVisible();

    const previous = screen.getByRole("button", { name: "上一页" });
    const next = screen.getByRole("button", { name: "下一页" });
    expect(previous.tagName).toBe("BUTTON");
    expect(next.tagName).toBe("BUTTON");
    expect(previous).toBeDisabled();
    expect(next).toBeEnabled();

    await user.click(next);
    expect(screen.getAllByTestId("raw-activity-event")).toHaveLength(200);
    expect(screen.getByText("#201")).toBeVisible();
    expect(screen.getByText("#400")).toBeVisible();
    expect(screen.queryByText("#200")).not.toBeInTheDocument();
    expect(screen.getByText("201–400 / 450")).toBeVisible();

    await user.click(next);
    expect(screen.getAllByTestId("raw-activity-event")).toHaveLength(50);
    expect(screen.getByText("#401")).toBeVisible();
    expect(screen.getByText("#450")).toBeVisible();
    expect(screen.getByText("401–450 / 450")).toBeVisible();
    expect(next).toBeDisabled();

    await user.click(previous);
    expect(screen.getAllByTestId("raw-activity-event")).toHaveLength(200);
    expect(screen.getByText("#201")).toBeVisible();
    expect(screen.getByText("#400")).toBeVisible();
  });

  it("keeps a legal current page stable when new events arrive", async () => {
    const user = userEvent.setup();
    const { rerender } = render(<RawActivityRail events={journal(450)} />);

    await user.click(screen.getByRole("button", { name: "下一页" }));
    rerender(<RawActivityRail events={journal(451)} />);

    expect(screen.getAllByTestId("raw-activity-event")).toHaveLength(200);
    expect(screen.getByText("#201")).toBeVisible();
    expect(screen.getByText("#400")).toBeVisible();
    expect(screen.getByText("201–400 / 451")).toBeVisible();
  });

  it("clamps the current page when the journal becomes shorter", async () => {
    const user = userEvent.setup();
    const { rerender } = render(<RawActivityRail events={journal(450)} />);
    const next = screen.getByRole("button", { name: "下一页" });

    await user.click(next);
    await user.click(next);
    expect(screen.getByText("#450")).toBeVisible();

    rerender(<RawActivityRail events={journal(250)} />);
    expect(screen.getAllByTestId("raw-activity-event")).toHaveLength(50);
    expect(screen.getByText("#201")).toBeVisible();
    expect(screen.getByText("#250")).toBeVisible();
    expect(screen.getByText("201–250 / 250")).toBeVisible();

    rerender(<RawActivityRail events={journal(50)} />);
    expect(screen.getAllByTestId("raw-activity-event")).toHaveLength(50);
    expect(screen.getByText("#1")).toBeVisible();
    expect(screen.getByText("#50")).toBeVisible();
    expect(screen.queryByRole("navigation")).not.toBeInTheDocument();

    rerender(<RawActivityRail events={journal(450)} />);
    expect(screen.getByText("#1")).toBeVisible();
    expect(screen.getByText("1–200 / 450")).toBeVisible();
  });

  it("renders every short-journal row without pagination controls", () => {
    render(<RawActivityRail events={journal(25)} />);

    expect(screen.getAllByTestId("raw-activity-event")).toHaveLength(25);
    expect(screen.queryByRole("navigation")).not.toBeInTheDocument();
  });

  it("localizes pagination controls in English", async () => {
    await i18n.changeLanguage("en");

    render(<RawActivityRail events={journal(201)} />);

    expect(screen.getByRole("navigation", { name: "Journal pages" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Previous page" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Next page" })).toBeEnabled();
    expect(screen.getByText("1–200 / 201")).toBeVisible();
  });

  it("does not materialize large raw payloads while paging collapsed rows", async () => {
    const user = userEvent.setup();
    const parse = vi.spyOn(JSON, "parse");
    const events = journal(450, {
      type: "rawEngineEvent",
      kind: "large",
      payloadJson: '{"large":true}',
    });

    render(<RawActivityRail events={events} />);
    await user.click(screen.getByRole("button", { name: "下一页" }));

    expect(parse).not.toHaveBeenCalled();
    expect(events.map(({ sequence }) => sequence)).toEqual(
      Array.from({ length: 450 }, (_, index) => index + 1),
    );
  });

  it("shows only metadata fields that exist", async () => {
    const user = userEvent.setup();
    render(
      <RawActivityRail
        events={[
          event(1, { type: "assistantDelta", text: "metadata" }, {
            agentId: "agent-1",
            assignmentId: undefined,
            causationId: "cause-1",
            sessionId: undefined,
          }),
        ]}
      />,
    );

    await user.click(screen.getByText("#1").closest("summary")!);
    expect(screen.getAllByRole("term").map((term) => term.textContent)).toEqual([
      "event",
      "work",
      "run",
      "turn",
      "agent",
      "causation",
      "correlation",
    ]);
    expect(screen.getByText("agent-1")).toBeInTheDocument();
    expect(screen.getByText("cause-1")).toBeInTheDocument();
    expect(screen.queryByText("旧版事件")).not.toBeInTheDocument();
  });

  it("renders the localized empty state", () => {
    render(<RawActivityRail events={[]} />);

    expect(screen.getByText("还没有活动事件。")).toBeVisible();
    expect(screen.queryByTestId("raw-activity-event")).not.toBeInTheDocument();
  });

  it("keeps compact raw ledger labels at AA contrast in the light theme", () => {
    for (const rule of [
      /\.raw-activity-event > summary time\s*\{[^}]*color: var\(--pw-text-secondary\)/su,
      /\.raw-activity-event__sequence\s*\{[^}]*color: var\(--pw-text-secondary\)/su,
      /\.raw-activity-event__identity dt\s*\{[^}]*color: var\(--pw-text-secondary\)/su,
    ]) {
      expect(workspaceStyles).toMatch(rule);
    }
    expect(workspaceStyles).toMatch(
      /\.raw-activity-rail__pagination\s*\{[^}]*flex-wrap:\s*wrap/su,
    );
    expect(workspaceStyles).toMatch(
      /\.raw-activity-rail__pagination button\s*\{[^}]*color:\s*var\(--pw-text-primary\)/su,
    );

    const lightTokens = tokenStyles.slice(0, tokenStyles.indexOf("@media"));
    const copy = tokenHex(lightTokens, "--pw-text-secondary");
    const panel = tokenHex(lightTokens, "--pw-surface-panel");
    expect(contrastRatio(copy, panel)).toBeGreaterThanOrEqual(4.5);
  });
});
