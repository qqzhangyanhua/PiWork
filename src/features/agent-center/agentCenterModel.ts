import type {
  CapabilityPackStatus,
  CapabilityPackSummary,
  WorkAgentSummary,
  WorkTeamSummary,
} from "../../bindings";
import {
  getCatalogCapabilityId,
  type AgentCapability,
} from "./agentCapabilities";

const CAPABILITY_PACK_STATUSES: ReadonlySet<CapabilityPackStatus> = new Set([
  "catalog_only",
  "executable",
  "deprecated",
]);

export type CapabilityPackGroups = {
  catalog: CapabilityPackSummary[];
  system: CapabilityPackSummary[];
};

export type SystemCapabilityPackModel = CapabilityPackSummary & {
  canUninstall: false;
};

export type TeamMemberModel = WorkAgentSummary & {
  isLead: boolean;
  systemCapabilityPacks: SystemCapabilityPackModel[];
};

export function splitCapabilityPacks(
  packs: readonly CapabilityPackSummary[],
): CapabilityPackGroups {
  const catalog: CapabilityPackSummary[] = [];
  const system: CapabilityPackSummary[] = [];

  for (const pack of packs) {
    (pack.catalogCapabilityId === null ? system : catalog).push(pack);
  }

  return { catalog, system };
}

export function buildCapabilityLibrary(
  staticCapabilities: readonly AgentCapability[],
  catalogPacks: readonly CapabilityPackSummary[],
): AgentCapability[] {
  const staticByCatalogId = new Map<string, AgentCapability>();
  for (const capability of staticCapabilities) {
    if (!capability.catalogId) {
      throw new Error(`Static capability '${capability.id}' is missing catalogId`);
    }
    if (staticByCatalogId.has(capability.catalogId)) {
      throw new Error(`Duplicate static catalogId '${capability.catalogId}'`);
    }
    staticByCatalogId.set(capability.catalogId, capability);
  }
  for (const capability of staticCapabilities) {
    const expectedCatalogId = getCatalogCapabilityId(capability.id);
    if (capability.catalogId !== expectedCatalogId) {
      throw new Error(
        `Static capability '${capability.id}' has catalogId '${capability.catalogId}'; expected '${expectedCatalogId}'`,
      );
    }
  }

  const packByCatalogId = new Map<string, CapabilityPackSummary>();
  for (const pack of catalogPacks) {
    const catalogId = pack.catalogCapabilityId;
    if (!catalogId) {
      throw new Error(`Server capability pack '${pack.id}' is missing catalogCapabilityId`);
    }
    if (packByCatalogId.has(catalogId)) {
      throw new Error(`Duplicate server catalogCapabilityId '${catalogId}'`);
    }
    if (!staticByCatalogId.has(catalogId)) {
      throw new Error(`Unknown server catalogCapabilityId '${catalogId}'`);
    }
    if (!CAPABILITY_PACK_STATUSES.has(pack.status)) {
      throw new Error(
        `Unknown server capability pack status '${pack.status}' for '${pack.id}'`,
      );
    }
    packByCatalogId.set(catalogId, pack);
  }

  return staticCapabilities.map((capability) => {
    const pack = packByCatalogId.get(capability.catalogId);
    if (!pack) {
      throw new Error(
        `Missing server capability pack for catalogId '${capability.catalogId}'`,
      );
    }
    return {
      ...capability,
      status: pack.status,
      capabilityPackId: pack.id,
    };
  });
}

function buildMemberModel(
  member: WorkAgentSummary,
  isLead: boolean,
): TeamMemberModel {
  const { system } = splitCapabilityPacks(member.instance.definition.capabilityPacks);
  return {
    ...member,
    isLead,
    systemCapabilityPacks: system.map((pack) => ({
      ...pack,
      canUninstall: false,
    })),
  };
}

export function buildTeamModel(team: WorkTeamSummary): TeamMemberModel[] {
  const members = team.members.filter((member) =>
    member.roleKind !== "lead" && member.instance.id !== team.lead.instance.id
  );
  return [
    buildMemberModel(team.lead, true),
    ...members.map((member) => buildMemberModel(member, false)),
  ];
}
