import { act, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { PiWorkClient } from "../../app/tauriClient";
import type { WorkEventEnvelope } from "../../bindings";
import { WorkStoreProvider, useWorkStore } from "./WorkStoreProvider";
import { useWorkEvents } from "./useWorkEvents";

const event: WorkEventEnvelope = {
  version: 1,
  workId: "w1",
  runId: "r1",
  sequence: 1,
  occurredAt: "2026-07-28T09:00:01.000Z",
  payload: { type: "assistantDelta", text: "hello" },
};

const deferred = <T,>() => {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
};

const makeClient = (
  listenToWorkEvents: PiWorkClient["listenToWorkEvents"],
): PiWorkClient => ({
  createWork: async () => {
    throw new Error("unused");
  },
  listWorks: async () => [],
  getWork: async () => {
    throw new Error("unused");
  },
  startWork: async () => {
    throw new Error("unused");
  },
  listenToWorkEvents,
});

function EventSubscriber() {
  useWorkEvents();
  const count = useWorkStore((state) => state.timelines.w1?.length ?? 0);
  return <output>{count}</output>;
}

function ErrorSubscriber() {
  useWorkEvents();
  const message = useWorkStore((state) => state.error?.message ?? "none");
  return <output>{message}</output>;
}

describe("useWorkEvents", () => {
  it("throws a clear error outside WorkStoreProvider", () => {
    const OutsideProvider = () => {
      useWorkStore((state) => state.loading);
      return null;
    };

    expect(() => render(<OutsideProvider />)).toThrow(
      "useWorkStore must be used within WorkStoreProvider",
    );
  });

  it("forwards events and unsubscribes once on unmount", async () => {
    let handler: ((workEvent: WorkEventEnvelope) => void) | undefined;
    const unlisten = vi.fn();
    const client = makeClient(async (receivedHandler) => {
      handler = receivedHandler;
      return unlisten;
    });
    const view = render(
      <WorkStoreProvider client={client}>
        <EventSubscriber />
      </WorkStoreProvider>,
    );

    await waitFor(() => expect(handler).toBeDefined());
    act(() => handler?.(event));
    expect(screen.getByText("1")).toBeInTheDocument();

    view.unmount();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it("unsubscribes once when listen resolves after unmount", async () => {
    const listenResult = deferred<() => void>();
    const unlisten = vi.fn();
    const client = makeClient(() => listenResult.promise);
    const view = render(
      <WorkStoreProvider client={client}>
        <EventSubscriber />
      </WorkStoreProvider>,
    );

    view.unmount();
    await act(async () => listenResult.resolve(unlisten));

    await waitFor(() => expect(unlisten).toHaveBeenCalledTimes(1));
  });

  it("records a listen rejection without an unhandled rejection", async () => {
    const client = makeClient(async () => {
      throw new Error("event channel unavailable");
    });

    render(
      <WorkStoreProvider client={client}>
        <ErrorSubscriber />
      </WorkStoreProvider>,
    );

    expect(
      await screen.findByText("event channel unavailable"),
    ).toBeInTheDocument();
  });
});
