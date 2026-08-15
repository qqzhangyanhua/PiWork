import { describe, expect, it } from "vitest";

import type {
  CapabilityPackStatus,
  CapabilityPackSummary,
  RoleKind,
  WorkAgentSummary,
  WorkTeamSummary,
} from "../../bindings";
import { AGENT_CAPABILITIES, type AgentCapability } from "./agentCapabilities";
import {
  buildCapabilityLibrary,
  buildTeamModel,
  splitCapabilityPacks,
} from "./agentCenterModel";

function capabilityPack({
  catalogCapabilityId,
  id,
  status,
}: {
  catalogCapabilityId: string | null;
  id: string;
  status: CapabilityPackStatus;
}): CapabilityPackSummary {
  return {
    id,
    catalogCapabilityId,
    name: `Server ${id}`,
    description: "Server-owned description",
    instructions: "",
    inputSchema: {},
    outputSchema: {},
    procedure: {},
    validationRubric: {},
    requiredTools: [],
    defaultPermissionScope: "read_only",
    compatibleRoleTemplateIds: [],
    requiredEngineCapabilities: [],
    conflictsWithCapabilityPackIds: [],
    version: 1,
    status,
  };
}

function catalogCapabilityPacks(): CapabilityPackSummary[] {
  return Array.from({ length: 96 }, (_, index) => {
    const sequence = String(index + 1).padStart(3, "0");
    return capabilityPack({
      catalogCapabilityId: `catalog-capability:${sequence}`,
      id: `capability-pack:catalog-${sequence}:v1`,
      status: "catalog_only",
    });
  });
}

function workAgent(
  roleKind: RoleKind,
  instanceId: string,
  capabilityPacks: CapabilityPackSummary[] = [],
): WorkAgentSummary {
  return {
    roleKind,
    instance: {
      id: instanceId,
      definition: { capabilityPacks },
    },
  } as WorkAgentSummary;
}

