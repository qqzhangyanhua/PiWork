import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18n } from "../../i18n";
import { AgentCenterPage } from "./AgentCenterPage";

describe("AgentCenterPage", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("zh-CN");
  });

  it("opens with guided P0 discovery and accurate catalog statistics", () => {
    render(<AgentCenterPage onStartCapability={vi.fn()} />);

    expect(screen.getByRole("heading", { name: "智能体中心" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "首期推荐" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByText("9", { selector: ".agent-center-stat__value" })).toBeInTheDocument();
    expect(screen.getByText("96", { selector: ".agent-center-stat__value" })).toBeInTheDocument();
    expect(screen.getByText("23", { selector: ".agent-center-stat__value" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "需求到报价闭环" })).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /查看.*详情/u })).toHaveLength(23);
  });

  it("filters the complete catalog by text, domain and priority", async () => {
    const user = userEvent.setup();
    render(<AgentCenterPage onStartCapability={vi.fn()} />);

    await user.click(screen.getByRole("tab", { name: "全部能力" }));
    expect(screen.getAllByRole("button", { name: /查看.*详情/u })).toHaveLength(96);

    await user.type(screen.getByRole("searchbox", { name: "搜索能力" }), "报价");
    await user.selectOptions(screen.getByLabelText("能力域"), "quote-finance");
    await user.selectOptions(screen.getByLabelText("优先级"), "P0");

    expect(screen.getByText("找到 3 项能力")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "查看参考报价智能体详情" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "查看报价解释与谈判辅助智能体详情" })).not.toBeInTheDocument();
  });

  it("clears an empty search state", async () => {
    const user = userEvent.setup();
    render(<AgentCenterPage onStartCapability={vi.fn()} />);

    await user.click(screen.getByRole("tab", { name: "全部能力" }));
    await user.type(screen.getByRole("searchbox", { name: "搜索能力" }), "不存在的能力" );
    expect(screen.getByRole("heading", { name: "没有匹配的能力" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "清除筛选" }));
    expect(screen.getAllByRole("button", { name: /查看.*详情/u })).toHaveLength(96);
  });

  it("shows capability evidence in a keyboard-dismissable drawer and starts the selected task", async () => {
    const user = userEvent.setup();
    const onStartCapability = vi.fn();
    render(<AgentCenterPage onStartCapability={onStartCapability} />);

    await user.click(screen.getByRole("tab", { name: "全部能力" }));
    const opener = screen.getByRole("button", { name: "查看需求澄清智能体详情" });
    await user.click(opener);

    const dialog = screen.getByRole("dialog", { name: "需求澄清智能体" });
    expect(within(dialog).getByRole("heading", { name: "需求澄清智能体" })).toBeInTheDocument();
    expect(within(dialog).getByRole("heading", { name: "实现方式" })).toBeInTheDocument();
    expect(within(dialog).getByText("RAG + 动态问卷 + Schema")).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "关闭详情" })).toHaveFocus();

    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "需求澄清智能体" })).not.toBeInTheDocument();
    expect(opener).toHaveFocus();

    await user.click(opener);
    await user.click(within(screen.getByRole("dialog", { name: "需求澄清智能体" })).getByRole("button", { name: "开始使用" }));
    expect(onStartCapability).toHaveBeenCalledWith(expect.objectContaining({ id: 16, name: "需求澄清智能体" }));
  });
});
