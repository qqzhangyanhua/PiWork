import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { ExtensionSummary } from "../../app/tauriClient";
import { i18n } from "../../i18n";
import { createMockTauriClient } from "../../test/mockTauriClient";
import { ExtensionMarketplacePage } from "./ExtensionMarketplacePage";

const webAccess: ExtensionSummary = {
  packageId: "pi-web-access",
  displayName: "Pi Web Access",
  description: "Search and fetch web content",
  publisher: "Pi",
  trustTier: "builtin",
  sourceKind: "bundled",
  installedVersion: "0.24.0",
  latestVersion: "0.24.0",
  lifecycleStatus: "installed",
  builtin: true,
  manifest: { tools: ["web_search", "fetch_url"] },
  permissions: { network: true, filesystem: "runtime-cache", subprocess: false },
  enabledAgentIds: [],
};

describe("ExtensionMarketplacePage", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("zh-CN");
  });

  it("shows the bundled web plugin and grants it to an agent", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const agents = await client.listAgentInstances();
    const lead = agents.find(({ definition }) => definition.roleKind === "lead")!;
    const listExtensions = vi.fn(async () => [webAccess]);
    const setExtensionAgentEnabled = vi.fn(async () => ({
      ...webAccess,
      enabledAgentIds: [lead.id],
    }));
    client.listExtensions = listExtensions;
    client.setExtensionAgentEnabled = setExtensionAgentEnabled;

    render(<ExtensionMarketplacePage client={client} />);

    const extension = await screen.findByRole("article");
    expect(within(extension).getByText("Pi Web Access")).toBeInTheDocument();
    expect(within(extension).getByText("PiWork 内置")).toBeInTheDocument();
    expect(within(extension).getByText("web_search")).toBeInTheDocument();

    await user.click(within(extension).getByRole("checkbox", { name: /lead/u }));

    expect(setExtensionAgentEnabled).toHaveBeenCalledWith(
      "pi-web-access",
      lead.id,
      true,
    );
    await waitFor(() => expect(
      within(extension).getByRole("checkbox", { name: /lead/u }),
    ).toBeChecked());
  });

  it("searches npm pi-package resources as review-only community results", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const searchCommunityExtensions = vi.fn(async () => [{
      packageId: "pi-community-search",
      version: "1.2.3",
      description: "Community search tools",
      publisher: "community-author",
      npmUrl: "https://www.npmjs.com/package/pi-community-search",
      score: 0.9,
      executable: false as const,
    }]);
    client.listExtensions = vi.fn(async () => [webAccess]);
    client.searchCommunityExtensions = searchCommunityExtensions;

    render(<ExtensionMarketplacePage client={client} />);
    await screen.findByText("Pi Web Access");
    await user.click(screen.getByRole("tab", { name: "社区搜索" }));
    await user.type(screen.getByPlaceholderText("插件名称或关键词"), "search");
    await user.click(screen.getByRole("button", { name: "搜索" }));

    expect(searchCommunityExtensions).toHaveBeenCalledWith("search");
    expect(await screen.findByText("pi-community-search")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "等待安全审核" })).toBeDisabled();
  });
});
