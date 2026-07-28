import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  CreateWorkInput,
  RunSummary,
  WorkDetail,
  WorkEventEnvelope,
  WorkSummary,
} from "../bindings";

export type PiWorkClient = {
  createWork(input: CreateWorkInput): Promise<WorkDetail>;
  listWorks(): Promise<WorkSummary[]>;
  getWork(workId: string): Promise<WorkDetail>;
  startWork(workId: string, prompt: string): Promise<RunSummary>;
  listenToWorkEvents(
    handler: (event: WorkEventEnvelope) => void,
  ): Promise<UnlistenFn>;
};

export const tauriClient: PiWorkClient = {
  createWork: (input) => invoke<WorkDetail>("create_work", { input }),
  listWorks: () => invoke<WorkSummary[]>("list_works"),
  getWork: (workId) => invoke<WorkDetail>("get_work", { workId }),
  startWork: (workId, prompt) =>
    invoke<RunSummary>("start_work", { workId, prompt }),
  listenToWorkEvents: (handler) =>
    listen<WorkEventEnvelope>("piwork://work-event", ({ payload }) =>
      handler(payload),
    ),
};
