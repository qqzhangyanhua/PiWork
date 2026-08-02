# PiWork GSAP Execution Motion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add restrained GSAP motion for PiWork loading, execution progress, completion, and error states using only authoritative persisted Work events.

**Architecture:** Keep Work events as the fact source. A pure `buildExecutionProgress()` projection converts each Run's events into five semantic phases, `ExecutionProgressCard` renders that model inline in the conversation, and scoped GSAP hooks animate only DOM presentation. Existing tool activity stays available inside the card, while terminal delivery and diagnostics remain separate product surfaces.

**Tech Stack:** React 19, TypeScript, Zustand, GSAP 3, `@gsap/react`, i18next, Vitest, Testing Library, Tauri 2.

---

## File structure

- Create `src/motion/gsap.ts`: one registration/import boundary for `gsap` and `useGSAP`.
- Create `src/components/motion/AnimatedSurfaceState.tsx`: reusable loading/error entrance and loading-loop behavior.
- Create `src/components/motion/AnimatedSurfaceState.test.tsx`: semantic rendering and cleanup tests.
- Create `src/features/workspace/executionProgress.ts`: pure event-to-phase projection.
- Create `src/features/workspace/executionProgress.test.ts`: phase, recovery, terminal, duplicate, and missing-event tests.
- Create `src/features/workspace/ExecutionProgressCard.tsx`: inline accessible progress card and scoped GSAP transitions.
- Create `src/features/workspace/ExecutionProgressCard.test.tsx`: expansion, collapse, failure, and activity-detail tests.
- Modify `src/features/workspace/WorkTimeline.tsx`: replace the old activity disclosure with the progress card and animate failure nodes.
- Modify `src/features/workspace/WorkTimeline.test.tsx`: assert progress semantics and preserve scroll-follow behavior.
- Modify `src/app/App.tsx`: animate model-loading and load-error states.
- Modify `src/features/workspace/WorkSurface.tsx`: animate hydration loading and hydration errors.
- Modify `src/app/App.test.tsx` and `src/features/workspace/WorkSurface.test.tsx`: keep the loading/error recovery contracts covered.
- Modify `src/i18n/locales/en.json` and `src/i18n/locales/zh-CN.json`: add execution phase and progress labels.
- Modify `src/styles/workspace.css` and `src/styles/globals.css`: semantic progress-card styling and reduced-motion fallbacks.
- Modify `package.json` and `pnpm-lock.yaml`: add `gsap` and `@gsap/react`.

Scope guards: do not change the Rust/sidecar event contract, do not parse free-form assistant text into plans, do not animate or strengthen the in-memory `queuedInstructions` promise, and do not introduce `planUpdated` until the engine has a trustworthy structured-plan source. The projection and card interfaces should remain ready for that later event without implementing it now.

### Task 1: Add the GSAP runtime boundary

**Files:**
- Modify: `package.json`
- Modify: `pnpm-lock.yaml`
- Create: `src/motion/gsap.ts`

- [ ] **Step 1: Install the exact project dependencies**

Run:

```powershell
pnpm add gsap @gsap/react
```

Expected: `package.json` lists both packages under `dependencies` and the lockfile updates without changing unrelated packages.

- [ ] **Step 2: Add one registration boundary**

Create `src/motion/gsap.ts`:

```ts
import { useGSAP } from "@gsap/react";
import { gsap } from "gsap";

gsap.registerPlugin(useGSAP);

export { gsap, useGSAP };
```

- [ ] **Step 3: Verify dependency and type resolution**

Run:

```powershell
pnpm typecheck
```

Expected: PASS with no missing-module or plugin-registration errors.

- [ ] **Step 4: Commit the dependency boundary**

```powershell
git add package.json pnpm-lock.yaml src/motion/gsap.ts
git commit -m "build: add GSAP motion runtime"
```

### Task 2: Project Work events into semantic execution phases

**Files:**
- Create: `src/features/workspace/executionProgress.ts`
- Create: `src/features/workspace/executionProgress.test.ts`

