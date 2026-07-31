import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  CreateWorkInput,
  ImportResourcesInput,
  ProjectFileSummary,
  ResourceSummary,
  ResourceThumbnail,
  StartWorkOutput,
  WorkDetail,
  WorkEventEnvelope,
  WorkSummary,
} from "../bindings";

export type ModelProvider =
  | "openai"
  | "anthropic"
  | "google"
  | "openrouter"
  | "deepseek"
  | "custom";

export type ModelConfigurationSummary = {
  provider: ModelProvider;
  modelId: string;
};

export type ModelConfigurationStatus = {
  configured: boolean;
  configuration: ModelConfigurationSummary | null;
};

export type ModelConnectionInput = {
  provider: ModelProvider;
  apiKey: string;
  baseUrl: string;
};

export type AvailableModel = {
  id: string;
  label: string;
};

export type ModelConnectionResult = {
  models: AvailableModel[];
};

export type SaveModelConfigurationInput = ModelConnectionInput & {
  modelId: string;
};

export type PiWorkClient = {
  getModelConfigurationStatus(): Promise<ModelConfigurationStatus>;
  testModelConnection(input: ModelConnectionInput): Promise<ModelConnectionResult>;
  saveModelConfiguration(input: SaveModelConfigurationInput): Promise<ModelConfigurationSummary>;
  createWork(input: CreateWorkInput): Promise<WorkDetail>;
  listWorks(): Promise<WorkSummary[]>;
  getWork(workId: string): Promise<WorkDetail>;
  listProjectFiles(rootPath: string): Promise<ProjectFileSummary[]>;
  importResources(input: ImportResourcesInput): Promise<ResourceSummary[]>;
  listWorkResources(workId: string): Promise<ResourceSummary[]>;
  getResourceThumbnail(resourceId: string): Promise<ResourceThumbnail>;
  detachDraftResource(draftId: string, resourceId: string): Promise<void>;
  startWork(
    workId: string,
    prompt: string,
    referencedFiles?: string[],
    resourceIds?: string[],
  ): Promise<StartWorkOutput>;
  listenToWorkEvents(
    handler: (event: WorkEventEnvelope) => void,
  ): Promise<UnlistenFn>;
};

export const tauriClient: PiWorkClient = {
  getModelConfigurationStatus: () =>
    invoke<ModelConfigurationStatus>("get_model_configuration_status"),
  testModelConnection: (input) =>
    invoke<ModelConnectionResult>("test_model_connection", { input }),
  saveModelConfiguration: (input) =>
    invoke<ModelConfigurationSummary>("save_model_configuration", { input }),
  createWork: (input) => invoke<WorkDetail>("create_work", { input }),
  listWorks: () => invoke<WorkSummary[]>("list_works"),
  getWork: (workId) => invoke<WorkDetail>("get_work", { workId }),
  listProjectFiles: (rootPath) =>
    invoke<ProjectFileSummary[]>("list_project_files", { rootPath }),
  importResources: (input) =>
    invoke<ResourceSummary[]>("import_resources", { input }),
  listWorkResources: (workId) =>
    invoke<ResourceSummary[]>("list_work_resources", { workId }),
  getResourceThumbnail: (resourceId) =>
    invoke<ResourceThumbnail>("get_resource_thumbnail", { resourceId }),
  detachDraftResource: (draftId, resourceId) =>
    invoke<void>("detach_draft_resource", { draftId, resourceId }),
  startWork: (workId, prompt, referencedFiles = [], resourceIds = []) =>
    invoke<StartWorkOutput>("start_work", {
      workId,
      input: { prompt, referencedFiles, resourceIds },
    }),
  listenToWorkEvents: (handler) =>
    listen<WorkEventEnvelope>("piwork://work-event", ({ payload }) =>
      handler(payload),
    ),
};
