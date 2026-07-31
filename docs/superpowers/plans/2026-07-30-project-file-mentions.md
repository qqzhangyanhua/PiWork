# Project File Mentions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace manual project-path entry with a searchable picker and deliver `@`-selected project files as validated execution-time context to Pi.

**Architecture:** Rust owns project traversal, filtering, path containment, content limits, and engine-prompt assembly. React owns searchable project/file menus and a reusable mention-aware editor model; the Work store carries the original prompt plus relative file references for immediate and queued instructions. The persisted user message remains unchanged while the engine receives a separately expanded prompt.

**Tech Stack:** React 19, TypeScript, Zustand, Vitest/Testing Library, Tauri 2, Rust, Tokio, SQLx.

**Repository note:** The checkout contained extensive uncommitted product work before this feature began. Implementation checkpoints use targeted diffs and tests instead of commits so unrelated user changes are never accidentally captured.

---

### Task 1: Define structured file-reference contracts

**Files:**
- Modify: `src-tauri/src/domain/work.rs`
- Modify: `src/bindings/index.ts`
- Create: `src/bindings/ProjectFileSummary.ts`
- Create: `src/bindings/StartWorkInput.ts`
- Modify: `src/app/tauriClient.ts`
- Modify: `src/test/mockTauriClient.ts`
- Test: `src-tauri/src/domain/work.rs`
- Test: `src/features/works/workStore.test.ts`

- [ ] **Step 1: Write failing Rust serialization tests**

Add tests proving the camelCase wire contracts:

```rust
let input = StartWorkInput {
    prompt: "Review @src/main.ts".into(),
    referenced_files: vec!["src/main.ts".into()],
};
assert_eq!(serde_json::to_value(input).unwrap(), json!({
    "prompt": "Review @src/main.ts",
    "referencedFiles": ["src/main.ts"]
}));
```

- [ ] **Step 2: Run the contract test and verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib domain::work::tests::start_work_input_uses_structured_references`

Expected: compilation failure because `StartWorkInput` and `ProjectFileSummary` do not exist.

- [ ] **Step 3: Add minimal DTOs and TypeScript client signatures**

Define:

```rust
pub struct StartWorkInput {
    pub prompt: String,
    pub referenced_files: Vec<String>,
}

pub struct ProjectFileSummary {
    pub relative_path: String,
}
```

Expose matching TypeScript bindings and add `listProjectFiles(rootPath)` plus `startWork(workId, prompt, referencedFiles)` to `PiWorkClient`.

- [ ] **Step 4: Run focused Rust and TypeScript tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib domain::work::tests`

Run: `pnpm test src/features/works/workStore.test.ts`

Expected: both pass.

### Task 2: Index and validate project files in Rust

**Files:**
- Create: `src-tauri/src/work/project_files.rs`
- Modify: `src-tauri/src/work/mod.rs`
- Modify: `src-tauri/src/error.rs`
- Test: `src-tauri/src/work/project_files.rs`

- [ ] **Step 1: Write failing filesystem-policy tests**

Cover recursive enumeration, sorted relative paths, ignored directories, binary and `.env` exclusion, the 256 KiB per-file limit, absolute/parent paths, symlink escapes where supported, 10-file count, 512 KiB combined content, and execution-time reads.

- [ ] **Step 2: Run project-file tests and verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib work::project_files::tests`

Expected: compilation failure because the module is absent.

- [ ] **Step 3: Implement the focused project-files module**

Expose:

```rust
pub fn list_project_files(root: &Path) -> Result<Vec<ProjectFileSummary>, AppError>;
pub fn build_engine_prompt(
    root: &Path,
    user_prompt: &str,
    referenced_files: &[String],
) -> Result<String, AppError>;
```

Use canonical containment checks, regular-file metadata, content sampling, explicit ignore sets, deterministic sorting, and stable localized-safe reference errors.

- [ ] **Step 4: Run project-file and error tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib work::project_files::tests error::tests`

Expected: pass.

### Task 3: Deliver separate persisted and engine prompts

**Files:**
- Modify: `src-tauri/src/work/commands.rs`
- Modify: `src-tauri/src/work/service.rs`
- Modify: `src-tauri/src/engine/supervisor.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/engine/supervisor.rs`
- Test: `src-tauri/tests/work_lifecycle.rs`

- [ ] **Step 1: Write a failing supervisor contract test**

Use a recording fake engine to prove the repository persists `Review @src/main.ts` while the adapter receives a prompt containing the current file contents and relative path.