- [ ] **Step 1: Write failing phase-projection tests**

Cover these exact cases in `executionProgress.test.ts`:

```ts
expect(buildExecutionProgress([runStarted])).toMatchObject({
  status: "preparing",
  currentPhase: "prepare",
});

expect(buildExecutionProgress([runStarted, readStarted])).toMatchObject({
  status: "running",
  currentPhase: "analyze",
});

expect(buildExecutionProgress([
  runStarted,
  editStarted,
  editFailed,
  testStarted,
  testSucceeded,
  runCompleted,
])).toMatchObject({
  status: "completed",
  currentPhase: "deliver",
  failedToolCount: 1,
});

expect(buildExecutionProgress([runStarted, editStarted, runFailed])).toMatchObject({
  status: "failed",
  currentPhase: "deliver",
  failureMessage: "failed safely",
});
```

Also assert that repeated `toolStarted`/`toolFinished` records with the same `toolCallId` count as one tool, missing `runStarted` remains safe, and unvisited terminal phases become `skipped` rather than `completed`.

- [ ] **Step 2: Run the test to confirm red**

Run:

```powershell
pnpm test -- src/features/workspace/executionProgress.test.ts
```

Expected: FAIL because `buildExecutionProgress` does not exist.

- [ ] **Step 3: Implement the pure projection**

Define these public types and function:

```ts
export type ExecutionPhaseId =
  | "prepare"
  | "analyze"
  | "execute"
  | "validate"
  | "deliver";

export type ExecutionPhaseStatus =
  | "pending"
  | "active"
  | "completed"
  | "skipped"
  | "failed";

export type ExecutionProgressModel = {
  status: "preparing" | "running" | "completed" | "failed";
  currentPhase: ExecutionPhaseId;
  phases: Array<{
    id: ExecutionPhaseId;
    status: ExecutionPhaseStatus;
    toolCount: number;
    failedToolCount: number;
  }>;
  toolCount: number;
  failedToolCount: number;
  failureMessage: string | null;
};

export function buildExecutionProgress(
  events: WorkEventEnvelope[],
): ExecutionProgressModel;
```

Map `read`, `grep`, `find`, and `ls` (including names such as `read_file`) to `analyze`; map `edit` and `write` variants to `execute`; map `bash` commands whose input summary contains the case-insensitive words `test`, `check`, `lint`, or `build` to `validate`; map other bash activity to `execute`. Never regress the visible current phase when a later tool belongs to an earlier category. Use terminal events as authoritative.

- [ ] **Step 4: Run focused tests to confirm green**

Run:

```powershell
pnpm test -- src/features/workspace/executionProgress.test.ts
```

Expected: PASS.

- [ ] **Step 5: Commit the projection**

```powershell
git add src/features/workspace/executionProgress.ts src/features/workspace/executionProgress.test.ts
git commit -m "feat: derive execution progress from Work events"
```

### Task 3: Build the accessible GSAP execution progress card

**Files:**
- Create: `src/features/workspace/ExecutionProgressCard.tsx`
- Create: `src/features/workspace/ExecutionProgressCard.test.tsx`
- Modify: `src/i18n/locales/en.json`
- Modify: `src/i18n/locales/zh-CN.json`
- Modify: `src/styles/workspace.css`

- [ ] **Step 1: Write failing component tests**

Render the card with realistic event arrays and assert:

```ts
expect(screen.getByRole("status", { name: "执行进度" })).toBeInTheDocument();
expect(screen.getByText("分析项目")).toHaveAttribute("data-status", "active");
expect(screen.getByRole("button", { name: "收起执行详情" })).toHaveAttribute(
  "aria-expanded",
  "true",
);
```

For a hydrated successful Run, assert the card starts collapsed and can be manually expanded. For a live transition from running to completed, use fake timers to assert that the success confirmation becomes collapsed after the animation callback. For `runFailed`, assert the card stays expanded. For a failed tool followed by `runCompleted`, assert the overall label is successful and the failed activity remains present in details.

