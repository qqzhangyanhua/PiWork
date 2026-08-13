# PiWork Assignment Queue 与 Session Harness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在持久 Agent 身份之上实现可恢复的 Assignment/Run/Session 执行底座：先持久化、按 Work 公平调度、同 Work 单 in-flight、Agent × Work Session 隔离，以及用户可解释的 queue/steer/interrupt 控制。

**Architecture:** `AssignmentRepository` 是产品事实源；Buzz 派生的纯内存 `WorkQueue` 只决定可运行项，`AssignmentScheduler` 负责 claim/dispatch/retry/dead-letter，`EngineHarness` 把一个已解析 Assignment 映射到现有 `EngineAdapter` 和 `EngineSupervisor` 生命周期。运行身份从 `work_id` 扩展为 `assignment_id + agent_instance_id + agent_session_id`，Pi session 目录迁移到 `engine-sessions/pi/<agent>/<work>/<generation>`。本计划只提供“可执行 Assignment”底座和用户控制，不向模型暴露 `delegate_assignment`，也不实现 Result Envelope、Context Builder 或 Memory；这些属于 D。

**Tech Stack:** Rust 2024、Tokio、SQLx/SQLite、Serde、ts-rs、Tauri 2、Pi RPC、React 19、TypeScript、Zustand、Vitest、Testing Library。

**Design source:** `docs/superpowers/specs/2026-08-09-piwork-agent-assembly-design.md` §§6.8–6.9、7.5、9、10、11、12、14.2、15–18、19.C、20。

**Prerequisites:** 子项目 A 与 B 已完成，尤其依赖 `AgentInstance`、`WorkLead/WorkAgent`、`PermissionPolicy` 和 Activity Protocol v2。执行本计划前先运行 B 的完整质量门。

**Plan index:** `docs/superpowers/plans/2026-08-13-piwork-agent-assembly-plan-index.md`。本文件只拥有 C；不能把 D 的委派/Result/Context 提前塞入本计划。

---

## 范围、不变量与明确延期

本计划完成后：

1. 每个用户 `start_work` 请求先创建分配给 Work Lead 的持久 Lead Assignment，再由 Scheduler 执行；HTTP/Tauri command 返回的是已接受的 Assignment，而不是“模型一定已经启动”。
2. Assignment 是产品事实，Run 是一次尝试；一个 Assignment 可拥有多个 Run。
3. Queue 只处理已持久记录；claim/start/retry/complete/dead-letter 都是数据库事务。
4. 同一 Work 最多一个 in-flight Assignment；不同 Work 按 oldest-head 公平且有全局上限；每个 AgentInstance 受 parallelism 上限约束。
5. 第一版同一 Work 即便全为只读也不并行。
6. Pi、Fake 与未来 Adapter 只实现统一 `EngineAdapter`；Scheduler/UI 不依赖 Pi RPC 类型。
7. 每个 Agent × Work 有独立、可 rotate 的 Session；Engine-specific ref 只存数据库，不进入产品 UI 术语。
8. restart 时 orphaned `claimed/running` 进入 `interrupted`；只读/幂等可重排，可能有写副作用的任务等待用户确认。
9. 用户 queue/steer/interrupt 的降级是显式事件；无 native steer 时执行 cancel + merge + new Run。
10. Journal event 必须填充真实 `agent_id`、`assignment_id`、`session_id`。

明确延期到 D：

- 主理 Agent 宿主工具和模型发起的子 Assignment。
- Lead waiting-on-children/resume、Result Envelope、Work Ledger、Context Builder、Memory。
- 成员提出 `delegation_requested` 的闭环。

为了让 C 自身可验收，提供测试/内部 command `enqueue_assignment` 所需的 Service API，但 production UI 只通过现有 Composer 创建 Lead Assignment；任意成员 Assignment 的创建不作为普通用户功能暴露。

---

## 文件职责图

### Rust / SQLite

