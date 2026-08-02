import {
  createElement,
  useRef,
  type HTMLAttributes,
  type ReactNode,
} from "react";

import { gsap, useGSAP } from "../../motion/gsap";

export type AnimatedSurfaceStateProps = HTMLAttributes<HTMLElement> & {
  as?: "main" | "section";
  children?: ReactNode;
  variant: "loading" | "error";
};

export function AnimatedSurfaceState({
  as = "section",
  children,
  variant,
  ...props
}: AnimatedSurfaceStateProps) {
  const rootRef = useRef<HTMLElement>(null);

  useGSAP(
    () => {
      const root = rootRef.current;
      if (!root) return;

      const animate = () => {
        gsap.fromTo(
          root,
          { opacity: 0, y: variant === "error" ? 6 : 4 },
          {
            opacity: 1,
            duration: variant === "error" ? 0.18 : 0.28,
            ease: "power2.out",
            y: 0,
          },
        );

        if (variant === "loading") {
          const lines = Array.from(
            root.querySelectorAll<HTMLElement>("[data-motion-line]"),
          );
          if (lines.length > 0) {
            gsap.fromTo(
              lines,
              {
                autoAlpha: 0.46,
                scaleX: 0.72,
                transformOrigin: "left center",
              },
              {
                autoAlpha: 1,
                duration: 0.4,
                ease: "power2.out",
                scaleX: 1,
                stagger: 0.06,
              },
            );
            gsap.to(lines, {
              autoAlpha: 0.58,
              delay: 0.46,
              duration: 1.25,
              ease: "sine.inOut",
              repeat: -1,
              stagger: 0.12,
              yoyo: true,
            });
          }

          const mark = root.querySelector<SVGElement>(
            ".continuous-loop-logo__mark",
          );
          if (mark) {
            gsap.to(mark, {
              autoAlpha: 0.76,
              duration: 1.4,
              ease: "sine.inOut",
              repeat: -1,
              scale: 1.025,
              transformOrigin: "50% 50%",
              yoyo: true,
            });
          }
        }
      };

      if (typeof window.matchMedia !== "function") {
        return animate();
      }
      const media = gsap.matchMedia();
      media.add("(prefers-reduced-motion: no-preference)", animate);
      return () => media.revert();
    },
    { dependencies: [variant], scope: rootRef, revertOnUpdate: true },
  );

  return createElement(
    as,
    {
      ...props,
      "data-motion-state": variant,
      ref: rootRef,
    },
    children,
  );
}
