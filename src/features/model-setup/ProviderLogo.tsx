import type { ModelProvider } from "../../app/tauriClient";

const providerNames: Record<ModelProvider, string> = {
  openai: "OpenAI",
  anthropic: "Anthropic",
  google: "Google Gemini",
  openrouter: "OpenRouter",
  deepseek: "DeepSeek",
  custom: "OpenAI-compatible",
};

function OpenAiMark() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <g fill="none" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth="1.8">
        <path d="M12 3.2a4.4 4.4 0 0 1 4.2 3.05 4.4 4.4 0 0 1 2.55 7.82 4.4 4.4 0 0 1-6.74 5.3 4.4 4.4 0 0 1-6.76-5.3A4.4 4.4 0 0 1 7.8 6.25 4.4 4.4 0 0 1 12 3.2Z" />
        <path d="m8.05 6.45 7.84 4.5v5.1M5.6 13.85l7.83-4.5 4.42 2.55M10.58 19.4v-9l-4.42-2.55M18.4 10.15l-7.83 4.5-4.42-2.55M13.42 4.6v9l4.42 2.55" />
      </g>
    </svg>
  );
}

function AnthropicMark() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path fill="currentColor" d="M8.58 4h2.58l5.77 16h-2.62l-1.38-4H6.77l-1.4 4H2.75L8.58 4Zm-.99 9.62h4.52L9.87 7.04l-2.28 6.58ZM18.08 4h2.42v16h-2.42V4Z" />
    </svg>
  );
}

function GoogleMark() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <defs>
        <linearGradient id="pw-gemini-a" x1="5" y1="19" x2="19" y2="5" gradientUnits="userSpaceOnUse">
          <stop stopColor="#1aa260" />
          <stop offset=".34" stopColor="#4285f4" />
          <stop offset=".68" stopColor="#a142f4" />
          <stop offset="1" stopColor="#fbbc04" />
        </linearGradient>
      </defs>
      <path fill="url(#pw-gemini-a)" d="M12 2.5c.78 5.14 4.36 8.72 9.5 9.5-5.14.78-8.72 4.36-9.5 9.5-.78-5.14-4.36-8.72-9.5-9.5 5.14-.78 8.72-4.36 9.5-9.5Z" />
    </svg>
  );
}

function OpenRouterMark() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path fill="currentColor" d="M3 7.25h10.2l-2.1-2.1 1.42-1.42 4.52 4.52-4.52 4.52-1.42-1.42 2.1-2.1H3v-2Zm18 9.5H10.8l2.1 2.1-1.42 1.42-4.52-4.52 4.52-4.52 1.42 1.42-2.1 2.1H21v2Z" />
    </svg>
  );
}

function DeepSeekMark() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path fill="currentColor" d="M20.85 7.15c-1.4.12-2.65.72-3.6 1.66-1.53-1.28-3.5-2.06-5.66-2.06-4.06 0-7.46 2.74-8.49 6.47a6.2 6.2 0 0 0 6.02 4.72c3.8 0 6.68-1.85 8.38-4.45 1.43-.04 2.73-.64 3.66-1.6-.4-.16-1.22-.57-1.68-1.32.9-.72 1.46-1.92 1.37-3.42ZM8.22 15.2a1.25 1.25 0 1 1 0-2.5 1.25 1.25 0 0 1 0 2.5Zm5.6-1.2c-1.2 1.35-2.64 2.15-4.2 2.36 2.23-1.07 3.46-2.48 4.16-4.4.55.63.62 1.37.04 2.04Z" />
    </svg>
  );
}

function LinkMark() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true" data-mark="link">
      <path d="m9.5 14.5 5-5M7.8 17.8l-1.1 1.1a3.25 3.25 0 0 1-4.6-4.6l3.1-3.1a3.25 3.25 0 0 1 4.6 0M16.2 6.2l1.1-1.1a3.25 3.25 0 1 1 4.6 4.6l-3.1 3.1a3.25 3.25 0 0 1-4.6 0" fill="none" stroke="currentColor" strokeLinecap="round" strokeWidth="1.8" />
    </svg>
  );
}

const marks: Record<ModelProvider, () => React.JSX.Element> = {
  openai: OpenAiMark,
  anthropic: AnthropicMark,
  google: GoogleMark,
  openrouter: OpenRouterMark,
  deepseek: DeepSeekMark,
  custom: LinkMark,
};

export function ProviderLogo({
  provider,
  size = 20,
  decorative = true,
}: {
  provider: ModelProvider;
  size?: number;
  decorative?: boolean;
}) {
  const Mark = marks[provider];
  return (
    <span
      aria-hidden={decorative || undefined}
      aria-label={decorative ? undefined : providerNames[provider]}
      className="provider-logo"
      data-provider={provider}
      role={decorative ? undefined : "img"}
      style={{ "--provider-logo-size": `${size}px` } as React.CSSProperties}
    >
      <Mark />
    </span>
  );
}
