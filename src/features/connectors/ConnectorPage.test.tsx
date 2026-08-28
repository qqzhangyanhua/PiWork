import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type {
  EmailConnectorSummary,
  SaveEmailConnectorInput,
} from "../../app/tauriClient";
import { i18n } from "../../i18n";
import { createMockTauriClient } from "../../test/mockTauriClient";
import { ConnectorPage } from "./ConnectorPage";
import openConnectorCatalog from "./openConnectorCatalog.generated.json";

const connector = (
  overrides: Partial<EmailConnectorSummary> = {},
): EmailConnectorSummary => ({
  id: "connector-1",
  displayName: "公司邮箱",
  emailAddress: "owner@example.com",
  username: "owner@example.com",
  preset: "aliyun-enterprise",
  imapHost: "imap.qiye.aliyun.com",
  imapPort: 993,
  smtpHost: "smtp.qiye.aliyun.com",
  smtpPort: 465,
  enabled: false,
  pollIntervalMinutes: 2,
  healthStatus: "healthy",
  lastErrorCode: null,
  lastCheckedAt: "2026-08-21T08:00:00.000Z",
  lastPolledAt: null,
  credentialConfigured: true,
  grantedWorkIds: [],
  workGrants: [],
  agentGrants: [],
  ...overrides,
});

describe("ConnectorPage", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("zh-CN");
  });

  it("ships the complete pinned OpenConnector discovery catalog", () => {
    expect(openConnectorCatalog.providers).toHaveLength(1444);
    expect(new Set(openConnectorCatalog.providers.map(({ service }) => service)).size).toBe(1444);
    expect(openConnectorCatalog.providers.reduce((sum, provider) => sum + provider.actionCount, 0))
      .toBe(15151);
    expect(openConnectorCatalog.providers.find(({ service }) => service === "cloudflare_mcp")?.iconUrl)
      .toBe("https://workers.cloudflare.com/favicon.ico");
  });

  it("keeps a connected mailbox unavailable until it is granted to an Agent", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const saveEmailConnector = vi.fn(async (_input: SaveEmailConnectorInput) => connector());
    const setEmailConnectorEnabled = vi.fn(async () => connector({ enabled: true }));
    const setConnectorAgentGrant = vi.fn(async () => connector({
      enabled: true,
      agentGrants: [{
        agentInstanceId: "agent-instance:piwork-lead",
        permissions: ["metadata", "read_body", "send"],
      }],
    }));
    client.listEmailConnectors = vi.fn(async () => []);
    client.saveEmailConnector = saveEmailConnector;
    client.setEmailConnectorEnabled = setEmailConnectorEnabled;
    client.setConnectorAgentGrant = setConnectorAgentGrant;
    client.listEmailMetadata = vi.fn(async () => []);

    render(<ConnectorPage client={client} />);
    await screen.findByRole("heading", { name: "接入企业邮箱" });
    await user.type(screen.getByLabelText("显示名称"), "公司邮箱");
    await user.type(screen.getByLabelText("邮箱地址"), "owner@example.com");
    await user.type(screen.getByLabelText("客户端专用密码"), "app-password");
    await user.click(screen.getByRole("button", { name: "保存" }));

    expect(saveEmailConnector).toHaveBeenCalledWith({
      id: null,
      displayName: "公司邮箱",
      emailAddress: "owner@example.com",
      username: "owner@example.com",
      password: "app-password",
      preset: "aliyun-enterprise",
      imapHost: "imap.qiye.aliyun.com",
      imapPort: 993,
      smtpHost: "smtp.qiye.aliyun.com",
      smtpPort: 465,
      pollIntervalMinutes: 2,
    });

    await user.click(await screen.findByRole("button", { name: "开启" }));
    expect(setEmailConnectorEnabled).toHaveBeenCalledWith("connector-1", true);

    expect((await screen.findAllByText("未授权")).length).toBeGreaterThan(0);
    expect(screen.queryByText("已自动提供给所有 Work 和智能体")).not.toBeInTheDocument();

    await user.click(screen.getByRole("switch", { name: "将 公司邮箱 添加给 lead" }));
    expect(setConnectorAgentGrant).toHaveBeenCalledWith(
      "connector-1",
      "agent-instance:piwork-lead",
      true,
      ["metadata", "read_body", "send"],
    );
    expect(await screen.findByText("Agent 授权已更新")).toBeInTheDocument();
  }, 12_000);

  it("searches the bundled catalog without creating a connection", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.listEmailConnectors = vi.fn(async () => []);

    render(<ConnectorPage client={client} />);
    await screen.findByRole("heading", { name: "接入企业邮箱" });
    await user.type(screen.getByRole("searchbox", { name: "搜索连接器" }), "GitHub");

    expect(await screen.findByText("GitHub")).toBeInTheDocument();
    const githubCard = screen.getByRole("button", { name: /GitHub/ });
    expect(githubCard).toHaveClass("connector-catalog-card");
    expect(githubCard.querySelector('[data-icon-source="brand"]')).toBeInTheDocument();
    expect(screen.getByText("仅加载目录元数据")).toBeInTheDocument();
    expect(client.saveEmailConnector).toBeUndefined();
  });

  it("loads and displays a cached homepage icon for long-tail providers", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.listEmailConnectors = vi.fn(async () => []);
    client.resolveConnectorIcon = vi.fn(async () =>
      "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAAB",
    );

    render(<ConnectorPage client={client} />);
    await screen.findByRole("heading", { name: "接入企业邮箱" });
    await user.type(screen.getByRole("searchbox", { name: "搜索连接器" }), "17TRACK");

    const card = await screen.findByRole("button", { name: /17TRACK/ });
    await waitFor(() => {
      expect(card.querySelector('[data-icon-source="cached"]')).toBeInTheDocument();
    });
    expect(client.resolveConnectorIcon).toHaveBeenCalledWith(
      "17track",
      "https://www.17track.net/",
      null,
    );
  });

  it("explains an IMAP authentication failure instead of showing a generic connection error", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.listEmailConnectors = vi.fn(async () => []);
    client.testEmailConnector = vi.fn(async () => ({
      imapOk: false,
      smtpOk: false,
      errorCode: "imap_authentication",
    }));

    render(<ConnectorPage client={client} />);
    await screen.findByRole("heading", { name: "接入企业邮箱" });
    await user.click(screen.getByRole("button", { name: "测试连接" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "IMAP 登录失败，请确认邮箱账号和客户端专用密码，并检查管理员是否已开启 IMAP 服务。",
    );
  });
});
