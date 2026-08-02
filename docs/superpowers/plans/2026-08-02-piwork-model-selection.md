# PiWork Saved Model Selection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let users switch the model stored inside an existing provider configuration without re-entering its credential, and use the new model for future Runs.

**Architecture:** Add one Rust-owned selection command that reloads the saved credential, revalidates the requested model against the provider, and persists only `model_id`. Project the command through `PiWorkClient`; SettingsPage keeps provider results as ephemeral per-card UI state and forwards an updated active configuration to App.

**Tech Stack:** Rust, Tauri 2, SQLx, ts-rs, React 19, TypeScript, i18next, Vitest, Testing Library.

---

## File structure

- Modify `src-tauri/src/model/mod.rs`: input contract and validated model-selection service method.
- Modify `src-tauri/src/model/commands.rs`: Tauri command boundary.
- Modify `src-tauri/src/lib.rs`: register the command.
- Modify `src-tauri/tests/model_configuration.rs`: persistence, validation, and runtime tests.
- Modify `src/app/tauriClient.ts`: typed frontend command.
- Modify `src/test/mockTauriClient.ts`: test client support.
- Modify `src/features/settings/SettingsPage.tsx`: per-configuration model picker and feedback.
- Modify `src/features/settings/SettingsPage.test.tsx`: component behavior.
- Modify `src/app/App.test.tsx`: active model propagation.
- Modify `src/i18n/locales/en.json` and `src/i18n/locales/zh-CN.json`: accessible labels and product feedback.
- Modify `src/styles/linear-fidelity.css`: restrained picker layout and responsive behavior.

### Task 1: Define and implement the validated Rust selection contract

**Files:**
- Modify: `src-tauri/tests/model_configuration.rs`
- Modify: `src-tauri/src/model/mod.rs`
- Modify: `src-tauri/src/model/commands.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write failing service tests**

Add tests that save an active configuration with `gpt-5.2`, configure the fake connection tester to return `gpt-5.2` and `gpt-5.3`, then assert:

```rust
let selected = service
    .select_model(SelectModelForConfigurationInput {
        configuration_id: saved.id.clone(),
        model_id: "gpt-5.3".into(),
    })
    .await
    .expect("select model");

assert_eq!(selected.model_id, "gpt-5.3");
assert!(selected.active);
assert_eq!(
    service.runtime_configuration().await.expect("runtime").model_id,
    "gpt-5.3",
);
```

Add a second test that requests `missing-model`, expects `invalid_input`, and confirms the stored and runtime model remain `gpt-5.2`. Add an unknown configuration assertion using `configuration_id: "missing"`.

- [ ] **Step 2: Run the Rust test to verify RED**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test model_configuration
```

Expected: compile failure because `SelectModelForConfigurationInput` and `select_model` do not exist.

- [ ] **Step 3: Add the public input and service method**

Add the exported input:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct SelectModelForConfigurationInput {
    pub configuration_id: String,
    pub model_id: String,
}
```

Add `ModelService::select_model` that trims and rejects an empty `model_id`, finds the configuration by stable ID, calls `test_saved_configuration`, verifies exact model-ID membership, updates only `configurations[target].model_id`, saves the complete list, and returns `self.summary(configurations.remove(target))`. Use `AppError::invalid_input("modelId", "selected model is not available from the provider")` for a missing model.

- [ ] **Step 4: Expose and register the command**

Add to `commands.rs`:

```rust
#[tauri::command(rename_all = "camelCase")]
pub async fn select_model_for_configuration(
    state: State<'_, AppState>,
    input: SelectModelForConfigurationInput,
) -> Result<ModelConfigurationSummary, AppError> {
    state.model_service().select_model(input).await
}
```

Import the input and register `model::commands::select_model_for_configuration` in the `generate_handler!` list.

- [ ] **Step 5: Run focused Rust verification**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test model_configuration
```

Expected: all model-configuration tests pass.

### Task 2: Define frontend behavior and extend the typed client

**Files:**
- Modify: `src/features/settings/SettingsPage.test.tsx`
- Modify: `src/app/App.test.tsx`
- Modify: `src/app/tauriClient.ts`
- Modify: `src/test/mockTauriClient.ts`

- [ ] **Step 1: Write failing component and App tests against the desired API**

Add the active and inactive selection cases described in Task 3 to `SettingsPage.test.tsx`, and reference:

```ts
client.selectModelForConfiguration.mockResolvedValue({
  ...openai,
  modelId: "gpt-5.3",
});
```

In `App.test.tsx`, open Settings, test the active saved connection, switch from `gpt-5.2` to `gpt-5.3`, and assert that the Settings current-model summary and the WorkSurface model label use `gpt-5.3` after returning to a conversation.

