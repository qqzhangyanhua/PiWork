# PiWork Attachment Resource Runtime Design

**Date:** 2026-07-31

**Status:** Approved for implementation planning

**Scope:** Local-first attachment ingestion, durable Work resources, document parsing, OCR, retrieval, Pi context integration, and future encrypted personal sync

## 1. Product decision

PiWork distinguishes two resource concepts that must not share the same retention semantics:

1. A project-file mention created with `@` is a live `WorkspaceReference`. It points at a file in the selected project, is resolved at Run time, and is not copied into PiWork's durable attachment library.
2. A file explicitly uploaded by the user is a durable `ManagedResource`. PiWork imports an immutable copy, links it to the current Work and message, exposes it in the Work attachment list, and can later use it as a source for long-term memory.

The first release supports reliable upload, parsing, offline OCR, retrieval, reading, and source citation for images, PDF, DOCX, XLS/XLSX/XLSM/XLSB, CSV, and plain-text files. Creating or editing Office/PDF artifacts is a later capability pack and is not part of this release.

The local implementation is the first storage backend, not a local-path domain model. Future personal cross-device sync adds an encrypted S3 replica without changing resource identities, Work associations, frontend APIs, or Pi engine semantics. Team spaces are a future ownership scope.

## 2. Goals

- Let users select one or more local files from either composer without typing a path.
- Preserve explicitly uploaded files as durable Work resources.
- Keep `@` project references live and non-duplicated.
- Support text PDFs and scanned PDFs with local OCR.
- Preserve document structure and stable locators such as page, section, Sheet, and cell range.
- Prevent large attachments from being eagerly injected into the model context.
- Reuse Pi's native image input and bounded `read`/search behavior without forking Pi.
- Keep storage, parsing, retrieval, and memory contracts engine-neutral.
- Keep all document parsing local and compatible with future end-to-end encrypted sync.
- Make document parsing failures isolated, recoverable, and visible in product language.

## 3. Non-goals

- Editing or creating DOCX, XLSX, PDF, or PPTX in the first attachment release.
- Sharing personal resources with a Team.
- Implementing S3, accounts, device enrollment, or key recovery in the local release.
- Server-side document parsing, OCR, embedding, search, or agent execution.
- Automatically turning every uploaded document into global memory.
- Eagerly sending all Work attachments to every Run.
- Executing Office macros, embedded programs, or document-provided external links.

## 4. Vocabulary and resource classes

### 4.1 WorkspaceReference

A `WorkspaceReference` is a borrowed reference to a mutable project file.

```ts
type WorkspaceReference = {
  kind: "workspace_reference";
  rootPath: string;
  relativePath: string;
  observedSize?: number;
  observedModifiedAt?: string;
};
```

Properties:

- The reference is constrained to the current Work root.
- PiWork does not copy it into managed resource storage.
- The latest file is resolved when a Run starts.
- It is rendered inline as an `@` mention, not listed in the Work attachment library.
- If the file moves or disappears, the Run fails with a precise stale-reference error.
- Reading the file during a Pi Run can still place read content in the private Pi session. "Not retained as an attachment" does not promise that execution context contains no file excerpts.

### 4.2 ManagedResource

A `ManagedResource` is an immutable logical object created from an explicit upload.

```ts
type ManagedResource = {
  id: string;
  spaceId: string;
  blobId: string;
  originalName: string;
  mediaType: string;
  size: number;
  status: "staging" | "processing" | "ready" | "failed" | "deleting";
  createdAt: string;
};
```

Properties:

- The source path is used only during import and is never the resource identity.
- The original filename is metadata; it is not used as a storage key.
- Uploading the same bytes can reuse a physical blob while creating or reusing logical links.
- A different file with the same filename creates a different immutable resource version.
- The resource remains available after the source file moves or is deleted.

### 4.3 GeneratedArtifact

Agent-generated files will later use the same durable resource infrastructure with `origin = generated_artifact`. They are outside the first release UI, but the resource model must not assume every durable resource came from a user file picker.

## 5. Ownership, visibility, and future collaboration

### 5.1 Space is the ownership boundary

