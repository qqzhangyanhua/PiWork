# PiWork Linear Workflow Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reframe PiWork around Linear-style Work objects with a Work Home, All Works index, document-like Work Detail, compact creation flow, and responsive Inspector while preserving all Rust and Tauri contracts.

**Architecture:** Keep the existing Zustand store and Tauri client as the state projection boundary. Add local workspace navigation state above the selected Work, compose Home and index pages from the already hydrated Work summaries, and reuse the existing creation/runtime components. Treat each Work as one continuous Agent conversation: Run remains an internal persistence and event correlation detail, never a visible navigation or disclosure level.

**Tech Stack:** React 19, TypeScript, Zustand, i18next, lucide-react, native CSS/Tailwind 4 tokens, Vitest and Testing Library.

---

### Task 1: Lock the object-navigation behavior

**Files:**
- Modify: `src/features/workspace/WorkSurface.test.tsx`
- Modify: `src/features/workspace/WorkTimeline.test.tsx`

- [ ] Add a WorkSurface test proving startup shows Work Home rather than opening the selected recent Work.
- [ ] Add navigation tests proving All Works and a Work row open distinct index/detail views.
- [ ] Add a timeline test proving all persisted interactions render in one continuous conversation without visible Run containers.
- [ ] Run the focused tests and confirm they fail because the new views and disclosure behavior do not exist.

### Task 2: Add Home and All Works product surfaces

**Files:**
- Create: `src/features/workspace/WorkList.tsx`
- Create: `src/features/workspace/WorkHome.tsx`
- Create: `src/features/workspace/AllWorks.tsx`
- Modify: `src/features/workspace/NewWorkStart.tsx`
- Modify: `src/i18n/locales/en.json`
- Modify: `src/i18n/locales/zh-CN.json`

- [ ] Implement a shared semantic Work row with status icon/text, project, and relative update time.
- [ ] Implement Work Home with the inline New Work composer followed by recent Work rows.
- [ ] Implement All Works as status groups whose rows are sorted by recent activity.
- [ ] Keep list filtering and grouping entirely derived from backend Work summaries.
- [ ] Run focused component tests and confirm the new page behavior passes.

### Task 3: Rebuild the application navigation shell

**Files:**
- Modify: `src/features/works/WorkSidebar.tsx`
- Modify: `src/features/workspace/WorkSurface.tsx`
- Modify: `src/features/workspace/NewWorkStart.tsx`

- [ ] Introduce local `home`, `all`, and `detail` workspace views without changing persisted Work selection semantics.
- [ ] Make startup land on Home, make recent Work rows open Detail, and make All Works open the index.
- [ ] Make New Work activate a full-height, unpersisted draft in the main workspace while preserving the sidebar; never open a creation dialog.
- [ ] Preserve model settings, resource import, runtime events, and diagnostics paths unchanged.
- [ ] Run WorkSurface tests and repair only regressions caused by the new navigation shell.

### Task 4: Convert execution history into one Agent conversation

**Files:**
- Modify: `src/features/workspace/WorkTimeline.tsx`
- Modify: `src/features/workspace/WorkHeader.tsx`
- Modify: `src/features/workspace/WorkComposer.tsx`

- [ ] Render persisted user messages, Agent Markdown, tool activity, delivery, and failures in one continuous conversation surface.
- [ ] Keep Run identifiers and boundaries out of visible UI while preserving their internal event-correlation role.
- [ ] Keep Agent Markdown directly in the reading flow; keep tools collapsed and delivery/error state blocks semantic.
- [ ] Reduce header and Composer weight while retaining IME-safe Enter behavior and real runtime disable rules.
- [ ] Run timeline and workspace tests.

### Task 5: Apply the Linear-first visual system and responsive rules

**Files:**
- Modify: `src/styles/tokens.css`
- Modify: `src/styles/workspace.css`
- Modify: `src/styles/globals.css`

- [ ] Tune semantic colors, type, spacing, radii, and elevation to a cool, quiet Linear-like light system.
- [ ] Use a 232px navigation pane, continuous hairline dividers, compact 36px rows, and restrained selected states.
- [ ] Keep content readable at 900x640 by collapsing navigation and presenting Inspector as a drawer.
- [ ] Ensure Composer reserves timeline space, popovers stay within the viewport, focus remains visible, and reduced motion is honored.
- [ ] Run tests, typecheck, and build.

### Task 6: Visual and full-stack verification

**Files:**
- Modify only files implicated by observed defects.

- [ ] Run the actual Tauri application.
- [ ] Capture and inspect 900x640, 1280x800, and 1440x900 screenshots.
- [ ] Fix clipping, overlap, hierarchy, focus, or scrolling defects and repeat affected captures.
- [ ] Run `pnpm typecheck`, `pnpm test`, `pnpm build`, and `pnpm cargo:test` from a clean command invocation.
- [ ] Review the final diff and report preserved/rejected Claude concepts, unchanged contracts, verification evidence, and future capabilities that remain intentionally absent.