- Create `src-tauri/migrations/0006_assignment_sessions.sql`：`agent_sessions`、`assignments`、`assignment_dependencies`，runs identity 列、索引与约束。
- Create `src-tauri/src/domain/assignment.rs`：Assignment/Session/queue control DTO 和状态枚举。
- Modify `src-tauri/src/domain/work.rs`：`RunSummary` 增加 nullable Assignment/Agent identity；`StartWorkOutput` 返回 accepted Assignment 和 nullable Run。
- Modify `src-tauri/src/domain/event.rs`：Assignment lifecycle、queue control、retry/dead-letter/recovery payload。
- Modify `src-tauri/src/domain/mod.rs`：binding exports。
- Create `src-tauri/src/assignment/mod.rs`。
- Create `src-tauri/src/assignment/state_machine.rs`：纯状态转换与 side-effect classification。
- Create `src-tauri/src/assignment/repository.rs`：持久化、claim、attempt、dependency、recovery 事务。
- Create `src-tauri/src/assignment/queue.rs`：Buzz 派生 per-Work queue、公平、caps、deadline、backoff。
- Create `src-tauri/src/assignment/scheduler.rs`：hydrate、dispatch、capacity、retry、wake。
- Create `src-tauri/src/assignment/service.rs`：用户 queue/steer/interrupt 和查询。
- Create `src-tauri/src/assignment/commands.rs`：Tauri commands。
- Create `src-tauri/src/engine/harness.rs`：Assignment 到 Engine execution 的独立边界。
- Modify `src-tauri/src/engine/mod.rs`：`EngineCapabilities`、扩展 `EngineRunContext`、resume/rotate/steer contract。
- Modify `src-tauri/src/engine/fake.rs`：conformance 行为。
- Modify `src-tauri/src/engine/pi/mod.rs`：isolated session path、明确 capabilities、abort/steer 降级支持。
- Refactor `src-tauri/src/engine/supervisor.rs`：active key 从 Work 改为 Assignment/Run execution key，并对外提供 Harness 使用的 attempt lifecycle。
- Modify `src-tauri/src/work/repository.rs`、`service.rs`、`commands.rs`：用户输入转为 Lead Assignment。
- Modify `src-tauri/src/app_state.rs`、`src-tauri/src/lib.rs`：Scheduler 生命周期和 startup recovery。
- Create `src-tauri/tests/assignment_lifecycle.rs`。
- Create `src-tauri/tests/engine_conformance.rs`。
- Modify `src-tauri/tests/storage_contract.rs`、`work_lifecycle.rs`、`pi_engine.rs`。
- Modify `docs/architecture/buzz-upstream-map.md`、`THIRD_PARTY_NOTICES.md`：Queue/Pool 派生映射。

### React / TypeScript

- Generate assignment/session bindings and update `src/bindings/index.ts`。
- Modify `src/app/tauriClient.ts`、`src/test/mockTauriClient.ts`。
- Modify `src/features/works/workStore.ts`：Assignment summaries、accepted-vs-running、control results。
- Modify `src/features/workspace/WorkComposer.tsx`：排在下一步/纠偏/中断替换。
- Modify `src/features/workspace/WorkDetail.tsx`、`WorkTimeline.tsx`：Assignment lifecycle projection。
- Modify `src/features/workspace/WorkInspector.tsx`：Assignments tab。
- Create `src/features/workspace/AssignmentInspector.tsx`。
- Modify `src/features/activity/activityTypes.ts`、`activityProjector.ts`、`activityGrouping.ts`、`activityPresentation.ts`：Agent/Assignment 语义。
- Modify styles、i18n 与 focused tests。

---

### Task 1: 冻结 Assignment、Session 与控制 wire contract

**Files:**
- Create: `src-tauri/src/domain/assignment.rs`
- Modify: `src-tauri/src/domain/mod.rs`
- Modify: `src-tauri/src/domain/work.rs`
- Modify: `src-tauri/src/domain/event.rs`
- Generate: `src/bindings/Assignment*.ts`
- Generate: `src/bindings/AgentSessionSummary.ts`
- Generate: `src/bindings/QueueWorkInput.ts`
- Generate: `src/bindings/SteerAssignmentInput.ts`
- Generate: `src/bindings/InterruptWorkInput.ts`
- Modify: `src/bindings/index.ts`

- [ ] **Step 1: 写失败的状态和 JSON 合同测试**

锁定：

```rust
pub enum AssignmentStatus {
    Queued, Claimed, Running, Waiting, Completed, Failed,
    Cancelled, Interrupted, DeadLetter, RecoveryConfirmationRequired,
}
pub enum AssignmentKind { Lead, Member }
pub enum AssignmentSideEffect { ReadOnly, IdempotentWrite, NonIdempotentWrite, Unknown }
pub enum AgentSessionStatus { Ready, Running, Invalidated }
pub enum QueueControlMode { EnqueueNext, SteerCurrent, InterruptAndReplace }
```

序列化测试必须断言 snake_case values、camelCase fields，并锁定 `AssignmentSummary` 包含设计 §6.8 字段、attempt/nextAttemptAt、recovery reason、result summary nullable 字段。

为 `WorkEventPayload` 新增并测试：

```text
assignmentQueued
assignmentClaimed
assignmentStarted
assignmentWaiting
assignmentRetryScheduled
assignmentCompleted
assignmentFailed
assignmentInterrupted
assignmentDeadLettered
assignmentRecoveryRequired
queueControlApplied
```

- [ ] **Step 2: 运行测试确认类型不存在**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml domain::assignment -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml domain::event -- --nocapture
```

Expected: FAIL。

- [ ] **Step 3: 实现 DTO 和兼容性**

`RunSummary` 添加：

```rust
pub assignment_id: Option<String>,
pub agent_instance_id: Option<String>,
```

`StartWorkOutput` 固定为 `pub assignment: AssignmentSummary`、`pub run: Option<RunSummary>`、`pub user_message: UserMessageSummary`；删除依赖必有 Run 的 `Deref<Target = RunSummary>`，调用方显式处理 `run`。旧 persisted runs 的两个 identity 字段允许 NULL。事件 payload 只表达产品语义，不包含 scheduler 内部 slot/permit。

- [ ] **Step 4: 导出并验证 bindings**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml domain::tests::export_bindings -- --nocapture
pnpm typecheck
```

