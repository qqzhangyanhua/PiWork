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
  validateCapabilityPackInventory,
} from "./agentCenterModel";

const SYSTEM_CAPABILITY_PACK_IDS = [
  "capability-pack:lead-coordination:v1",
  "capability-pack:source-research:v1",
  "capability-pack:engineering-execution:v1",
  "capability-pack:independent-review:v1",
] as const;

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
    const catalogCapabilityId = `catalog-capability:${sequence}`;
    return capabilityPack({
      catalogCapabilityId,
      id: catalogCapabilityId,
      status: "catalog_only",
    });
  });
}

function systemCapabilityPacks(): CapabilityPackSummary[] {
  return SYSTEM_CAPABILITY_PACK_IDS.map((id) => capabilityPack({
    catalogCapabilityId: null,
    id,
    status: "executable",
  }));
}

function capabilityPacksFixture(): CapabilityPackSummary[] {
  return [...catalogCapabilityPacks(), ...systemCapabilityPacks()];
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
  it("splits business catalog packs from system packs without rewriting known status", () => {
    const catalog = capabilityPack({
      catalogCapabilityId: "catalog-capability:001",
      id: "catalog-capability:001",
      status: "deprecated",
    });
    const executableSystemPack = capabilityPack({
      catalogCapabilityId: null,
      id: "capability-pack:lead-coordination:v1",
      status: "executable",
    });
    const packs = splitCapabilityPacks([executableSystemPack, catalog]);

    expect(packs.catalog).toEqual([catalog]);
    expect(packs.system).toEqual([executableSystemPack]);
    expect(packs.catalog[0]?.status).toBe("deprecated");
  });

  it.each([
    { catalogCapabilityId: null, id: "capability-pack:future-system:v1", kind: "system" },
    { catalogCapabilityId: "catalog-capability:001", id: "catalog-capability:001", kind: "catalog" },
  ])("fails loudly for an unknown $kind pack status", ({ catalogCapabilityId, id }) => {
    const pack = {
      ...capabilityPack({ catalogCapabilityId, id, status: "deprecated" }),
      status: "future_server_status",
    } as unknown as CapabilityPackSummary;

    expect(() => splitCapabilityPacks([pack])).toThrow(
      `Unknown server capability pack status 'future_server_status' for '${id}'`,
    );
  });

  it("splits the canonical inventory into 96 catalog and 4 system packs", () => {
    const { catalog, system } = splitCapabilityPacks(capabilityPacksFixture());

    expect(catalog).toHaveLength(96);
    expect(system).toHaveLength(4);
    expect(system.map(({ id }) => id)).toEqual(SYSTEM_CAPABILITY_PACK_IDS);
  });

  it("fails inventory validation for missing or duplicate stable system packs", () => {
    const missingSystemPack = capabilityPacksFixture().filter(
      ({ id }) => id !== "capability-pack:source-research:v1",
    );
    const duplicateSystemPack = capabilityPack({
      catalogCapabilityId: null,
      id: "capability-pack:lead-coordination:v1",
      status: "executable",
    });

    expect(() => validateCapabilityPackInventory(missingSystemPack)).toThrow(
      "Missing system capability pack 'capability-pack:source-research:v1'",
    );
    expect(() => validateCapabilityPackInventory([
      ...capabilityPacksFixture(),
      duplicateSystemPack,
    ])).toThrow(
      "Duplicate system capability pack 'capability-pack:lead-coordination:v1'",
    );
  });

  it("fails inventory validation unless exactly 96 catalog packs are present", () => {
    const missingCatalogPack = capabilityPacksFixture().filter(
      ({ id }) => id !== "catalog-capability:096",
    );

    expect(() => validateCapabilityPackInventory(missingCatalogPack)).toThrow(
      "Expected 96 catalog capability packs, received 95",
    );
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
      capabilityPackId: "catalog-capability:016",
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

  it("fails loudly when a catalog pack id differs from its catalog capability id", () => {
    const packs = catalogCapabilityPacks();
    packs[15] = {
      ...packs[15]!,
      id: "catalog-capability:999",
    };
    const message = "Catalog capability pack id 'catalog-capability:999' must match catalogCapabilityId 'catalog-capability:016'";

    expect(() => splitCapabilityPacks([packs[15]!])).toThrow(message);
    expect(() => buildCapabilityLibrary(AGENT_CAPABILITIES, packs)).toThrow(message);
  });

  it("fails closed for an unknown server catalog status", () => {
    const packs = catalogCapabilityPacks();
    packs[15] = {
      ...packs[15]!,
      status: "future_server_status",
    } as unknown as CapabilityPackSummary;

    expect(() => buildCapabilityLibrary(AGENT_CAPABILITIES, packs)).toThrow(
      "Unknown server capability pack status 'future_server_status' for 'catalog-capability:016'",
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
      id: "catalog-capability:001",
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
