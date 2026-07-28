import { createStore } from "zustand/vanilla";

import { tauriClient, type PiWorkClient } from "../../app/tauriClient";
import type {
  CreateWorkInput,
  RunSummary,
  WorkDetail,
  WorkEventEnvelope,
  WorkSummary,
} from "../../bindings";
import type { AppError, TimelineItem } from "../../domain/work";

export type WorkState = {
  works: Record<string, WorkSummary>;
  selectedWorkId: string | null;
  timelines: Record<string, TimelineItem[]>;
  lastSequenceByRun: Record<string, number>;
  loading: boolean;
  error: AppError | null;
  hydrate(): Promise<void>;
  createWork(input: CreateWorkInput): Promise<WorkDetail>;
  startWork(workId: string, prompt: string): Promise<RunSummary>;
  selectWork(workId: string): void;
  upsertWork(work: WorkSummary): void;
  applyEvent(event: WorkEventEnvelope): void;
};

const normalizeError = (error: unknown): AppError => {
  if (error instanceof Error) {
    return { code: "unknown", message: error.message };
  }

  return { code: "unknown", message: String(error) };
};

const sortPersistedEvents = (detail: WorkDetail) => {
  const runOrder = new Map(
    [...detail.runs]
      .sort((left, right) => left.createdAt.localeCompare(right.createdAt))
      .map((run, index) => [run.id, index]),
  );

  return detail.events
    .map((event, index) => ({ event, index }))
    .sort((left, right) => {
      const runDifference =
        (runOrder.get(left.event.runId) ?? Number.MAX_SAFE_INTEGER) -
        (runOrder.get(right.event.runId) ?? Number.MAX_SAFE_INTEGER);
      return (
        runDifference ||
        left.event.sequence - right.event.sequence ||
        left.index - right.index
      );
    })
    .map(({ event }) => event);
};

export const createWorkStore = (client: PiWorkClient = tauriClient) => {
  let hydration: Promise<void> | null = null;
  let selectionRequest = 0;
  let activeSelectionRequest: number | null = null;

  const store = createStore<WorkState>((set, get) => ({
    works: {},
    selectedWorkId: null,
    timelines: {},
    lastSequenceByRun: {},
    loading: false,
    error: null,
    hydrate: async () => {
      if (hydration) {
        return hydration;
      }

      hydration = (async () => {
        set({ loading: true, error: null });
        try {
          const listedWorks = await client.listWorks();
          const sortedWorks = [...listedWorks].sort((left, right) =>
            right.updatedAt.localeCompare(left.updatedAt),
          );
          for (const listedWork of sortedWorks) {
            get().upsertWork(listedWork);
          }

          const selectedWorkId = get().selectedWorkId ?? sortedWorks[0]?.id ?? null;
          set({ selectedWorkId });
          if (!selectedWorkId) {
            return;
          }

          const request = selectionRequest;
          const detail = await client.getWork(selectedWorkId);
          if (
            request !== selectionRequest ||
            get().selectedWorkId !== selectedWorkId
          ) {
            return;
          }
          get().upsertWork(detail.summary);
          for (const persistedEvent of sortPersistedEvents(detail)) {
            get().applyEvent(persistedEvent);
          }
        } catch (error) {
          set({ error: normalizeError(error) });
        } finally {
          if (activeSelectionRequest === null) {
            set({ loading: false });
          }
          hydration = null;
        }
      })();

      return hydration;
    },
    createWork: async (input) => {
      set({ loading: true, error: null });
      try {
        const detail = await client.createWork(input);
        selectionRequest += 1;
        get().upsertWork(detail.summary);
        set({ selectedWorkId: detail.summary.id });
        for (const persistedEvent of sortPersistedEvents(detail)) {
          get().applyEvent(persistedEvent);
        }
        return detail;
      } catch (error) {
        set({ error: normalizeError(error) });
        throw error;
      } finally {
        set({ loading: false });
      }
    },
    startWork: async (workId, prompt) => {
      set({ loading: true, error: null });
      try {
        const run = await client.startWork(workId, prompt);
        set((state) => {
          const currentWork = state.works[workId];
          return currentWork
            ? {
                works: {
                  ...state.works,
                  [workId]: {
                    ...currentWork,
                    status: "running",
                    updatedAt: run.createdAt,
                  },
                },
              }
            : state;
        });
        return run;
      } catch (error) {
        set({ error: normalizeError(error) });
        throw error;
      } finally {
        set({ loading: false });
      }
    },
    selectWork: (workId) => {
      const request = ++selectionRequest;
      activeSelectionRequest = request;
      set({ selectedWorkId: workId, loading: true, error: null });
      void (async () => {
        try {
          const detail = await client.getWork(workId);
          if (
            request !== selectionRequest ||
            get().selectedWorkId !== workId
          ) {
            return;
          }
          get().upsertWork(detail.summary);
          for (const persistedEvent of sortPersistedEvents(detail)) {
            get().applyEvent(persistedEvent);
          }
        } catch (error) {
          if (request === selectionRequest) {
            set({ error: normalizeError(error) });
          }
        } finally {
          if (request === selectionRequest) {
            activeSelectionRequest = null;
            set({ loading: false });
          }
        }
      })();
    },
    upsertWork: (work) =>
      set((state) => {
        const current = state.works[work.id];
        if (current && current.updatedAt > work.updatedAt) {
          return state;
        }
        return { works: { ...state.works, [work.id]: work } };
      }),
    applyEvent: (event) => {
      if (event.sequence <= (get().lastSequenceByRun[event.runId] ?? 0)) {
        return;
      }

      set((state) => {
        const currentWork = state.works[event.workId];
        const status =
          event.payload.type === "runStarted"
            ? "running"
            : event.payload.type === "runCompleted"
              ? "completed"
              : event.payload.type === "runFailed"
                ? "failed"
                : currentWork?.status;
        const updatedWork = currentWork
          ? {
              ...currentWork,
              status: status ?? currentWork.status,
              updatedAt: event.occurredAt,
            }
          : undefined;

        return {
          works: updatedWork
            ? { ...state.works, [event.workId]: updatedWork }
            : state.works,
          timelines: {
            ...state.timelines,
            [event.workId]: [...(state.timelines[event.workId] ?? []), event],
          },
          lastSequenceByRun: {
            ...state.lastSequenceByRun,
            [event.runId]: event.sequence,
          },
        };
      });
    },
  }));

  return store;
};

export type WorkStore = ReturnType<typeof createWorkStore>;