describe("Agent Center model", () => {
  it("splits business catalog packs from system packs without rewriting server status", () => {
    const catalog = capabilityPack({
      catalogCapabilityId: "catalog-capability:001",
      id: "capability-pack:catalog-001:v1",
      status: "deprecated",
    });
    const executableSystemPack = capabilityPack({
      catalogCapabilityId: null,
      id: "capability-pack:lead-coordination:v1",
      status: "executable",
    });
    const unknownSystemPack = {
      ...capabilityPack({
        catalogCapabilityId: null,
        id: "capability-pack:future-system:v1",
        status: "deprecated",
      }),
      status: "future_server_status",
    } as unknown as CapabilityPackSummary;

    const packs = splitCapabilityPacks([
      executableSystemPack,
      catalog,
      unknownSystemPack,
    ]);

    expect(packs.catalog).toEqual([catalog]);
    expect(packs.system).toEqual([executableSystemPack, unknownSystemPack]);
    expect(packs.system[1]?.status).toBe("future_server_status");
  });

  it("merges all 96 static descriptions with authoritative catalog status", () => {
    const { catalog } = splitCapabilityPacks(catalogCapabilityPacks());
    const model = buildCapabilityLibrary(AGENT_CAPABILITIES, catalog);
    const requirementClarification = model.find(({ id }) => id === 16);

    expect(model).toHaveLength(96);
    expect(model.every(({ status }) => status === "catalog_only")).toBe(true);
    expect(requirementClarification).toMatchObject({
      id: 16,
      catalogId: "catalog-capability:016",
      capabilityPackId: "capability-pack:catalog-016:v1",
      status: "catalog_only",
      name: "需求澄清智能体",
      domainId: "requirements-assessment",
      audiences: ["客户、商务、工程师"],
      coreCapability: "根据资料成熟度动态追问影响工程实现的关键问题",
      suggestedInputs: [
        "需求文档、图纸或附件",
        "范围、约束、交付期望和待确认问题",
      ],
    });
  });

  it("keeps a non-executable server status authoritative", () => {
    const packs = catalogCapabilityPacks();
    packs[15] = { ...packs[15]!, status: "deprecated" };

    const model = buildCapabilityLibrary(AGENT_CAPABILITIES, packs);

    expect(model.find(({ id }) => id === 16)?.status).toBe("deprecated");
  });

  it("fails closed for an unknown server catalog status", () => {
    const packs = catalogCapabilityPacks();
    packs[15] = {
      ...packs[15]!,
      status: "future_server_status",
    } as unknown as CapabilityPackSummary;

    expect(() => buildCapabilityLibrary(AGENT_CAPABILITIES, packs)).toThrow(
      "Unknown server capability pack status 'future_server_status' for 'capability-pack:catalog-016:v1'",
    );
  });

  it("fails loudly when a catalog capability id is missing", () => {
    const packs = catalogCapabilityPacks();
    packs[15] = { ...packs[15]!, catalogCapabilityId: null };
    const { catalog } = splitCapabilityPacks(packs);

    expect(() => buildCapabilityLibrary(AGENT_CAPABILITIES, catalog)).toThrow(
      "Missing server capability pack for catalogId 'catalog-capability:016'",
    );
  });

  it("fails loudly for duplicate and unknown server catalog capability ids", () => {
    const duplicatePacks = catalogCapabilityPacks();
    duplicatePacks[15] = {
      ...duplicatePacks[15]!,
      catalogCapabilityId: "catalog-capability:015",
    };
    const unknownPacks = catalogCapabilityPacks();
    unknownPacks.push(capabilityPack({
      catalogCapabilityId: "catalog-capability:999",
      id: "capability-pack:unknown:v1",
      status: "catalog_only",
    }));

    expect(() => buildCapabilityLibrary(AGENT_CAPABILITIES, duplicatePacks)).toThrow(
      "Duplicate server catalogCapabilityId 'catalog-capability:015'",
    );
    expect(() => buildCapabilityLibrary(AGENT_CAPABILITIES, unknownPacks)).toThrow(
      "Unknown server catalogCapabilityId 'catalog-capability:999'",
    );
  });

  it("fails loudly when static catalog ids are missing or duplicated", () => {
    const missingCatalogId = AGENT_CAPABILITIES.map((capability) => ({ ...capability }));
    delete (missingCatalogId[0] as Partial<AgentCapability>).catalogId;
    const duplicateCatalogId = AGENT_CAPABILITIES.map((capability) => ({ ...capability }));
    duplicateCatalogId[1]!.catalogId = duplicateCatalogId[0]!.catalogId;

    expect(() => buildCapabilityLibrary(missingCatalogId, catalogCapabilityPacks())).toThrow(
      "Static capability '1' is missing catalogId",
    );
    expect(() => buildCapabilityLibrary(duplicateCatalogId, catalogCapabilityPacks())).toThrow(
      "Duplicate static catalogId 'catalog-capability:001'",
    );
  });

  it("fails loudly when a static catalog id is assigned to the wrong capability", () => {
    const swappedCatalogIds = AGENT_CAPABILITIES.map((capability) => ({ ...capability }));
    const firstCatalogId = swappedCatalogIds[0]!.catalogId;
    swappedCatalogIds[0]!.catalogId = swappedCatalogIds[1]!.catalogId;
    swappedCatalogIds[1]!.catalogId = firstCatalogId;

    expect(() => buildCapabilityLibrary(swappedCatalogIds, catalogCapabilityPacks())).toThrow(
      "Static capability '1' has catalogId 'catalog-capability:002'; expected 'catalog-capability:001'",
    );
  });

  it("places the unique lead first and never duplicates it in members", () => {
    const lead = workAgent("lead", "agent-instance:lead");
    const researcher = workAgent("researcher", "agent-instance:researcher");
    const duplicateLead = workAgent("lead", "agent-instance:duplicate-lead");
    const team: WorkTeamSummary = {
      workId: "work:1",
      lead,
      members: [researcher, lead, duplicateLead],
    };

    const model = buildTeamModel(team);

    expect(model.map(({ roleKind }) => roleKind)).toEqual(["lead", "researcher"]);
    expect(model.map(({ instance }) => instance.id)).toEqual([
      "agent-instance:lead",
      "agent-instance:researcher",
    ]);
  });

  it("exposes system packs only on member details and marks them non-removable", () => {
    const systemPack = capabilityPack({
      catalogCapabilityId: null,
      id: "capability-pack:lead-coordination:v1",
      status: "executable",
    });
    const catalogPack = capabilityPack({
      catalogCapabilityId: "catalog-capability:001",
      id: "capability-pack:catalog-001:v1",
      status: "catalog_only",
    });
    const lead = workAgent("lead", "agent-instance:lead", [systemPack, catalogPack]);

    const [member] = buildTeamModel({ workId: "work:1", lead, members: [] });

    expect(member?.systemCapabilityPacks).toEqual([
      expect.objectContaining({
        id: systemPack.id,
        canUninstall: false,
      }),
    ]);
    expect(member?.systemCapabilityPacks).not.toEqual(
      expect.arrayContaining([expect.objectContaining({ id: catalogPack.id })]),
    );
  });
});
