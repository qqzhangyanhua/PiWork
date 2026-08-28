import { readdir, readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

const [, , catalogDirectoryArgument, revisionArgument = "unknown"] = process.argv;

if (!catalogDirectoryArgument) {
  throw new Error(
    "Usage: node scripts/sync-open-connector-catalog.mjs <open-connector/catalog/apps> [revision]",
  );
}

const catalogDirectory = resolve(catalogDirectoryArgument);
const outputPath = resolve(
  "src/features/connectors/openConnectorCatalog.generated.json",
);
const files = (await readdir(catalogDirectory))
  .filter((file) => file.endsWith(".json"))
  .sort((left, right) => left.localeCompare(right));

const providers = [];
for (const file of files) {
  const provider = JSON.parse(
    await readFile(resolve(catalogDirectory, file), "utf8"),
  );
  providers.push({
    service: provider.service,
    displayName: provider.displayName,
    categories: provider.categories ?? [],
    authTypes: provider.authTypes ?? [],
    homepageUrl: provider.homepageUrl ?? null,
    iconUrl: provider.iconUrl ?? null,
    actionCount: provider.actions?.length ?? 0,
    scenario: provider.scenario ?? "other",
  });
}

await writeFile(
  outputPath,
  `${JSON.stringify(
    {
      source: "oomol-lab/open-connector",
      revision: revisionArgument,
      providers,
    },
    null,
    2,
  )}\n`,
);

console.log(`Synced ${providers.length} OpenConnector providers to ${outputPath}.`);
