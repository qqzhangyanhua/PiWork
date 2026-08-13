# PiWork 主理委派、Result Envelope 与 Context Builder Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 B 的长期成员与 C 的持久调度底座上实现第一个完整多 Agent 闭环：用户只对话 Work/主理人，主理人通过宿主工具按需委派研究员、工程师或审阅者，成员返回可验证 Result Envelope，主理 Assignment 等待后恢复、综合并完成最终交付。

**Architecture:** `LeadToolService` 是所有宿主工具的权威 Rust 边界；Pi 通过 PiWork 自带、每 Run 生成的受限 extension 调用 localhost loopback tool bridge，bridge 将 `run_id + unique run-scoped token` 映射回服务端授权上下文，不把数据库或任意 Tauri command 暴露给模型。`ContextBuilder` 按固定八层顺序生成 EngineInput，`ResultEnvelope` 和 Work Ledger 通过 append-only WorkEvent 投影持久化；成员只能提交结果/澄清/委派请求，只有 Lead 可创建子 Assignment。Lead Run 调用委派后以 `waiting_on_assignments` 结束，Scheduler 在依赖终止后恢复 Lead 的新 Run，最终 Delivery 始终由 Lead 提交。

**Tech Stack:** Rust 2024、Tokio、SQLx/SQLite、Serde/serde_json、ts-rs、Tauri 2、Pi RPC extension API、loopback HTTP/JSON、React 19、TypeScript、Zustand、Vitest、Testing Library。

**Design source:** `docs/superpowers/specs/2026-08-09-piwork-agent-assembly-design.md` §§5.1–5.2、6.10、7、8、10.5、11、12–18、19.D、20–21。

**Prerequisites:** A、B、C 全部完成并通过各自完成定义。特别依赖 C 的 `AssignmentRepository`、dependency/waiting 状态、Scheduler、EngineHarness、Agent Session 隔离和真实 Activity identity。

**Plan index:** `docs/superpowers/plans/2026-08-13-piwork-agent-assembly-plan-index.md`。本文件只拥有 D，并在 Task 16 按索引完成最终 A–D 覆盖审计。

---

## 范围、不变量与明确延期

本计划完成后：

1. 用户默认只向 Work Lead 发消息；不存在独立成员聊天页。
2. 主理人默认自己完成，仅在明确专业边界、独立取证或独立审核时委派；一般 Work 已加入成员不超过三个。
3. 只有 Lead Assignment 的 Lead Agent 能调用 `delegate_assignment`；Member 不能直接生成子 Agent。
4. 一个 Assignment 只有一个 assignee；第一版只允许单层 parent→member，禁止 member→member 和任意深度树。
5. 所有内部通信使用 Assignment/Result Event，不写成普通 `messages`。
6. `delegate_assignment` 只持久化并返回 accepted，不在 tool call 内等待执行。
7. Lead 等待成员时不占用持续 Engine turn；依赖终止后通过新的 Lead Run 恢复综合。
8. 成员结果必须通过 `ResultEnvelope` 基础合同和 provenance 校验；失败只允许一次结构化修复尝试。
9. 主理默认只看到 Result Envelope、Work 决定和显式引用，不复制完整成员工具日志。
10. Work Ledger 从 append-only events 投影，不维护第二份可漂移的手写状态。
11. AgentMemory 只有经 Lead 或用户确认的 candidate 才写入；敏感信息、临时路径、推测和大段工具输出拒绝。
12. 权限为 Work ∩ Agent ∩ Capability ∩ Assignment override ∩ Engine capability；委派不能扩大权限。
13. 最终交付只有 Lead 可提交；成员结果不能直接冒充 assistant final delivery。

明确延期到 E：

- Codex/Generic ACP/Claude/Goose Adapter、RemoteRunner。
- 用户从空白 author executable capability pack。
- 同 Work 多个只读 Assignment 并行。
- 任意层级子 Agent、自由 Agent 群聊。

---

## 主理宿主工具的安全传输决策

Pi 官方 extension API 支持 `pi.registerTool()`，RPC mode 可加载显式 `--extension`；现有 PiWork 反而传了 `--no-extensions`。D 采用“进程隔离 + 显式单个内置 extension + loopback bridge”：

```text
Pi model
  -> piwork-host-tools.ts (只注册角色允许的固定工具)
  -> HTTP POST 127.0.0.1:<ephemeral>/tool
     Authorization: unique per-Run lease token
  -> HostToolBridge (验证 run/agent/work/assignment/tool/schema/deadline)
  -> LeadToolService / MemberResultService
  -> SQLite transaction + WorkEvent
```

约束：

- bridge 只绑定 `127.0.0.1`，不监听 LAN。
- token 每 Run 生成，只注入该 Pi 子进程的环境，不写磁盘；进程终止即撤销，日志与事件不记录 token。
- extension 只允许固定 tool name；参数由 Rust schema 再校验。
- Lead 与 Member 加载不同 allowlist；Member 无 `delegate_assignment`、decision、plan、delivery 工具。
- bridge tool call 只做短事务；`delegate_assignment` 返回 accepted，绝不阻塞等待 child。
- 测试必须证明跨 Run token、伪造 Assignment、过期 token、错误角色、超权限参数均 fail closed。

实现合同以仓库内置 `src-tauri/binaries/pi-sidecar/package.json` 的 Pi 版本和官方 extension/RPC 文档为准；执行 Task 8 时先用 conformance test 锁定 `pi.registerTool()`、`--extension` 与 `--no-extensions` 组合，不依赖未固定的全局 Pi 安装。

---

## 文件职责图

### Rust / SQLite

