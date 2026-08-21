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
  MemoryCandidateSummary,
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

export type ExtensionTrustTier = "builtin" | "verified" | "community";

export type ExtensionSummary = {
  packageId: string;
  displayName: string;
  description: string;
  publisher: string;
  trustTier: ExtensionTrustTier;
  sourceKind: "bundled" | "npm";
  installedVersion: string | null;
  latestVersion: string;
  lifecycleStatus: "available" | "installed" | "disabled" | "revoked" | "pending_removal";
  builtin: boolean;
  manifest: Record<string, unknown>;
  permissions: Record<string, unknown>;
  enabledAgentIds: string[];
};

export type CommunityExtensionSummary = {
  packageId: string;
  version: string;
  description: string;
  publisher: string;
  npmUrl: string | null;
  score: number;
  executable: false;
};

export type WebSearchProviderSummary = {
  providerId: string;
  enabled: boolean;
  endpoint: string | null;
  credentialConfigured: boolean;
};

export type WebAccessSettingsSummary = {
  enabled: boolean;
  urlFetchEnabled: boolean;
  defaultProvider: string | null;
  fallbackProvider: string | null;
  providers: WebSearchProviderSummary[];
};

export type SaveWebAccessSettingsInput = {
  enabled: boolean;
  urlFetchEnabled: boolean;
  defaultProvider: string | null;
  fallbackProvider: string | null;
  providers: Array<{
    providerId: string;
    enabled: boolean;
    endpoint: string | null;
    apiKey?: string | null;
    clearCredential?: boolean;
  }>;
};

export type ConnectorPermission = "metadata" | "read_body" | "send";

export type ConnectorWorkGrantSummary = {
  workId: string;
  permissions: ConnectorPermission[];
};

export type EmailConnectorSummary = {
  id: string;
  displayName: string;
  emailAddress: string;
  username: string;
  preset: string;
  imapHost: string;
  imapPort: number;
  smtpHost: string;
  smtpPort: number;
  enabled: boolean;
  pollIntervalMinutes: 1 | 2 | 5 | 15;
  healthStatus: "untested" | "healthy" | "degraded" | "error";
  lastErrorCode: string | null;
  lastCheckedAt: string | null;
  lastPolledAt: string | null;
  credentialConfigured: boolean;
  grantedWorkIds: string[];
  workGrants: ConnectorWorkGrantSummary[];
};

export type SaveEmailConnectorInput = {
  id?: string | null;
  displayName: string;
  emailAddress: string;
  username: string;
  password?: string | null;
  preset: string;
  imapHost: string;
  imapPort: number;
  smtpHost: string;
  smtpPort: number;
  pollIntervalMinutes: 1 | 2 | 5 | 15;
};

export type ConnectorTestResult = {
  imapOk: boolean;
  smtpOk: boolean;
  errorCode: string | null;
};

export type EmailMetadataSummary = {
  connectionId: string;
  folder: string;
  uid: number;
  messageId: string | null;
  senderName: string | null;
  senderAddress: string;
  subject: string;
  sentAt: string | null;
  receivedAt: string | null;
  flagsJson: string;
  attachmentCount: number;
  sizeBytes: number | null;
};

export type AppNotificationSummary = {
  id: string;
  category: "mail" | "approval" | "plugin" | "connector";
  connectionId: string | null;
  workId: string | null;
  title: string;
  summary: string;
  action: Record<string, unknown>;
  readAt: string | null;
  expiresAt: string | null;
  createdAt: string;
};