Expected: PASS；所有新 binding 存在且 `WorkEventPayload.ts` 含新 discriminator。

- [ ] **Step 5: 提交合同**

```powershell
git add src-tauri/src/domain src/bindings
git commit -m "feat: define assignment execution contracts"
```

---

### Task 2: 创建 Assignment/Session schema 与 Run identity

**Files:**
- Create: `src-tauri/migrations/0006_assignment_sessions.sql`
- Modify: `src-tauri/tests/storage_contract.rs`

- [ ] **Step 1: 写失败的 schema 测试**

断言新增表：

```text
agent_sessions
assignments
assignment_dependencies
```

断言 `runs.assignment_id`、`runs.agent_instance_id` nullable；约束包括：Assignment agent 必须为 Work member、parent 与 child 属于同一 Work、Run 的 Assignment/Agent/Work 组合一致、Session key 唯一为 `(agent_instance_id, work_id, engine_kind, generation)`、每个 Assignment attempt number 唯一。

- [ ] **Step 2: 运行测试确认缺表失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test storage_contract assignment -- --nocapture`

Expected: FAIL，缺少 `assignments`。

- [ ] **Step 3: 写 migration**

`assignments` 包含设计 §6.8 的全部字段，额外持久列：

```text
kind
side_effect
result_summary
last_error
next_attempt_at
recovery_reason
updated_at
```

使用 partial indexes：

```sql
CREATE UNIQUE INDEX idx_assignments_one_inflight_per_work
ON assignments(work_id)
WHERE status IN ('claimed','running');

CREATE INDEX idx_assignments_schedulable
ON assignments(status, not_before, created_at, id);
```

`assignment_dependencies` 主键 `(assignment_id, depends_on_assignment_id)`，CHECK 禁止自依赖；跨 Work 由 Repository 事务验证并以测试锁定。

- [ ] **Step 4: 验证 migration 与 legacy runs**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test storage_contract -- --nocapture`

Expected: PASS；旧 Run 两列为 NULL 且仍可读取。

- [ ] **Step 5: 提交 schema**

```powershell
git add src-tauri/migrations/0006_assignment_sessions.sql src-tauri/tests/storage_contract.rs
git commit -m "feat: persist assignments and agent sessions"
```

---

### Task 3: 实现 Assignment 状态机与持久 Repository

**Files:**
- Create: `src-tauri/src/assignment/mod.rs`
- Create: `src-tauri/src/assignment/state_machine.rs`
- Create: `src-tauri/src/assignment/repository.rs`
- Modify: `src-tauri/src/lib.rs`
- Create: `src-tauri/tests/assignment_lifecycle.rs`

- [ ] **Step 1: 写状态转换失败测试**

覆盖每个合法边和非法边：

```text
queued -> claimed -> running -> completed|failed|waiting|cancelled|interrupted
failed|interrupted -> queued (retry policy only)
failed|interrupted -> dead_letter
interrupted -> recovery_confirmation_required
recovery_confirmation_required -> queued|cancelled
waiting -> queued|cancelled
```

终态 `completed/cancelled/dead_letter` 不可再变；`attempt_count <= max_attempts`。

- [ ] **Step 2: 写 Repository 事务失败测试**

覆盖：accepted before dispatch、原子 claim、同 Work 第二个 claim 失败、attempt 创建与 Run identity、complete、retry/backoff、dead-letter、dependency terminal 检查、跨 Work dependency 拒绝、重启 orphan recovery。

- [ ] **Step 3: 运行测试确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test assignment_lifecycle repository -- --nocapture`

Expected: FAIL。

- [ ] **Step 4: 实现纯状态机**

入口：

```rust
pub fn transition(current: AssignmentStatus, action: AssignmentAction) -> Result<AssignmentStatus, AppError>;
pub fn recovery_decision(side_effect: AssignmentSideEffect, attempt_started: bool) -> RecoveryDecision;
pub fn retry_delay(attempt: u32, base: Duration, max: Duration, jitter_seed: u64) -> Duration;
```

Jitter 必须由注入 seed 可测试；production seed 来源于 Assignment ID hash，不使用全局随机导致 flaky tests。

- [ ] **Step 5: 实现 Repository 事务**

公开方法至少包括：

```rust
accept(input) -> AssignmentSummary
list_for_work(work_id) -> Vec<AssignmentSummary>
load_schedulable(now, limit) -> Vec<AssignmentSummary>
claim(assignment_id, runtime_owner, now) -> AssignmentSummary
begin_attempt(assignment_id, engine_kind, model_label) -> RunSummary
mark_running(assignment_id, run_id, session_id, runtime_owner, now)
mark_waiting(assignment_id, run_id, reason, now)
complete(assignment_id, run_id, result_summary, now)
fail_and_schedule_retry(assignment_id, run_id, error, next_attempt_at, now)
dead_letter(assignment_id, run_id, error, now)
recover_orphans(active_owner_ids) -> RecoveryReport
confirm_recovery(assignment_id, resume: bool) -> AssignmentSummary
```

每次状态更改同时 append 对应 WorkEvent；commit 后才 publish（沿用 A 的 journal-before-publish 适配方式）。

- [ ] **Step 6: 运行测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test assignment_lifecycle repository -- --nocapture`