Every Work and durable resource belongs to a stable Space:

```ts
type Space = {
  id: string;
  kind: "personal" | "team";
};
```

The first local installation creates one Personal Space with a client-generated stable ID. No account or membership UI is required. A future cloud account adopts or maps this ID into the user's remote Personal Space.

Future Team support adds membership and roles without changing Work or resource ownership:

```ts
type SpaceMember = {
  spaceId: string;
  userId: string;
  role: "owner" | "admin" | "member" | "viewer";
};
```

Team membership is not implemented now.

### 5.2 Ownership and visibility are separate

A personal upload belongs to the Personal Space but is visible only through explicit resource links:

```ts
type ResourceLink = {
  resourceId: string;
  workId: string;
  messageId?: string;
  runId?: string;
  role: "attached" | "pinned" | "memory_source";
  createdAt: string;
};
```

- `attached` records that a message used the resource.
- `pinned` makes the resource prominent and eligible for Work-level retrieval, not automatic full-context injection.
- `memory_source` allows derived, cited memory to reference the resource.
- Cross-Work reuse adds another explicit Work link.
- Moving a personal resource into a future Team Space is an explicit copy operation with a new Team-owned resource record and audit event.

## 6. Blob storage and replicas

### 6.1 Stable resource identity is independent of storage

```ts
type ResourceBlob = {
  id: string;
  plaintextHash: string;
  size: number;
};

type BlobReplica = {
  blobId: string;
  storeId: string;
  objectKey: string;
  version?: string;
  state: "pending" | "ready" | "failed" | "deleting";
};
```

The plaintext hash is locally protected metadata. It is never used as a public S3 key and is not exposed to an untrusted sync service.

The first release creates only a `local-default` replica. Personal sync later adds an encrypted `s3-personal` replica. A second device may initially have only remote metadata, then create a local cache replica on demand. Local and S3 are replicas, not mutually exclusive backends.

### 6.2 BlobStore contract

```rust
#[async_trait]
trait BlobStore: Send + Sync {
    async fn put(&self, input: BlobInput) -> Result<StoredBlob, ResourceError>;
    async fn open(&self, reference: &StorageRef) -> Result<BlobReader, ResourceError>;
    async fn stat(&self, reference: &StorageRef) -> Result<BlobMetadata, ResourceError>;
    async fn delete(&self, reference: &StorageRef) -> Result<(), ResourceError>;
}
```

The interface is asynchronous and streaming from the first local implementation so S3 does not require an API redesign.

### 6.3 Resource materialization

Pi and document processors operate on a local file lease rather than storage credentials:

```rust
#[async_trait]
trait ResourceMaterializer: Send + Sync {
    async fn acquire_local(
        &self,
        resource_id: &str,
        purpose: MaterializationPurpose,
    ) -> Result<ResourceLease, ResourceError>;
}
```

- Local storage can return a read-only managed path or safe copy.
- S3 downloads and verifies an encrypted object before producing a local lease.
- The lease owns cleanup for temporary plaintext.
- Pi never receives S3 credentials, signed URLs, encryption keys, or source paths.

## 7. Local import lifecycle

The import lifecycle is an explicit state machine because the database and blob store do not share a transaction:

```text
selected
  -> staging
  -> original stored and hash verified
  -> processing
  -> derivatives committed
  -> ready
```

Failure transitions the resource to `failed` with a product-safe error code and internal diagnostics. Retry is idempotent.

The new-Work composer does not yet have a Work ID. It creates a stable draft ID and imports resources as Space-owned staging resources associated with that draft. Creating the Work consumes the draft: the backend links its ready resources to the new Work and first message in the same authoritative start flow. Abandoned draft resources are unlinked and garbage-collected after a grace period. An existing-Work composer can link imported resources to its Work immediately.

Local import steps:

1. The composer supplies either an existing Work ID or a new-Work draft ID.
2. The Tauri file picker returns source paths for the current interaction only.
3. The backend opens files without following unexpected links and validates type by signature, not extension alone.
4. It creates staging records and streams each source into a temporary local object while computing a plaintext hash.
5. It applies size, archive-entry, decompression-ratio, page, Sheet, row, and column limits.
6. It atomically promotes the local blob and creates a `ready` replica record.
7. It invokes the document runtime in an isolated process.
8. It stores versioned derivatives and creates or prepares the authoritative Work/message links.
9. It marks the resource `ready` and emits progress to the composer.

Startup recovery removes expired staging files, resumes safe processing, and garbage-collects unreferenced blobs after a grace period. Deletion uses tombstones so future devices cannot resurrect a deleted resource.

## 8. Document Runtime Pack

### 8.1 Packaging boundary

Document processing is a versioned PiWork capability pack, not part of Pi and not implemented as an LLM Skill. It runs as an isolated local sidecar with a narrow JSON-lines protocol.

The pack returns deterministic resource derivatives. Agent Skills may later teach Pi how to use those derivatives, but parsing, OCR, encryption, limits, and storage never depend on the model choosing to follow a Skill.

### 8.2 Recommended components

- `docling-slim` with PDF, Office, chunking, and extraction extras for a unified document model.
- `pypdfium2` and `docling-parse` for PDF text, layout, page rendering, and reading order.
- RapidOCR with ONNX Runtime as the default offline OCR engine.
- Tesseract 5 with Simplified Chinese and English data as a fallback and diagnostic baseline.
- `python-docx` through Docling for DOCX semantic extraction.
- Rust `calamine` for workbook manifests, values, formulas, defined names, hidden Sheet metadata, and macro detection.

The first pack excludes Torch, local VLMs, audio/video models, HTML browser rendering, and remote services. Optional model licenses must be audited separately from Docling's MIT code license.

Apache Tika is not the default because it adds a Java runtime and prioritizes broad text/metadata extraction over the layout-preserving representation required by PiWork. It remains a possible fallback for long-tail formats.

Codex Documents/PDF/Spreadsheets/Presentations Skills are workflow references only. Their host-managed `@oai/artifact-tool` and bundled runtime are not PiWork runtime dependencies and must not be redistributed without an explicit license.

### 8.3 Canonical derivative layout

Each processor emits a stable PiWork-owned format:

```text
derivatives/<processor-version>/
  manifest.json
  document.json
  content.md
  chunks.jsonl
  pages/page-0001.png
  assets/*
  sheets/<sheet-id>/metadata.json
  sheets/<sheet-id>/values.csv
  sheets/<sheet-id>/formulas.csv
```

- `manifest.json` contains type, page/Sheet counts, languages, processing warnings, and derivative paths.
- `document.json` stores the lossless normalized document tree.
- `content.md` provides a human- and agent-readable representation.
- `chunks.jsonl` contains bounded retrieval units with stable locators.
- Page images are generated on demand unless OCR or layout extraction already requires them.
- Spreadsheet outputs are per Sheet so a large workbook is never materialized as one prompt.

### 8.4 Stable document model

PiWork owns the schema exposed to retrieval and memory:

```ts
type DocumentNode = {
  id: string;
  kind:
    | "heading"
    | "paragraph"
    | "list_item"
    | "table"
    | "image"
    | "formula"
    | "sheet_range";
  text?: string;
  locator: {
    page?: number;
    section?: string;
    sheet?: string;
    range?: string;
  };
  children?: DocumentNode[];
};
```

Docling, OCR, and Calamine outputs are adapters into this schema. Replacing a parser does not change Work data, attachment APIs, memory citations, or Pi tools.

### 8.5 Format behavior

#### PDF

- Extract native text and reading order first.
- Detect pages with insufficient usable text and run local OCR only for those pages.
- Preserve page numbers, tables, images, bounding boxes, and confidence warnings.
- Keep page rendering available for visual inspection by a vision-capable model.
- Reject password-protected PDFs with a clear error until password input is designed.

#### DOCX

- Preserve headings, paragraphs, lists, tables, footnotes/endnotes where available, comments metadata, and embedded images.
- Do not execute macros or fetch external relationships.
- Produce semantic content for retrieval; pixel-perfect Word layout reproduction is not required for read-only attachment analysis.