- Create `src-tauri/migrations/0007_work_memory_and_results.sql`：`agent_memory`、`work_memory`、`assignment_results`、`memory_candidates`；必要索引/约束。
- Create `src-tauri/src/domain/collaboration.rs`：Result、Ledger、Memory、host-tool DTO。
- Modify `src-tauri/src/domain/event.rs`：delegation/result/decision/plan/delivery/memory payload。
- Modify `src-tauri/src/domain/mod.rs`、`src/bindings`。
- Create `src-tauri/src/collaboration/mod.rs`。
- Create `src-tauri/src/collaboration/result.rs`：Result Envelope schema 与一次修复策略。
- Create `src-tauri/src/collaboration/ledger.rs`：Work Ledger 纯投影。
- Create `src-tauri/src/collaboration/memory.rs`：candidate/confirmation/safety policy。
- Create `src-tauri/src/collaboration/context.rs`：固定八层 Context Builder、bounded manifest 和 provenance。
- Create `src-tauri/src/collaboration/tools.rs`：Lead/Member tool authorization 与业务操作。
- Create `src-tauri/src/collaboration/tool_bridge.rs`：loopback server、token registry、schema dispatch。
- Create `src-tauri/src/collaboration/service.rs`、`commands.rs`：UI queries/confirmation。
- Modify `src-tauri/src/assignment/repository.rs`、`state_machine.rs`、`scheduler.rs`、`service.rs`：dependencies、Lead waiting/resume、result repair。
- Modify `src-tauri/src/engine/harness.rs`：Context Builder 和 bridge lease 注入。
- Modify `src-tauri/src/engine/mod.rs`：EngineInput 可包含 system/context sections 或 rendered prompt contract。
- Modify `src-tauri/src/engine/pi/mod.rs`：写入内置 extension/config、显式 `--extension`、tool allowlist、清理 secrets。
- Create `src-tauri/assets/piwork-host-tools.ts`：Pi extension，不含业务逻辑。
- Modify `src-tauri/tauri.conf.json`：在 `bundle.resources` 中把 `assets/piwork-host-tools.ts` 映射为 `piwork-host-tools.ts`。
- Modify `src-tauri/src/lib.rs`、`app_state.rs`。
- Create `src-tauri/tests/collaboration_loop.rs`。
- Create `src-tauri/tests/context_builder.rs`。
- Create `src-tauri/tests/host_tool_bridge.rs`。
- Modify `src-tauri/tests/assignment_lifecycle.rs`、`pi_engine.rs`、`storage_contract.rs`。

### React / TypeScript

- Generate collaboration bindings；modify `src/app/tauriClient.ts`、mock。
- Modify `src/features/workspace/WorkTimeline.tsx`：语义化委派/结果/最终交付。
- Modify `src/features/workspace/WorkInspector.tsx`：Team、Assignments、Plan/Decisions、Memory Candidates tabs。
- Create `src/features/workspace/WorkTeamInspector.tsx`。
- Create `src/features/workspace/WorkLedgerInspector.tsx`。
- Create `src/features/workspace/MemoryCandidatesInspector.tsx`。
- Modify Agent Center 的“加入当前 Work”和成员活动跳转。
- Modify Activity projector/presentation/grouping。
- Modify i18n/styles/tests。

---

### Task 1: 冻结 Result、Ledger、Memory 与工具 wire contract

**Files:**
- Create: `src-tauri/src/domain/collaboration.rs`
- Modify: `src-tauri/src/domain/event.rs`
- Modify: `src-tauri/src/domain/mod.rs`
- Generate: `src/bindings/ResultEnvelope.ts`
- Generate: `src/bindings/WorkLedger.ts`
- Generate: `src/bindings/MemoryCandidateSummary.ts`
- Generate: `src/bindings/LeadTool*.ts`
- Modify: `src/bindings/index.ts`

- [ ] **Step 1: 写失败的 wire 测试**

`ResultEnvelope` 基础字段完全对应设计 §7.4：

```rust
pub struct ResultEnvelope {
    pub status: ResultStatus,
    pub summary: String,
    pub findings: Vec<ResultFinding>,
    pub evidence: Vec<ResultEvidence>,
    pub artifacts: Vec<ResultArtifact>,
    pub validation: Vec<ResultValidation>,
    pub decisions_recommended: Vec<String>,
    pub uncertainties: Vec<String>,
    pub delegation_requests: Vec<DelegationRequest>,
    pub memory_candidates: Vec<MemoryCandidateInput>,
    pub limitations: Vec<String>,
    pub extensions: BTreeMap<String, serde_json::Value>,
}
```

每个 finding/evidence/artifact/validation 必含 provenance：source event/resource/path/command、author agent、assignment、occurredAt 中适用字段。测试拒绝缺失 provenance 的 wire input。

事件 payload 新增：

```text
assignmentDelegated
assignmentResultSubmitted
assignmentResultRejected
delegationRequested
workDecisionRecorded
workPlanUpdated
workDeliveryCompleted
memoryCandidateProposed
memoryCandidateResolved
leadResumed
```

