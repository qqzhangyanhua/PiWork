import "@testing-library/jest-dom/vitest";

import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { ModelProvider } from "../../app/tauriClient";
import { ProviderLogo } from "./ProviderLogo";

const providers: ModelProvider[] = [
  "openai",
  "anthropic",
  "google",
  "openrouter",
  "deepseek",
  "custom",
];

describe("ProviderLogo", () => {
  it("为每个支持的 Provider 渲染稳定的本地品牌标识", () => {
    const { container } = render(
      <div>{providers.map((provider) => <ProviderLogo key={provider} provider={provider} />)}</div>,
    );

    for (const provider of providers) {
      expect(container.querySelector(`[data-provider="${provider}"]`)).toBeInTheDocument();
    }
  });

  it("非装饰模式提供可访问名称", () => {
    render(<ProviderLogo decorative={false} provider="deepseek" />);

    expect(screen.getByRole("img", { name: "DeepSeek" })).toBeInTheDocument();
  });

  it("自定义 Provider 使用通用标识而不是供应商品牌", () => {
    const { container } = render(<ProviderLogo provider="custom" />);

    expect(container.querySelector('[data-mark="link"]')).toBeInTheDocument();
  });
});
