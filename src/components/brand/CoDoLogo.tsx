import type { CSSProperties } from "react";

import markUrl from "../../../assets/codo_mark.png";
import signatureUrl from "../../../assets/codo_signature.png";
import signatureDarkUrl from "../../../assets/codo_signature_dark.png";
import wordmarkUrl from "../../../assets/codo_wordmark.png";
import wordmarkDarkUrl from "../../../assets/codo_wordmark_dark.png";

export interface CoDoLogoProps {
  showWordmark?: boolean;
  size?: number;
  variant?: "default" | "signature";
}

export function CoDoLogo({ showWordmark = false, size = 32, variant = "default" }: CoDoLogoProps) {
  const className = `codo-logo${variant === "signature" ? " codo-logo--signature" : ""}`;

  return (
    <span
      aria-label="CoDo"
      className={className}
      data-testid="codo-logo"
      role="img"
      style={{ "--codo-logo-size": `${size}px` } as CSSProperties}
    >
      {variant === "signature" ? (
        <picture aria-hidden="true" className="codo-logo__signature">
          <source media="(prefers-color-scheme: dark)" srcSet={signatureDarkUrl} />
          <img alt="" src={signatureUrl} />
        </picture>
      ) : (
        <>
          <img alt="" aria-hidden="true" className="codo-logo__mark" src={markUrl} />
          {showWordmark ? (
            <picture aria-hidden="true" className="codo-logo__wordmark">
              <source media="(prefers-color-scheme: dark)" srcSet={wordmarkDarkUrl} />
              <img alt="" src={wordmarkUrl} />
            </picture>
          ) : null}
        </>
      )}
    </span>
  );
}