- [ ] **Step 2: 运行测试确认失败**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml domain::collaboration -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml domain::event -- --nocapture
```

Expected: FAIL。

- [ ] **Step 3: 实现 DTO**

`WorkLedger` 至少包含目标、plan/steps、decisions、constraints/permissions、active/waiting/completed assignments、artifacts/validation index、open questions、last delivery。`MemoryCandidateSummary` 包含 source_work_id、source_event_id、author_agent_id、content、reason、version、status、created_at/resolved_at/resolved_by。

Host tool DTO 为严格 tagged enum：

```rust
pub enum HostToolCall {
    ListWorkMembers,
    InspectCapabilityPacks { ids: Vec<String> },
    DelegateAssignment(DelegateAssignmentInput),
    GetAssignmentStatus { assignment_ids: Vec<String> },
    CancelAssignment { assignment_id: String },
    RequestAssignmentRetry { assignment_id: String },
    RecordWorkDecision(RecordWorkDecisionInput),
    UpdateWorkPlan(UpdateWorkPlanInput),
    CompleteWorkDelivery(CompleteWorkDeliveryInput),
    SubmitAssignmentResult(SubmitAssignmentResultInput),
    RequestClarification(RequestClarificationInput),
}
```

- [ ] **Step 4: 导出 bindings**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml domain::tests::export_bindings -- --nocapture
pnpm typecheck
```

Expected: PASS。

- [ ] **Step 5: 提交合同**

```powershell
git add src-tauri/src/domain src/bindings
git commit -m "feat: define collaboration result contracts"
```

---

### Task 2: 创建 Result 与 Memory schema

**Files:**
- Create: `src-tauri/migrations/0007_work_memory_and_results.sql`
- Modify: `src-tauri/tests/storage_contract.rs`

- [ ] **Step 1: 写失败的 migration 测试**

断言设计 §15 剩余表：

```text
agent_memory
work_memory
```

并添加实现 Result/confirmation 所需的：

```text
assignment_results
memory_candidates
```

测试 version、author/source foreign key、status CHECK、一个 Assignment 每 revision 唯一、一个 Work Memory projection row 每 revision 唯一；敏感内容策略不放 SQL trigger，由 Service 测试负责。

- [ ] **Step 2: 运行确认缺表失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test storage_contract collaboration -- --nocapture`

Expected: FAIL。

- [ ] **Step 3: 写 migration**

`assignment_results` 保存 validated envelope JSON、schema version、repair attempt、author/assignment/run/event provenance。`work_memory` 保存 Ledger projection snapshot + source sequence cursor，仍可从 events 重建；它是缓存，不是第二事实源。`agent_memory` 只允许 confirmed candidate source；Repository 事务验证 source。

- [ ] **Step 4: 验证 schema**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test storage_contract -- --nocapture`

Expected: PASS；B+C 表和 legacy data 无回归。

- [ ] **Step 5: 提交 schema**

```powershell
git add src-tauri/migrations/0007_work_memory_and_results.sql src-tauri/tests/storage_contract.rs
git commit -m "feat: persist collaboration results and memory"
```

---

### Task 3: 实现 Result Envelope 校验与一次结构化修复

**Files:**
- Create: `src-tauri/src/collaboration/mod.rs`
- Create: `src-tauri/src/collaboration/result.rs`
- Create: `src-tauri/tests/collaboration_loop.rs`

- [ ] **Step 1: 写失败的 Result 测试矩阵**

覆盖：有效基础合同、capability extension 不覆盖基础字段、缺 provenance、unknown artifact path、validation 声明无对应 event、非成员 author、错误 Assignment/Run、过大数组/字符串、第一次 invalid 返回 repair request、第二次 invalid 标 failed 并通知 Lead。

固定限额：summary 8 KiB、单文本 16 KiB、每数组 128 项、整个 envelope 256 KiB UTF-8；extension key 使用 capability pack stable id。

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test collaboration_loop result -- --nocapture`

Expected: FAIL。

- [ ] **Step 3: 实现 validator**

```rust
pub async fn validate_result(
    repository: &AssignmentRepository,
    context: &ResultSubmissionContext,
    envelope: ResultEnvelope,
) -> Result<ValidatedResultEnvelope, ResultValidationFailure>;
```

不仅做 JSON schema：逐个解析 evidence/resource/event/path/command provenance 并确认属于当前 Work/Assignment 可见范围。validation success 不能仅凭模型文本，必须能关联 `ValidationProduced` 或已知 Run completion validation。

- [ ] **Step 4: 实现一次 repair policy**

第一次失败返回机器可读 diagnostics，Assignment 保持 running，向同一成员 Session 注入修复 prompt；第二次失败 `failed`，持久 `assignmentResultRejected`，Scheduler 唤醒 Lead 处理失败结果。不要无限循环。

- [ ] **Step 5: 运行 tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test collaboration_loop result -- --nocapture`

Expected: PASS。

- [ ] **Step 6: 提交 Result 层**

```powershell
git add src-tauri/src/collaboration src-tauri/tests/collaboration_loop.rs
git commit -m "feat: validate expert result envelopes"
```

---

### Task 4: 实现 Work Ledger append-only 投影

**Files:**
- Create: `src-tauri/src/collaboration/ledger.rs`
- Create: `src-tauri/tests/context_builder.rs`
- Modify: `src-tauri/tests/collaboration_loop.rs`

- [ ] **Step 1: 写纯投影失败测试**

给定乱序 hydrate + live append（按 sequence 归一）证明：目标、plan revision、decision version、constraints、Assignment states、artifact/validation、open questions、last delivery 可从 WorkEvent 重建；重复 event 幂等；迟到旧 revision 不覆盖新 revision；缺失可选事件安全。

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test context_builder ledger -- --nocapture`

Expected: FAIL。

- [ ] **Step 3: 实现 Ledger projector**

```rust
pub fn project_work_ledger(work: &WorkSummary, events: &[WorkEventEnvelope]) -> Result<WorkLedger, LedgerError>;
pub async fn rebuild_and_cache(repository: &WorkLedgerRepository, work_id: &str) -> Result<WorkLedger, AppError>;
```

projection cursor 为最后 event sequence/id；cache 写失败不影响 events 已提交事实，下一读可重建。不要从 UI/store 反写 Ledger。

- [ ] **Step 4: 运行投影与恢复测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test context_builder ledger -- --nocapture`