- [ ] **Step 2: Run the component test to confirm red**

Run:

```powershell
pnpm test -- src/features/workspace/ExecutionProgressCard.test.tsx
```

Expected: FAIL because the component does not exist.

- [ ] **Step 3: Implement semantic rendering before motion**

Use this component boundary:

```ts
export type ExecutionProgressCardProps = {
  events: WorkEventEnvelope[];
  children?: ReactNode;
};

export function ExecutionProgressCard({
  events,
  children,
}: ExecutionProgressCardProps) {
  const progress = buildExecutionProgress(events);
  // Render a button-controlled disclosure, five semantic phase rows,
  // safe failure copy, and the existing tool activity as children.
}
```

The disclosure button must own `aria-expanded` and `aria-controls`. Use `role="status"` with `aria-live="polite"` while running. Do not expose raw `runFailed.message` in product copy.

- [ ] **Step 4: Add scoped GSAP transitions**

Use `useGSAP` from `src/motion/gsap.ts` with a root ref. On a live mount, animate the card from `{ y: 8, autoAlpha: 0 }` over `0.28` seconds. Animate only the newly active phase over `0.22` seconds. On a running-to-completed transition, animate the success indicator from `{ scale: 0.72, autoAlpha: 0 }`, then collapse the disclosure. On failure, use one non-repeating `x: -2` to `x: 0` transition. Wrap animations in `gsap.matchMedia().add("(prefers-reduced-motion: no-preference)", ...)` and call `revert()` on cleanup.

- [ ] **Step 5: Add localized labels and restrained styling**

Add keys for `progress.label`, `progress.collapse`, `progress.expand`, `progress.preparing`, `progress.running`, `progress.completed`, `progress.failed`, and all five phase labels in both locale files. Add `.execution-progress` styles using existing PiWork color tokens; style state dots with `data-status` and add a CSS reduced-motion fallback without hard-coded Chinese content.

- [ ] **Step 6: Run focused tests**

Run:

```powershell
pnpm test -- src/features/workspace/ExecutionProgressCard.test.tsx src/i18n/locales/locales.test.ts
```

Expected: PASS.

- [ ] **Step 7: Commit the progress card**

```powershell
git add src/features/workspace/ExecutionProgressCard.tsx src/features/workspace/ExecutionProgressCard.test.tsx src/i18n/locales/en.json src/i18n/locales/zh-CN.json src/styles/workspace.css
git commit -m "feat: show animated execution progress"
```

### Task 4: Integrate progress into the continuous Work timeline

**Files:**
- Modify: `src/features/workspace/WorkTimeline.tsx`
- Modify: `src/features/workspace/WorkTimeline.test.tsx`

- [ ] **Step 1: Update timeline tests to require the new card**

Replace expectations for the old `.agent-activity` details root with semantic progress assertions. Preserve tests proving no visible Run container, delivery separation, safe diagnostics, current activity expansion, tool details, and the existing near-bottom scroll-follow/new-output behavior.

- [ ] **Step 2: Run the timeline test to confirm red**

Run:

```powershell
pnpm test -- src/features/workspace/WorkTimeline.test.tsx
```

Expected: FAIL because WorkTimeline still renders `AgentActivity`.

- [ ] **Step 3: Replace AgentActivity with ExecutionProgressCard**

Pass all Run events, including terminal events, to `ExecutionProgressCard`; pass existing `ActivityRow` elements as its details children. Keep assistant Markdown, delivery, failure, attachments, timestamps, and diagnostics unchanged. Add a scoped one-time entrance to `.timeline-failure` without displaying the raw engine error.

- [ ] **Step 4: Run timeline and workspace regression tests**

Run:

```powershell
pnpm test -- src/features/workspace/WorkTimeline.test.tsx src/features/workspace/WorkSurface.test.tsx
```

Expected: PASS, including the existing “preserves active reading position” test.

- [ ] **Step 5: Commit the timeline integration**

```powershell
git add src/features/workspace/WorkTimeline.tsx src/features/workspace/WorkTimeline.test.tsx
git commit -m "feat: integrate progress into Work timeline"
```