Expected: PASS。

- [ ] **Step 7: 提交 Repository**

```powershell
git add src-tauri/src/assignment src-tauri/src/lib.rs src-tauri/tests/assignment_lifecycle.rs
git commit -m "feat: add durable assignment lifecycle"
```

---

### Task 4: 移植 Buzz per-Work Queue 与公平调度不变量

**Files:**
- Create: `src-tauri/src/assignment/queue.rs`
- Test: `src-tauri/src/assignment/queue.rs`
- Modify: `docs/architecture/buzz-upstream-map.md`
- Modify: `THIRD_PARTY_NOTICES.md`

- [ ] **Step 1: 固定 Buzz 来源并复制适用测试**

固定 `block/buzz@5bf78671f45178f8de02ba18d3d321cbbf19cd1f` 的 `crates/buzz-acp/src/queue.rs`。先移植测试结构并替换：Channel→Work、batch item→Assignment queue item。每个派生文件头写清 upstream path、commit 和 PiWork differences。

- [ ] **Step 2: 写/运行失败测试**

必须覆盖：

```rust
oldest_head_is_fair_across_works
one_work_has_only_one_inflight_item
global_and_agent_parallelism_caps_are_enforced
queue_depth_and_batch_caps_fail_closed
pool_exhaustion_requeues_with_original_timestamp
deadline_expiry_releases_capacity_once
retry_backoff_and_dead_letter_are_monotonic
cancelled_batch_merge_preserves_user_order
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml assignment::queue -- --nocapture`

Expected: FAIL，queue 未实现。

- [ ] **Step 3: 实现纯内存 Queue**

Queue 不访问 SQLite、Engine 或 Tauri：

```rust
pub struct WorkQueue {
    per_work: BTreeMap<String, VecDeque<QueueItem>>,
    inflight_by_work: BTreeMap<String, QueueClaim>,
    inflight_by_agent: BTreeMap<String, usize>,
    limits: QueueLimits,
}
pub fn hydrate(items: impl IntoIterator<Item = QueueItem>, limits: QueueLimits) -> Result<Self, QueueError>;
pub fn push(&mut self, item: QueueItem) -> Result<(), QueueError>;
pub fn next_claim(&mut self, now: DateTime<Utc>) -> Option<QueueClaim>;
pub fn release(&mut self, completion: QueueCompletion) -> QueueRelease;
pub fn cancel_work(&mut self, work_id: &str) -> Vec<QueueItem>;
```

排序键固定 `(created_at, id)`；pool exhausted/requeue 不改 created_at。

- [ ] **Step 4: 运行上游派生测试并记录差异**

Run: `cargo test --manifest-path src-tauri/Cargo.toml assignment::queue -- --nocapture`

Expected: PASS。

更新 upstream map，明确保留/删除的 Buzz 行为，不复制 Relay/ACP/Channel 类型。

- [ ] **Step 5: 提交 Queue**

```powershell
git add src-tauri/src/assignment/queue.rs docs/architecture/buzz-upstream-map.md THIRD_PARTY_NOTICES.md
git commit -m "feat: port fair assignment queue semantics"
```

---

### Task 5: 扩展 EngineAdapter capabilities 与 conformance suite

**Files:**
- Modify: `src-tauri/src/engine/mod.rs`
- Modify: `src-tauri/src/engine/fake.rs`
- Create: `src-tauri/tests/engine_conformance.rs`
- Modify: `src-tauri/tests/pi_engine.rs`

- [ ] **Step 1: 写通用 Adapter conformance 失败测试**

构造 factory harness，所有 Adapter 必须声明并遵守：

```rust
pub struct EngineCapabilities {
    pub session_resume: bool,
    pub session_rotate: bool,
    pub native_steer: bool,
    pub cancel: bool,
    pub thought_stream: bool,
    pub plan_updates: bool,
    pub permission_requests: bool,
    pub tool_progress: bool,
    pub usage_reporting: bool,
    pub parallel_tool_calls: bool,
}
```

测试 start/resume/abort、event ordering、terminal exactly once、rotate、native-or-degraded steer、liveness timeout 和 capability truthfulness。

