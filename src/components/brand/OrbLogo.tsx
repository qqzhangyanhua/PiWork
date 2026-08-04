import type { CSSProperties } from "react";

import referenceUrl from "../../assets/piwork-home-reference.png";

export function OrbLogo({ size = 32 }: { size?: number }) {
  return (
    <span
      aria-label="PiWork"
      className="orb-logo"
      data-testid="orb-logo"
      role="img"
      style={
        {
          "--orb-size": `${size}px`,
          "--orb-reference": `url(${referenceUrl})`,
        } as CSSProperties
      }
    >
      <span aria-hidden="true" className="orb-logo__image" />
    </span>
  );
}