- [ ] **Step 2: Run the frontend tests to verify RED**

Run:

```powershell
pnpm test -- src/features/settings/SettingsPage.test.tsx src/app/App.test.tsx
```

Expected: TypeScript compilation fails because the client method does not exist.

- [ ] **Step 3: Add the client contract**

Add:

```ts
export type SelectModelForConfigurationInput = {
  configurationId: string;
  modelId: string;
};
```

Add `selectModelForConfiguration(input): Promise<ModelConfigurationSummary>` to `PiWorkClient`, invoke `select_model_for_configuration`, and add a typed Vitest mock with a safe default implementation that returns an active summary using `input.modelId`.

- [ ] **Step 4: Re-run focused tests to reach behavioral RED**

Run:

```powershell
pnpm test -- src/features/settings/SettingsPage.test.tsx src/app/App.test.tsx
```

Expected: tests compile, then fail because the picker is not rendered.

### Task 3: Build the accessible settings-page picker with TDD

**Files:**
- Modify: `src/features/settings/SettingsPage.tsx`
- Modify: `src/i18n/locales/en.json`
- Modify: `src/i18n/locales/zh-CN.json`
- Modify: `src/styles/linear-fidelity.css`

- [ ] **Step 1: Confirm the failing SettingsPage expectations**

After clicking “测试 gpt-5.2 连接”, make the mock return two models and assert:

```ts
const picker = await screen.findByRole("combobox", {
  name: "切换 gpt-5.2 使用的模型",
});
expect(picker).toHaveValue("gpt-5.2");
await user.selectOptions(picker, "gpt-5.3");
await user.click(screen.getByRole("button", { name: "设为当前模型 gpt-5.3" }));
expect(client.selectModelForConfiguration).toHaveBeenCalledWith({
  configurationId: "openai-main",
  modelId: "gpt-5.3",
});
expect(onModelConfigured).toHaveBeenCalledWith({
  ...openai,
  modelId: "gpt-5.3",
});
```

The Task 2 tests also prove an inactive configuration updates its card without calling `onModelConfigured`, and a rejected selection keeps the old heading while rendering a safe localized alert.

- [ ] **Step 2: Run the component test to verify RED**

Run:

```powershell
pnpm test -- src/features/settings/SettingsPage.test.tsx src/app/App.test.tsx
```

Expected: FAIL because the picker and selection command are not rendered.

- [ ] **Step 3: Implement per-card ephemeral model state**

Add:

```ts
const [availableModels, setAvailableModels] = useState<Record<string, AvailableModel[]>>({});
const [selectedModels, setSelectedModels] = useState<Record<string, string>>({});
const [selectionErrors, setSelectionErrors] = useState<Record<string, boolean>>({});
```

On successful saved-connection testing, store `result.models` by configuration ID and choose the saved model when present, otherwise the first returned model. Implement `selectModel(item)` to call the new client method, replace only the returned configuration, update the draft, clear the error, and call `onModelConfigured` only when the returned summary is active.

- [ ] **Step 4: Render the picker inside the configuration row**

When available models exist, render a `.model-configuration__model-picker` spanning the card columns with a unique label/select and a button. Disable saving when the draft equals `item.modelId`, while another operation is pending, or credentials are unavailable. Render success via the updated heading/current summary and selection failure through `role="alert"` without raw exception text.

- [ ] **Step 5: Add localized strings and restrained styles**

Add English and Chinese keys for `switchModel`, `useSelectedModel`, `switchingModel`, `modelSwitched`, and `switchModelError`. Style the picker as an inline label/select/button group with existing tokens, visible focus, and a single-column layout under the current settings breakpoint.

- [ ] **Step 6: Run focused frontend tests**

Run:

```powershell
pnpm test -- src/features/settings/SettingsPage.test.tsx src/i18n/locales/locales.test.ts
pnpm typecheck
```

Expected: PASS.

### Task 4: Complete regression coverage and visual QA

**Files:**
- No production files are expected to change in this task.

- [ ] **Step 1: Run complete verification**

Run separately:

```powershell
pnpm typecheck
pnpm test
pnpm build
pnpm cargo:test
git diff --check
```

Expected: every command exits 0; no raw credential or provider error is visible in product UI.

- [ ] **Step 2: Perform desktop visual QA**

In the PiWork Settings page, test a saved connection with at least two models, verify the picker layout, switch models, return to a conversation, and confirm the composer/header model label reflects the selection. Check the current desktop size and the narrowest available window without changing system privacy or accessibility settings.