- [ ] **Step 2: 运行测试确认 trait 不满足**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test engine_conformance -- --nocapture`

Expected: FAIL。

- [ ] **Step 3: 扩展 trait 和 Run context**

`EngineRunContext` 增加：

```rust
pub assignment_id: String,
pub agent_instance_id: String,
pub agent_session_id: String,
pub session_generation: u32,
pub resolved_model_configuration_id: Option<String>,
pub effective_permission: PermissionMode,
```

trait 增加：

```rust
fn capabilities(&self) -> EngineCapabilities;
async fn resume(
    &self,
    context: EngineRunContext,
    input: EngineInput,
    sink: mpsc::Sender<EngineEvent>,
) -> Result<EngineSessionRef, EngineError>;
async fn rotate(
    &self,
    context: EngineRunContext,
    reason: &str,
) -> Result<EngineSessionRef, EngineError>;
async fn steer(&self, run_id: &str, input: EngineInput) -> Result<(), EngineError>;
```

默认实现依据 capabilities 返回 `Unsupported`，不伪造成功。

- [ ] **Step 4: 让 Fake Adapter 覆盖所有分支**

Fake 可配置 capability bitmap、start barrier、crash、timeout、steer/abort calls 和 session ref；conformance tests 不依赖真实凭据。

- [ ] **Step 5: 声明 Pi 的诚实能力**

当前 Pi RPC 支持 start、session resume（相同 session id/path）和 cancel；C 阶段没有已验证 native steer/permission bridge 时对应 flag 为 false。thought/plan/tool progress/usage 只按已有 translator 实际支持声明。

- [ ] **Step 6: 运行 conformance 和现有 Pi tests**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test engine_conformance -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test pi_engine -- --nocapture
```

Expected: PASS。

- [ ] **Step 7: 提交 Adapter contract**

```powershell
git add src-tauri/src/engine/mod.rs src-tauri/src/engine/fake.rs src-tauri/tests/engine_conformance.rs src-tauri/tests/pi_engine.rs
git commit -m "feat: negotiate engine execution capabilities"
```

---

### Task 6: 实现 Agent × Work Session Repository 与 Pi 隔离路径

**Files:**
- Modify: `src-tauri/src/assignment/repository.rs`
- Modify: `src-tauri/src/engine/pi/mod.rs`
- Modify: `src-tauri/tests/assignment_lifecycle.rs`
- Modify: `src-tauri/tests/pi_engine.rs`

- [ ] **Step 1: 写 Session 生命周期失败测试**

覆盖：

```rust
same_agent_and_work_resume_same_ready_generation
different_agents_never_share_session_or_directory
same_agent_in_different_works_never_shares_session
rotate_invalidates_old_generation_without_changing_identity
process_crash_invalidates_all_owned_sessions
legacy_lead_session_is_read_once_then_rotates_to_new_path
```

Pi argument test 必须断言目录为：

```text
<sessions_root>/pi/<agent_instance_id>/<work_id>/<generation>
```

且 `--session-id` 使用 `agent_session_id`，不再使用 work id。

- [ ] **Step 2: 运行 tests 确认旧路径失败**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test assignment_lifecycle session -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test pi_engine session -- --nocapture
```

Expected: FAIL，当前仍是 `sessions_root/<work_id>`。

- [ ] **Step 3: 实现 Session Repository**

方法：

```rust
claim_or_create_session(agent_instance_id, work_id, engine_kind, owner_id)
attach_engine_reference(session_id, opaque_ref)
mark_ready(session_id, last_successful_turn)
rotate(session_id, reason)
invalidate_owner(owner_id, reason)
```

engine opaque ref 只存 `agent_sessions.engine_reference`；UI DTO 不输出该字段。

- [ ] **Step 4: 修改 Pi session path 和 legacy 策略**

`PiEngineAdapter` 根据 context 构造新路径。legacy path 只在：agent 是内置 Lead、new generation 还没有 session、旧目录存在时作为 resume source；第一次 rotate 后永不回写旧目录。路径 segment 使用经过验证的 canonical UUID/stable built-in ID，拒绝 `..`、分隔符和空值。

- [ ] **Step 5: 运行 Session/Pi tests**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test assignment_lifecycle session -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test pi_engine session -- --nocapture
```

Expected: PASS；不同 Agent 无 Session 污染。

- [ ] **Step 6: 提交 Session 隔离**

```powershell
git add src-tauri/src/assignment/repository.rs src-tauri/src/engine/pi/mod.rs src-tauri/tests/assignment_lifecycle.rs src-tauri/tests/pi_engine.rs
git commit -m "feat: isolate agent work sessions"
```

---

### Task 7: 建立 Engine Harness 和 Assignment-aware Supervisor

**Files:**
- Create: `src-tauri/src/engine/harness.rs`
- Modify: `src-tauri/src/engine/mod.rs`
- Modify: `src-tauri/src/engine/supervisor.rs`
- Modify: `src-tauri/src/work/repository.rs`
- Modify: `src-tauri/tests/work_lifecycle.rs`
- Modify: `src-tauri/tests/assignment_lifecycle.rs`

