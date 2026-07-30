# PiWork Project Selection and File Mentions Design

**Date:** 2026-07-30

## Goal

Make local project selection a searchable, picker-driven interaction and let users reference project files with `@` so Pi receives those files as explicit, current context when a Run begins.

## Scope

This design covers two related improvements to the new-Work and existing-Work composers:

1. Replace manual working-directory entry with a searchable recent-project list and the native folder picker.
2. Add structured project-file mentions to both composers and carry those references through queued and immediate Runs.

The Work root remains mandatory. PiWork will not add a “work outside a project” mode in this change.

## User Experience

### Project selection

The project chip opens a popover modeled on the reference interaction:

- A search field filters the distinct project roots found in recent Works.
- Each result shows a folder icon and project folder name. A duplicate folder name also shows enough parent-path context to distinguish it.
- The selected project displays a checkmark.
- “Choose another folder…” opens the native directory picker.
- The popover never displays an editable filesystem-path field.
- With no recent projects, the popover shows only the empty-state guidance and “Choose another folder…”.
- The send action remains disabled until a project is selected.

Changing the selected project removes all file mentions from the previous project while preserving the rest of the draft text.

### File mentions

Typing `@` at the start of the draft or after whitespace opens a project-file menu above the composer. Text after `@` filters by filename and relative path. Matching is case-insensitive where the platform filesystem is case-insensitive.

Keyboard behavior:

- `ArrowUp` and `ArrowDown` move through results.
- `Enter` or `Tab` selects the active result.
- `Escape` closes the menu without changing the draft.
- Chinese IME composition never triggers or selects a result until composition ends.

A selected file becomes an atomic inline mention. The mention normally shows the filename; duplicates show the shortest unique parent-path suffix, such as `workspace/index.ts`. The editor serializes the mention as the unambiguous project-relative path for the visible user message and separately records a structured reference. Deleting the mention atomically removes its structured reference.

Both the first task used to create a Work and all later Work instructions support file mentions. Queued instructions retain their structured references.

## Architecture

### Project file index

A dedicated Tauri command enumerates referenceable files below a supplied Work root. The backend owns traversal and filtering so the frontend never receives unsafe or irrelevant candidates. The frontend lazily loads the index for the selected project, caches it for the current composer, and reloads it when the project changes or the composer is remounted.

The command returns lightweight summaries containing the project-relative path and display metadata. It never returns file contents.

### Structured Run input

The Run-start boundary changes from a bare `prompt` string to an input containing:

- The serialized user prompt.
- A list of project-relative referenced file paths.

The same structure is used by frontend queued instructions. The original prompt remains the value persisted as the user message and rendered in the Work timeline.

Immediately before the engine starts, the backend resolves and reads every referenced file against the Work root. It then builds a separate engine prompt containing the original user prompt plus delimited reference context. The expanded engine prompt is not stored as the visible user message.

### Agent context format

Each referenced file is wrapped in a deterministic boundary carrying its relative path. The wrapper tells the model that file contents are reference data and not additional instructions. Paths and boundary values are escaped before prompt construction.

Conceptually, the engine receives:

```text
<user_instruction>
…original serialized prompt…
</user_instruction>

<referenced_files>
  <file path="src/example.ts">
  …content read when the Run begins…
  </file>
</referenced_files>
```

The implementation may use an equivalent collision-resistant delimiter, but it must keep the original instruction and file data visibly separate.

## Filtering and Limits

Traversal recursively includes regular text files and excludes obvious noise or unsafe context.

Excluded directories include version-control metadata, dependency trees, generated build outputs, caches, virtual environments, and platform trash or temporary directories. At minimum this includes `.git`, `node_modules`, `dist`, `build`, `out`, `target`, `coverage`, `.next`, `.cache`, `__pycache__`, `.venv`, and `vendor`.

Excluded files include:

- Binary files detected by extension or content sampling.
- Archives, images, audio, video, executables, libraries, fonts, and disk images.
- Secret-bearing environment and credential files, including `.env` and `.env.*`.
- Files larger than 256 KiB.

A single instruction can reference at most 10 files and at most 512 KiB of combined content. Index results exclude individually oversized files. The backend enforces every rule again when the Run starts because files may change after selection or while an instruction is queued.

## Path Safety

Every reference must be a normalized relative path. The backend rejects:

- Absolute paths.
- Parent traversal such as `..`.
- Missing or non-regular files.
- Canonical paths outside the canonical Work root.
- Symbolic links or junctions that resolve outside the Work root.

The execution-time check is authoritative. A path previously returned by the index is not trusted after it crosses the command boundary.

## Errors and Recovery

If indexing fails, the mention menu shows a localized project-file loading error and allows retry without losing the draft.

If any selected file is missing, unreadable, no longer text, oversized, outside the Work root, or causes the total context limit to be exceeded, PiWork rejects that instruction before starting the engine. The product-safe error identifies the affected relative path and leaves the visible instruction recoverable for correction and resubmission.

No reference failure is silently ignored. A Run must never start with only a subset of the files the user explicitly selected.

## Component Boundaries

- The project picker owns recent-project filtering, selection, native browsing, and duplicate-name labels.
- A reusable prompt editor owns inline mention rendering, caret-trigger detection, keyboard interaction, IME handling, and prompt/reference serialization.
- A mention model module owns pure operations such as inserting/removing mentions, deriving structured references, and shortest-unique-path labels.
- The Tauri file-index command owns recursive enumeration, filtering, and safe relative-path production.
- The Run-context resolver owns execution-time validation, content loading, limits, and agent-context assembly.
- The Work store owns immediate and queued structured instructions but does not read files or build model prompts.

These boundaries keep filesystem policy and security in Rust while keeping editor behavior independently testable in TypeScript.

## Testing

### Rust

- Recursively enumerate supported project text files.
- Exclude every configured noise directory and disallowed file class.
- Reject absolute paths, parent traversal, and symlink or junction escapes.
- Enforce individual-file, file-count, and combined-content limits.
- Read the content present at execution time rather than selection time.
- Keep the persisted user prompt separate from the expanded engine prompt.
- Fail the complete instruction when any referenced file is invalid.

### React and TypeScript

- Search recent projects and choose the current project without a path input.
- Browse through the native picker and show the selected project.
- Open and filter the mention menu from both composers.
- Navigate and select by keyboard, close with Escape, and preserve Chinese IME composition.
- Insert and atomically delete mentions while keeping structured references synchronized.
- Use shortest unique labels for duplicate filenames.
- Clear old-project mentions after changing projects.
- Preserve structured references in queued instructions.
- Show loading and execution-time reference errors without losing unrelated draft text.

### End-to-end contract

- The Work timeline stores and displays only the original serialized prompt.
- Pi receives the original instruction plus the latest valid contents of every referenced file.
- A queued instruction resolves file contents only when its Run actually begins.

## Acceptance Criteria

1. Users never need to type a local directory path to create a Work.
2. Typing `@` in either composer produces a keyboard-accessible list of relevant files from the active project.
3. Selecting a file creates an atomic inline mention and a structured project-relative reference.
4. The backend validates and reads every referenced file immediately before engine startup.
5. The timeline remains concise while the agent receives explicit file context.
6. Noise, binary, secret-bearing, oversized, out-of-root, and stale references are excluded or rejected predictably.
7. Immediate and queued instructions obey the same reference semantics.
