# Project Picker Dismiss and Compact Sidebar Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the new-Work project picker on outside clicks and make recent Work items compact by removing relative timestamps.

**Architecture:** `NewWorkStart` owns the project-picker open state and will use a container ref plus a document-level pointer listener to distinguish internal from external clicks. `WorkSidebar` will render one-line items containing only the status dot and title; existing CSS will be tightened around that simpler structure.

**Tech Stack:** React 19, TypeScript, Testing Library, Vitest, CSS.

---

### Task 1: Dismiss the project picker on outside clicks

**Files:**
- Modify: `src/features/workspace/NewWorkStart.tsx`
- Test: `src/features/workspace/WorkSurface.test.tsx`

- [ ] **Step 1: Write the failing interaction test**

Open the picker, click its search input to prove internal interaction keeps it open, then click the `new-work-start` page area and assert the search input disappears.

- [ ] **Step 2: Run the focused test and verify RED**

Run: `pnpm exec vitest run src/features/workspace/WorkSurface.test.tsx -t "点击项目浮层外部时关闭浮层"`

Expected: FAIL because the picker remains visible after the outside click.

- [ ] **Step 3: Implement outside-click detection**

Add a ref around the trigger and picker, then while `projectOpen` is true attach a `pointerdown` listener to `document`. If `event.target` is a `Node` outside that ref, call `setProjectOpen(false)`. Remove the listener in the effect cleanup.

- [ ] **Step 4: Run the focused test and verify GREEN**

Run the command from Step 2.

Expected: PASS.

### Task 2: Remove relative minutes and compact Work items

**Files:**
- Modify: `src/features/works/WorkSidebar.tsx`
- Modify: `src/styles/workspace.css`
- Test: `src/features/workspace/WorkSurface.test.tsx`

- [ ] **Step 1: Write the failing rendering test**

Render a seeded Work and assert its title remains visible while no clock icon or minute-based timestamp is present in the corresponding navigation button.

- [ ] **Step 2: Run the focused test and verify RED**

Run: `pnpm exec vitest run src/features/workspace/WorkSurface.test.tsx -t "最近 Work 使用紧凑单行显示"`

Expected: FAIL because the current item still renders the clock and relative time.

- [ ] **Step 3: Implement compact single-line items**

Remove `Clock3`, `relativeTime`, the language lookup, and `.work-sidebar__time`. Render the title directly after the status dot. Update `.work-sidebar__item` to center-align with compact vertical padding and retain ellipsis on `.work-sidebar__title`.

- [ ] **Step 4: Run the focused test and verify GREEN**

Run the command from Step 2.

Expected: PASS.

### Task 3: Verify the combined UI changes

**Files:**
- Verify only.

- [ ] **Step 1: Run the complete frontend suite**

Run: `pnpm test`

Expected: all tests pass.

- [ ] **Step 2: Build the production frontend**

Run: `pnpm build`

Expected: TypeScript and Vite build successfully.

- [ ] **Step 3: Check the final diff**

Run: `git diff --check`

Expected: no whitespace errors.
