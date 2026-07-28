export interface ContinuousLoopLogoProps {
  showWordmark?: boolean;
  size?: number;
}

export function ContinuousLoopLogo({
  showWordmark = false,
  size = 32,
}: ContinuousLoopLogoProps) {
  return (
    <span
      aria-label="PiWork"
      className="continuous-loop-logo"
      data-testid="continuous-loop-logo"
      role="img"
    >
      <svg
        aria-hidden="true"
        className="continuous-loop-logo__mark"
        focusable="false"
        height={size}
        viewBox="0 0 70 70"
        width={size}
      >
        <path d="M11 25C18 17 26 17 34 21C42 25 49 25 59 17" />
        <path d="M27 19C25 30 24 40 23 49C22 56 26 59 31 56C34 54 36 51 37 47" />
        <path d="M46 23C44 31 42 39 42 47C42 55 46 59 52 57C59 54 62 44 60 34C59 28 55 23 49 22" />
      </svg>
      {showWordmark ? (
        <span aria-hidden="true" className="continuous-loop-logo__wordmark">
          PiWork
        </span>
      ) : null}
    </span>
  );
}