export type PendingConnectorActionSummary = {
  id: string;
  connectionId: string;
  workId: string;
  runId: string;
  actionType: "read_email_body" | "send_email";
  preview: Record<string, unknown>;
  status: "pending" | "approved" | "denied" | "expired" | "executed" | "failed";
  expiresAt: string;
  createdAt: string;
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
  listExtensions?(): Promise<ExtensionSummary[]>;
  searchCommunityExtensions?(query: string): Promise<CommunityExtensionSummary[]>;
  setExtensionAgentEnabled?(
    packageId: string,
    agentInstanceId: string,
    enabled: boolean,
    toolAllowlist?: string[],
  ): Promise<ExtensionSummary>;
  getWebAccessSettings?(): Promise<WebAccessSettingsSummary>;
  saveWebAccessSettings?(input: SaveWebAccessSettingsInput): Promise<WebAccessSettingsSummary>;
  listEmailConnectors?(): Promise<EmailConnectorSummary[]>;
  saveEmailConnector?(input: SaveEmailConnectorInput): Promise<EmailConnectorSummary>;
  testEmailConnector?(input: SaveEmailConnectorInput): Promise<ConnectorTestResult>;
  setEmailConnectorEnabled?(connectionId: string, enabled: boolean): Promise<EmailConnectorSummary>;
  deleteEmailConnector?(connectionId: string): Promise<void>;
  setConnectorWorkGrant?(
    connectionId: string,
    workId: string,
    enabled: boolean,
    permissions: ConnectorPermission[],
  ): Promise<EmailConnectorSummary>;
  listEmailMetadata?(connectionId: string, query?: string, limit?: number): Promise<EmailMetadataSummary[]>;
  listAppNotifications?(limit?: number): Promise<AppNotificationSummary[]>;
  markAppNotificationRead?(notificationId: string): Promise<void>;
  clearAppNotification?(notificationId: string): Promise<void>;
  listPendingConnectorActions?(workId?: string | null): Promise<PendingConnectorActionSummary[]>;
  resolvePendingConnectorAction?(actionId: string, approve: boolean): Promise<PendingConnectorActionSummary>;
  listenToAppNotifications?(
    handler: (notification: AppNotificationSummary) => void,
  ): Promise<UnlistenFn>;
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
  listMemoryCandidates(workId: string): Promise<MemoryCandidateSummary[]>;
  resolveMemoryCandidate(
    candidateId: string,
    confirm: boolean,
  ): Promise<MemoryCandidateSummary>;
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
  listExtensions: () => invoke<ExtensionSummary[]>("list_extensions"),
  searchCommunityExtensions: (query) =>
    invoke<CommunityExtensionSummary[]>("search_community_extensions", { query }),
  setExtensionAgentEnabled: (packageId, agentInstanceId, enabled, toolAllowlist = []) =>
    invoke<ExtensionSummary>("set_extension_agent_enabled", {
      packageId,
      agentInstanceId,
      enabled,
      toolAllowlist,
    }),
  getWebAccessSettings: () =>
    invoke<WebAccessSettingsSummary>("get_web_access_settings"),
  saveWebAccessSettings: (input) =>
    invoke<WebAccessSettingsSummary>("save_web_access_settings", { input }),
  listEmailConnectors: () =>
    invoke<EmailConnectorSummary[]>("list_email_connectors"),
  saveEmailConnector: (input) =>
    invoke<EmailConnectorSummary>("save_email_connector", { input }),
  testEmailConnector: (input) =>
    invoke<ConnectorTestResult>("test_email_connector", { input }),
  setEmailConnectorEnabled: (connectionId, enabled) =>
    invoke<EmailConnectorSummary>("set_email_connector_enabled", { connectionId, enabled }),
  deleteEmailConnector: (connectionId) =>
    invoke<void>("delete_email_connector", { connectionId }),
  setConnectorWorkGrant: (connectionId, workId, enabled, permissions) =>
    invoke<EmailConnectorSummary>("set_connector_work_grant", {
      input: { connectionId, workId, enabled, permissions },
    }),
  listEmailMetadata: (connectionId, query = "", limit = 50) =>
    invoke<EmailMetadataSummary[]>("list_email_metadata", { connectionId, query, limit }),
  listAppNotifications: (limit = 50) =>
    invoke<AppNotificationSummary[]>("list_app_notifications", { limit }),
  markAppNotificationRead: (notificationId) =>
    invoke<void>("mark_app_notification_read", { notificationId }),
  clearAppNotification: (notificationId) =>
    invoke<void>("clear_app_notification", { notificationId }),
  listPendingConnectorActions: (workId = null) =>
    invoke<PendingConnectorActionSummary[]>("list_pending_connector_actions", { workId }),
  resolvePendingConnectorAction: (actionId, approve) =>
    invoke<PendingConnectorActionSummary>("resolve_pending_connector_action", { actionId, approve }),
  listenToAppNotifications: async (handler) =>
    listen<AppNotificationSummary>("piwork://notification", ({ payload }) => handler(payload)),
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
  listMemoryCandidates: (workId) =>
    invoke<MemoryCandidateSummary[]>("list_memory_candidates", { workId }),
  resolveMemoryCandidate: (candidateId, confirm) =>
    invoke<MemoryCandidateSummary>("resolve_memory_candidate", {
      candidateId,
      confirm,
    }),
  listenToWorkEvents: (handler) =>
    listen<WorkEventEnvelope>("piwork://work-event", ({ payload }) =>
      handler(payload),
    ),
};