Expected: PASS。

- [ ] **Step 5: 提交 Ledger**

```powershell
git add src-tauri/src/collaboration/ledger.rs src-tauri/tests/context_builder.rs src-tauri/tests/collaboration_loop.rs
git commit -m "feat: project durable work ledgers"
```

---

### Task 5: 实现固定八层 Context Builder 与 bounded Assignment Packet

**Files:**
- Create: `src-tauri/src/collaboration/context.rs`
- Modify: `src-tauri/src/engine/mod.rs`
- Modify: `src-tauri/src/engine/harness.rs`
- Modify: `src-tauri/src/resource/context.rs`
- Modify: `src-tauri/src/work/project_files.rs`
- Modify: `src-tauri/tests/context_builder.rs`

- [ ] **Step 1: 写上下文顺序和边界失败测试**

精确顺序：

```text
1 PiWork Base Protocol
2 Agent Definition
3 Capability Pack
4 Agent Core Memory
5 Work Brief
6 Current Assignment Packet
7 Dependency Result Envelopes
8 Explicit Files/Attachments/Recent Relevant Messages
```

测试每节独立标题与 source id、Member 不收到完整聊天/工具日志、只注入显式引用、dependency result 压缩、token/char budget 截断次序、路径/附件权限、Lead 与 Member base protocol 差异、prompt injection 标记为不可信数据。

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test context_builder context -- --nocapture`

Expected: FAIL。

- [ ] **Step 3: 实现结构化 section builder**

```rust
pub struct ContextSection { pub kind: ContextSectionKind, pub source_ids: Vec<String>, pub content: String, pub truncated: bool }
pub struct BuiltAssignmentContext { pub sections: Vec<ContextSection>, pub rendered_prompt: String, pub manifest: ContextManifest }
```

Adapter 只接收 builder 输出，不重排 sections。Base Protocol 明确 Lead 的少量委派原则、Member 的禁止委派和必须 submit result 规则。

- [ ] **Step 4: 接入资源/文件选择**

复用现有 bounded document/file loader，但必须由 Assignment `context_manifest` 明确列出。未经引用的历史附件不自动进入成员上下文；错误/缺失资源产生 manifest diagnostic，不静默扩大搜索范围。

- [ ] **Step 5: 接入 Harness**

Scheduler claim 后、Engine start 前构建 context；失败按 Assignment start failure 处理。保存 manifest hash/revision 到 attempt provenance，便于 Result 审计。

- [ ] **Step 6: 运行 context + resource regressions**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test context_builder -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test resource_lifecycle -- --nocapture
```

Expected: PASS。

- [ ] **Step 7: 提交 Context Builder**

```powershell
git add src-tauri/src/collaboration/context.rs src-tauri/src/engine/mod.rs src-tauri/src/engine/harness.rs src-tauri/src/resource/context.rs src-tauri/src/work/project_files.rs src-tauri/tests/context_builder.rs
git commit -m "feat: build bounded assignment contexts"
```

---

### Task 6: 实现权限交集和 Lead/Member 工具授权

**Files:**
- Create: `src-tauri/src/collaboration/tools.rs`
- Modify: `src-tauri/src/agent/assembly.rs`
- Modify: `src-tauri/src/assignment/service.rs`
- Create: `src-tauri/tests/host_tool_bridge.rs`
- Modify: `src-tauri/tests/collaboration_loop.rs`

- [ ] **Step 1: 写权限与角色失败测试**

覆盖 Work ∩ Agent ∩ Capability ∩ Assignment override ∩ Engine capability；研究员/审阅者只读、工程师继承 Work、Lead 工具不扩大文件权限。Member 调 delegate/decision/plan/delivery 拒绝；Lead submit member result 拒绝；跨 Work/非 parent child 操作拒绝。

- [ ] **Step 2: 运行确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test host_tool_bridge authorization -- --nocapture`

Expected: FAIL。

- [ ] **Step 3: 实现 effective permission resolver**

返回可审计决策：

```rust
pub struct EffectivePermission {
    pub tools: BTreeSet<String>,
    pub mode: PermissionMode,
    pub sources: Vec<PermissionDecisionSource>,
}
```

任何未知 policy/capability 为 deny；结果和决策来源写 `PermissionRequested/Resolved` 或相应 policy event。

- [ ] **Step 4: 实现 tool authorization table**

```text
Lead: list members, inspect packs, delegate, status, cancel, retry, decision, plan, delivery
Member: status(self/dependencies as granted), submit result, request clarification
```

成员的 delegation request 只在 Result Envelope 中表达，不提供直接 delegate tool。

- [ ] **Step 5: 运行 authorization tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test host_tool_bridge authorization -- --nocapture`

Expected: PASS。

- [ ] **Step 6: 提交权限层**

```powershell
git add src-tauri/src/collaboration/tools.rs src-tauri/src/agent/assembly.rs src-tauri/src/assignment/service.rs src-tauri/tests/host_tool_bridge.rs src-tauri/tests/collaboration_loop.rs
git commit -m "feat: enforce collaboration tool authority"
```

---

### Task 7: 实现 loopback Host Tool Bridge 与 Run-scoped token

