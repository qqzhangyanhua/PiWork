# PiWork Screenshot-Faithful Model Settings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the current model configuration list with the approved screenshot-faithful settings shell, connection tabs, single-connection editor, and local Provider/model logos without removing existing multi-configuration behavior.

**Architecture:** `SettingsPage` remains the page-level coordinator and owns configuration loading plus the selected connection. Focused components under `features/model-setup` own Provider logos, the accessible branded listbox, and connection editing; the existing first-run `ModelConfigurationForm` remains intact. Existing Tauri contracts are reused, including `saveModelConfiguration({ id })` for updates and `testSavedModelConfiguration` for credentials that never enter the frontend.

**Tech Stack:** React 19, TypeScript, i18next, lucide-react, Testing Library/Vitest, existing PiWork CSS tokens and Tauri model client.

---

### Task 1: Add Provider logos and an accessible branded selector

**Files:**
- Create: `src/features/model-setup/ProviderLogo.tsx`
- Create: `src/features/model-setup/ProviderLogo.test.tsx`
- Create: `src/features/model-setup/BrandSelect.tsx`
- Create: `src/features/model-setup/BrandSelect.test.tsx`

- [ ] **Step 1: Write failing Provider logo mapping tests**

Render each supported `ModelProvider` and assert a stable `data-provider` mark, an accessible hidden label when requested, and the generic link mark for `custom`.

```tsx
for (const provider of providers) {
  const { container } = render(<ProviderLogo provider={provider} />);
  expect(container.querySelector(`[data-provider="${provider}"]`)).toBeInTheDocument();
}
```

- [ ] **Step 2: Run the Provider logo test and verify RED**

Run: `pnpm test -- src/features/model-setup/ProviderLogo.test.tsx`

Expected: FAIL because `ProviderLogo.tsx` does not exist.

- [ ] **Step 3: Implement the local SVG logo map**

Export `ProviderLogo({ provider, size = 20, decorative = true })`. Use inline SVG paths for OpenAI, Anthropic, Gemini, OpenRouter and DeepSeek; render a neutral link glyph for `custom`. Never load a remote image.

- [ ] **Step 4: Run the Provider logo test and verify GREEN**

Run: `pnpm test -- src/features/model-setup/ProviderLogo.test.tsx`

Expected: PASS.

- [ ] **Step 5: Write failing branded selector keyboard tests**

Cover trigger rendering, logo rendering, ArrowDown opening, ArrowUp/ArrowDown traversal, Home/End, Enter selection, Escape close, outside click close, and focus return.

```tsx
await user.click(screen.getByRole("combobox", { name: "Provider" }));
await user.keyboard("{End}{Enter}");
expect(onChange).toHaveBeenCalledWith("deepseek");
expect(screen.getByRole("combobox", { name: "Provider" })).toHaveFocus();
```

- [ ] **Step 6: Run the selector test and verify RED**

Run: `pnpm test -- src/features/model-setup/BrandSelect.test.tsx`

Expected: FAIL because `BrandSelect.tsx` does not exist.

- [ ] **Step 7: Implement `BrandSelect`**

Create a controlled component with this public contract:

```tsx
export type BrandSelectOption = {
  value: string;
  label: string;
  provider: ModelProvider;
  badge?: string;
  description?: string;
};

export function BrandSelect({
  id, label, value, options, disabled = false, onChange,
}: {
  id: string;
  label: string;
  value: string;
  options: BrandSelectOption[];
  disabled?: boolean;
  onChange(value: string): void;
}) { /* button combobox + popup listbox */ }
```

Keep active option state separate from the selected value. Apply `aria-expanded`, `aria-controls`, `aria-activedescendant`, `role="listbox"`, `role="option"`, and `aria-selected`; use a containing ref for outside pointer detection.

- [ ] **Step 8: Run both component tests and verify GREEN**

Run: `pnpm test -- src/features/model-setup/ProviderLogo.test.tsx src/features/model-setup/BrandSelect.test.tsx`

Expected: both files PASS with no console warnings.

### Task 2: Build the single-connection editor with existing client contracts

**Files:**
- Create: `src/features/model-setup/ModelConnectionEditor.tsx`
- Create: `src/features/model-setup/ModelConnectionEditor.test.tsx`
- Modify: `src/i18n/locales/zh-CN.json`
- Modify: `src/i18n/locales/en.json`

- [ ] **Step 1: Write failing saved-connection tests**

Cover masked credential state, saved-credential testing, returned model options, model selection, inactive connection activation, and failure feedback that hides raw errors.

```tsx
render(<ModelConnectionEditor client={client} configuration={deepseek} onSaved={onSaved} onActivated={onActivated} />);
expect(screen.getByText("••••••••••••••••••••••••")).toBeInTheDocument();
await user.click(screen.getByRole("button", { name: "测试连接" }));
expect(client.testSavedModelConfiguration).toHaveBeenCalledWith("deepseek-code");
```

- [ ] **Step 2: Run the saved-connection tests and verify RED**

Run: `pnpm test -- src/features/model-setup/ModelConnectionEditor.test.tsx`

Expected: FAIL because the editor does not exist.

- [ ] **Step 3: Implement saved credential and model switching flow**