- [ ] **Step 2: Run the focused test and verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib engine::supervisor::tests::persists_user_prompt_and_sends_separate_engine_prompt`

Expected: failure because supervisor accepts only one prompt.

- [ ] **Step 3: Separate user and engine prompts through the lifecycle**

Change the supervisor boundary to:

```rust
pub async fn start(
    &self,
    work_id: &str,
    user_prompt: &str,
    engine_prompt: String,
) -> Result<StartWorkOutput, AppError>;
```

`WorkService` loads the Work root, builds the engine prompt immediately before `supervisor.start`, and passes the original prompt to repository persistence. Register `list_project_files` in Tauri.

- [ ] **Step 4: Run lifecycle tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib engine::supervisor::tests`

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test work_lifecycle`

Expected: pass without replacing the running desktop executable.

### Task 4: Add a pure mention editor model

**Files:**
- Create: `src/features/workspace/fileMentions.ts`
- Create: `src/features/workspace/fileMentions.test.ts`

- [ ] **Step 1: Write failing model tests**

Test trigger detection after whitespace, query extraction, insertion at the caret, structured-reference synchronization after edits, case-insensitive path filtering, and shortest unique duplicate labels.

- [ ] **Step 2: Run the model test and verify RED**

Run: `pnpm test src/features/workspace/fileMentions.test.ts`

Expected: compilation failure because the module is absent.

- [ ] **Step 3: Implement pure mention functions**

Use a stable serialized token `@{project/relative path}` for unambiguous paths with whitespace, keep selected references as relative paths, and expose display labels separately.

- [ ] **Step 4: Run model tests**

Run: `pnpm test src/features/workspace/fileMentions.test.ts`

Expected: pass.

### Task 5: Build the reusable mention-aware prompt editor

**Files:**
- Create: `src/features/workspace/ProjectPromptEditor.tsx`
- Create: `src/features/workspace/ProjectPromptEditor.test.tsx`
- Modify: `src/styles/workspace.css`
- Modify: `src/i18n/locales/zh-CN.json`
- Modify: `src/i18n/locales/en.json`

- [ ] **Step 1: Write failing interaction tests**

Test lazy project-file loading, `@` menu opening, filename/path filtering, Arrow navigation, Enter/Tab selection, Escape, IME composition suppression, selected-reference removal, retryable loading errors, and accessible labels.

- [ ] **Step 2: Run editor tests and verify RED**

Run: `pnpm test src/features/workspace/ProjectPromptEditor.test.tsx`

Expected: compilation failure because the component is absent.

- [ ] **Step 3: Implement an accessible rich prompt editor**

Use an ARIA multiline `contenteditable` surface with non-editable inline mention spans. Preserve focus, disabled, multiline, and IME behavior; serialize mention spans into stable path tokens and synchronize references when a span is deleted. Style the menu and tokens to match PiWork’s restrained neutral workspace.

- [ ] **Step 4: Run editor and locale tests**

Run: `pnpm test src/features/workspace/ProjectPromptEditor.test.tsx src/i18n/locales/locales.test.ts`

Expected: pass.

### Task 6: Integrate project search and mentions into both composers

**Files:**
- Modify: `src/features/workspace/NewWorkStart.tsx`
- Modify: `src/features/workspace/WorkComposer.tsx`
- Modify: `src/features/workspace/WorkSurface.tsx`
- Modify: `src/features/works/workStore.ts`
- Modify: `src/features/works/workStore.test.ts`
- Modify: `src/features/workspace/WorkSurface.test.tsx`
- Modify: `src/app/App.test.tsx`

- [ ] **Step 1: Write failing integration tests**

Prove there is no working-directory textbox, recent projects are searchable with a selected checkmark, native browsing still works, both composers pass references, changing project clears old references, and queued entries retain `{ prompt, referencedFiles }`.

- [ ] **Step 2: Run integration tests and verify RED**

Run: `pnpm test src/features/workspace/WorkSurface.test.tsx src/features/works/workStore.test.ts`

Expected: failures for the manual path field and string-only instructions.

- [ ] **Step 3: Integrate the shared editor and structured queue**

Replace manual directory input with project search and native browsing. Use `ProjectPromptEditor` in `NewWorkStart` and `WorkComposer`; pass references through store/client calls and store queued instructions as:

```ts
type QueuedInstruction = {
  prompt: string;
  referencedFiles: string[];
};
```

- [ ] **Step 4: Run the frontend suite and typecheck**

Run: `pnpm test`

Run: `pnpm typecheck`

Expected: 0 failures and 0 type errors.

### Task 7: Full verification and requirement audit

**Files:**
- Verify all modified files only.

- [ ] **Step 1: Run formatting and static checks**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`

Run: `pnpm typecheck`

Expected: pass.

- [ ] **Step 2: Run fresh frontend and Rust verification**

Run: `pnpm test`

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib`

Run each integration test target individually if the active desktop process still prevents Cargo from replacing `piwork.exe`.

- [ ] **Step 3: Build production artifacts**

Run: `pnpm build`

Run: `cargo check --manifest-path src-tauri/Cargo.toml --lib`

Expected: pass.

- [ ] **Step 4: Audit the final diff against acceptance criteria**

Confirm: no manual path input; searchable recent projects; both composers support `@`; ignored/binary/secret/oversized files never appear; execution-time validation is authoritative; persisted prompt stays concise; immediate and queued instructions carry identical references.
