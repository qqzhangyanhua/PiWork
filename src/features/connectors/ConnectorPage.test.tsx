import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type {
  EmailConnectorSummary,
  SaveEmailConnectorInput,
} from "../../app/tauriClient";
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

describe("ConnectorPage", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("zh-CN");
  });

  it("saves and enables an Alibaba enterprise mailbox for every Agent", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const saveEmailConnector = vi.fn(async (_input: SaveEmailConnectorInput) => connector());
    const setEmailConnectorEnabled = vi.fn(async () => connector({ enabled: true }));
    client.listEmailConnectors = vi.fn(async () => []);
    client.saveEmailConnector = saveEmailConnector;
    client.setEmailConnectorEnabled = setEmailConnectorEnabled;
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

    expect(await screen.findByText("已自动提供给所有 Work 和智能体")).toBeInTheDocument();
    expect(screen.getByText("读取正文")).toBeInTheDocument();
    expect(screen.getByText("发送邮件")).toBeInTheDocument();
    expect(screen.queryByText("读取正文 · 需确认")).not.toBeInTheDocument();
    expect(screen.queryByText("发送邮件 · 需确认")).not.toBeInTheDocument();
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