**Files:**
- Create: `src-tauri/src/collaboration/tool_bridge.rs`
- Modify: `src-tauri/src/collaboration/mod.rs`
- Modify: `src-tauri/src/app_state.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Modify: `src-tauri/tests/host_tool_bridge.rs`

- [ ] **Step 1: 写 bridge 安全失败测试**

覆盖仅 loopback bind、合法 call、同一有效 lease 的多次授权调用、token 不匹配/过期/撤销后复用、跨 Run、错误 assignment/agent/work、unknown tool、oversized body、slowloris/read timeout、concurrency cap、schema error 不泄露 SQL/path/token、lease drop 撤销。

- [ ] **Step 2: 运行确认 bridge 不存在**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test host_tool_bridge bridge -- --nocapture`

Expected: FAIL。

- [ ] **Step 3: 选择最小 loopback HTTP 实现**

在 `Cargo.toml` 增加 `axum = { version = "0.8", default-features = false, features = ["http1", "json", "tokio"] }`、`rand = "0.9"`、`subtle = "2"`，并为现有 Tokio 打开 `net` feature；更新 lockfile。固定：body ≤ 256 KiB、request deadline 10s、每 Run 最大 4 并发 tool calls、JSON content-type、no redirects/CORS。

- [ ] **Step 4: 实现 token registry/lease**

```rust
pub struct HostToolLease { pub endpoint: String, pub token: HostToolToken, pub allowed_tools: Vec<String> }
pub async fn issue(&self, context: AuthorizedRunContext) -> HostToolLease;
```

`HostToolToken` 是本模块的私有 wrapper，使用现有 `zeroize` 在 drop 时清零并手写 redacted `Debug`。token 用 OS CSPRNG 32 bytes；只比较 constant-time hash，registry 不存明文。它不是每个请求一次性的 token，而是唯一且只属于一个 Run 的 lease token，允许该 Run 内多个授权工具调用。

- [ ] **Step 5: 运行安全 tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test host_tool_bridge bridge -- --nocapture`

Expected: PASS，无网络外部依赖。

- [ ] **Step 6: 提交 bridge**

```powershell
git add src-tauri/src/collaboration/tool_bridge.rs src-tauri/src/collaboration/mod.rs src-tauri/src/app_state.rs src-tauri/src/lib.rs src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tests/host_tool_bridge.rs
git commit -m "feat: bridge scoped host tools to engine runs"
```

---

### Task 8: 创建 Pi 内置 extension 并只加载授权工具

**Files:**
- Create: `src-tauri/assets/piwork-host-tools.ts`
- Modify: `src-tauri/src/engine/pi/mod.rs`
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/tests/pi_engine.rs`
- Modify: `src-tauri/tests/host_tool_bridge.rs`

- [ ] **Step 1: 写 Pi arguments/extension 失败测试**

断言 `PiRunArguments`：继续 `--no-extensions` 禁止 discovery，但显式追加 `--extension runtime/<run_id>/agent/extensions/piwork-host-tools.ts`（Pi CLI 允许 no-discovery + explicit extension）；`--tools` 只列 built-ins + lease allowed tools。Lead 和 Member 工具列表不同，Member 不含 delegate/decision/delivery。

- [ ] **Step 2: 运行确认旧参数失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test pi_engine host_tools -- --nocapture`

Expected: FAIL。

- [ ] **Step 3: 实现薄 extension**

extension 用 `pi.registerTool()` 注册固定工具，每个 execute：读取 `PIWORK_HOST_TOOL_ENDPOINT`、`PIWORK_HOST_TOOL_TOKEN`、`PIWORK_RUN_ID`，POST `{ tool, arguments }`，将安全 JSON result 转成 text/details；AbortSignal 取消 fetch。不得读取 SQLite、workspace 或任意 URL 参数。

- [ ] **Step 4: 实现 per-Run 私有复制和环境注入**

Harness 通过 `BaseDirectory::Resource/piwork-host-tools.ts` 读取 bundle asset，复制到 `runtime/<run_id>/agent/extensions/piwork-host-tools.ts`，只读属性 best effort；启动 Pi 时显式加载。环境注入 lease endpoint/token；process 结束删除 extension/config/token material，失败也清理。

- [ ] **Step 5: 端到端 Fake Pi extension protocol test**

使用 fake executable 读取 args/env 并模拟 tool HTTP call，证明注册的 tool 能到 Rust service、错误被安全返回、token 不进入 stdout/stderr/Event Journal。

- [ ] **Step 6: 运行 Pi/bridge tests**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test pi_engine host_tools -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test host_tool_bridge -- --nocapture
```

Expected: PASS。

- [ ] **Step 7: 提交 Pi bridge adapter**

```powershell
git add src-tauri/assets/piwork-host-tools.ts src-tauri/src/engine/pi/mod.rs src-tauri/tauri.conf.json src-tauri/tests/pi_engine.rs src-tauri/tests/host_tool_bridge.rs
git commit -m "feat: load scoped PiWork host tools"
```

---

### Task 9: 实现 LeadToolService 的九个主理工具

**Files:**
- Modify: `src-tauri/src/collaboration/tools.rs`
- Modify: `src-tauri/src/assignment/repository.rs`
- Modify: `src-tauri/src/assignment/service.rs`
- Modify: `src-tauri/tests/collaboration_loop.rs`

- [ ] **Step 1: 写每个工具的失败/成功测试**

精确覆盖设计 §7.1：

```text
list_work_members
inspect_capability_packs
delegate_assignment
get_assignment_status
cancel_assignment
request_assignment_retry
record_work_decision
update_work_plan
complete_work_delivery
```

设计列出的是八行但实际为九个名称；本计划以九个明确工具为准，不丢 `complete_work_delivery`。每个测试验证角色、Work 范围、schema、transaction、event 和返回值。

