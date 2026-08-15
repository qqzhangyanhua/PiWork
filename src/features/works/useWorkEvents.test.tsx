import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useEffect } from "react";
import { describe, expect, it, vi } from "vitest";

import type { PiWorkClient } from "../../app/tauriClient";
import type { WorkEventEnvelope } from "../../bindings";
import { WorkStoreProvider, useWorkStore } from "./WorkStoreProvider";
import { useWorkEvents } from "./useWorkEvents";

const event: WorkEventEnvelope = {
  version: 2,
  eventId: "event-r1-1",
  workId: "w1",
  runId: "r1",
  turnId: "r1",
  correlationId: "r1",
  sequence: 1,
  occurredAt: "2026-07-28T09:00:01.000Z",
  payload: { type: "assistantDelta", text: "hello" },
};

const deferred = <T,>() => {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, reject, resolve };
};

const makeClient = (
  listenToWorkEvents: PiWorkClient["listenToWorkEvents"],
): PiWorkClient => ({
  getModelConfigurationStatus: async () => ({ configured: true, configuration: { id: "openai-default", provider: "openai", baseUrl: "https://api.openai.com/v1", modelId: "gpt-5.2", active: true, credentialConfigured: true } }),
  listModelConfigurations: async () => [],
  testModelConnection: async () => ({ models: [] }),
  testSavedModelConfiguration: async () => ({ models: [] }),
  saveModelConfiguration: async (input) => ({ id: input.id ?? "model", provider: input.provider, baseUrl: input.baseUrl, modelId: input.modelId, active: true, credentialConfigured: true }),
  activateModelConfiguration: async () => { throw new Error("unused"); },
  selectModelForConfiguration: async () => { throw new Error("unused"); },
  createWork: async () => {
    throw new Error("unused");
  },
  listWorks: async () => [],
  getWork: async () => {
    throw new Error("unused");
  },
  listAgentInstances: vi.fn(async () => { throw new Error("unused"); }),
  listCapabilityPacks: vi.fn(async () => { throw new Error("unused"); }),
  getWorkTeam: vi.fn(async () => { throw new Error("unused"); }),
  validateAgentAssembly: vi.fn(async () => { throw new Error("unused"); }),
  saveAgentCopy: vi.fn(async () => { throw new Error("unused"); }),
  addWorkMember: vi.fn(async () => { throw new Error("unused"); }),
  listProjectFiles: async () => [],
  importResources: async () => [],
  listWorkResources: async () => [],
  getResourceThumbnail: async () => ({ mediaType: "image/png", dataBase64: "" }),
  detachDraftResource: async () => undefined,
  startWork: async () => {
    throw new Error("unused");
  },
  stopWork: async () => {
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

function Listener() {
  useWorkEvents();
  return null;
}

function TimelineProbe() {
  const count = useWorkStore((state) => state.timelines.w1?.length ?? 0);
  return <output>{count}</output>;
}

function StoreErrorProbe() {
  const message = useWorkStore((state) => state.error?.message ?? "none");
  return <output data-testid="store-error">{message}</output>;
}

function HydratingListener() {
  useWorkEvents();
  const hydrate = useWorkStore((state) => state.hydrate);
  useEffect(() => {
    void hydrate();
  }, [hydrate]);
  return <StoreErrorProbe />;
}

function OperationListener({ operation }: { operation: "create" | "start" }) {
  useWorkEvents();
  const createWork = useWorkStore((state) => state.createWork);
  const startWork = useWorkStore((state) => state.startWork);
  const execute = () => {
    const promise =
      operation === "start"
        ? startWork("w1", "go")
        : createWork({
            title: "Created",
            goal: "Test ownership",
            rootPath: "D:/dev/PiWork",
            permissionMode: "balanced",
            resourceDraftId: null,
          });
    void promise.catch(() => undefined);
  };
  return (
    <>
      <button onClick={execute} type="button">
        run operation
      </button>
      <StoreErrorProbe />
    </>
  );
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

  it("ignores a saved handler after disposal while listen is unresolved", async () => {
    const listenResult = deferred<() => void>();
    const unlisten = vi.fn();
    let handler: ((workEvent: WorkEventEnvelope) => void) | undefined;
    const client = makeClient((receivedHandler) => {
      handler = receivedHandler;
      return listenResult.promise;
    });
    const tree = (listening: boolean) => (
      <WorkStoreProvider client={client}>
        <TimelineProbe />
        {listening ? <Listener /> : null}
      </WorkStoreProvider>
    );
    const view = render(tree(true));
    await waitFor(() => expect(handler).toBeDefined());

    view.rerender(tree(false));
    act(() => handler?.(event));

    expect(screen.getByText("0")).toBeInTheDocument();
    await act(async () => listenResult.resolve(unlisten));
    expect(unlisten).toHaveBeenCalledTimes(1);
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

  it("keeps a subscription error when hydrate later succeeds", async () => {
    const listResult = deferred<[]>();
    const client: PiWorkClient = {
      ...makeClient(async () => {
        throw { code: "subscription", message: "subscription offline" };
      }),
      listWorks: () => listResult.promise,
    };

    render(
      <WorkStoreProvider client={client}>
        <HydratingListener />
      </WorkStoreProvider>,
    );
    expect(await screen.findByText("subscription offline")).toBeInTheDocument();

    await act(async () => listResult.resolve([]));
    expect(screen.getByTestId("store-error")).toHaveTextContent(
      "subscription offline",
    );
  });

  it.each(["start", "create"] as const)(
    "keeps a subscription error when %s succeeds",
    async (operation) => {
      const client: PiWorkClient = {
        ...makeClient(async () => {
          throw { code: "subscription", message: "subscription offline" };
        }),
        createWork: async () => ({
          summary: {
            id: "created",
            title: "Created",
            goal: "Test ownership",
            rootPath: "D:/dev/PiWork",
            permissionMode: "balanced",
            status: "draft",
            createdAt: "2026-07-28T09:00:00.000Z",
            updatedAt: "2026-07-28T09:00:00.000Z",
          },
          runs: [],
          messages: [],
          events: [],
        }),
        startWork: async () => ({
          run: {
            id: "r1",
            workId: "w1",
            engineKind: "codex",
            engineSessionId: null,
            modelLabel: "gpt-5",
            status: "running",
            createdAt: "2026-07-28T09:00:01.000Z",
            startedAt: "2026-07-28T09:00:01.000Z",
            completedAt: null,
          },
          userMessage: {
            id: "m1",
            workId: "w1",
            runId: "r1",
            role: "user",
            content: "go",
            resourceIds: [],
            createdAt: "2026-07-28T09:00:01.000Z",
          },
        }),
      };
      render(
        <WorkStoreProvider client={client}>
          <OperationListener operation={operation} />
        </WorkStoreProvider>,
      );
      expect(
        await screen.findByText("subscription offline"),
      ).toBeInTheDocument();

      fireEvent.click(screen.getByRole("button", { name: "run operation" }));
      await waitFor(() =>
        expect(screen.getByTestId("store-error")).toHaveTextContent(
          "subscription offline",
        ),
      );
    },
  );

  it("does not let a stale command failure replace a subscription error", async () => {
    const firstStart = deferred<never>();
    let starts = 0;
    const client: PiWorkClient = {
      ...makeClient(async () => {
        throw { code: "subscription", message: "subscription offline" };
      }),
      startWork: () => {
        starts += 1;
        return starts === 1
          ? firstStart.promise
          : Promise.resolve({
              run: {
                id: "r2",
                workId: "w1",
                engineKind: "codex",
                engineSessionId: null,
                modelLabel: "gpt-5",
                status: "running",
                createdAt: "2026-07-28T09:00:02.000Z",
                startedAt: "2026-07-28T09:00:02.000Z",
                completedAt: null,
              },
              userMessage: {
                id: "m2",
                workId: "w1",
                runId: "r2",
                role: "user",
                content: "go",
                resourceIds: [],
                createdAt: "2026-07-28T09:00:02.000Z",
              },
            });
      },
    };
    render(
      <WorkStoreProvider client={client}>
        <OperationListener operation="start" />
      </WorkStoreProvider>,
    );
    expect(await screen.findByText("subscription offline")).toBeInTheDocument();

    const button = screen.getByRole("button", { name: "run operation" });
    fireEvent.click(button);
    fireEvent.click(button);
    await act(async () => firstStart.reject(new Error("stale command")));

    expect(screen.getByTestId("store-error")).toHaveTextContent(
      "subscription offline",
    );
  });

  it("clears a subscription error only after a new subscription succeeds", async () => {
    const secondListen = deferred<() => void>();
    let listens = 0;
    const client = makeClient(() => {
      listens += 1;
      return listens === 1
        ? Promise.reject({
            code: "subscription",
            message: "subscription offline",
          })
        : secondListen.promise;
    });
    const tree = (listening: boolean) => (
      <WorkStoreProvider client={client}>
        <StoreErrorProbe />
        {listening ? <Listener /> : null}
      </WorkStoreProvider>
    );
    const view = render(tree(true));
    expect(await screen.findByText("subscription offline")).toBeInTheDocument();

    view.rerender(tree(false));
    view.rerender(tree(true));
    await waitFor(() => expect(listens).toBe(2));
    expect(screen.getByTestId("store-error")).toHaveTextContent(
      "subscription offline",
    );

    await act(async () => secondListen.resolve(() => undefined));
    await waitFor(() =>
      expect(screen.getByTestId("store-error")).toHaveTextContent("none"),
    );
  });
});