Initialize Provider, Base URL and model from `configuration`. When the credential is unchanged, `testConnection` calls `testSavedModelConfiguration(configuration.id)`. After success, select the saved model when available. Persist a changed model through `selectModelForConfiguration`; call `onSaved` and call `onActivated` only when the returned configuration is active.

- [ ] **Step 4: Write failing new/update credential tests**

Cover add mode, reset credential mode, automatic reset when Provider changes, testing with entered API Key, passing the original ID on update, and not enabling save before successful testing.

```tsx
await user.click(screen.getByRole("button", { name: "重置 API Key" }));
await user.type(screen.getByLabelText("API Key"), "replacement-key");
await user.click(screen.getByRole("button", { name: "测试连接" }));
await user.click(screen.getByRole("button", { name: "保存模型配置" }));
expect(client.saveModelConfiguration).toHaveBeenCalledWith(expect.objectContaining({
  id: "deepseek-code",
  apiKey: "replacement-key",
}));
```

- [ ] **Step 5: Run the new/update tests and verify RED**

Run: `pnpm test -- src/features/model-setup/ModelConnectionEditor.test.tsx`

Expected: new assertions FAIL because editable credential flow is missing.

- [ ] **Step 6: Implement new/update flow and localized copy**

Use `BrandSelect` for Provider and model. Keep the saved API Key masked until explicit reset. Clear test results whenever Provider, Base URL or API Key changes. Call `saveModelConfiguration` with the original `configuration.id` for updates and without an ID for additions. Add all labels, statuses and errors to both locale files.

- [ ] **Step 7: Run editor tests and verify GREEN**

Run: `pnpm test -- src/features/model-setup/ModelConnectionEditor.test.tsx`

Expected: PASS with saved, new, update, activation and failure cases covered.

### Task 3: Replace the settings list with the approved screenshot layout

**Files:**
- Modify: `src/features/settings/SettingsPage.tsx`
- Modify: `src/features/settings/SettingsPage.test.tsx`
- Modify: `src/styles/linear-fidelity.css`

- [ ] **Step 1: Replace list assertions with failing page-structure tests**

Assert the settings secondary navigation, model panel heading, connection tablist, active tab, inactive connection switching without activation, add tab, current connection state, and the single rendered editor.

```tsx
expect(await screen.findByRole("navigation", { name: "设置导航" })).toBeInTheDocument();
const tabs = screen.getByRole("tablist", { name: "模型连接" });
expect(within(tabs).getByRole("tab", { name: /OpenAI/ })).toHaveAttribute("aria-selected", "true");
expect(screen.getAllByLabelText("Provider")).toHaveLength(1);
```

- [ ] **Step 2: Run settings tests and verify RED**

Run: `pnpm test -- src/features/settings/SettingsPage.test.tsx`

Expected: FAIL because the page still renders a configuration list.

- [ ] **Step 3: Implement the page coordinator and screenshot hierarchy**

Load configurations once, choose the active ID or the first ID, and render:

```tsx
<section className="settings-page">
  <header className="settings-page__header">...</header>
  <div className="settings-page__layout">
    <SettingsNavigation />
    <section className="model-settings-panel">
      <ConnectionTabs ... />
      <ModelConnectionEditor ... />
      <ConnectionTestCard ... />
      <details className="model-settings-advanced">...</details>
    </section>
  </div>
</section>
```

Update configuration state by stable ID after saves and activations. Selecting a tab must not call `activateModelConfiguration`. Add mode uses a synthetic selected state and returns to the new saved ID after success.

- [ ] **Step 4: Run settings tests and verify GREEN**

Run: `pnpm test -- src/features/settings/SettingsPage.test.tsx`

Expected: PASS.

- [ ] **Step 5: Implement screenshot-faithful styles**

Replace the existing settings/model-configuration block in `linear-fidelity.css` with CSS for the bordered white settings shell, 200–240px secondary navigation, flexible main panel, connection tab overflow, 46px branded controls, masked credential row, security callout, connection test row and advanced disclosure. Add responsive rules at 1100px and 760px plus reduced-motion overrides.

- [ ] **Step 6: Run settings and workspace regression tests**

Run: `pnpm test -- src/features/settings/SettingsPage.test.tsx src/features/workspace/WorkSurface.test.tsx src/app/App.test.tsx`

Expected: PASS with existing settings navigation and application model synchronization preserved.

### Task 4: Verify the complete implementation and visually compare it

**Files:**
- Modify only if verification reveals an in-scope regression.

- [ ] **Step 1: Run static and unit verification**

Run: `pnpm typecheck`

Expected: exit code 0.

Run: `pnpm test`

Expected: zero failed test files and zero failed tests.

- [ ] **Step 2: Run production and Rust regression builds**

Run: `pnpm build`

Expected: exit code 0.

Run: `pnpm cargo:test`

Expected: exit code 0; intentionally ignored credential integration tests may remain ignored.

- [ ] **Step 3: Perform visual QA**

Open the desktop app at 1440×900, 1280×800 and 900×640. Compare title position, settings navigation width, panel padding, control height, borders, radii, state colors and provider/model logos with the approved browser mockup and the supplied screenshot. Confirm no clipping, overlap or inaccessible overflow.

- [ ] **Step 4: Review the final diff**

Run: `git diff --check` and `git status --short`.

Expected: no whitespace errors; only the implementation plan and model-settings-related files are intentionally changed by this task. Preserve all unrelated pre-existing modifications.
