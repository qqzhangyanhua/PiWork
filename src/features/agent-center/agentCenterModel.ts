import type {
  CapabilityPackStatus,
  CapabilityPackSummary,
  WorkAgentSummary,
  WorkTeamSummary,
} from "../../bindings";
import {
  getCatalogCapabilityId,
  type CapabilityLibraryItem,
  type CapabilityLibrarySource,
  type StaticAgentCapability,
} from "./agentCapabilities";

const CAPABILITY_PACK_STATUSES: ReadonlySet<CapabilityPackStatus> = new Set([
  "catalog_only",
  "executable",
  "deprecated",
]);

const SYSTEM_CAPABILITY_PACK_IDS = [
  "capability-pack:lead-coordination:v1",
  "capability-pack:source-research:v1",
  "capability-pack:engineering-execution:v1",
  "capability-pack:independent-review:v1",
] as const;

const CATALOG_CAPABILITY_IDS: ReadonlyArray<string> = Array.from(
  { length: 96 },
  (_, index) => getCatalogCapabilityId(index + 1),
);

const CATALOG_CAPABILITY_ID_SET: ReadonlySet<string> = new Set(
  CATALOG_CAPABILITY_IDS,
);

const SYSTEM_CAPABILITY_PACK_ID_SET: ReadonlySet<string> = new Set(
  SYSTEM_CAPABILITY_PACK_IDS,
);

function assertKnownCapabilityPackStatus(pack: CapabilityPackSummary): void {
  if (!CAPABILITY_PACK_STATUSES.has(pack.status)) {
    throw new Error(
      `Unknown server capability pack status '${pack.status}' for '${pack.id}'`,
    );
  }
}

function assertCatalogCapabilityPackIdentity(pack: CapabilityPackSummary): void {
  const catalogId = pack.catalogCapabilityId;
  if (!catalogId) {
    throw new Error(`Server capability pack '${pack.id}' is missing catalogCapabilityId`);
  }
  if (pack.id !== catalogId) {
    throw new Error(
      `Catalog capability pack id '${pack.id}' must match catalogCapabilityId '${catalogId}'`,
    );
  }
}

function isCatalogCapabilityId(
  value: unknown,
): value is StaticAgentCapability["catalogId"] {
  return typeof value === "string" && /^catalog-capability:\d{3}$/.test(value);
}

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
    assertKnownCapabilityPackStatus(pack);
    if (pack.catalogCapabilityId === null) {
      system.push(pack);
    } else {
      assertCatalogCapabilityPackIdentity(pack);
      catalog.push(pack);
    }
  }

  return { catalog, system };
}

export function validateCapabilityPackInventory(
  packs: readonly CapabilityPackSummary[],
): CapabilityPackGroups {
  const groups = splitCapabilityPacks(packs);

  const catalogIds = new Set<string>();
  for (const pack of groups.catalog) {
    const catalogId = pack.catalogCapabilityId;
    if (!catalogId) {
      throw new Error(`Server capability pack '${pack.id}' is missing catalogCapabilityId`);
    }
    if (!CATALOG_CAPABILITY_ID_SET.has(catalogId)) {
      throw new Error(`Unknown catalog capability pack '${catalogId}'`);
    }
    if (catalogIds.has(catalogId)) {
      throw new Error(`Duplicate catalog capability pack '${catalogId}'`);
    }
    catalogIds.add(catalogId);
  }
  for (const id of CATALOG_CAPABILITY_IDS) {
    if (!catalogIds.has(id)) {
      throw new Error(`Missing catalog capability pack '${id}'`);
    }
  }

  const systemIds = new Set<string>();
  for (const pack of groups.system) {
    if (!SYSTEM_CAPABILITY_PACK_ID_SET.has(pack.id)) {
      throw new Error(`Unknown system capability pack '${pack.id}'`);
    }
    if (systemIds.has(pack.id)) {
      throw new Error(`Duplicate system capability pack '${pack.id}'`);
    }
    systemIds.add(pack.id);
  }
  for (const id of SYSTEM_CAPABILITY_PACK_IDS) {
    if (!systemIds.has(id)) {
      throw new Error(`Missing system capability pack '${id}'`);
    }
  }

  return groups;
}

export function buildCapabilityLibrary(
  staticCapabilities: readonly CapabilityLibrarySource[],
  catalogPacks: readonly CapabilityPackSummary[],
): ReadonlyArray<CapabilityLibraryItem> {
  const validatedCapabilities: StaticAgentCapability[] = [];
  const staticByCatalogId = new Map<string, StaticAgentCapability>();
  for (const source of staticCapabilities) {
    const catalogId = source.catalogId;
    if (catalogId === undefined || catalogId === null || catalogId === "") {
      throw new Error(`Static capability '${source.id}' is missing catalogId`);
    }
    if (!isCatalogCapabilityId(catalogId)) {
      throw new Error(
        `Static capability '${source.id}' has malformed catalogId '${String(catalogId)}'`,
      );
    }
    const capability: StaticAgentCapability = {
      ...source,
      catalogId,
    };
    if (staticByCatalogId.has(capability.catalogId)) {
      throw new Error(`Duplicate static catalogId '${capability.catalogId}'`);
    }
    staticByCatalogId.set(capability.catalogId, capability);
    validatedCapabilities.push(capability);
  }
  for (const capability of validatedCapabilities) {
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
    assertCatalogCapabilityPackIdentity(pack);
    assertKnownCapabilityPackStatus(pack);
    packByCatalogId.set(catalogId, pack);
  }

  return validatedCapabilities.map((capability) => {
    const pack = packByCatalogId.get(capability.catalogId);
    if (!pack) {
      throw new Error(
        `Missing server capability pack for catalogId '${capability.catalogId}'`,
      );
    }
    return {
      ...capability,
      audiences: [...capability.audiences],
      outputs: [...capability.outputs],
      suggestedInputs: [...capability.suggestedInputs],
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
    member.instance.id !== team.lead.instance.id
  );
  return [
    buildMemberModel(team.lead, true),
    ...members.map((member) => buildMemberModel(member, false)),
  ];
}