- [ ] **Step 1: 写 attempt lifecycle 失败测试**

证明：

```text
Assignment claimed -> Run queued -> engine accepted -> Run/Assignment running
event envelope always carries assignment/agent/session
terminal engine event finalizes Run and Assignment exactly once
startup failure schedules retry or dead-letter
abort timeout faults only the execution owner and never frees capacity early
two different Works can execute up to global cap
same Work cannot overlap even through a race
```

- [ ] **Step 2: 运行测试确认 Supervisor 仍以 Work 为 key**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test assignment_lifecycle harness -- --nocapture`

Expected: FAIL。

- [ ] **Step 3: 提取 Harness request**

```rust
pub struct AssignmentExecutionRequest {
    pub assignment: AssignmentSummary,
    pub work: WorkDetail,
    pub agent: AgentInstanceSummary,
    pub session: AgentSessionSummary,
    pub input: EngineInput,
    pub resolved_model_label: String,
    pub effective_permission: PermissionMode,
}
```

`EngineHarness::execute` 只执行一个已 claim Assignment；不从自然语言创建 Assignment，也不构建 D 的分层上下文。

- [ ] **Step 4: 将 Supervisor active map 改为 execution key**

使用：

```rust
struct ExecutionKey { work_id: String, assignment_id: String, run_id: String }
```

仍由 queue 保证同 Work 单 in-flight；Supervisor 自身也用 Work guard 防御 race。`consume_events` 用 request identity 填充 envelope，不再写 `agent_id: None`/`assignment_id: None`。

- [ ] **Step 5: 让 Run creation 接受 Assignment identity**

将现有 `begin_run` 拆为 legacy wrapper 和 `begin_assignment_attempt`；消息只对 Lead 用户输入写入普通 `messages`，Member Assignment 不伪装成用户消息。

- [ ] **Step 6: 运行 lifecycle suites**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test assignment_lifecycle -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test work_lifecycle -- --nocapture
```

Expected: PASS。

- [ ] **Step 7: 提交 Harness**

```powershell
git add src-tauri/src/engine src-tauri/src/work/repository.rs src-tauri/tests/assignment_lifecycle.rs src-tauri/tests/work_lifecycle.rs
git commit -m "feat: execute assignment aware engine runs"
```

---

### Task 8: 实现 Scheduler、capacity 与 restart recovery

**Files:**
- Create: `src-tauri/src/assignment/scheduler.rs`
- Modify: `src-tauri/src/assignment/mod.rs`
- Modify: `src-tauri/src/app_state.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/tests/assignment_lifecycle.rs`

- [ ] **Step 1: 写 Scheduler 失败测试**

使用 paused Tokio time 和 Fake Harness 覆盖：hydrate after restart、oldest-head fairness、global cap、per-Agent cap、pool exhausted、wake on enqueue/completion、deadline、retry/backoff、dead-letter、clean shutdown、orphan recovery decision。

- [ ] **Step 2: 运行测试确认 Scheduler 不存在**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test assignment_lifecycle scheduler -- --nocapture`

Expected: FAIL。

- [ ] **Step 3: 实现单 owner Scheduler loop**

```rust
pub struct AssignmentSchedulerHandle {
    commands: mpsc::Sender<SchedulerCommand>,
    join: Arc<Mutex<Option<JoinHandle<()>>>>,
}
pub enum SchedulerCommand { Enqueued(String), CapacityReleased, Shutdown }
```

启动顺序：Repository recovery → load schedulable → hydrate Queue → spawn loop → 显示窗口。Scheduler 先 DB claim，成功后才在 Queue 标 in-flight；若 Queue claim 后 DB claim 失败，立即 release/reconcile。

- [ ] **Step 4: 实现恢复安全边界**

restart：

- 尚未开始或 `read_only`：重排。
- `idempotent_write`：按 retry policy 重排并发事件。
- `non_idempotent_write/unknown` 且 attempt 已启动：`recovery_confirmation_required`。

不允许因为 App 启动而静默重做不确定写操作。

- [ ] **Step 5: 组装 production startup/shutdown**

`AppState` 持有 scheduler handle；窗口显示前完成 recovery/hydration；Tauri exit 时发送 shutdown 并停止接受新 claim。不要让 detached task 持续写已关闭 DB。

- [ ] **Step 6: 运行 Scheduler 和 startup tests**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test assignment_lifecycle scheduler -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml lib::tests -- --nocapture
```

Expected: PASS。

- [ ] **Step 7: 提交 Scheduler**

```powershell
git add src-tauri/src/assignment src-tauri/src/app_state.rs src-tauri/src/lib.rs src-tauri/tests/assignment_lifecycle.rs
git commit -m "feat: schedule durable assignments fairly"
```

---

### Task 9: 把用户输入切换为 Lead Assignment 并实现 queue/steer/interrupt

