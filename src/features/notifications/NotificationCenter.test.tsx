import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type {
  AppNotificationSummary,
  PendingConnectorActionSummary,
} from "../../app/tauriClient";
import { i18n } from "../../i18n";
import { createMockTauriClient } from "../../test/mockTauriClient";
import { NotificationCenter } from "./NotificationCenter";

const notification: AppNotificationSummary = {
  id: "notification-1",
  category: "approval",
  connectionId: "connector-1",
  workId: "work-1",
  title: "发送邮件需要确认",
  summary: "发送给 customer@example.com",
  action: { approvalId: "approval-1" },
  readAt: null,
  expiresAt: "2026-08-22T08:00:00.000Z",
  createdAt: "2026-08-21T08:00:00.000Z",
};

const approval: PendingConnectorActionSummary = {
  id: "approval-1",
  connectionId: "connector-1",
  workId: "work-1",
  runId: "run-1",
  actionType: "send_email",
  preview: {
    to: "customer@example.com",
    subject: "合同确认",
    bodyPreview: "请确认附件中的合同内容",
  },
  status: "pending",
  expiresAt: "2026-08-22T08:00:00.000Z",
  createdAt: "2026-08-21T08:00:00.000Z",
};

describe("NotificationCenter", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("zh-CN");
  });

  it.each([
    ["允许", true],
    ["拒绝", false],
  ])("resolves a sensitive connector action with %s", async (label, approved) => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    const resolvePendingConnectorAction = vi.fn(async () => ({
      ...approval,
      status: approved ? "approved" as const : "denied" as const,
    }));
    client.listAppNotifications = vi.fn(async () => [notification]);
    client.listPendingConnectorActions = vi.fn(async () => [approval]);
    client.resolvePendingConnectorAction = resolvePendingConnectorAction;
    client.markAppNotificationRead = vi.fn(async () => undefined);

    render(<NotificationCenter client={client} onOpenConnectors={vi.fn()} />);
    await user.click(await screen.findByRole("button", { name: "通知" }));
    expect(await screen.findByText("customer@example.com")).toBeInTheDocument();
    expect(screen.getByText("合同确认")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: label }));

    expect(resolvePendingConnectorAction).toHaveBeenCalledWith("approval-1", approved);
    expect(client.markAppNotificationRead).toHaveBeenCalledWith("notification-1");
  });

  it("refreshes pending actions when the notification center opens", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.listAppNotifications = vi.fn(async () => [notification]);
    client.listPendingConnectorActions = vi
      .fn()
      .mockResolvedValueOnce([])
      .mockResolvedValue([approval]);

    render(<NotificationCenter client={client} onOpenConnectors={vi.fn()} />);
    await user.click(await screen.findByRole("button", { name: "通知" }));

    expect(await screen.findByRole("button", { name: "允许" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "拒绝" })).toBeInTheDocument();
    expect(client.listPendingConnectorActions).toHaveBeenCalledTimes(2);
  });
});