- [ ] **Step 2: 运行确认 tools 未实现**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test collaboration_loop lead_tools -- --nocapture`

Expected: FAIL。

- [ ] **Step 3: 实现查询和 Ledger 工具**

list/status/inspect 返回 bounded summaries；decision/plan 使用 monotonic revision 并写 event。`complete_work_delivery` 要求当前 Assignment 是 Lead、无 required non-terminal dependency、包含 summary/artifacts/validation/limitations，随后完成 Lead/Work。

- [ ] **Step 4: 实现 delegate_assignment**

事务验证：caller 是当前 Lead、target 是 active Work member（未加入时允许自动加入但总成员数不超过 3）、target 不是 Lead、parent kind=Lead、无 parent parent、capability executable 且已绑定或明确授权、permission 不扩大、同一问题 fingerprint 不重复、一个 assignee、context manifest bounded。写 child + dependency + parent waiting intent，commit 后 wake scheduler；返回 `{ assignmentId, status: "queued" }`。

- [ ] **Step 5: 实现 cancel/retry**

只允许 parent Lead 管自己的 child；终态不可取消，retry 遵守 max attempts/dead-letter，不重置审计 history。

- [ ] **Step 6: 运行 lead tools tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test collaboration_loop lead_tools -- --nocapture`

Expected: PASS。

- [ ] **Step 7: 提交 tools**

```powershell
git add src-tauri/src/collaboration/tools.rs src-tauri/src/assignment/repository.rs src-tauri/src/assignment/service.rs src-tauri/tests/collaboration_loop.rs
git commit -m "feat: give lead agents durable host tools"
```

---

### Task 10: 实现 Member 结果/澄清工具与单层委派请求

**Files:**
- Modify: `src-tauri/src/collaboration/tools.rs`
- Modify: `src-tauri/src/collaboration/result.rs`
- Modify: `src-tauri/tests/collaboration_loop.rs`

- [ ] **Step 1: 写 Member 工具失败测试**

Member 可以：提交 Result、请求用户/Lead 澄清、查询自身/显式 dependencies 状态。Result 中 `delegation_requests` 可建议成员/能力、原因、上下文、期望输出，但不会创建 Assignment。测试 Member 直接 delegate 被拒绝。

- [ ] **Step 2: 运行确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test collaboration_loop member_tools -- --nocapture`

Expected: FAIL。

- [ ] **Step 3: 实现 submit/result/clarification**

valid result 在同一事务写 `assignment_results`、events、Assignment terminal；clarification 进入 waiting 并带明确 target。delegation request 留在 envelope，Lead 恢复后决定；不自动批准。

- [ ] **Step 4: 运行 tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test collaboration_loop member_tools -- --nocapture`

Expected: PASS。

- [ ] **Step 5: 提交 Member tools**

```powershell
git add src-tauri/src/collaboration/tools.rs src-tauri/src/collaboration/result.rs src-tauri/tests/collaboration_loop.rs
git commit -m "feat: collect structured expert results"
```

---

### Task 11: 实现 Lead waiting → children terminal → resume 闭环

**Files:**
- Modify: `src-tauri/src/assignment/state_machine.rs`
- Modify: `src-tauri/src/assignment/repository.rs`
- Modify: `src-tauri/src/assignment/scheduler.rs`
- Modify: `src-tauri/src/engine/harness.rs`
- Modify: `src-tauri/tests/assignment_lifecycle.rs`
- Modify: `src-tauri/tests/collaboration_loop.rs`

- [ ] **Step 1: 写闭环失败测试**

覆盖：Lead direct completion；Lead delegates one child；delegates sequential children；child failed/dead-letter still resumes Lead with failure envelope；required vs optional dependency；restart while Lead waiting；Lead crash after child commit；duplicate terminal wake；继续委派；最终 delivery。

- [ ] **Step 2: 运行确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test collaboration_loop lead_resume -- --nocapture`

Expected: FAIL。

- [ ] **Step 3: 实现 waiting handshake**

Lead tool call 记录 child 后，extension 返回 accepted；Lead 必须结束当前 Run，Harness 将明确 completion reason `waiting_on_assignments` 映射为 Assignment waiting，而非 completed。若模型错误地继续并尝试 delivery，Service 因 required child non-terminal 拒绝。

- [ ] **Step 4: 实现 dependency terminal wake**

child terminal transaction 后检查 parent required dependencies；全终止则 parent waiting→queued 并写 `leadResumed`，Scheduler wake。去重 key `(parent_assignment_id, dependency_generation)` 防止重复 resume。

- [ ] **Step 5: 恢复 Lead 新 Run**

Context Builder section 7 注入每个 dependency 的 validated Result Envelope/terminal failure summary；复用 Lead Agent × Work Session；创建新 Run attempt，不创建新普通 user message。

- [ ] **Step 6: 运行闭环/restart/race tests**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test collaboration_loop lead_resume -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test assignment_lifecycle -- --nocapture
```

Expected: PASS。

- [ ] **Step 7: 提交闭环**

```powershell
git add src-tauri/src/assignment src-tauri/src/engine/harness.rs src-tauri/tests/assignment_lifecycle.rs src-tauri/tests/collaboration_loop.rs
git commit -m "feat: resume leads after expert assignments"
```

---

### Task 12: 实现受确认的 Memory Candidate 写入

**Files:**
- Create: `src-tauri/src/collaboration/memory.rs`
- Create: `src-tauri/src/collaboration/service.rs`
- Create: `src-tauri/src/collaboration/commands.rs`
- Modify: `src-tauri/src/collaboration/mod.rs`
- Modify: `src-tauri/src/app_state.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/tests/collaboration_loop.rs`