**Files:**
- Create: `src-tauri/src/assignment/service.rs`
- Create: `src-tauri/src/assignment/commands.rs`
- Modify: `src-tauri/src/assignment/mod.rs`
- Modify: `src-tauri/src/work/service.rs`
- Modify: `src-tauri/src/work/commands.rs`
- Modify: `src-tauri/src/app_state.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/tests/work_lifecycle.rs`
- Modify: `src-tauri/tests/assignment_lifecycle.rs`

- [ ] **Step 1: 写用户控制失败测试**

覆盖：

```rust
start_work_persists_lead_assignment_before_scheduler_wake
enqueue_next_preserves_user_order
steer_uses_native_engine_when_supported
steer_degrades_to_cancel_merge_and_new_run_when_unsupported
interrupt_cancels_current_and_prioritizes_replacement
control_race_emits_one_authoritative_result
archived_work_rejects_new_assignments
```

- [ ] **Step 2: 运行测试确认旧 start_work 直接启动 Engine**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test assignment_lifecycle control -- --nocapture`

Expected: FAIL。

- [ ] **Step 3: 实现 Lead Assignment acceptance**

`WorkService::start_work` 继续解析文件/附件，但将用户可见 prompt/message、context manifest、Lead Agent ID、permission scope 作为一个事务写入 Assignment；commit 后 wake Scheduler。API 返回 `StartWorkOutput { assignment, run?, user_message }`，其中 Run 在异步 Scheduler 尚未启动时可为 `None`；相应调整 DTO，不能制造 fake Run。

- [ ] **Step 4: 实现三种控制**

commands：

```text
queue_work_input(workId, input)
steer_assignment(workId, assignmentId, input)
interrupt_and_replace(workId, input)
confirm_assignment_recovery(assignmentId, resume)
list_work_assignments(workId)
```

native steer only when capability true；降级时先持久化 control intent，再 cancel，合并旧 Assignment packet + 新指令，创建新 Run attempt；所有降级写 `queueControlApplied`。

- [ ] **Step 5: 运行 Service/Work tests**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test assignment_lifecycle control -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test work_lifecycle -- --nocapture
```

Expected: PASS。

- [ ] **Step 6: 提交控制 API**

```powershell
git add src-tauri/src/assignment src-tauri/src/work src-tauri/src/app_state.rs src-tauri/src/lib.rs src-tauri/tests/assignment_lifecycle.rs src-tauri/tests/work_lifecycle.rs
git commit -m "feat: queue and control lead assignments"
```

---

### Task 10: 让前端显示 Assignment 状态和明确控制语义

**Files:**
- Modify: `src/app/tauriClient.ts`
- Modify: `src/test/mockTauriClient.ts`
- Modify: `src/features/works/workStore.ts`
- Modify: `src/features/works/workStore.test.ts`
- Modify: `src/features/workspace/WorkComposer.tsx`
- Modify: `src/features/workspace/WorkDetail.tsx`
- Modify: `src/features/workspace/WorkTimeline.tsx`
- Modify: `src/features/workspace/WorkTimeline.test.tsx`
- Modify: `src/features/workspace/WorkInspector.tsx`
- Modify: `src/features/workspace/WorkInspector.test.tsx`
- Create: `src/features/workspace/AssignmentInspector.tsx`
- Create: `src/features/workspace/AssignmentInspector.test.tsx`
- Modify: `src/i18n/locales/en.json`
- Modify: `src/i18n/locales/zh-CN.json`
- Modify: `src/styles/workspace.css`

- [ ] **Step 1: 写 store/UI 失败测试**

覆盖：accepted queued 不显示“正在运行”、Run started 后才 running、Composer 三种控制、recovery confirmation、retry/dead-letter、Assignments Inspector 的依赖/attempt/result 摘要、control error 保留用户输入。

- [ ] **Step 2: 运行 focused tests 确认失败**

```powershell
pnpm test -- src/features/works/workStore.test.ts src/features/workspace/WorkTimeline.test.tsx src/features/workspace/WorkInspector.test.tsx src/features/workspace/AssignmentInspector.test.tsx
```

Expected: FAIL。

- [ ] **Step 3: 更新 typed client/store**

新增 C commands；store 按 Assignment ID 去重并以服务端状态为权威。`startWork` 可先收到 accepted Assignment、稍后从 event 收到 Run identity；不能假设同步返回 Run。

- [ ] **Step 4: 实现 Composer 控制**

Work 有 in-flight Assignment 时显示清晰菜单：排在下一步（默认）、纠偏当前任务、中断并替换。确认 destructive interrupt，显示 native/degraded 结果；不提供成员私聊。

- [ ] **Step 5: 实现 Inspector Assignments tab**

显示 assignee、kind、状态、依赖、attempt、retry/dead-letter、recovery reason 和 result summary（D 前允许为空）。不要显示 Engine opaque session ref。

- [ ] **Step 6: 运行 UI/type tests**

```powershell
pnpm test -- src/features/works src/features/workspace
pnpm typecheck
```