#### Excel

- Create a workbook manifest before reading cell bodies.
- Preserve Sheet name, hidden state, used range, values, formulas, defined names, and macro presence.
- Export values and formulas separately with stable A1 locators.
- Do not calculate unsupported formulas or claim cached values are freshly recalculated.
- Pi reads the manifest first, then a requested Sheet and bounded range.

#### Images and scanned documents

- Support PNG, JPEG, WebP, GIF, TIFF, and BMP subject to limits.
- Run local OCR for textual retrieval and retain the original image for vision tasks.
- Default OCR languages are Simplified Chinese and English.
- Preserve OCR confidence and geometry; low-confidence text is marked rather than silently trusted.

#### Presentations

PPTX ingestion is compatible with the pack and canonical model but is not required by the first acceptance gate. It can be enabled after PDF/DOCX/Excel quality is established.

## 9. Parser isolation and hostile input

All uploaded files are untrusted. The document runtime process has:

- no network access;
- a read-only input lease;
- a dedicated writable temporary directory;
- CPU, memory, wall-time, output-size, and child-process limits;
- strict archive entry and decompression-ratio limits;
- disabled Office macros and external relationships;
- sanitized HTML/Markdown output;
- no access to API keys, Pi sessions, project files, or other attachments.

Crashes, timeouts, and malformed output fail only the affected resource. The parent validates every sidecar response before committing derivatives. Parser versions and dependency licenses are recorded in the capability-pack manifest and product NOTICE.

## 10. Engine-neutral context integration

### 10.1 Start input

The frontend sends resource IDs, never durable local paths:

```ts
type StartWorkInput = {
  prompt: string;
  referencedFiles: string[];
  resourceIds: string[];
};
```

Queued instructions copy the resource IDs. The backend validates that each ready resource is linked to the Work before starting the Run.

### 10.2 Engine input

The engine abstraction changes from a bare string to a typed input:

```rust
struct EngineInput {
    instruction: String,
    resources: Vec<EngineResource>,
    images: Vec<EngineImage>,
}
```

The Pi adapter serializes native image blocks through Pi RPC's `images` field and provides a compact resource manifest in the text instruction. The fake engine and future adapters consume the same engine-neutral input.

Pi does not need to be forked. The bundled Pi RPC already accepts images, and its `read` tool supports bounded text reads and image input. PiWork initially materializes normalized files and gives Pi read-only paths. A later signed PiWork extension can expose richer resource tools without moving resource ownership into Pi.

### 10.3 Resource tools

The long-term engine-neutral tool surface is:

```text
attachment_list
attachment_search
attachment_read
attachment_read_page
attachment_read_sheet
```

- `attachment_list` returns compact metadata and processing status.
- `attachment_search` returns bounded snippets and stable locators.
- `attachment_read` reads a bounded structured chunk.
- `attachment_read_page` returns page text and optionally a rendered image.
- `attachment_read_sheet` accepts Sheet and A1 range.

Every result includes `resourceId` and the narrowest available page, section, Sheet, or range locator.

## 11. Context budget

The governing rule is: **available is not injected**.

- The Work attachment library may contain any number of resources subject to storage limits.
- Only resources explicitly active on the current message are mentioned in the initial Run manifest.
- Pinned resources are eligible for retrieval but are not fully included automatically.
- The manifest contains metadata, not document bodies, and has a hard serialized-size limit.
- Search results are capped by hit count and snippet bytes.
- Read tools are paginated and enforce per-call bytes and estimated-token limits.
- Large spreadsheets require an explicit Sheet and range after the manifest is read.
- Vision input is added only for explicitly active images or pages selected for visual inspection.

The first implementation should instrument actual model token usage before fixing a permanent product-wide budget. Safe defaults are a small metadata-only manifest, no eager document body, at most 20 search hits, and an approximately 8,000-token default ceiling per read response.

## 12. Work attachment UX

### 12.1 Composer