- [ ] **Step 1: 写 memory 安全失败测试**

candidate 不直接写 AgentMemory；Lead 或用户确认后才写；保存 source/version/author；拒绝 secret/API key/private key patterns、绝对临时路径、无证据推测、大段 tool output、跨 Work source、被撤销 candidate；并发确认幂等。

- [ ] **Step 2: 运行确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test collaboration_loop memory -- --nocapture`

Expected: FAIL。

- [ ] **Step 3: 实现 candidate policy/repository**

```rust
propose_candidate(validated_result, input)
list_work_candidates(work_id)
resolve_candidate(candidate_id, Confirm|Reject, actor)
list_agent_memory(agent_id, budget)
```

敏感扫描只是防线之一；所有写入仍要求显式确认。确认写 `agent_memory` 和 event，reject 保留 candidate audit。

- [ ] **Step 4: 暴露 UI commands**

```text
get_work_ledger(workId)
list_memory_candidates(workId)
resolve_memory_candidate(candidateId, outcome)
get_work_team_activity(workId)
```

- [ ] **Step 5: 运行 tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test collaboration_loop memory -- --nocapture`

Expected: PASS。

- [ ] **Step 6: 提交 Memory**

```powershell
git add src-tauri/src/collaboration src-tauri/src/app_state.rs src-tauri/src/lib.rs src-tauri/tests/collaboration_loop.rs
git commit -m "feat: confirm durable agent memory candidates"
```

---

### Task 13: 在 Timeline/Inspector 展示真正的团队协作

**Files:**
- Modify: `src/app/tauriClient.ts`
- Modify: `src/test/mockTauriClient.ts`
- Modify: `src/features/workspace/WorkTimeline.tsx`
- Modify: `src/features/workspace/WorkTimeline.test.tsx`
- Modify: `src/features/workspace/WorkInspector.tsx`
- Modify: `src/features/workspace/WorkInspector.test.tsx`
- Create: `src/features/workspace/WorkTeamInspector.tsx`
- Create: `src/features/workspace/WorkTeamInspector.test.tsx`
- Create: `src/features/workspace/WorkLedgerInspector.tsx`
- Create: `src/features/workspace/WorkLedgerInspector.test.tsx`
- Create: `src/features/workspace/MemoryCandidatesInspector.tsx`
- Create: `src/features/workspace/MemoryCandidatesInspector.test.tsx`
- Modify: `src/features/agent-center/AgentCenterPage.tsx`
- Modify: `src/i18n/locales/en.json`
- Modify: `src/i18n/locales/zh-CN.json`
- Modify: `src/styles/workspace.css`

- [ ] **Step 1: 写协作 UI 失败测试**

覆盖 Team 当前状态/能力、Assignments dependencies/results、Plan/Decisions Ledger、Memory confirm/reject、成员 Activity 跳转、Lead waiting 文案、child failure/dead-letter、final delivery only from Lead、无成员私聊入口。

- [ ] **Step 2: 运行 focused tests 确认失败**

```powershell
pnpm test -- src/features/workspace/WorkTimeline.test.tsx src/features/workspace/WorkInspector.test.tsx src/features/workspace/WorkTeamInspector.test.tsx src/features/workspace/WorkLedgerInspector.test.tsx src/features/workspace/MemoryCandidatesInspector.test.tsx
```

Expected: FAIL。

- [ ] **Step 3: 更新 typed client 和 Inspector tabs**

Inspector 一级内容实现设计 §12.3：Team、Assignments、Plan/Decisions、Memory Candidates、Raw Activity；保留 delivery/attachments/validation 的现有能力，可重新分组但不可删除。

- [ ] **Step 4: 实现主时间线语义聚合**

显示：Lead 拆分、成员做什么及结果、Reviewer 阻塞、Engineer 修复、Lead final。Agent 内部参数/完整结果默认折叠，Result summary/evidence count 可展开；Raw Rail 可追溯。

- [ ] **Step 5: 实现 Memory 确认**

明确显示 content/source/reason/author；确认/拒绝需可恢复错误处理。UI 不提供直接编辑 AgentMemory。

- [ ] **Step 6: 运行 UI/type tests**

```powershell
pnpm test -- src/features/workspace src/features/agent-center
pnpm typecheck
```

Expected: PASS。

- [ ] **Step 7: 提交 UI**

```powershell
git add src/app/tauriClient.ts src/test/mockTauriClient.ts src/features/workspace src/features/agent-center/AgentCenterPage.tsx src/i18n/locales/en.json src/i18n/locales/zh-CN.json src/styles/workspace.css
git commit -m "feat: surface lead and expert collaboration"
```

---

### Task 14: 扩展 Activity Projector 的委派、结果和交付语义

**Files:**
- Modify: `src/features/activity/activityTypes.ts`
- Modify: `src/features/activity/activityProjector.ts`
- Modify: `src/features/activity/activityProjector.test.ts`
- Modify: `src/features/activity/activityGrouping.ts`
- Modify: `src/features/activity/activityGrouping.test.ts`
- Modify: `src/features/activity/activityPresentation.ts`
- Modify: `src/features/activity/activityPresentation.test.ts`
- Modify: `src/features/activity/ActivityFeed.tsx`
- Modify: `src/features/activity/ActivityFeed.test.tsx`

- [ ] **Step 1: 写设计示例的失败测试**

投影必须能产生设计 §12.2 等价语义：主理拆分 2 项、研究员核验、工程师修改并验证、审阅者阻塞、工程师修复、主理交付。Result revision 原地合并，Lead resume 开新 turn，最终交付 terminal 单调。

