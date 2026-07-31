# Composer Model Indicator Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove model and Work status metadata from the workspace header and show a compact, image-two-inspired model indicator in both composer action rows.

**Architecture:** Add a small shared `ComposerModelIndicator` presentation component that compacts provider-style model IDs while retaining the full value in a tooltip. Pass the configured model from `App` through `WorkSurface`; for existing Works prefer the latest Run's authoritative model label. Keep the indicator non-interactive because PiWork does not currently expose per-Run model switching.

**Tech Stack:** React 19, TypeScript, lucide-react, CSS, Testing Library, Vitest.

---

### Task 1: Lock down model placement and label compaction

**Files:**
- Modify: `src/features/workspace/WorkSurface.test.tsx`

- [ ] **Step 1: Write failing tests**

Add one test that hydrates a completed Work with model `gpt-5.6-sol`, then asserts the header contains neither the model nor a Work status and the composer contains `5.6 Sol`. Add another test that renders the empty new-Work surface with configured model `gpt-5.6-sol` and asserts the compact model appears in the new-Work action row.

- [ ] **Step 2: Verify RED**

Run:

```powershell
pnpm exec vitest run src/features/workspace/WorkSurface.test.tsx -t "模型入口"
```

Expected: both tests fail because the model is still in the header and the new-Work surface still shows `Agent · 自动`.

### Task 2: Move the model indicator into composer action rows

**Files:**
- Create: `src/features/workspace/ComposerModelIndicator.tsx`
- Modify: `src/app/App.tsx`
- Modify: `src/features/workspace/WorkSurface.tsx`
- Modify: `src/features/workspace/WorkHeader.tsx`
- Modify: `src/features/workspace/WorkComposer.tsx`
- Modify: `src/features/workspace/NewWorkStart.tsx`
- Modify: `src/styles/workspace.css`

- [ ] **Step 1: Create the shared presentation component**

Render a filled `Zap` icon plus a compact label. Strip a leading `gpt-`, split hyphenated suffixes into title-cased words, and expose the complete raw model label through `title`.

- [ ] **Step 2: Thread the configured model through the surface**

Pass `status.configuration.modelId` from `App` to `WorkSurface`, then into `NewWorkStart`. For an existing Work pass `latestRun.modelLabel ?? configuredModelLabel` into `WorkComposer`.

- [ ] **Step 3: Simplify the header**

Remove timeline/latest-Run model derivation and the visible Work status badge from `WorkHeader`. Retain title, project, More, and inspector controls.

- [ ] **Step 4: Restructure both composer action rows**

Place `ComposerModelIndicator` immediately before the circular send button. Change the existing Work composer box to a two-row layout so the editor spans the first row and model/send controls sit on the lower right.

- [ ] **Step 5: Verify GREEN**

Run the focused command from Task 1 and expect both tests to pass.

### Task 3: Verify the integrated UI

**Files:**
- Verify only.

- [ ] **Step 1: Run all frontend tests**

```powershell
pnpm test
```

- [ ] **Step 2: Run the production build**

```powershell
pnpm build
```

- [ ] **Step 3: Check whitespace**

```powershell
git diff --check
```
