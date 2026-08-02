import {
  Check,
  ChevronDown,
  CircleAlert,
  LoaderCircle,
} from "lucide-react";
import {
  useEffect,
  useId,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { useTranslation } from "react-i18next";

import type { WorkEventEnvelope } from "../../bindings";
import { gsap, useGSAP } from "../../motion/gsap";
import {
  buildExecutionProgress,
  type ExecutionProgressModel,
} from "./executionProgress";

export type ExecutionProgressCardProps = {
  events: WorkEventEnvelope[];
  children?: ReactNode;
};

const runMotion = (create: () => gsap.core.Animation | void) => {
  const start = () => {
    const animation = create();
    return () => animation?.kill();
  };
  if (typeof window.matchMedia !== "function") {
    return start();
  }
  const media = gsap.matchMedia();
  media.add("(prefers-reduced-motion: no-preference)", start);
  return () => media.revert();
};

const summaryKey = (progress: ExecutionProgressModel) => {
  if (progress.status === "preparing") return "progress.preparing";
  if (progress.status === "running") return "progress.running";
  if (progress.status === "failed") return "progress.failed";
  return "progress.completed";
};

export function ExecutionProgressCard({
  events,
  children,
}: ExecutionProgressCardProps) {
  const { t } = useTranslation();
  const progress = buildExecutionProgress(events);
  const detailsId = useId();
  const rootRef = useRef<HTMLElement>(null);
  const previousStatusRef = useRef<ExecutionProgressModel["status"] | null>(
    null,
  );
  const previousPhaseRef = useRef(progress.currentPhase);
  const [expanded, setExpanded] = useState(progress.status !== "completed");

  useEffect(() => {
    if (progress.status === "failed") setExpanded(true);
    if (progress.status === "completed") setExpanded(false);
  }, [progress.status]);

  useGSAP(
    () => {
      const root = rootRef.current;
      if (!root) return;
      const previousStatus = previousStatusRef.current;
      const previousPhase = previousPhaseRef.current;
      const cleanups: Array<() => void> = [];

      if (previousStatus === null && progress.status !== "completed") {
        cleanups.push(
          runMotion(() =>
            gsap.fromTo(
              root,
              { autoAlpha: 0, y: 8 },
              { autoAlpha: 1, duration: 0.28, ease: "power2.out", y: 0 },
            ),
          ),
        );
      }

      if (
        previousPhase !== progress.currentPhase &&
        (progress.status === "running" || progress.status === "preparing")
      ) {
        const activePhase = root.querySelector(
          `[data-phase="${progress.currentPhase}"]`,
        );
        if (activePhase) {
          cleanups.push(
            runMotion(() =>
              gsap.fromTo(
                activePhase,
                { autoAlpha: 0.62, x: -4 },
                {
                  autoAlpha: 1,
                  duration: 0.22,
                  ease: "power1.out",
                  x: 0,
                },
              ),
            ),
          );
        }
      }

      if (previousStatus && previousStatus !== progress.status) {
        const icon = root.querySelector("[data-progress-icon]");
        if (progress.status === "completed" && icon) {
          cleanups.push(
            runMotion(() =>
              gsap.fromTo(
                icon,
                { autoAlpha: 0, scale: 0.72 },
                {
                  autoAlpha: 1,
                  duration: 0.32,
                  ease: "back.out(1.2)",
                  scale: 1,
                },
              ),
            ),
          );
        }
        if (progress.status === "failed") {
          cleanups.push(
            runMotion(() =>
              gsap.fromTo(
                root,
                { x: -2 },
                { duration: 0.18, ease: "power1.out", x: 0 },
              ),
            ),
          );
        }
      }

      previousStatusRef.current = progress.status;
      previousPhaseRef.current = progress.currentPhase;
      return () => cleanups.forEach((cleanup) => cleanup());
    },
    {
      dependencies: [progress.currentPhase, progress.status],
      scope: rootRef,
      revertOnUpdate: true,
    },
  );

  const Icon =
    progress.status === "completed"
      ? Check
      : progress.status === "failed"
        ? CircleAlert
        : LoaderCircle;
  const summary = t(summaryKey(progress), { count: progress.toolCount });

  return (
    <article
      aria-label={t("progress.label")}
      aria-live={
        progress.status === "preparing" || progress.status === "running"
          ? "polite"
          : undefined
      }
      className="execution-progress"
      data-status={progress.status}
      ref={rootRef}
      role="status"
    >
      <button
        aria-controls={detailsId}
        aria-expanded={expanded}
        aria-label={t(expanded ? "progress.collapse" : "progress.expand")}
        className="execution-progress__summary"
        onClick={() => setExpanded((current) => !current)}
        type="button"
      >
        <span className="execution-progress__icon" data-progress-icon>
          <Icon aria-hidden="true" />
        </span>
        <span className="execution-progress__headline">
          <strong>{summary}</strong>
          {progress.failedToolCount > 0 && progress.status === "completed" ? (
            <small>
              {t("progress.recovered", { count: progress.failedToolCount })}
            </small>
          ) : null}
        </span>
        <span className="execution-progress__count" aria-hidden="true">
          {progress.phases.filter(({ status }) => status === "completed").length}
          /{progress.phases.length}
        </span>
        <ChevronDown
          aria-hidden="true"
          className="execution-progress__chevron"
        />
      </button>
      {expanded ? (
        <div className="execution-progress__details" id={detailsId}>
          <ol className="execution-progress__phases">
            {progress.phases.map((phase) => (
              <li
                className="execution-progress__phase"
                data-phase={phase.id}
                data-status={phase.status}
                data-testid={`execution-phase:${phase.id}`}
                key={phase.id}
              >
                <span className="execution-progress__dot" aria-hidden="true" />
                <span>{t(`progress.phase.${phase.id}`)}</span>
                <small>{t(`progress.status.${phase.status}`)}</small>
              </li>
            ))}
          </ol>
          {children ? (
            <div className="execution-progress__activity">{children}</div>
          ) : null}
        </div>
      ) : null}
    </article>
  );
}