- [ ] **Step 2: 运行 Activity tests 确认失败**

Run: `pnpm test -- src/features/activity`

Expected: FAIL。

- [ ] **Step 3: 实现分类/分组**

增加 Agent display identity lookup；缺失成员 metadata 时用 stable id，不丢事件。Result/decision/plan/delivery 是高 salience；tool bridge transport events suppressed from main feed but kept Raw Rail。

- [ ] **Step 4: 运行 Activity tests**

Run: `pnpm test -- src/features/activity`

Expected: PASS，A/C tests 无回归。

- [ ] **Step 5: 提交 Activity**

```powershell
git add src/features/activity
git commit -m "feat: project multi agent collaboration activity"
```

---

### Task 15: 端到端多 Agent 场景、故障与安全验收

**Files:**
- Modify: `src-tauri/tests/collaboration_loop.rs`
- Modify: `src-tauri/tests/host_tool_bridge.rs`
- Modify: `src-tauri/tests/context_builder.rs`
- Modify: relevant frontend tests only if gaps are found

- [ ] **Step 1: 增加四条 deterministic end-to-end tests**

```text
lead_completes_without_delegation
lead_delegates_researcher_then_synthesizes
lead_delegates_engineer_then_reviewer_and_handles_blocker
restart_during_member_run_recovers_without_losing_result_or_duplicate_write
```

使用 Fake Adapter 脚本化 tool calls/results，不依赖真实 LLM。断言 DB rows、event sequence、contexts、Run count、final delivery author。

- [ ] **Step 2: 增加安全/失败场景**

Member 试图委派；Lead 试图扩大权限；malformed result repair twice；bridge forged token；child dead-letter；Lead crash after child commit；Session rotate；用户 interrupt；memory secret candidate；UI realtime event 丢失后 hydrate 一致。

- [ ] **Step 3: 运行完整 Rust tests**

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: 全部 exit 0。

- [ ] **Step 4: 运行完整前端 tests/build**

```powershell
pnpm test
pnpm typecheck
pnpm build
```

Expected: 全部 exit 0。

- [ ] **Step 5: 提交验收 tests**

```powershell
git add src-tauri/tests src/features
git commit -m "test: cover the lead expert collaboration loop"
```

---

### Task 16: 真实窗口验收与 A–D 设计覆盖审计

**Files:**
- Review: all A–D implementation files and tests
- Review: `docs/architecture/buzz-upstream-map.md`（D 不移植新的 Buzz 代码；确认 C 的映射仍完整）
- Review: `THIRD_PARTY_NOTICES.md`（若 Task 7/8 仅使用依赖与自研 extension，确认现有许可聚合流程覆盖新依赖）

- [ ] **Step 1: 启动真实桌面应用**

Run: `pnpm tauri dev`

使用可控模型或开发 fixture 完成：Lead 自行完成；Lead→研究员→综合；Lead→工程师→审阅者→修复→交付。

- [ ] **Step 2: 验收产品语义**

确认：用户只看到一个 Work；Lead 唯一负责；成员按需出现；无群聊；内部事件不伪装为 message；Lead waiting 不占 Engine；成员结果由 Lead 综合；最终交付 author 为 Lead。

- [ ] **Step 3: 验收上下文与权限**

检查测试日志/fixture（不泄露 secrets）：成员没有完整聊天/无关附件；Context section 顺序固定；研究员/审阅者只读；工程师不超过 Work 权限；bridge 只 loopback；token 不进 Journal。

- [ ] **Step 4: 验收恢复与监督**

强制结束 Member/Lead/App：Assignment/Result/Run history 不丢；重启行为可解释；Timeline 可看懂；Raw Rail 可追溯；Session rotate 不改变 Agent identity。

- [ ] **Step 5: 逐条审计设计验收标准**

建立表格逐项勾选设计 §20 的 14 项标准，并关联实现 task/test。再审计 §21 成功指标可观测字段：委派次数、Result contract pass、context size、用户介入状态、恢复结果、Reviewer pass/block；本计划只提供数据，不引入遥测上传。

- [ ] **Step 6: 检查归属、范围和工作树**

```powershell
git diff --check
git status --short
```

确保 Pi extension 使用官方 API 但不是复制上游实现；若有实质 Buzz 派生则更新 map/notices。无 E 范围。

- [ ] **Step 7: 记录最终验收结果**

本任务默认不产生新代码。如果 Step 1–6 暴露 D 范围缺陷，返回对应 Task 修复并重新运行完整门禁；不要创建只含验收文案的空提交。

---

## 子项目 D 完成定义

- [ ] Lead 与 Member 通过受限、可审计宿主工具协作；工具桥通过安全测试。
- [ ] 只有 Lead 可委派，且仅单层；成员只能结果/澄清/委派建议。
- [ ] `delegate_assignment` 先持久化且不等待；Lead waiting 不占 Engine。
- [ ] 依赖终止后 Lead 以新 Run 恢复并收到结构化结果摘要。
- [ ] Result Envelope 基础合同、provenance 与一次 repair policy 已验证。
- [ ] Context Builder 严格按八层顺序、bounded、least-context。
- [ ] Work Ledger 可从 events 重建；Memory 只有确认后写入。
- [ ] Timeline/Inspector 展示 Team、Assignments、Plan/Decisions、Memory 与 Raw Activity。
- [ ] 最终交付只能由 Lead 完成；用户默认只对话 Work/Lead。
- [ ] 设计 §20 的 A–D 全部验收标准有对应自动化测试或真实窗口验收证据。
- [ ] 没有实现 E 范围。
