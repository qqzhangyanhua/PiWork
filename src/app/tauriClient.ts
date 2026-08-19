import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  AgentInstanceSummary,
  AssemblyDiagnostic,
  AssignmentSummary,
  CapabilityPackSummary,
  CreateWorkInput,
  ImportResourcesInput,
  InterruptWorkInput,
  ProjectFileSummary,
  QueueWorkInput,
  ResourceSummary,
  ResourceThumbnail,
  RuntimeStatus,
  SaveAgentAssemblyInput,
  StartWorkOutput,
  WorkDetail,
  WorkEventEnvelope,
  WorkSummary,
  WorkTeamSummary,
} from "../bindings";

export type ModelProvider =
  | "openai"
  | "anthropic"
  | "google"
  | "openrouter"
  | "deepseek"
  | "custom";

export type ModelConfigurationSummary = {
  id: string;
  provider: ModelProvider;
  baseUrl: string;
  modelId: string;
  active: boolean;
  credentialConfigured: boolean;
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
  id?: string;
  modelId: string;
};

export type SelectModelForConfigurationInput = {
  configurationId: string;
  modelId: string;
};

export type PiWorkClient = {
  getDefaultProjectDirectory?(): Promise<string>;
  getRuntimeStatus?(): Promise<RuntimeStatus>;
  getModelConfigurationStatus(): Promise<ModelConfigurationStatus>;
  listModelConfigurations(): Promise<ModelConfigurationSummary[]>;
  testModelConnection(input: ModelConnectionInput): Promise<ModelConnectionResult>;
  testSavedModelConfiguration(configurationId: string): Promise<ModelConnectionResult>;
  saveModelConfiguration(input: SaveModelConfigurationInput): Promise<ModelConfigurationSummary>;
  activateModelConfiguration(configurationId: string): Promise<ModelConfigurationSummary>;
  selectModelForConfiguration(input: SelectModelForConfigurationInput): Promise<ModelConfigurationSummary>;
  createWork(input: CreateWorkInput): Promise<WorkDetail>;
  listWorks(): Promise<WorkSummary[]>;
  getWork(workId: string): Promise<WorkDetail>;
  listAgentInstances(): Promise<AgentInstanceSummary[]>;
  listCapabilityPacks(): Promise<CapabilityPackSummary[]>;
  getWorkTeam(workId: string): Promise<WorkTeamSummary>;
  validateAgentAssembly(input: SaveAgentAssemblyInput): Promise<AssemblyDiagnostic[]>;
  saveAgentCopy(input: SaveAgentAssemblyInput): Promise<AgentInstanceSummary>;
  addWorkMember(workId: string, agentInstanceId: string): Promise<WorkTeamSummary>;
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
  stopWork(workId: string): Promise<WorkDetail>;
  drainAssignmentEventOutbox(): Promise<void>;
  listWorkAssignments(workId: string): Promise<AssignmentSummary[]>;
  queueWorkInput(workId: string, input: QueueWorkInput): Promise<StartWorkOutput>;
  confirmAssignmentRecovery(
    assignmentId: string,
    resume: boolean,
  ): Promise<AssignmentSummary>;
  interruptAndReplace(
    workId: string,
    input: InterruptWorkInput,
  ): Promise<StartWorkOutput>;
  listenToWorkEvents(
    handler: (event: WorkEventEnvelope) => void,
  ): Promise<UnlistenFn>;
};

export const tauriClient: PiWorkClient = {
  getDefaultProjectDirectory: () => invoke<string>("get_default_project_directory"),
  getRuntimeStatus: () => invoke<RuntimeStatus>("get_runtime_status"),
  getModelConfigurationStatus: () =>
    invoke<ModelConfigurationStatus>("get_model_configuration_status"),
  listModelConfigurations: () =>
    invoke<ModelConfigurationSummary[]>("list_model_configurations"),
  testModelConnection: (input) =>
    invoke<ModelConnectionResult>("test_model_connection", { input }),
  testSavedModelConfiguration: (configurationId) =>
    invoke<ModelConnectionResult>("test_saved_model_configuration", { configurationId }),
  saveModelConfiguration: (input) =>
    invoke<ModelConfigurationSummary>("save_model_configuration", { input }),
  activateModelConfiguration: (configurationId) =>
    invoke<ModelConfigurationSummary>("activate_model_configuration", { configurationId }),
  selectModelForConfiguration: (input) =>
    invoke<ModelConfigurationSummary>("select_model_for_configuration", { input }),
  createWork: (input) => invoke<WorkDetail>("create_work", { input }),
  listWorks: () => invoke<WorkSummary[]>("list_works"),
  getWork: (workId) => invoke<WorkDetail>("get_work", { workId }),
  listAgentInstances: () => invoke<AgentInstanceSummary[]>("list_agent_instances"),
  listCapabilityPacks: () => invoke<CapabilityPackSummary[]>("list_capability_packs"),
  getWorkTeam: (workId) => invoke<WorkTeamSummary>("get_work_team", { workId }),
  validateAgentAssembly: (input) =>
    invoke<AssemblyDiagnostic[]>("validate_agent_assembly", { input }),
  saveAgentCopy: (input) => invoke<AgentInstanceSummary>("save_agent_copy", { input }),
  addWorkMember: (workId, agentInstanceId) =>
    invoke<WorkTeamSummary>("add_work_member", { workId, agentInstanceId }),
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
  stopWork: (workId) => invoke<WorkDetail>("stop_work", { workId }),
  drainAssignmentEventOutbox: () =>
    invoke<void>("drain_assignment_event_outbox"),
  listWorkAssignments: (workId) =>
    invoke<AssignmentSummary[]>("list_work_assignments", { workId }),
  queueWorkInput: (workId, input) =>
    invoke<StartWorkOutput>("queue_work_input", { workId, input }),
  confirmAssignmentRecovery: (assignmentId, resume) =>
    invoke<AssignmentSummary>("confirm_assignment_recovery", {
      assignmentId,
      resume,
    }),
  interruptAndReplace: (workId, input) =>
    invoke<StartWorkOutput>("interrupt_and_replace", { workId, input }),
  listenToWorkEvents: (handler) =>
    listen<WorkEventEnvelope>("piwork://work-event", ({ payload }) =>
      handler(payload),
    ),
};