Expected: PASS。

- [ ] **Step 7: 提交前端控制**

```powershell
git add src/app/tauriClient.ts src/test/mockTauriClient.ts src/features/works src/features/workspace src/i18n/locales/en.json src/i18n/locales/zh-CN.json src/styles/workspace.css
git commit -m "feat: expose assignment queue controls"
```

---

### Task 11: 扩展 Activity Projector 的 Agent/Assignment 语义

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
- Modify: `src/features/activity/RawActivityRail.tsx`
- Modify: `src/features/activity/RawActivityRail.test.tsx`

- [ ] **Step 1: 写语义投影失败测试**

同一 Assignment 的 queued→claimed→running→terminal 原地聚合；不同 attempt 保留历史；identity key 含 agent/assignment/session；legacy `agentId=null` 显示为 legacy lead；retry/dead-letter/recovery/control 具备正确 salience；Raw Rail 完整保留。

- [ ] **Step 2: 运行 Activity tests 确认失败**

Run: `pnpm test -- src/features/activity`

Expected: FAIL，新 payload 尚未分类。

- [ ] **Step 3: 实现投影与展示**

主时间线使用“动词—对象—结果”，例如“主理人 · 任务已排队”“研究员 · 第 2 次尝试失败，30 秒后重试”；不暴露 queue slot、engine ref 或内部 JSON。终态保持单调，迟到的 running event 不覆盖 dead-letter/completed。

- [ ] **Step 4: 运行 Activity tests**

Run: `pnpm test -- src/features/activity`

Expected: PASS，A 的 persisted/live reconstruction 测试仍通过。

- [ ] **Step 5: 提交 Activity 语义**

```powershell
git add src/features/activity
git commit -m "feat: project assignment execution activity"
```

---

### Task 12: 完整验证、故障恢复演练与 C→D 交接审计

**Files:**
- Review: all C files
- Modify: `docs/architecture/buzz-upstream-map.md`（记录实际移植的 Buzz queue/fairness 函数、PiWork 改写点和对应测试；若最终为 clean-room 实现，明确记录“参考语义、无派生代码”）

- [ ] **Step 1: 运行完整质量门**

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
pnpm test
pnpm typecheck
pnpm build
```

Expected: 全部 exit 0。

- [ ] **Step 2: 运行确定性的恢复/竞态压力测试**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test assignment_lifecycle -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test engine_conformance -- --nocapture
```

重复运行 focused race tests 20 次（使用测试过滤器逐项调用，不用不稳定 sleep），确认 terminal exactly once、同 Work 无 overlap、abort timeout 不泄漏 capacity。

- [ ] **Step 3: 真实窗口恢复演练**

Run: `pnpm tauri dev`

验收：

1. 提交输入先显示“已排队”，引擎实际 start 后才显示运行。
2. 两个 Work 可按全局 cap 并行；单 Work 不重叠。
3. queue/steer/interrupt 三种语义可见；Pi 无 native steer 时显示降级。
4. Agent/Assignment/Session identity 出现在 Activity/Raw Rail，Engine opaque ref 不出现。
5. 强制结束应用后重启：只读任务可解释地重排；不确定写任务要求确认。
6. 不同 Agent/Work session 目录隔离，legacy Work 可打开。

- [ ] **Step 4: 审计设计覆盖与 D 接口**

逐项对照设计 §§6.8–6.9、7.5、9–12、14.2、15–18、19.C、20。确认 D 可依赖：

```text
AssignmentRepository.accept / dependency / waiting / retry
AssignmentSchedulerHandle.wake
EngineHarness.execute
AgentSessionRepository
Assignment-aware WorkEvent identity
queue/steer/interrupt commands
```

确认没有模型可调用 `delegate_assignment`、没有 Result Envelope/Work Ledger/Memory/Context Builder。

- [ ] **Step 5: 检查 diff**

```powershell
git diff --check
git status --short
```

Expected: 无 whitespace errors，仅 C 范围与用户原有改动。

- [ ] **Step 6: 记录最终验收结果**

本任务默认不产生新代码。如果 Step 1–5 暴露 C 范围缺陷，返回对应 Task 修复并重新运行完整门禁；不要创建只含验收文案的空提交。

---

## 子项目 C 完成定义

- [ ] Assignment/Session schema、状态机、Repository 与恢复已验证。
- [ ] Buzz 派生 queue 满足单 Work in-flight、跨 Work fairness、caps、retry、deadline、dead-letter。
- [ ] EngineAdapter capability negotiation 和 conformance suite 通过。
- [ ] 每个 Agent × Work Session 隔离；Pi 不再按 Work 共用 session。
- [ ] 用户输入经持久 Lead Assignment 调度；accepted 与 running 语义不混淆。
- [ ] queue/steer/interrupt 及降级、恢复确认在 UI/Activity 可见。
- [ ] 所有事件携带真实 Agent/Assignment/Session identity。
- [ ] 没有提前实现 D/E 范围。