- A `+` button opens a compact menu with "Add photos or files".
- The native picker supports multiple selection and appropriate file filters.
- Selected resources appear as compact chips or image thumbnails above the action row.
- The UI shows filename, concise type/size information, processing state, and remove action; it does not show full source paths.
- Removing a draft selection unlinks it from the draft. It does not delete an already durable Work resource.
- A prompt can be sent when it contains text or at least one ready attachment.
- New-Work draft uploads survive an incidental view change during the draft but are not permanent until the Work is created; abandoned drafts are collected after the configured grace period.

### 12.2 Work attachment library

The Work exposes:

- **This message:** resources active on the current draft.
- **Work files:** durable uploads linked anywhere in the Work.
- **Pinned:** resources eligible for prominent Work retrieval.
- **Generated artifacts:** later, agent-created resources.

Timeline messages show only resources used by that message. `@` workspace mentions remain inline in message text and do not appear in the durable attachment library.

Selecting an existing Work file for a new message creates another link; it does not upload or copy the blob again.

### 12.3 User-visible states

Local states:

```text
importing -> processing -> ready | failed
```

Future sync states:

```text
local_only | uploading | synced | remote_only | downloading | sync_failed
```

Processing and sync are separate dimensions. A resource can be locally ready while its cloud replica is still uploading.

## 13. Long-term memory

An attachment is evidence, not memory. PiWork never automatically copies an entire resource into every future context.

Memory promotion creates cited knowledge:

```ts
type MemoryEntry = {
  id: string;
  spaceId: string;
  workId?: string;
  content: string;
  sourceResourceId: string;
  locator: {
    page?: number;
    section?: string;
    sheet?: string;
    range?: string;
  };
  scope: "work" | "personal";
  createdAt: string;
};
```

The first release stores derivatives needed for later retrieval but does not implement automatic memory promotion. A later workflow searches attachments, produces candidate facts with citations, and requires an explicit product policy or user action before Personal-scope promotion.

Deleting a source resource tombstones or invalidates dependent memories according to the selected deletion policy. The UI must not present a memory as sourced if its source is unavailable.

## 14. Future personal sync and end-to-end encryption

The sync service is untrusted for content. It stores ciphertext and routing metadata but cannot parse, index, summarize, or run agents over attachments.

### 14.1 Key hierarchy

```text
user/device recovery secret
  -> wraps Personal Space key
      -> wraps random per-blob data key
          -> encrypts original and derivative blobs
```

- Raw Space and data keys are never stored in SQLite or S3.
- Device-held keys live in the OS credential vault.
- Object keys are random and reveal neither filename nor plaintext hash.
- Sensitive metadata, derivatives, indexes, and memory entries are encrypted.
- New-device enrollment requires approval from an existing device or a recovery key.

Chunked authenticated encryption allows streaming and bounded materialization. The exact algorithm and envelope format are deferred to a security-focused design review, but the storage schema already separates resource identity, replica location, and protected plaintext hash.

### 14.2 Team compatibility

A future Team Space key is wrapped independently for each member. Removing a member requires key rotation for future access. The local release does not implement this, but no schema assumes one Space has one device or one permanent owner key.

## 15. Error handling

Product errors are typed and localized:

- unsupported or mismatched file type;
- file exceeds configured size;
- archive expansion exceeds safe limits;
- password-protected document;
- malformed or corrupted document;
- parser timeout or crash;
- OCR unavailable or low confidence;
- workbook exceeds Sheet/cell limits;
- insufficient local disk space;
- resource is still processing;
- stale or missing `@` workspace reference;
- resource is not linked to the current Work;
- resource download/decryption/integrity failure in future sync.

A failed attachment does not fail unrelated attachments. A Run cannot start with an active failed or unready resource; the composer identifies the exact file and offers retry or removal.

## 16. Verification strategy

### 16.1 Unit tests

- Resource and link authorization rules.
- Import state-machine transitions and idempotent recovery.
- Content hashing, deduplication, reference counting, tombstones, and garbage collection.
- Storage-key opacity and path traversal rejection.
- Context budget, pagination, and locator preservation.
- Workspace-reference resolution without managed-blob creation.

### 16.2 Parser contract tests

Maintain a versioned fixture corpus with:

