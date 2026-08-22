import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { WorkSummary } from "../../bindings";
import { i18n } from "../../i18n";
import { AllWorks } from "./AllWorks";

const work = (id: string, title: string, status: WorkSummary["status"]): WorkSummary => ({
  id,
  title,
  goal: title,
  rootPath: `D:\\workspace\\${id}`,
  permissionMode: "balanced",
  status,
  createdAt: "2026-07-28T08:00:00.000Z",
  updatedAt: "2026-07-28T08:00:00.000Z",
});

describe("AllWorks", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("zh-CN");
  });

  it("uses the Linear-style view tabs to filter the authoritative Work list", async () => {
    const user = userEvent.setup();
    const onWorkRestore = vi.fn();
    render(
      <AllWorks
        onWorkRestore={onWorkRestore}
        onWorkSelected={vi.fn()}
        works={[
          work("active", "正在执行", "running"),
          work("draft", "等待继续", "draft"),
          work("done", "已经完成", "completed"),
          work("archived", "旧对话", "archived"),
        ]}
      />,
    );

    const tabs = screen.getByRole("tablist", { name: "对话视图" });
    expect(within(tabs).getByRole("tab", { name: "全部" })).toHaveAttribute("aria-selected", "true");

    await user.click(within(tabs).getByRole("tab", { name: "进行中" }));
    expect(screen.getByRole("button", { name: /正在执行/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /等待继续/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /已经完成/ })).not.toBeInTheDocument();

    await user.click(within(tabs).getByRole("tab", { name: "已完成" }));
    expect(screen.getByRole("button", { name: /已经完成/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /正在执行/ })).not.toBeInTheDocument();

    await user.click(within(tabs).getByRole("tab", { name: "已归档" }));
    await user.click(screen.getByRole("button", { name: "恢复对话“旧对话”" }));
    expect(onWorkRestore).toHaveBeenCalledWith(expect.objectContaining({ id: "archived" }));
  });
});
