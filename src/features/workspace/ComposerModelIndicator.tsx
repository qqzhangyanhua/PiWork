import { Zap } from "lucide-react";

const compactModelLabel = (modelLabel: string) => {
  const label = modelLabel.trim() || "Pi";
  const gptMatch = /^gpt-(.+)$/i.exec(label);
  if (!gptMatch) return label;
  return gptMatch[1]!
    .split("-")
    .map((part, index) => index === 0 ? part : `${part.charAt(0).toUpperCase()}${part.slice(1)}`)
    .join(" ");
};

export function ComposerModelIndicator({ modelLabel }: { modelLabel: string }) {
  return (
    <span className="composer-model-indicator" title={modelLabel}>
      <Zap aria-hidden="true" fill="currentColor" size={13} />
      <span>{compactModelLabel(modelLabel)}</span>
    </span>
  );
}