- native and scanned Chinese/English PDFs;
- multi-column PDFs, tables, images, forms, and malformed objects;
- DOCX headings, lists, tables, comments, footnotes, images, and external relationships;
- XLS/XLSX/XLSM/XLSB with formulas, cached values, hidden Sheets, defined names, large ranges, and macros;
- password-protected, corrupted, oversized, and archive-bomb fixtures.

Each fixture asserts canonical nodes, locators, warnings, bounded output, and deterministic processor versioning. Golden outputs are reviewed when parser versions change.

### 16.3 Security tests

- Network isolation.
- External relationship and macro non-execution.
- ZIP slip, path traversal, symlink, archive bomb, and oversized-output rejection.
- Sidecar timeout, crash, malformed JSON, and resource exhaustion containment.
- No access to project files, credentials, Pi sessions, or sibling attachments.

### 16.4 Integration tests

- Upload in both new-Work and existing-Work composers.
- Queue an instruction with resource IDs and run it after the source file is deleted.
- Reuse an existing Work resource without duplicating the blob.
- Start Pi with a compact manifest and native image input.
- Have Pi search and read a PDF page and Excel range with verified locators.
- Reopen the database and recover the Work attachment library.
- Delete links and verify grace-period garbage collection.

### 16.5 End-to-end acceptance

The first release is accepted when a user can:

1. Upload images, PDF, DOCX, and Excel files from either composer.
2. Close or move the source files and continue using the durable Work copies.
3. Reopen PiWork and see the same Work attachment list.
4. Ask questions over a native PDF, scanned Chinese/English PDF, Word document, and multi-Sheet workbook.
5. Receive answers whose supporting excerpts retain page or Sheet/range locators.
6. Observe that a large document is searched and read in bounded pieces rather than inserted wholesale into the initial prompt.
7. Use `@` project references without creating durable attachment records.

## 17. Delivery phases

### Phase 1: local durable resources

- Space/resource/blob/link schema.
- Local BlobStore and materializer.
- Composer picker, draft chips, Work attachment library.
- Import state machine and recovery.

### Phase 2: local document runtime

- Versioned sidecar protocol and isolation.
- PDF/DOCX/image parsing and offline OCR.
- Calamine workbook processor.
- Canonical derivatives and fixture corpus.

### Phase 3: Pi context and retrieval

- Typed `EngineInput` with resources and images.
- Compact manifests and bounded read/search tools.
- Timeline attachment rendering and source locators.

### Phase 4: personal encrypted sync

- Account/device model and Personal Space adoption.
- Encrypted S3 replicas, local cache, tombstones, and recovery flow.
- Encrypted derivatives, indexes, and memory metadata.

### Phase 5: artifact authoring and collaboration

- PiWork-owned Documents/PDF/Spreadsheets/Presentations Skills.
- Deterministic create/edit tools with render-and-verify workflows.
- Generated artifacts as managed resources.
- Team Space membership, explicit personal-to-Team copy, and key rotation.

## 18. Implementation constraints

- Pi remains a hidden, replaceable engine behind PiWork's adapter.
- The local release does not expose Pi terminology in product UI.
- PiWork owns resource identities, storage, parsing, retrieval, memory, and authorization.
- No frontend or persisted domain record relies on a durable absolute local path.
- No file body is eagerly included solely because it is available in the Work library.
- Dependency and model licenses are pinned, audited, and included in NOTICE before shipping.

## 19. Primary references

- PiWork existing product design: `docs/superpowers/specs/2026-07-28-piwork-design.md`
- PiWork existing `@` design: `docs/superpowers/specs/2026-07-30-project-file-mentions-design.md`
- Docling: <https://github.com/docling-project/docling>
- Docling modular dependencies: <https://github.com/docling-project/docling/blob/main/pyproject.toml>
- Docling supported formats: <https://github.com/docling-project/docling/blob/main/docs/usage/supported_formats.md>
- Calamine: <https://github.com/tafia/calamine>
- Tesseract: <https://github.com/tesseract-ocr/tesseract>
- Apache Tika: <https://github.com/apache/tika>