### Task 5: Animate loading and global error surfaces

**Files:**
- Create: `src/components/motion/AnimatedSurfaceState.tsx`
- Create: `src/components/motion/AnimatedSurfaceState.test.tsx`
- Modify: `src/app/App.tsx`
- Modify: `src/app/App.test.tsx`
- Modify: `src/features/workspace/WorkSurface.tsx`
- Modify: `src/features/workspace/WorkSurface.test.tsx`
- Modify: `src/styles/globals.css`
- Modify: `src/styles/workspace.css`

- [ ] **Step 1: Write failing AnimatedSurfaceState tests**

Assert that the component preserves `main` or `section`, role, label, and children; loading lines are selectable through `data-motion-line`; and unmounting does not leave document-level inline styles. Stub `matchMedia` for both no-preference and reduce cases.

- [ ] **Step 2: Run the focused test to confirm red**

Run:

```powershell
pnpm test -- src/components/motion/AnimatedSurfaceState.test.tsx
```

Expected: FAIL because the component does not exist.

- [ ] **Step 3: Implement the reusable surface**

Use this public API:

```ts
type AnimatedSurfaceStateProps = HTMLAttributes<HTMLElement> & {
  as?: "main" | "section";
  variant: "loading" | "error";
};
```

For `loading`, animate the root entrance once, stagger `[data-motion-line]` children, and apply a low-frequency repeat to `.continuous-loop-logo__mark` or the lines only while mounted. For `error`, animate `{ y: 6, autoAlpha: 0 }` to rest once. Scope all selectors to the component root and use `gsap.matchMedia()` for reduced motion.

- [ ] **Step 4: Integrate the surface without changing recovery behavior**

Use `AnimatedSurfaceState` for App model-loading/load-error branches and WorkSurface hydration loading/page-error branches. Keep existing roles, translated labels, retry callbacks, diagnostics toggles, and button focus behavior. Mark the two hydration skeleton bars with `data-motion-line`.

- [ ] **Step 5: Run focused recovery tests**

Run:

```powershell
pnpm test -- src/components/motion/AnimatedSurfaceState.test.tsx src/app/App.test.tsx src/features/workspace/WorkSurface.test.tsx
```

Expected: PASS.

- [ ] **Step 6: Commit loading and error motion**

```powershell
git add src/components/motion/AnimatedSurfaceState.tsx src/components/motion/AnimatedSurfaceState.test.tsx src/app/App.tsx src/app/App.test.tsx src/features/workspace/WorkSurface.tsx src/features/workspace/WorkSurface.test.tsx src/styles/globals.css src/styles/workspace.css
git commit -m "feat: animate loading and error states"
```

### Task 6: Full verification and visual QA

**Files:**
- Modify only files implicated by observed failures.

- [ ] **Step 1: Run static and automated verification**

Run each command separately:

```powershell
pnpm typecheck
pnpm test
pnpm build
```

Expected: all commands exit 0.

- [ ] **Step 2: Run Rust regression tests**

Run:

```powershell
pnpm cargo:test
```

Expected: all Rust tests pass; no event-contract changes were introduced.

- [ ] **Step 3: Run actual Tauri visual QA**

Launch PiWork and inspect 900×640, 1280×800, and 1440×900. Exercise model loading, Work hydration, a Run with read/edit/test events, a failed tool followed by recovery, a terminal failure, manual history scrolling, and Windows reduced-motion mode. Fix only defects in the files owned by this feature and repeat affected checks.

- [ ] **Step 4: Review the final diff**

Run:

```powershell
git status --short
git diff --check
git diff --stat HEAD~5..HEAD
```

Expected: no whitespace errors; unrelated pre-existing working-tree changes remain untouched.

- [ ] **Step 5: Commit verification-only fixes if needed**

```powershell
git add <only-files-fixed-during-verification>
git commit -m "fix: polish execution motion verification"
```

Do not create an empty commit when no fixes were necessary.
