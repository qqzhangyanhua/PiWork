# PiWork Frontend Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild PiWork's frontend as a mature, compact desktop Agent workspace while preserving the existing Tauri commands, events, Work/Run semantics, and resource lifecycle.

**Architecture:** Keep the current React, Zustand, i18next, and Tauri boundaries. Change presentation and local interaction behavior inside existing feature components, add pure helpers for testable UI derivations, and consolidate all visual values into semantic CSS tokens. Do not add product capabilities that are absent from Rust.

**Tech Stack:** React 19, TypeScript, Vite, Zustand, i18next, react-markdown, lucide-react, CSS variables, Vitest, Testing Library, Tauri 2.

---

### Task 1: Token system and application shell

**Files:**
- Modify: `src/styles/tokens.css`
- Modify: `src/styles/globals.css`
- Modify: `src/styles/workspace.css`

- [ ] Replace visual literals with semantic surface, text, border, accent, status, spacing, radius, and elevation tokens.
- [ ] Add system dark-mode values without changing theme inside individual sections.
- [ ] Establish 14px body text, 11px minimum metadata, 6px/8px radii, and 36px minimum controls.
- [ ] Add explicit 900px compact navigation and overlay Inspector layouts.

### Task 2: Honest Work shell and navigation

**Files:**
- Test: `src/features/workspace/WorkSurface.test.tsx`
- Modify: `src/features/works/WorkSidebar.tsx`
- Modify: `src/features/workspace/WorkHeader.tsx`
- Modify: `src/features/workspace/WorkSurface.tsx`
- Modify: `src/i18n/locales/zh-CN.json`
- Modify: `src/i18n/locales/en.json`

- [ ] Write failing tests that require text/icon status signals, real project context, and removal of the inert more-action button.
- [ ] Run the focused tests and confirm the assertions fail for the missing redesign behavior.
- [ ] Implement the compact Work rows, truthful header actions, project/model context, and responsive shell.
- [ ] Run the focused tests and confirm they pass.

### Task 3: Run-aware timeline and non-interrupting output follow

**Files:**
- Test: `src/features/workspace/WorkTimeline.test.tsx`
- Modify: `src/features/workspace/WorkTimeline.tsx`
- Modify: `src/styles/workspace.css`
- Modify: `src/i18n/locales/zh-CN.json`
- Modify: `src/i18n/locales/en.json`

- [ ] Write failing tests for explicit Run boundaries, delivery and failure nodes, and a new-output control when the reader is away from the bottom.
- [ ] Run the focused tests and confirm the expected failures.
- [ ] Render Agent prose directly, collapse tool process by default after completion, and separate delivery/failure from process details.
- [ ] Follow new output only while within 120px of the bottom; otherwise preserve scroll and expose a button that returns to the latest content.
- [ ] Run the focused tests and confirm they pass.

### Task 4: Composer, resources, Inspector, and model configuration

**Files:**
- Test: `src/features/workspace/WorkSurface.test.tsx`
- Modify: `src/features/workspace/WorkComposer.tsx`
- Modify: `src/features/workspace/NewWorkStart.tsx`
- Modify: `src/features/workspace/AttachmentButton.tsx`
- Modify: `src/features/workspace/AttachmentChips.tsx`
- Modify: `src/features/workspace/AttachmentDraftList.tsx`
- Modify: `src/features/workspace/WorkInspector.tsx`
- Modify: `src/features/model-setup/ModelSetup.tsx`
- Modify: `src/styles/globals.css`
- Modify: `src/styles/workspace.css`

- [ ] Write failing tests that forbid queue promises while a Work is active and remove the unsupported Changes tab.
- [ ] Run the focused tests and confirm the expected failures.
- [ ] Preserve active-run drafts without sending, show resource lifecycle and safe failure reasons, and keep one send button.
- [ ] Reduce Inspector to Delivery, Attachments, Validation, and Logs with specific empty states and drawer semantics.
- [ ] Complete model connection, success, failure, empty-model, and saving states with real inputs and accessible status text.
- [ ] Run the focused tests and confirm they pass.

### Task 5: Desktop visual verification and release checks

**Files:**
- Modify only files implicated by visual or test failures.

- [ ] Run the real Tauri application and inspect 900x640, 1280x800, and 1440x900.
- [ ] Check compact navigation, Inspector drawer, Composer clearance, popover bounds, focus-visible states, and timeline reading position.
- [ ] Run `pnpm typecheck`, `pnpm test`, `pnpm build`, and `pnpm cargo:test`.
- [ ] Search visible copy and markup for unsupported controls, internal implementation terms, and accidental em-dashes.
- [ ] Score the final UI against the Refactoring UI hierarchy, spacing, typography, color, depth, image/icon, and composition criteria.
