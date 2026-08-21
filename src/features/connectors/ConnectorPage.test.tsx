import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type {
  EmailConnectorSummary,
  SaveEmailConnectorInput,
} from "../../app/tauriClient";
import type { WorkSummary } from "../../bindings";
import { i18n } from "../../i18n";
import { createMockTauriClient } from "../../test/mockTauriClient";
import { ConnectorPage } from "./ConnectorPage";

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
  ...overrides,
});

const work: WorkSummary = {
  id: "work-1",
  title: "客户支持",
  goal: "Process customer requests",
  rootPath: "D:\\Projects\\Support",
  permissionMode: "balanced",
  status: "idle",
  createdAt: "2026-08-21T08:00:00.000Z",
  updatedAt: "2026-08-21T08:00:00.000Z",
};

describe("ConnectorPage", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("zh-CN");
  });

  it("saves, enables, and grants an Alibaba enterprise mailbox to a Work", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const saveEmailConnector = vi.fn(async (_input: SaveEmailConnectorInput) => connector());
    const setEmailConnectorEnabled = vi.fn(async () => connector({ enabled: true }));
    const setConnectorWorkGrant = vi.fn(async () => connector({
      enabled: true,
      grantedWorkIds: [work.id],
      workGrants: [{ workId: work.id, permissions: ["metadata", "read_body", "send"] }],
    }));
    client.listEmailConnectors = vi.fn(async () => []);
    client.saveEmailConnector = saveEmailConnector;
    client.setEmailConnectorEnabled = setEmailConnectorEnabled;
    client.setConnectorWorkGrant = setConnectorWorkGrant;
    client.listEmailMetadata = vi.fn(async () => []);

    render(<ConnectorPage client={client} works={[work]} />);
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

    const workGrant = await screen.findByRole("checkbox", { name: /客户支持/u });
    await user.click(workGrant);
    expect(setConnectorWorkGrant).toHaveBeenCalledWith(
      "connector-1",
      "work-1",
      true,
      ["metadata", "read_body", "send"],
    );
    await waitFor(() => expect(workGrant).toBeChecked());
  });
});
