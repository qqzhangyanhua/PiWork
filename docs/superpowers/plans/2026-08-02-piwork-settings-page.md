# PiWork Settings Page Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a Linear-inspired local-account entry in the sidebar that opens a real account menu and navigates to an in-app settings page containing the existing model configuration workflow.

**Architecture:** `WorkSidebar` owns the anchored account popover because its trigger and responsive placement belong to sidebar navigation. `WorkSurface` owns the active `settings` view and the global `Ctrl+,` route. A focused `SettingsPage` component reuses `ModelConfigurationForm`; it does not add backend state or account capabilities.

**Tech Stack:** React 19, TypeScript, i18next, lucide-react, Testing Library/Vitest, existing CSS tokens.

---

### Task 1: Lock the navigation contract with tests

**Files:**
- Modify: `src/app/App.test.tsx`
- Modify: `src/features/workspace/WorkSurface.test.tsx`

- [ ] Replace the old model-dialog assertions with a test that clicks the `Local user` account button, finds a `menu`, selects `Settings`, and finds the settings page heading plus the model form.
- [ ] Assert saving an updated provider remains on the settings page and updates the model label.
- [ ] Add keyboard tests for `Escape` closing the account menu and returning focus, plus `Ctrl+,` opening settings.
- [ ] Run `pnpm test -- src/app/App.test.tsx src/features/workspace/WorkSurface.test.tsx` and confirm failure occurs because the account menu/settings view does not exist.

### Task 2: Implement account navigation and the settings surface

**Files:**
- Create: `src/features/settings/SettingsPage.tsx`
- Modify: `src/features/works/WorkSidebar.tsx`
- Modify: `src/features/workspace/WorkSurface.tsx`
- Modify: `src/features/model-setup/ModelSetup.tsx`
- Modify: `src/i18n/locales/zh-CN.json`
- Modify: `src/i18n/locales/en.json`

- [ ] Extend `WorkspaceView` with `settings` and route both the popover item and `Ctrl+,` to `setActiveView("settings")`.
- [ ] Implement the local account trigger with initial avatar, primary text, device-only secondary text, `aria-expanded`, and an anchored `role="menu"` containing one real `role="menuitem"` settings action.
- [ ] Close the menu on outside pointer interaction or `Escape`; return focus to the account trigger after `Escape`.
- [ ] Render `SettingsPage` inside the existing product shell and pass `client`, current configuration, and `onModelConfigured` to `ModelConfigurationForm`.
- [ ] In persistent settings mode, clear the saving state after a successful save and expose a localized success status instead of relying on dialog unmounting.
- [ ] Run the focused tests until green.

### Task 3: Refine the Linear/Raycast visual system

**Files:**
- Modify: `src/styles/linear-fidelity.css`

- [ ] Style the account row as a flat sidebar navigation item with a 24–28px monochrome avatar, restrained hover/selected surface, and no competing color block.
- [ ] Place the menu above the trigger, constrain it to the viewport, use an 8px radius, 1px border, and only the existing light floating shadow.
- [ ] Give the settings page a quiet header and a readable 480–560px form column; avoid cards around every section.
- [ ] At the compact-sidebar breakpoint, show only the avatar while preserving a 40px target and open the menu to the right within the window.
- [ ] Respect existing focus-visible and reduced-motion rules.

### Task 4: Verify behavior and the desktop build

**Files:**
- No production files beyond Tasks 1–3.

- [ ] Run `pnpm typecheck` and require exit code 0.
- [ ] Run `pnpm test` and require zero failed tests.
- [ ] Run `pnpm build` and require exit code 0.
- [ ] Run `pnpm cargo:test` and require zero failed Rust tests (document intentionally ignored credential tests).
- [ ] Open the real Tauri app and inspect the account row, popover, settings page, focus return, `Ctrl+,`, and compact sidebar at 900×640, then inspect 1280×800 and 1440×900 for clipping.
