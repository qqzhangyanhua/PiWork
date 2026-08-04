import "@testing-library/jest-dom/vitest";

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { BrandSelect, type BrandSelectOption } from "./BrandSelect";

const options: BrandSelectOption[] = [
  { value: "openai", label: "OpenAI", provider: "openai" },
  { value: "anthropic", label: "Anthropic", provider: "anthropic" },
  { value: "deepseek", label: "DeepSeek", provider: "deepseek", badge: "推荐" },
];

describe("BrandSelect", () => {
  it("在触发器和选项中显示 Provider Logo", async () => {
    const user = userEvent.setup();
    const { container } = render(
      <BrandSelect id="provider" label="Provider" value="openai" options={options} onChange={vi.fn()} />,
    );

    expect(screen.getByRole("combobox", { name: "Provider" })).toHaveAttribute("aria-expanded", "false");
    expect(container.querySelector('[data-provider="openai"]')).toBeInTheDocument();

    await user.click(screen.getByRole("combobox", { name: "Provider" }));

    expect(screen.getByRole("listbox", { name: "Provider" })).toBeInTheDocument();
    expect(container.querySelectorAll(".brand-select__option .provider-logo")).toHaveLength(3);
  });

  it("支持键盘定位、选择并把焦点返回触发器", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<BrandSelect id="provider" label="Provider" value="openai" options={options} onChange={onChange} />);
    const trigger = screen.getByRole("combobox", { name: "Provider" });

    trigger.focus();
    await user.keyboard("{ArrowDown}{End}{Enter}");

    expect(onChange).toHaveBeenCalledWith("deepseek");
    expect(trigger).toHaveFocus();
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
  });

  it("Escape 与外部点击会关闭下拉框", async () => {
    const user = userEvent.setup();
    render(
      <div>
        <BrandSelect id="provider" label="Provider" value="openai" options={options} onChange={vi.fn()} />
        <button type="button">外部按钮</button>
      </div>,
    );
    const trigger = screen.getByRole("combobox", { name: "Provider" });

    await user.click(trigger);
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();

    await user.click(trigger);
    await user.click(screen.getByRole("button", { name: "外部按钮" }));
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
  });
});
