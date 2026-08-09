# PiWork Activity Protocol 与 Buzz Projector Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在不引入多成员和 Assignment 的前提下，把当前单 Pi Run 升级为可持久、可关联、可聚合、可从 SQLite 重建的 Activity Protocol v2，并用 Buzz 派生 Projector 驱动主时间线和 Inspector Raw Rail。

**Architecture:** `PiEngineAdapter` 继续把 Pi RPC 翻译为引擎无关的 `EngineEvent`；`EngineSupervisor` 为事件补齐 Work/Run/Turn/Session 因果上下文，先写 SQLite，再交给 Buzz Observer 派生的进程内观察器和 Tauri Publisher。前端只消费持久化的 `WorkEventEnvelope`，通过 Buzz Transcript 派生的纯 Projector 合并 delta、工具生命周期、计划和权限事件；主时间线展示高信噪比投影，Inspector 展示同一 Journal 的完整 Raw Rail。

**Tech Stack:** Rust 2024、Tokio、SQLx/SQLite、Serde、ts-rs、Tauri 2、React 19、TypeScript、Zustand、Vitest、Testing Library。

---

## 范围与实施约束

本计划只实现已确认设计中的子项目 A。完成后仍只有当前单 Agent、单 `EngineSupervisor` 和 Pi production adapter；不创建 `AgentDefinition`、`AgentInstance`、`Assignment`、Scheduler 或 Agent × Work Session 目录。后续子项目顺序保持：

```text
A Activity Protocol 与 Buzz Projector
→ B Agent Domain 与智能体中心
→ C Assignment Queue 与 Session Harness
→ D 主理委派、Result Envelope 与 Context Builder
```

必须保持以下不变量：

1. `EngineEvent` 不包含 React 展示文案。
2. 每个 `WorkEventEnvelope` 先成功写入 SQLite，之后才进入观察器和 Tauri 实时事件。
3. UI 丢失实时事件后，重新读取 `WorkDetail.events` 可以得到相同投影。
4. Projector 可以原地合并进行中活动，但数据库事件永远 append-only。
5. Pi 不支持的 plan/permission/session rotate 能力只在协议中可表达，不能伪造事件。
6. 旧 `version = 1` Event 无损读取；新事件写 `version = 2`。
7. `research_buzz_piwork_20260809.md` 当前是用户保留的未跟踪调研文件。所有提交必须逐文件 `git add`，不得用 `git add .`。

## Buzz 固定来源与移植方式

固定基线：`block/buzz@5bf78671f45178f8de02ba18d3d321cbbf19cd1f`。

| Buzz 文件 | PiWork 目标 | 保留内容 | 删除/替换内容 |
|---|---|---|---|
| `crates/buzz-acp/src/observer.rs` | `src-tauri/src/engine/activity_observer.rs` | bounded replay、broadcast、snapshot、best-effort observer | Channel/ACP/agent-index 字段改为已经 journaled 的 `WorkEventEnvelope` |
| `desktop/src/features/agents/ui/agentSessionTypes.ts` | `src/features/activity/activityTypes.ts` | item union、render class、tone、descriptor、identity | Nostr、Relay、pubkey、ACP-only 字段 |
| `desktop/src/features/agents/ui/agentSessionTranscript.ts` | `src/features/activity/activityProjector.ts` | copy-on-write reducer、delta upsert、tool/plan/permission correlation、terminal monotonicity | raw ACP JSON-RPC parsing改为 typed `WorkEventPayload` parsing |
| `desktop/src/features/agents/ui/agentSessionTranscriptGrouping.ts` | `src/features/activity/activityGrouping.ts` | Session/Turn bucket、同类工具 burst、stable keys | Buzz Channel prompt framing、Relay setup blocks |
| Buzz transcript/grouping tests | 对应 `*.test.ts` | 工具 start/update/end 合一、delta 合并、plan replacement、permission resolution、Session boundary key stability | MJS loader、Vue alias、Relay fixtures |

每个派生源码文件都写明固定 commit、原文件和 PiWork 差异；归属集中记录在 `THIRD_PARTY_NOTICES.md` 与 `docs/architecture/buzz-upstream-map.md`。

## 文件职责图

### Rust / SQLite

- Create `src-tauri/migrations/0004_activity_protocol_v2.sql`：为 Event Journal 增加可选关联列和索引。
- Modify `src-tauri/src/domain/event.rs`：定义 Activity Protocol v2 envelope、枚举和向后兼容 JSON。
- Modify `src-tauri/src/domain/mod.rs`：导出新增 TypeScript bindings。
- Create `src-tauri/src/engine/activity_observer.rs`：保存 Buzz 派生的 committed-event replay/broadcast。
- Modify `src-tauri/src/engine/publisher.rs`：Tauri 发布前把已提交事件送入 observer。
- Modify `src-tauri/src/engine/mod.rs`：扩展引擎无关 `EngineEvent`。
- Modify `src-tauri/src/engine/pi/mod.rs`：翻译 Pi thought、tool progress、usage 和未知 raw event。
- Modify `src-tauri/src/engine/fake.rs`：让 Fake Adapter 覆盖 v2 事件。
- Modify `src-tauri/src/engine/supervisor.rs`：补齐 Session/Turn/causation/correlation 并保持 journal-before-publish。
- Modify `src-tauri/src/work/repository.rs`：持久化、读取 v2 envelope；旧 v1 行继续可读。
- Modify `src-tauri/tests/storage_contract.rs`、`src-tauri/tests/pi_engine.rs`：迁移和 Pi translator 合同测试。

### React / TypeScript

- Regenerate `src/bindings/WorkEventEnvelope.ts`、`WorkEventPayload.ts` 及新增枚举 bindings。
- Create `src/features/activity/activityTypes.ts`：Buzz 派生的 UI Activity 类型。
- Create `src/features/activity/activityProjector.ts`：append-only events → consolidated activity items。
- Create `src/features/activity/activityProjector.test.ts`：Projector 上游派生回归测试。
- Create `src/features/activity/activityPresentation.ts`：工具 action/render class/tone 分类。
- Create `src/features/activity/activityPresentation.test.ts`：read/write/shell/permission/error 分类测试。
- Create `src/features/activity/activityGrouping.ts`：Session/Turn 与 tool burst 分组。
- Create `src/features/activity/activityGrouping.test.ts`：stable boundary/group key 测试。
- Create `src/features/activity/ActivityFeed.tsx`：高信噪比活动详情。
- Create `src/features/activity/ActivityFeed.test.tsx`：折叠、权限、错误和 suppressed 行为。
- Create `src/features/activity/RawActivityRail.tsx`：完整 Journal 检查轨道。
- Create `src/features/activity/RawActivityRail.test.tsx`：不丢事件、关联字段可见。
- Modify `src/features/workspace/WorkTimeline.tsx`：用 Projector 代替手写 delta/tool 拼装。
- Modify `src/features/workspace/WorkTimeline.test.tsx`：锁定单 Run 集成行为。
- Modify `src/features/workspace/executionProgress.ts` 和测试：识别 pending/progress，按 `toolCallId` 计一次。
- Modify `src/features/workspace/WorkInspector.tsx`；Create `WorkInspector.test.tsx`：logs tab 使用 Raw Rail。
- Modify `src/domain/work.ts`、`src/test/mockTauriClient.ts`、相关事件 fixture：优先使用 `eventId` 去重，兼容旧事件 fallback。
- Modify `src/i18n/locales/en.json`、`zh-CN.json`、`src/styles/workspace.css`：Activity 文案和视觉层级。

### 归属与架构记录

- Create `THIRD_PARTY_NOTICES.md`：声明 Buzz Apache-2.0 来源。
- Create `docs/architecture/buzz-upstream-map.md`：记录上游文件、测试和本地差异。

---

### Task 1: 冻结 Activity Protocol v2 的领域与 SQLite 合同

**Files:**
- Create: `src-tauri/migrations/0004_activity_protocol_v2.sql`
- Modify: `src-tauri/src/domain/event.rs`
- Modify: `src-tauri/tests/storage_contract.rs`

- [ ] **Step 1: 写失败的迁移与兼容性测试**

在 `src-tauri/tests/storage_contract.rs` 增加：

```rust
#[test]
fn activity_protocol_migration_uses_stable_lf_line_endings() {
    let migration = include_bytes!("../migrations/0004_activity_protocol_v2.sql");
    assert!(!migration.contains(&b'\r'));
}

#[tokio::test]
async fn activity_protocol_migration_adds_event_context_columns() {
    let database = Database::open_in_memory().await.unwrap();
    let columns = sqlx::query_scalar::<_, String>(
        "SELECT name FROM pragma_table_info('events') ORDER BY cid",
    )
    .fetch_all(database.pool())
    .await
    .unwrap();

    for expected in [
        "turn_id",
        "session_id",
        "agent_id",
        "assignment_id",
        "causation_id",
        "correlation_id",
    ] {
        assert!(columns.contains(&expected.to_string()), "missing {expected}");
    }
}
```

在 `src-tauri/src/domain/event.rs` 的测试模块增加两个测试：

```rust
#[test]
fn v2_envelope_serializes_optional_activity_identity() {
    let envelope = WorkEventEnvelope {
        version: 2,
        event_id: Some("event-1".into()),
        work_id: "work-1".into(),
        run_id: "run-1".into(),
        turn_id: Some("turn-1".into()),
        session_id: Some("session-1".into()),
        agent_id: None,
        assignment_id: None,
        causation_id: Some("event-0".into()),
        correlation_id: Some("run-1".into()),
        sequence: 2,
        occurred_at: Utc.with_ymd_and_hms(2026, 8, 9, 8, 0, 0).unwrap(),
        payload: WorkEventPayload::ThoughtDelta { text: "checking".into() },
    };

    let value = serde_json::to_value(envelope).unwrap();
    assert_eq!(value["eventId"], "event-1");
    assert_eq!(value["turnId"], "turn-1");
    assert_eq!(value["sessionId"], "session-1");
    assert_eq!(value["causationId"], "event-0");
    assert_eq!(value["payload"]["type"], "thoughtDelta");
}

#[test]
fn v1_envelope_without_activity_identity_still_deserializes() {
    let value = json!({
        "version": 1,
        "workId": "work-1",
        "runId": "run-1",
        "sequence": 1,
        "occurredAt": "2026-08-09T08:00:00Z",
        "payload": { "type": "assistantDelta", "text": "legacy" }
    });
    let envelope: WorkEventEnvelope = serde_json::from_value(value).unwrap();
    assert_eq!(envelope.event_id, None);
    assert_eq!(envelope.turn_id, None);
    assert_eq!(envelope.session_id, None);
}
```

- [ ] **Step 2: 运行测试并确认它们因缺少 schema/字段失败**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml activity_protocol -- --nocapture
```

Expected: FAIL，错误明确指向缺少 migration 文件、Event 列或 `WorkEventEnvelope`/`ThoughtDelta` 字段。

- [ ] **Step 3: 增加只追加 nullable 列的 migration**

`src-tauri/migrations/0004_activity_protocol_v2.sql` 完整内容：

```sql
ALTER TABLE events ADD COLUMN turn_id TEXT;
ALTER TABLE events ADD COLUMN session_id TEXT;
ALTER TABLE events ADD COLUMN agent_id TEXT;
ALTER TABLE events ADD COLUMN assignment_id TEXT;
ALTER TABLE events ADD COLUMN causation_id TEXT;
ALTER TABLE events ADD COLUMN correlation_id TEXT;

CREATE INDEX idx_events_work_turn_sequence
    ON events(work_id, turn_id, sequence);
CREATE INDEX idx_events_assignment_sequence
    ON events(assignment_id, sequence)
    WHERE assignment_id IS NOT NULL;
```

不要修改 `0001_foundation.sql`；已发布 migration 的 checksum 必须保持不变。

- [ ] **Step 4: 扩展 envelope 和 payload 类型**

在 `WorkEventEnvelope` 中保留现有字段并加入以下可选字段；每个 Option 同时使用 `serde(default, skip_serializing_if = "Option::is_none")` 和 `ts(optional)`，避免旧 JSON 和现有 TypeScript fixture 被迫伪造身份：

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
#[ts(optional)]
pub event_id: Option<String>,
#[serde(default, skip_serializing_if = "Option::is_none")]
#[ts(optional)]
pub turn_id: Option<String>,
#[serde(default, skip_serializing_if = "Option::is_none")]
#[ts(optional)]
pub session_id: Option<String>,
#[serde(default, skip_serializing_if = "Option::is_none")]
#[ts(optional)]
pub agent_id: Option<String>,
#[serde(default, skip_serializing_if = "Option::is_none")]
#[ts(optional)]
pub assignment_id: Option<String>,
#[serde(default, skip_serializing_if = "Option::is_none")]
#[ts(optional)]
pub causation_id: Option<String>,
#[serde(default, skip_serializing_if = "Option::is_none")]
#[ts(optional)]
pub correlation_id: Option<String>,
```

定义线协议枚举：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case", export_to = binding_path!())]
pub enum PermissionOutcome {
    AllowedOnce,
    AllowedForRun,
    Denied,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case", export_to = binding_path!())]
pub enum SessionTransition {
    Created,
    Resumed,
    Rotated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case", export_to = binding_path!())]
pub enum LivenessState {
    Alive,
    Stalled,
}
```

向 `WorkEventPayload` 增加以下 variants，保留现有六个 variants 原名以读取 v1：

```rust
ThoughtDelta { text: String },
PlanChanged { plan_id: String, revision: u32, text: String },
ToolPending { tool_call_id: String, tool_name: String, input_summary: String },
ToolProgress { tool_call_id: String, tool_name: String, output_summary: String },
PermissionRequested {
    request_id: String,
    tool_call_id: Option<String>,
    title: String,
    detail: String,
},
PermissionResolved { request_id: String, outcome: PermissionOutcome },
Waiting { reason: String },
Liveness { state: LivenessState },
SessionChanged { transition: SessionTransition, reason: Option<String> },
ArtifactProduced { path: String },
ValidationProduced { command: String, success: bool, summary: String },
UsageUpdated {
    input_tokens: u32,
    output_tokens: u32,
    cache_read_tokens: u32,
    cache_write_tokens: u32,
    total_tokens: u32,
},
RawEngineEvent { kind: String, payload_json: String },
```

同步更新手写 `Deserialize` 的 `WireEnvelope`，并把 `wire.event_id`、`wire.turn_id`、`wire.session_id`、`wire.agent_id`、`wire.assignment_id`、`wire.causation_id`、`wire.correlation_id` 逐字段写入返回的 `WorkEventEnvelope`，不能只改 Serialize 路径。

- [ ] **Step 5: 运行协议和 migration 测试**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml activity_protocol -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml domain::event::tests -- --nocapture
```

Expected: PASS。

- [ ] **Step 6: 提交协议骨架**

```powershell
git add -- src-tauri/migrations/0004_activity_protocol_v2.sql src-tauri/src/domain/event.rs src-tauri/tests/storage_contract.rs
git commit -m "feat: define activity protocol v2"
```

---

### Task 2: 让完整 Event 上下文从 SQLite 往返

**Files:**
- Modify: `src-tauri/src/work/repository.rs`

- [ ] **Step 1: 写失败的 Repository round-trip 测试**

在 `src-tauri/src/work/repository.rs` 的测试模块增加 `activity_context_round_trips_through_the_journal`。沿用现有 `event_append_waits_for_a_concurrent_session_attachment` 的 Work/Run setup，构造：

```rust
let envelope = WorkEventEnvelope {
    version: 2,
    event_id: Some("event-1".into()),
    work_id: work.summary.id.clone(),
    run_id: started.run.id.clone(),
    turn_id: Some("turn-1".into()),
    session_id: Some("session-1".into()),
    agent_id: None,
    assignment_id: None,
    causation_id: Some("event-0".into()),
    correlation_id: Some("correlation-1".into()),
    sequence: 1,
    occurred_at: Utc::now(),
    payload: WorkEventPayload::ToolProgress {
        tool_call_id: "tool-1".into(),
        tool_name: "read".into(),
        output_summary: "halfway".into(),
    },
};
repository.append_event_and_transition(&envelope).await.unwrap();
let loaded = repository.events_for_run(&started.run.id).await.unwrap();
assert_eq!(loaded, vec![envelope]);
```

- [ ] **Step 2: 运行测试并确认上下文字段丢失**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml activity_context_round_trips -- --nocapture
```

Expected: FAIL，因为 `EventRow` 和 SQL 尚未读取/写入 v2 列与 `events.id`。

- [ ] **Step 3: 修改 EventRow 和所有生产查询**

`EventRow` 变为：

```rust
#[derive(FromRow)]
struct EventRow {
    id: String,
    work_id: String,
    run_id: String,
    turn_id: Option<String>,
    session_id: Option<String>,
    agent_id: Option<String>,
    assignment_id: Option<String>,
    causation_id: Option<String>,
    correlation_id: Option<String>,
    sequence: i64,
    version: i64,
    occurred_at: DateTime<Utc>,
    payload: String,
}
```

把 `load_detail` 和 `events_for_run` 的 SELECT 字段统一为：

```sql
events.id, events.work_id, events.run_id,
events.turn_id, events.session_id, events.agent_id, events.assignment_id,
events.causation_id, events.correlation_id,
events.sequence, events.version, events.occurred_at, events.payload
```

`TryFrom<EventRow>` 把 `id` 映射为 `event_id: Some(row.id)`，其余 nullable 列原样映射。

- [ ] **Step 4: 修改生产 INSERT，禁止 Repository 另造 Event ID**

`append_event_and_transition` 的 SQL 使用 envelope identity：

```rust
let event_id = envelope.event_id.as_deref().ok_or_else(|| {
    AppError::invalid_input("eventId", "new Work events require an event id")
})?;
sqlx::query(
    "INSERT INTO events \
     (id, work_id, run_id, turn_id, session_id, agent_id, assignment_id, \
      causation_id, correlation_id, sequence, version, occurred_at, payload) \
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
)
.bind(event_id)
.bind(&envelope.work_id)
.bind(&envelope.run_id)
.bind(&envelope.turn_id)
.bind(&envelope.session_id)
.bind(&envelope.agent_id)
.bind(&envelope.assignment_id)
.bind(&envelope.causation_id)
.bind(&envelope.correlation_id)
.bind(i64::from(envelope.sequence))
.bind(i64::from(envelope.version))
.bind(envelope.occurred_at)
.bind(payload)
.execute(&mut *transaction)
.await?;
```

`finalize_run_failure` 也必须先创建 `event_id`，并写相同列；不要留下第二条生产 INSERT 继续只写 v1 列。

- [ ] **Step 5: 修复现有 Rust event fixture 并运行 Repository 测试**

所有新写入 Repository 的测试 envelope 增加 `event_id: Some(Uuid::new_v4().to_string())` 和其余可选字段 `None`。直接测试 v1 JSON deserialization 的 fixture 保持缺字段，证明兼容性。

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml work::repository::tests -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test storage_contract -- --nocapture
```

Expected: PASS。

- [ ] **Step 6: 提交持久化往返**

```powershell
git add -- src-tauri/src/work/repository.rs
git commit -m "feat: persist activity event context"
```

---

### Task 3: 移植 Buzz committed-event Observer

**Files:**
- Create: `src-tauri/src/engine/activity_observer.rs`
- Modify: `src-tauri/src/engine/mod.rs`
- Modify: `src-tauri/src/engine/publisher.rs`

- [ ] **Step 1: 写 Buzz 派生的 buffer 与 broadcast 测试**

在新文件测试模块增加：

```rust
#[tokio::test]
async fn observer_replays_committed_events_and_broadcasts_live_events() {
    let observer = ActivityObserverHandle::in_process_with_capacity(2);
    let mut live = observer.subscribe();
    let first = event("event-1", 1);
    let second = event("event-2", 2);
    let third = event("event-3", 3);

    observer.emit_committed(first);
    observer.emit_committed(second.clone());
    observer.emit_committed(third.clone());

    assert_eq!(observer.snapshot(), vec![second, third.clone()]);
    assert_eq!(live.recv().await.unwrap().event_id.as_deref(), Some("event-1"));
    assert_eq!(live.recv().await.unwrap().event_id.as_deref(), Some("event-2"));
    assert_eq!(live.recv().await.unwrap(), third);
}
```

测试 helper `event(id, sequence)` 返回最小 `WorkEventEnvelope`，`version = 2`，payload 使用 `Liveness { state: Alive }`。

- [ ] **Step 2: 运行测试并确认模块不存在**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml observer_replays_committed -- --nocapture
```

Expected: FAIL，模块或类型未定义。

- [ ] **Step 3: 从 Buzz observer.rs 移植 bounded observer**

文件头必须包含：

```rust
// Adapted from block/buzz at 5bf78671f45178f8de02ba18d3d321cbbf19cd1f,
// Apache-2.0. Original: crates/buzz-acp/src/observer.rs.
// PiWork changes: observes only journaled WorkEventEnvelope values; removes
// Relay, Channel, ACP session and agent-slot transport concerns.
```

实现保持 Buzz 的 `Arc<Inner> + broadcast::Sender + Mutex<VecDeque>` 结构：

```rust
const ACTIVITY_BUFFER_CAP: usize = 1_000;

#[derive(Clone)]
pub struct ActivityObserverHandle {
    inner: Arc<ActivityObserverInner>,
}

struct ActivityObserverInner {
    tx: broadcast::Sender<WorkEventEnvelope>,
    buffer: Mutex<VecDeque<WorkEventEnvelope>>,
    capacity: usize,
}

impl ActivityObserverHandle {
    pub fn in_process() -> Self {
        Self::in_process_with_capacity(ACTIVITY_BUFFER_CAP)
    }

    #[cfg(test)]
    fn in_process_with_capacity(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self {
            inner: Arc::new(ActivityObserverInner {
                tx,
                buffer: Mutex::new(VecDeque::with_capacity(capacity)),
                capacity,
            }),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<WorkEventEnvelope> {
        self.inner.tx.subscribe()
    }

    pub fn snapshot(&self) -> Vec<WorkEventEnvelope> {
        self.inner
            .buffer
            .lock()
            .map(|buffer| buffer.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn emit_committed(&self, event: WorkEventEnvelope) {
        if let Ok(mut buffer) = self.inner.buffer.lock() {
            if buffer.len() >= self.inner.capacity {
                buffer.pop_front();
            }
            buffer.push_back(event.clone());
        }
        let _ = self.inner.tx.send(event);
    }
}
```

在 `engine/mod.rs` 增加 `pub mod activity_observer;`。

- [ ] **Step 4: 只观察已经 journaled 的事件**

`TauriEventPublisher` 增加 observer：

```rust
pub struct TauriEventPublisher {
    app_handle: tauri::AppHandle,
    observer: ActivityObserverHandle,
}

pub fn new(app_handle: tauri::AppHandle) -> Self {
    Self { app_handle, observer: ActivityObserverHandle::in_process() }
}
```

`publish` 的顺序为：

```rust
self.observer.emit_committed(envelope.clone());
self.app_handle
    .emit("piwork://work-event", envelope)
    .map_err(|error| AppError::event_publish(error.to_string()))
```

不要在 `EngineAdapter` 或 Repository transaction 内调用 observer。现有 `consume_events` 已保证只有 `append_event_and_transition` 成功后才调用 `publisher.publish`，因此 observer snapshot 不会包含未提交事实。

- [ ] **Step 5: 运行 Observer 与 Publisher 测试**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml engine::activity_observer -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml engine::publisher -- --nocapture
```

Expected: PASS。

- [ ] **Step 6: 提交 Observer 移植**

```powershell
git add -- src-tauri/src/engine/activity_observer.rs src-tauri/src/engine/mod.rs src-tauri/src/engine/publisher.rs
git commit -m "feat: add journal-aware activity observer"
```

---

### Task 4: 扩展 EngineEvent 并让 Supervisor 补齐因果上下文

**Files:**
- Modify: `src-tauri/src/engine/mod.rs`
- Modify: `src-tauri/src/engine/fake.rs`
- Modify: `src-tauri/src/engine/supervisor.rs`

- [ ] **Step 1: 写 EngineEvent 和 Supervisor 失败测试**

在 `engine/mod.rs` 测试中增加：

```rust
#[test]
fn only_run_terminal_events_are_terminal() {
    assert!(!EngineEvent::ThoughtDelta { text: "x".into() }.is_terminal());
    assert!(!EngineEvent::Waiting { reason: "dependency".into() }.is_terminal());
    assert!(EngineEvent::RunFailed { message: "boom".into() }.is_terminal());
}
```

在 `engine/supervisor.rs` 现有 Channel publisher 测试附近增加集成断言：启动 Fake Run，收集前两个 published envelopes，并验证：

```rust
assert_eq!(events[0].version, 2);
assert!(events[0].event_id.is_some());
assert_eq!(events[0].turn_id.as_deref(), Some(events[0].run_id.as_str()));
assert_eq!(events[0].session_id.as_deref(), Some("fake-session"));
assert_eq!(events[0].correlation_id.as_deref(), Some(events[0].run_id.as_str()));
assert_eq!(events[1].causation_id, events[0].event_id);
```

如果 Fake Adapter 当前生成随机 session ID，先在测试 adapter 中固定为 `fake-session`，production Fake 行为无需固定。

- [ ] **Step 2: 运行测试并确认 v2/context 断言失败**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml only_run_terminal_events -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml supervisor -- --nocapture
```

Expected: FAIL，因为 EngineEvent variants 和 session-aware start signal 尚不存在。

- [ ] **Step 3: 扩展 EngineEvent 并保持与 WorkEventPayload 一一映射**

给 `EngineEvent` 增加与 Task 1 相同的非终态 variants；`kind()` 使用稳定 snake_case：

```rust
use crate::domain::event::{LivenessState, PermissionOutcome, SessionTransition};
```

```rust
Self::ThoughtDelta { .. } => "thought_delta",
Self::PlanChanged { .. } => "plan_changed",
Self::ToolPending { .. } => "tool_pending",
Self::ToolProgress { .. } => "tool_progress",
Self::PermissionRequested { .. } => "permission_requested",
Self::PermissionResolved { .. } => "permission_resolved",
Self::Waiting { .. } => "waiting",
Self::Liveness { .. } => "liveness",
Self::SessionChanged { .. } => "session_changed",
Self::ArtifactProduced { .. } => "artifact_produced",
Self::ValidationProduced { .. } => "validation_produced",
Self::UsageUpdated { .. } => "usage_updated",
Self::RawEngineEvent { .. } => "raw_engine_event",
```

`is_terminal()` 仍然只匹配 `RunCompleted | RunFailed`。在 `impl From<EngineEvent> for WorkEventPayload` 中穷尽映射每个字段，不允许 `_ =>` 吞事件。

- [ ] **Step 4: 让 start signal 携带 Engine session ID**

把 Copy-only 的 signal 改为 cloneable metadata：

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
enum StartSignal {
    Pending,
    Ready { session_id: String },
    Failed,
}
```

所有 `*start_signal.borrow()` 改为 `start_signal.borrow().clone()`。`engine.start` 返回并且 `attach_engine_session` 成功后，发送：

```rust
signal_sender.send_replace(StartSignal::Ready {
    session_id: session.session_id.clone(),
});
```

不能只是读取 watch 当前值：Pi Adapter 会在 `start()` 返回前发送 `RunStarted`，直接处理会让第一批事件缺少 Session。把启动期事件有界缓存在 consumer 内，同时持续 drain mpsc，避免 Adapter 因 channel 满而无法返回：

```rust
const STARTUP_EVENT_BUFFER_CAP: usize = 1_000;

async fn wait_for_engine_session(
    receiver: &mut mpsc::Receiver<EngineEvent>,
    signal: &mut watch::Receiver<StartSignal>,
) -> Result<(String, VecDeque<EngineEvent>), ConsumerOutcome> {
    let mut buffered = VecDeque::new();
    let mut receiver_closed = false;
    loop {
        match signal.borrow().clone() {
            StartSignal::Ready { session_id } => return Ok((session_id, buffered)),
            StartSignal::Failed => return Err(ConsumerOutcome::StartFailed),
            StartSignal::Pending => {}
        }
        if receiver_closed {
            if signal.changed().await.is_err() {
                return Err(ConsumerOutcome::StartFailed);
            }
            continue;
        }
        tokio::select! {
            biased;
            changed = signal.changed() => {
                if changed.is_err() {
                    return Err(ConsumerOutcome::StartFailed);
                }
            }
            event = receiver.recv() => match event {
                Some(event) if buffered.len() < STARTUP_EVENT_BUFFER_CAP => {
                    buffered.push_back(event);
                }
                Some(_) => return Err(ConsumerOutcome::Abnormal(
                    "Engine emitted too many events before session attachment",
                )),
                None => receiver_closed = true,
            },
        }
    }
}
```

`consume_events` 先调用该 helper；Ready 后先 `pop_front()` 处理 buffered events，再读取 receiver。删除原来会在 session 未知时 journal 或在 terminal-startup drain 中丢事件的 `await_start_signal*` 分支。这样每条正常 Engine event 都有真实 Session ID，且 startup overflow 会显式失败而不是无限占用内存。

- [ ] **Step 5: 构造 v2 envelope 与因果链**

`consume_events` 初始化：

```rust
let turn_id = run_id.clone();
let correlation_id = run_id.clone();
let mut previous_event_id: Option<String> = None;
```

每个 event 写入前：

```rust
let event_id = Uuid::new_v4().to_string();
let envelope = WorkEventEnvelope {
    version: 2,
    event_id: Some(event_id.clone()),
    work_id: work_id.clone(),
    run_id: run_id.clone(),
    turn_id: Some(turn_id.clone()),
    session_id: Some(session_id.clone()),
    agent_id: None,
    assignment_id: None,
    causation_id: previous_event_id.clone(),
    correlation_id: Some(correlation_id.clone()),
    sequence,
    occurred_at: Utc::now(),
    payload: event.into(),
};
```

只有 `append_event_and_transition` 成功后执行 `previous_event_id = Some(event_id)`。失败事件不得成为后续 causation。

`finalize_run_failure` 生成的 envelope 同样写 `version = 2`、UUID event ID、turn/correlation = run ID；如果无法知道最后一个 committed event，`causation_id = None`，不要猜测。

- [ ] **Step 6: 更新 Fake Adapter 覆盖新增投影路径**

Fake 正常事件序列在现有 RunStarted 后加入：

```rust
EngineEvent::ThoughtDelta { text: "Inspecting the Work".into() },
EngineEvent::PlanChanged {
    plan_id: "default".into(),
    revision: 1,
    text: "- inspect\n- execute\n- validate".into(),
},
```

不要让 Fake 生成 permission request，因为正常测试会无人响应；permission 只由 domain/projector fixture 验证。

- [ ] **Step 7: 运行 Supervisor、Fake 和 Work lifecycle 测试**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml engine::supervisor -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml engine::fake -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test work_lifecycle -- --nocapture
```

Expected: PASS，且测试收到的 sequence 连续、causation 只指向上一条已提交 event。

- [ ] **Step 8: 提交 Engine/Supervisor 协议**

```powershell
git add -- src-tauri/src/engine/mod.rs src-tauri/src/engine/fake.rs src-tauri/src/engine/supervisor.rs
git commit -m "feat: attach activity context to engine events"
```

---

### Task 5: 深化 Pi RPC translator，未知事件不再静默丢失

**Files:**
- Modify: `src-tauri/src/engine/pi/mod.rs`
- Modify: `src-tauri/tests/pi_engine.rs`

- [ ] **Step 1: 写 Pi thought/progress/usage/raw 失败测试**

在 `src-tauri/tests/pi_engine.rs` 增加：

```rust
#[test]
fn rpc_rich_activity_is_translated_without_fabricating_capabilities() {
    let mut translator = RpcEventTranslator::default();

    assert_eq!(translator.translate(json!({
        "type": "message_update",
        "assistantMessageEvent": {"type": "thinking_delta", "delta": "checking"}
    })), Some(EngineEvent::ThoughtDelta { text: "checking".into() }));

    assert_eq!(translator.translate(json!({
        "type": "tool_execution_update",
        "toolCallId": "call-1",
        "toolName": "bash",
        "partialResult": {"content": [{"type": "text", "text": "12/20 tests"}]}
    })), Some(EngineEvent::ToolProgress {
        tool_call_id: "call-1".into(),
        tool_name: "bash".into(),
        output_summary: "12/20 tests".into(),
    }));

    assert_eq!(translator.translate(json!({
        "type": "message_end",
        "message": {"role": "assistant", "usage": {
            "input": 10, "output": 20, "cacheRead": 3,
            "cacheWrite": 4, "totalTokens": 37
        }}
    })), Some(EngineEvent::UsageUpdated {
        input_tokens: 10,
        output_tokens: 20,
        cache_read_tokens: 3,
        cache_write_tokens: 4,
        total_tokens: 37,
    }));

    assert!(matches!(
        translator.translate(json!({"type": "compaction_start", "reason": "overflow"})),
        Some(EngineEvent::RawEngineEvent { kind, payload_json })
            if kind == "compaction_start" && payload_json.contains("overflow")
    ));
}
```

再断言没有 Pi 原生来源时 translator 不产生 `PlanChanged` 或 `PermissionRequested`；测试方式是只检查上述输入的精确结果，不增加猜测映射。

- [ ] **Step 2: 运行 translator 测试并确认失败**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test pi_engine rpc_rich_activity -- --nocapture
```

Expected: FAIL。

- [ ] **Step 3: 翻译已知 Pi 0.80.2 事件**

在现有 `match` 增加：

```rust
"message_update"
    if message.pointer("/assistantMessageEvent/type").and_then(Value::as_str)
        == Some("thinking_delta") =>
{
    message.pointer("/assistantMessageEvent/delta")
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(|text| EngineEvent::ThoughtDelta { text: text.into() })
}
"tool_execution_update" => Some(EngineEvent::ToolProgress {
    tool_call_id: required_string(&message, "toolCallId")?,
    tool_name: required_string(&message, "toolName")?,
    output_summary: summarize_tool_result(message.get("partialResult")),
}),
"message_end" if message.pointer("/message/role").and_then(Value::as_str)
    == Some("assistant") => usage_event(&message),
```

`usage_event` 对缺字段使用 0，并用 `u32::try_from`；负数或溢出返回 None，随后走 raw fallback，而不是截断。

- [ ] **Step 4: 为未知事件增加 bounded raw fallback**

增加：

```rust
const MAX_RAW_EVENT_CHARS: usize = 32_000;

fn raw_event(message: Value) -> EngineEvent {
    let kind = message
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_owned();
    EngineEvent::RawEngineEvent {
        kind,
        payload_json: summarize_to_limit(
            &serde_json::to_string(&message).unwrap_or_else(|_| "null".into()),
            MAX_RAW_EVENT_CHARS,
        ),
    }
}
```

把现有 match 提取为允许使用 `?` 的 `translate_known`，outer function 统一 fallback，避免已知但数据损坏的事件因为 `?` 提前丢失：

```rust
impl RpcEventTranslator {
    pub fn translate(&mut self, message: Value) -> Option<EngineEvent> {
        let semantic = self.translate_known(&message);
        Some(semantic.unwrap_or_else(|| raw_event(message)))
    }
}
```

新增 private `translate_known(&mut self, message: &Value) -> Option<EngineEvent>`，把当前 `translate` 的完整 match body 移入其中并加入本任务的 thought/progress/usage 分支。每个分支从 `&Value` clone 所需字符串/JSON；最终 `_ => None` 只表示“交给 outer raw fallback”，不是静默丢弃。

不要把 API key、`PI_CODING_AGENT_DIR/models.json` 或进程环境加入 raw payload。这里只保存 Pi stdout 已输出的单条 JSON，最多 32,000 Unicode chars。

- [ ] **Step 5: 更新旧测试对 unknown-event 行为的期望**

任何原本断言 unknown 为 `None` 的测试改为断言 `RawEngineEvent`。空 text/thought delta 可以继续返回 raw，不进入主 Feed。

- [ ] **Step 6: 运行 Pi adapter 完整测试**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test pi_engine -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml engine::pi -- --nocapture
```

Expected: PASS。

- [ ] **Step 7: 提交 Pi activity translation**

```powershell
git add -- src-tauri/src/engine/pi/mod.rs src-tauri/tests/pi_engine.rs
git commit -m "feat: translate rich Pi activity events"
```

---

### Task 6: 生成并锁定 TypeScript Activity Protocol bindings

**Files:**
- Modify: `src-tauri/src/domain/mod.rs`
- Modify: `src/bindings/index.ts`
- Regenerate: `src/bindings/WorkEventEnvelope.ts`
- Regenerate: `src/bindings/WorkEventPayload.ts`
- Create: `src/bindings/PermissionOutcome.ts`
- Create: `src/bindings/SessionTransition.ts`
- Create: `src/bindings/LivenessState.ts`

- [ ] **Step 1: 扩展 binding export 测试**

在 `domain/mod.rs` 的 `export_bindings` 测试中导出并检查三个新 enum，并增加：

```rust
let envelope = std::fs::read_to_string(output_dir.join("WorkEventEnvelope.ts")).unwrap();
assert!(envelope.contains("eventId?: string"));
assert!(envelope.contains("turnId?: string"));
assert!(envelope.contains("sessionId?: string"));
assert!(envelope.contains("correlationId?: string"));

let payload = std::fs::read_to_string(output_dir.join("WorkEventPayload.ts")).unwrap();
for discriminator in [
    "thoughtDelta",
    "planChanged",
    "toolPending",
    "toolProgress",
    "permissionRequested",
    "permissionResolved",
    "waiting",
    "liveness",
    "sessionChanged",
    "usageUpdated",
    "rawEngineEvent",
] {
    assert!(payload.contains(discriminator), "missing {discriminator}");
}
```

- [ ] **Step 2: 运行 binding test 并确认失败**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml export_bindings -- --nocapture
```

Expected: FAIL，直到 export list 与生成文件同步。

- [ ] **Step 3: 注册新 Rust 类型并生成 bindings**

在 `domain/mod.rs` import/export/type-name list 中加入：

```rust
PermissionOutcome::export().unwrap();
SessionTransition::export().unwrap();
LivenessState::export().unwrap();
```

运行同一个测试让 ts-rs 写文件，然后在 `src/bindings/index.ts` 增加三个 `export type`。不要手工维护 `WorkEventPayload` union。

- [ ] **Step 4: 运行 Rust binding test 与 TypeScript typecheck**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml export_bindings -- --nocapture
pnpm typecheck
```

Expected: 两条命令 PASS。若现有 fixture 因 Option 被生成为必填字段而失败，修正 Rust `#[ts(optional)]`，不要在几十个 fixture 中填假 ID。

- [ ] **Step 5: 提交 generated contract**

```powershell
git add -- src-tauri/src/domain/mod.rs src/bindings/index.ts src/bindings/WorkEventEnvelope.ts src/bindings/WorkEventPayload.ts src/bindings/PermissionOutcome.ts src/bindings/SessionTransition.ts src/bindings/LivenessState.ts
git commit -m "feat: export activity protocol bindings"
```

---

### Task 7: 移植 Buzz Activity 类型与纯 Projector

**Files:**
- Create: `src/features/activity/activityTypes.ts`
- Create: `src/features/activity/activityProjector.ts`
- Create: `src/features/activity/activityProjector.test.ts`
- Create: `THIRD_PARTY_NOTICES.md`
- Create: `docs/architecture/buzz-upstream-map.md`

- [ ] **Step 1: 先写上游派生的 Projector 回归测试**

`activityProjector.test.ts` 使用一个统一 helper：

```ts
const event = (
  sequence: number,
  payload: WorkEventPayload,
  identity: Partial<WorkEventEnvelope> = {},
): WorkEventEnvelope => ({
  version: 2,
  eventId: `event-${sequence}`,
  workId: "work-1",
  runId: "run-1",
  turnId: "turn-1",
  sessionId: "session-1",
  correlationId: "run-1",
  sequence,
  occurredAt: `2026-08-09T00:00:${String(sequence).padStart(2, "0")}.000Z`,
  payload,
  ...identity,
});
```

至少加入以下四个测试，名称直接保留其上游不变量：

```ts
it("appends assistant and thought deltas into one item per turn", () => {
  const items = projectActivity([
    event(1, { type: "assistantDelta", text: "A" }),
    event(2, { type: "assistantDelta", text: "B" }),
    event(3, { type: "thoughtDelta", text: "X" }),
    event(4, { type: "thoughtDelta", text: "Y" }),
  ]);
  expect(items.filter((item) => item.type === "message")).toMatchObject([{ text: "AB" }]);
  expect(items.filter((item) => item.type === "thought")).toMatchObject([{ text: "XY" }]);
});

it("merges tool pending start progress and finish into one monotonic row", () => {
  const items = projectActivity([
    event(1, { type: "toolPending", toolCallId: "t1", toolName: "bash", inputSummary: "pnpm test" }),
    event(2, { type: "toolStarted", toolCallId: "t1", toolName: "bash", inputSummary: "pnpm test" }),
    event(3, { type: "toolProgress", toolCallId: "t1", toolName: "bash", outputSummary: "12/20" }),
    event(4, { type: "toolFinished", toolCallId: "t1", toolName: "bash", outputSummary: "20/20", success: true }),
    event(5, { type: "toolProgress", toolCallId: "t1", toolName: "bash", outputSummary: "late" }),
  ]);
  expect(items.filter((item) => item.type === "tool")).toMatchObject([
    { id: "tool:run-1:t1", status: "completed", result: "20/20", isError: false },
  ]);
});

it("replaces a plan only when its revision is newer", () => {
  const items = projectActivity([
    event(1, { type: "planChanged", planId: "main", revision: 2, text: "new" }),
    event(2, { type: "planChanged", planId: "main", revision: 1, text: "stale" }),
  ]);
  expect(items.filter((item) => item.type === "plan")).toMatchObject([{ text: "new", revision: 2 }]);
});

it("correlates permission resolution with its request", () => {
  const items = projectActivity([
    event(1, { type: "permissionRequested", requestId: "p1", toolCallId: "t1", title: "Write", detail: "src/app.tsx" }),
    event(2, { type: "permissionResolved", requestId: "p1", outcome: "denied" }),
  ]);
  expect(items.filter((item) => item.type === "permission")).toMatchObject([
    { id: "permission:run-1:p1", status: "resolved", outcome: "denied" },
  ]);
});
```

- [ ] **Step 2: 运行测试并确认模块不存在**

Run:

```powershell
pnpm exec vitest run src/features/activity/activityProjector.test.ts
```

Expected: FAIL，无法 import 新模块。

- [ ] **Step 3: 移植并收敛 Activity 类型**

`activityTypes.ts` 文件头：

```ts
/*
 * Adapted from block/buzz at 5bf78671f45178f8de02ba18d3d321cbbf19cd1f,
 * Apache-2.0. Original: desktop/src/features/agents/ui/agentSessionTypes.ts.
 * PiWork changes: removes Relay/Nostr/ACP identities and adds Work/Run activity.
 */
```

定义：

```ts
export type ActivityRenderClass =
  | "message" | "file-edit" | "file-read" | "shell" | "status"
  | "thought" | "plan" | "permission" | "error" | "generic"
  | "raw-rail" | "suppressed";
export type ActivityTone = "read" | "write" | "admin" | "neutral";
export type ActivityAction = "read" | "write" | "execute" | "invoke";
export type ToolStatus = "pending" | "executing" | "completed" | "failed";
export type ActivityIdentity = {
  workId: string;
  runId: string;
  turnId: string | null;
  sessionId: string | null;
  agentId: string | null;
  assignmentId: string | null;
};
export type ActivityDescriptor = {
  renderClass: ActivityRenderClass;
  action: ActivityAction;
  object: string | null;
  preview: string | null;
  tone: ActivityTone;
  groupKey: string | null;
};

type ActivityBase = ActivityIdentity & {
  id: string;
  timestamp: string;
};

export type ActivityItem = ActivityBase & (
  | { type: "message"; renderClass: "message"; text: string }
  | { type: "thought"; renderClass: "thought"; text: string }
  | { type: "plan"; renderClass: "plan"; planId: string; revision: number; text: string }
  | {
      type: "tool";
      renderClass: ActivityRenderClass;
      toolCallId: string;
      toolName: string;
      status: ToolStatus;
      input: string;
      result: string;
      isError: boolean;
      descriptor: ActivityDescriptor;
    }
  | {
      type: "permission";
      renderClass: "permission";
      requestId: string;
      toolCallId: string | null;
      title: string;
      detail: string;
      status: "requested" | "resolved";
      outcome: PermissionOutcome | null;
    }
  | {
      type: "lifecycle";
      renderClass: "status" | "error" | "suppressed";
      activityKind:
        | "runStarted" | "waiting" | "liveness" | "sessionChanged"
        | "artifactProduced" | "validationProduced" | "runCompleted" | "runFailed";
      detail: string | null;
    }
  | {
      type: "usage";
      renderClass: "suppressed";
      inputTokens: number;
      outputTokens: number;
      cacheReadTokens: number;
      cacheWriteTokens: number;
      totalTokens: number;
    }
  | { type: "raw"; renderClass: "raw-rail" | "suppressed"; kind: string; payloadJson: string }
);
```

从 `../../bindings` 导入 `PermissionOutcome`。不要把 React icon 放进这些纯数据类型；icon 由组件根据 `renderClass` 选择。

- [ ] **Step 4: 移植 copy-on-write Projector**

`activityProjector.ts` 文件头引用 Buzz `agentSessionTranscript.ts`。公开 API 固定为：

```ts
export type ActivityProjection = {
  items: ActivityItem[];
  indexById: Map<string, number>;
};

export const createEmptyActivityProjection = (): ActivityProjection => ({
  items: [],
  indexById: new Map(),
});

export function processActivityEvent(
  state: ActivityProjection,
  event: WorkEventEnvelope,
): ActivityProjection;

export function projectActivity(events: WorkEventEnvelope[]): ActivityItem[] {
  return [...events]
    .sort((left, right) => left.sequence - right.sequence)
    .reduce(processActivityEvent, createEmptyActivityProjection())
    .items;
}
```

从 Buzz 保留 `draftFrom`、`ensureMutable`、`replaceItem`、`pushItem` 和 tool terminal merge 思路。ID 规则固定：

```text
message:<runId>:<turnId-or-runId>
thought:<runId>:<turnId-or-runId>
plan:<runId>:<planId>
tool:<runId>:<toolCallId>
permission:<runId>:<requestId>
lifecycle:<runId>:<payload-type>:<sequence>
usage:<runId>:<turnId-or-runId>
raw:<eventId-or-runId:sequence>
```

`RawEngineEvent` 和 `Liveness { alive }` 投影为 `raw/suppressed`，仍保留在 items；`RunFailed` 投影为 error lifecycle；`ArtifactProduced`/`ValidationProduced` 投影为 status items；现有 `RunCompleted` 的 delivery data 继续由 WorkTimeline 单独呈现。

- [ ] **Step 5: 增加 Apache-2.0 归属文件**

`THIRD_PARTY_NOTICES.md` 至少包含：

```markdown
# Third-Party Notices

## Block Buzz

Parts of PiWork's activity observer and activity projection code are adapted
from Block's Buzz project at commit
`5bf78671f45178f8de02ba18d3d321cbbf19cd1f`.

Upstream: https://github.com/block/buzz
License: Apache License 2.0

Local modifications replace Buzz Relay, Nostr, Channel and ACP-specific
transport concepts with PiWork Work, Run and SQLite WorkEvent semantics.
```

`docs/architecture/buzz-upstream-map.md` 建立包含 commit、上游路径、本地路径、派生测试、差异和最近同步日期 `2026-08-09` 的表格；先记录 observer、types、transcript、grouping 四项。

- [ ] **Step 6: 运行 Projector tests 和 typecheck**

Run:

```powershell
pnpm exec vitest run src/features/activity/activityProjector.test.ts
pnpm typecheck
```

Expected: PASS。

- [ ] **Step 7: 提交 Projector 与归属**

```powershell
git add -- src/features/activity/activityTypes.ts src/features/activity/activityProjector.ts src/features/activity/activityProjector.test.ts THIRD_PARTY_NOTICES.md docs/architecture/buzz-upstream-map.md
git commit -m "feat: port Buzz activity projector"
```

---

### Task 8: 移植工具语义与 Session/Turn 分组

**Files:**
- Create: `src/features/activity/activityPresentation.ts`
- Create: `src/features/activity/activityPresentation.test.ts`
- Create: `src/features/activity/activityGrouping.ts`
- Create: `src/features/activity/activityGrouping.test.ts`
- Modify: `src/features/activity/activityProjector.ts`
- Modify: `src/features/workspace/executionProgress.ts`
- Modify: `src/features/workspace/executionProgress.test.ts`

- [ ] **Step 1: 写工具语义与分组失败测试**

Presentation 测试覆盖：

```ts
expect(describeTool("read", '{"path":"src/app.tsx"}')).toMatchObject({
  renderClass: "file-read", action: "read", object: "src/app.tsx", tone: "read",
});
expect(describeTool("edit", '{"path":"src/app.tsx"}')).toMatchObject({
  renderClass: "file-edit", action: "write", object: "src/app.tsx", tone: "write",
});
expect(describeTool("bash", '{"command":"pnpm test"}')).toMatchObject({
  renderClass: "shell", action: "execute", object: "pnpm test", tone: "admin",
});
```

Grouping 测试覆盖：

```ts
const tool = (
  id: string,
  sessionId: string,
  turnId: string,
  renderClass: ActivityRenderClass = "generic",
): Extract<ActivityItem, { type: "tool" }> => ({
  id: `tool:run-1:${id}`,
  type: "tool",
  workId: "work-1",
  runId: "run-1",
  turnId,
  sessionId,
  agentId: null,
  assignmentId: null,
  timestamp: "2026-08-09T00:00:00.000Z",
  renderClass,
  toolCallId: id,
  toolName: "read",
  status: "completed",
  input: "src/app.tsx",
  result: "done",
  isError: false,
  descriptor: {
    renderClass,
    action: "read",
    object: "src/app.tsx",
    preview: "done",
    tone: "read",
    groupKey: "read:read",
  },
});

it("keeps stable session and turn keys across prepended history", () => {
  const current = buildActivityDisplayGroups([tool("a", "s2", "t2")]);
  const withHistory = buildActivityDisplayGroups([
    tool("old", "s1", "t1"),
    tool("a", "s2", "t2"),
  ]);
  expect(withHistory.at(-1)?.key).toBe(current[0]?.key);
});

it("groups consecutive successful tools of the same render class", () => {
  const groups = buildActivityDisplayGroups([
    tool("read-1", "s1", "t1", "file-read"),
    tool("read-2", "s1", "t1", "file-read"),
  ]);
  expect(groups[0]?.blocks).toMatchObject([
    { kind: "toolBurst", renderClass: "file-read", count: 2 },
  ]);
});
```

- [ ] **Step 2: 运行测试并确认模块不存在**

Run:

```powershell
pnpm exec vitest run src/features/activity/activityPresentation.test.ts src/features/activity/activityGrouping.test.ts
```

Expected: FAIL。

- [ ] **Step 3: 移植 Buzz descriptor 思路**

`activityPresentation.ts` 从 Buzz render class/descriptor helper 派生，使用 JSON best-effort parsing：

```ts
const parseSummary = (summary: string): Record<string, unknown> => {
  try {
    const value: unknown = JSON.parse(summary);
    return value && typeof value === "object" && !Array.isArray(value)
      ? value as Record<string, unknown>
      : {};
  } catch {
    return {};
  }
};

const stringField = (
  fields: Record<string, unknown>,
  ...keys: string[]
): string | null => {
  for (const key of keys) {
    const value = fields[key];
    if (typeof value === "string" && value.trim()) return value.trim();
  }
  return null;
};

const descriptor = (
  renderClass: ActivityRenderClass,
  action: ActivityAction,
  object: string | null,
  tone: ActivityTone,
  groupKey: string,
): ActivityDescriptor => ({
  renderClass,
  action,
  object,
  preview: null,
  tone,
  groupKey,
});

export function describeTool(toolName: string, summary: string): ActivityDescriptor {
  const normalized = toolName.trim().toLocaleLowerCase();
  const fields = parseSummary(summary);
  const path = stringField(fields, "path", "file_path");
  const command = stringField(fields, "command");
  if (/^(read|grep|find|ls)(?:_|$)/u.test(normalized)) {
    return descriptor("file-read", "read", path ?? summary, "read", `read:${normalized}`);
  }
  if (/^(edit|write)(?:_|$)/u.test(normalized)) {
    return descriptor("file-edit", "write", path ?? summary, "write", `write:${normalized}`);
  }
  if (normalized === "bash") {
    return descriptor("shell", "execute", command ?? summary, "admin", "shell");
  }
  return descriptor("generic", "invoke", toolName, "neutral", `tool:${normalized}`);
}
```

Projector 创建/更新 tool item 时统一调用该函数；不在 React component 内重复分类。

- [ ] **Step 4: 移植精简的 Session/Turn grouping**

`activityGrouping.ts` 文件头引用 Buzz grouping 原文件。公开类型/API：

```ts
export type ActivityDisplayBlock =
  | { kind: "item"; key: string; item: ActivityItem }
  | { kind: "toolBurst"; key: string; renderClass: ActivityRenderClass; count: number; items: ActivityItem[] };

export type ActivityDisplayGroup = {
  key: string;
  sessionId: string | null;
  turnId: string | null;
  blocks: ActivityDisplayBlock[];
};

export function buildActivityDisplayGroups(items: ActivityItem[]): ActivityDisplayGroup[];
```

Group key 只能由 identity 生成：

```ts
const groupKey = (item: ActivityItem) =>
  `session:${item.sessionId ?? "legacy"}:turn:${item.turnId ?? item.runId}`;
```

只有连续、同 group、同 `descriptor.groupKey`、非 error 且数量至少 2 的 tool 才形成 `toolBurst`。Permission、plan、thought、raw 和失败 tool 永不吞进 burst。

- [ ] **Step 5: 复用分类修正 ExecutionProgress**

`executionProgress.ts` 处理 `toolPending`、`toolStarted`、`toolProgress`、`toolFinished`，仍以 `toolCallId` 的 Map 计数。ToolProgress 只更新已知 tool 的阶段，不创建错误的第二个 tool；如果 progress 是第一条，则用 toolName/outputSummary 创建 executing record。更新测试加入 pending→progress→finish 只计 1 次。

- [ ] **Step 6: 运行 activity、progress 和 typecheck**

Run:

```powershell
pnpm exec vitest run src/features/activity/activityPresentation.test.ts src/features/activity/activityGrouping.test.ts src/features/workspace/executionProgress.test.ts
pnpm typecheck
```

Expected: PASS。

- [ ] **Step 7: 提交 presentation/grouping**

```powershell
git add -- src/features/activity/activityPresentation.ts src/features/activity/activityPresentation.test.ts src/features/activity/activityGrouping.ts src/features/activity/activityGrouping.test.ts src/features/activity/activityProjector.ts src/features/workspace/executionProgress.ts src/features/workspace/executionProgress.test.ts
git commit -m "feat: group semantic activity by turn"
```

---

### Task 9: 用 Buzz Projector 驱动单 Agent Activity Feed

**Files:**
- Create: `src/features/activity/ActivityFeed.tsx`
- Create: `src/features/activity/ActivityFeed.test.tsx`
- Modify: `src/features/workspace/WorkTimeline.tsx`
- Modify: `src/features/workspace/WorkTimeline.test.tsx`
- Modify: `src/i18n/locales/en.json`
- Modify: `src/i18n/locales/zh-CN.json`
- Modify: `src/styles/workspace.css`

- [ ] **Step 1: 写 ActivityFeed 失败测试**

覆盖以下产品行为：

```tsx
const event = (
  sequence: number,
  payload: WorkEventPayload,
): WorkEventEnvelope => ({
  version: 2,
  eventId: `event-${sequence}`,
  workId: "work-1",
  runId: "run-1",
  turnId: "turn-1",
  sessionId: "session-1",
  correlationId: "run-1",
  sequence,
  occurredAt: `2026-08-09T00:00:${String(sequence).padStart(2, "0")}.000Z`,
  payload,
});

it("shows one consolidated tool row and hides raw events from the primary feed", () => {
  render(<ActivityFeed items={projectActivity([
    event(1, { type: "toolStarted", toolCallId: "t1", toolName: "read", inputSummary: '{"path":"src/app.tsx"}' }),
    event(2, { type: "toolProgress", toolCallId: "t1", toolName: "read", outputSummary: "half" }),
    event(3, { type: "toolFinished", toolCallId: "t1", toolName: "read", outputSummary: "done", success: true }),
    event(4, { type: "rawEngineEvent", kind: "queue_update", payloadJson: "{}" }),
  ])} />);
  expect(screen.getAllByText("src/app.tsx")).toHaveLength(1);
  expect(screen.queryByText("queue_update")).not.toBeInTheDocument();
});

it("keeps thought and plan collapsed but makes permission requests prominent", async () => {
  const user = userEvent.setup();
  const items = projectActivity([
    event(1, { type: "thoughtDelta", text: "private reasoning detail" }),
    event(2, { type: "planChanged", planId: "main", revision: 1, text: "- verify" }),
    event(3, {
      type: "permissionRequested",
      requestId: "permission-1",
      toolCallId: "tool-1",
      title: "Write file",
      detail: "src/app.tsx",
    }),
  ]);
  render(<ActivityFeed items={items} />);
  expect(screen.getByText("需要权限")).toBeVisible();
  expect(screen.queryByText("private reasoning detail")).not.toBeVisible();
  await user.click(screen.getByRole("button", { name: "展开思考" }));
  expect(screen.getByText("private reasoning detail")).toBeVisible();
});
```

- [ ] **Step 2: 运行测试并确认组件不存在**

Run:

```powershell
pnpm exec vitest run src/features/activity/ActivityFeed.test.tsx
```

Expected: FAIL。

- [ ] **Step 3: 实现高信噪比 Feed**

`ActivityFeed` 接收 `ActivityItem[]`，先调用 `buildActivityDisplayGroups`。渲染规则：

- `message` 不在 ActivityFeed 渲染，由 WorkTimeline 作为助手正文渲染。
- `raw`、`suppressed` 和 `usage` 不在主 Feed 渲染。
- `thought`、`plan` 使用默认关闭的 `<details>`。
- `tool` 用 `activity.actions.<action>` 翻译 descriptor 的动作，再展示对象、最新 preview 和 status。
- `toolBurst` 展示“读取了 N 项”等摘要，展开后显示 children。
- `permission` 使用 `role="alert"`；resolved 后显示 outcome。
- `lifecycle/error` 只展示需要用户理解的 waiting、session transition、run failure；普通 liveness 隐藏。

不要在组件中读原始 `WorkEventPayload`；组件只依赖 `ActivityItem`。

- [ ] **Step 4: 替换 WorkTimeline 手写投影**

在 `ConversationSegment` 中：

```ts
const projected = projectActivity(events);
const assistantText = projected
  .filter((item): item is Extract<ActivityItem, { type: "message" }> => item.type === "message")
  .map((item) => item.text)
  .join("");
const activityItems = projected.filter((item) => item.type !== "message");
```

删除现有逐 event 的 `ActivityRow` 和手写 assistant delta concat。`ExecutionProgressCard` 仍接收原始 events 计算 phase，children 改为 `<ActivityFeed items={activityItems} />`。Delivery、user message、scroll follow 和 failure CTA 保持现状。

- [ ] **Step 5: 加入明确的中英文 Activity 文案**

在两份 locale 新增根级 `activity` 节点。中文内容：

```json
{
  "expandThought": "展开思考",
  "collapseThought": "收起思考",
  "expandPlan": "展开计划",
  "collapsePlan": "收起计划",
  "permissionRequired": "需要权限",
  "toolPending": "等待执行",
  "toolRunning": "正在执行",
  "toolCompleted": "已完成",
  "toolFailed": "失败",
  "toolBurst": "{{action}}了 {{count}} 项",
  "actions": {
    "read": "读取",
    "write": "修改",
    "execute": "执行",
    "invoke": "调用"
  }
}
```

英文内容使用完全相同的 key：

```json
{
  "expandThought": "Show reasoning",
  "collapseThought": "Hide reasoning",
  "expandPlan": "Show plan",
  "collapsePlan": "Hide plan",
  "permissionRequired": "Permission required",
  "toolPending": "Waiting to run",
  "toolRunning": "Running",
  "toolCompleted": "Completed",
  "toolFailed": "Failed",
  "toolBurst": "{{action}} {{count}} items",
  "actions": {
    "read": "Read",
    "write": "Changed",
    "execute": "Ran",
    "invoke": "Called"
  }
}
```

`locales.test.ts` 必须继续保证 key parity。

- [ ] **Step 6: 增加 Feed 样式但保持现有布局**

在 `workspace.css` 使用现有 tokens；状态差异不能只靠颜色，必须同时有 icon/text。Thought/plan 边框弱于 tool，permission/error 强于普通 status；不增加页面级布局或导航变化。

- [ ] **Step 7: 运行 Feed、Timeline、locale 测试**

Run:

```powershell
pnpm exec vitest run src/features/activity/ActivityFeed.test.tsx src/features/workspace/WorkTimeline.test.tsx src/i18n/locales/locales.test.ts
pnpm typecheck
```

Expected: PASS；现有自动滚动、delivery、failure 隐私和附件测试不回归。

- [ ] **Step 8: 提交 Activity Feed**

```powershell
git add -- src/features/activity/ActivityFeed.tsx src/features/activity/ActivityFeed.test.tsx src/features/workspace/WorkTimeline.tsx src/features/workspace/WorkTimeline.test.tsx src/i18n/locales/en.json src/i18n/locales/zh-CN.json src/styles/workspace.css
git commit -m "feat: render projected Work activity"
```

---

### Task 10: 让 Inspector Raw Rail 从同一 Journal 完整重建

**Files:**
- Create: `src/features/activity/RawActivityRail.tsx`
- Create: `src/features/activity/RawActivityRail.test.tsx`
- Modify: `src/features/workspace/WorkInspector.tsx`
- Create: `src/features/workspace/WorkInspector.test.tsx`
- Modify: `src/i18n/locales/en.json`
- Modify: `src/i18n/locales/zh-CN.json`
- Modify: `src/styles/workspace.css`

- [ ] **Step 1: 写 Raw Rail 失败测试**

```tsx
const event = (
  sequence: number,
  payload: WorkEventPayload,
  identity: Partial<WorkEventEnvelope> = {},
): WorkEventEnvelope => ({
  version: 2,
  eventId: `event-${sequence}`,
  workId: "work-1",
  runId: "run-1",
  turnId: "turn-1",
  sessionId: "session-1",
  correlationId: "run-1",
  sequence,
  occurredAt: `2026-08-09T00:00:${String(sequence).padStart(2, "0")}.000Z`,
  payload,
  ...identity,
});

it("renders every journal event in stable order including suppressed raw events", () => {
  render(<RawActivityRail events={[
    event(2, { type: "rawEngineEvent", kind: "queue_update", payloadJson: '{"size":1}' }),
    event(1, { type: "assistantDelta", text: "hello" }),
  ]} />);
  const rows = screen.getAllByTestId("raw-activity-event");
  expect(rows).toHaveLength(2);
  expect(rows[0]).toHaveTextContent("assistantDelta");
  expect(rows[1]).toHaveTextContent("queue_update");
  expect(rows[1]).toHaveTextContent('"size": 1');
});

it("shows correlation identity without inventing missing legacy values", () => {
  render(<RawActivityRail events={[event(1, { type: "runStarted", modelLabel: "Pi" }, {
    turnId: undefined, sessionId: undefined, correlationId: undefined,
  })]} />);
  expect(screen.getByText("legacy event")).toBeVisible();
  expect(screen.queryByText("session-unknown")).not.toBeInTheDocument();
});
```

- [ ] **Step 2: 运行测试并确认组件不存在**

Run:

```powershell
pnpm exec vitest run src/features/activity/RawActivityRail.test.tsx
```

Expected: FAIL。

- [ ] **Step 3: 实现 RawActivityRail**

组件用 `(occurredAt, runId, sequence, eventId)` 排序；不能只按 sequence，因为每个 Run 都从 1 重新计数。每行 `<details>` summary 显示 sequence、payload type、occurredAt，metadata 显示 event/work/run/turn/session/agent/assignment/causation/correlation。Raw payload 使用安全 parse：

```ts
const presentPayload = (event: WorkEventEnvelope) => {
  if (event.payload.type !== "rawEngineEvent") return event.payload;
  try {
    return { ...event.payload, payload: JSON.parse(event.payload.payloadJson) };
  } catch {
    return event.payload;
  }
};
```

渲染 `<pre>{JSON.stringify(presentPayload(event), null, 2)}</pre>`。不要用 `dangerouslySetInnerHTML`。旧事件缺 identity 时只显示 `legacy event` badge，不伪造 UUID。

- [ ] **Step 4: 集成 WorkInspector logs tab**

保持 tab id `logs`，避免打断现有键盘/持久 UI 状态；显示标签改为“活动原始记录 / Raw activity”。删除现有会合并 assistant delta 的 `logEvents` 分支，直接：

```tsx
return (
  <div className="inspector-logs">
    {error ? (
      <pre className="diagnostics">
        {formatAppErrorDiagnostics(error, t("diagnostics.unavailable"))}
      </pre>
    ) : null}
    <RawActivityRail events={events} />
  </div>
);
```

`WorkInspector.test.tsx` 验证点击 logs tab 后 raw event 可见、所有事件数量不因 Projector suppression 减少、ArrowLeft/ArrowRight tab keyboard behavior 保持。

- [ ] **Step 5: 运行 Inspector 与 Raw Rail 测试**

Run:

```powershell
pnpm exec vitest run src/features/activity/RawActivityRail.test.tsx src/features/workspace/WorkInspector.test.tsx src/features/workspace/WorkSurface.test.tsx
pnpm typecheck
```

Expected: PASS。

- [ ] **Step 6: 提交 Raw Rail**

```powershell
git add -- src/features/activity/RawActivityRail.tsx src/features/activity/RawActivityRail.test.tsx src/features/workspace/WorkInspector.tsx src/features/workspace/WorkInspector.test.tsx src/i18n/locales/en.json src/i18n/locales/zh-CN.json src/styles/workspace.css
git commit -m "feat: add Work activity raw rail"
```

---

### Task 11: 更新实时去重、fixtures 与恢复一致性

**Files:**
- Modify: `src/domain/work.ts`
- Modify: `src/features/works/workStore.ts`
- Modify: `src/features/works/workStore.test.ts`
- Modify: `src/features/works/useWorkEvents.test.tsx`
- Modify: `src/test/mockTauriClient.ts`
- Modify: any TypeScript test fixture reported by `pnpm typecheck`

- [ ] **Step 1: 写 eventId 优先、legacy fallback 的失败测试**

在 `src/domain/work.test.ts` 或 `workStore.test.ts` 增加：

```ts
expect(timelineItemKey(v2Event({ eventId: "event-1", runId: "run-1", sequence: 1 })))
  .toBe("event:event-1");
expect(timelineItemKey(v1Event({ runId: "run-1", sequence: 1 })))
  .toBe("event:run-1:1");
```

再增加 store 测试：同一 `eventId` 通过 live channel 和 detail hydration 各到达一次，timeline 只保留一项；两个不同 eventId 即使 timestamp 相同也都保留。

- [ ] **Step 2: 运行测试并确认旧 key 行为失败**

Run:

```powershell
pnpm exec vitest run src/domain/work.test.ts src/features/works/workStore.test.ts
```

Expected: FAIL。

- [ ] **Step 3: 更新 timeline identity 与 fixtures**

`timelineItemKey` 改为：

```ts
export const timelineItemKey = (item: TimelineItem) =>
  isWorkEventTimelineItem(item)
    ? `event:${item.eventId ?? `${item.runId}:${item.sequence}`}`
    : `message:${item.id}`;
```

Store 继续使用 `timelineItemKey` merge，不另建第二套 eventId Map。`lastSequenceByRun` 仍负责拒绝同 Run 的旧实时帧；detail hydration 依赖 identity merge 恢复漏帧。

`mockTauriClient.event()` 默认生成 `eventId: event-${runId}-${sequence}`、`turnId: runId`、`correlationId: runId`；针对 v1 的测试显式删除这些字段。

- [ ] **Step 4: 运行 store、subscription、typecheck**

Run:

```powershell
pnpm exec vitest run src/domain/work.test.ts src/features/works/workStore.test.ts src/features/works/useWorkEvents.test.tsx
pnpm typecheck
```

Expected: PASS。

- [ ] **Step 5: 提交实时/恢复一致性**

```powershell
git add -- src/domain/work.ts src/domain/work.test.ts src/features/works/workStore.ts src/features/works/workStore.test.ts src/features/works/useWorkEvents.test.tsx src/test/mockTauriClient.ts
git commit -m "fix: deduplicate persisted and live activity"
```

---

### Task 12: 完整验证、视觉验收与上游差异审计

**Files:**
- Modify: `docs/architecture/buzz-upstream-map.md`
- Inspect without broad edits: all implementation files listed in Tasks 1-11

- [ ] **Step 1: 运行 Rust formatting 与全部测试**

Run:

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
pnpm cargo:test
```

Expected: 全部 exit code 0；不运行标记为外部凭据依赖的 `pi_live` ignored test。

- [ ] **Step 2: 运行前端完整验证**

Run:

```powershell
pnpm test
pnpm typecheck
pnpm build
```

Expected: Vitest 0 failures、TypeScript exit 0、Vite production build exit 0。

- [ ] **Step 3: 验证 journal-before-publish 与 restart reconstruction**

增加或确认一条 Supervisor/Repository 集成测试：Publisher receiver 收到事件时，`events_for_run` 已经能读取同一个 `eventId`。再用 persisted events 调用前端 `projectActivity` fixture，输出必须与 live 顺序输入一致。

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml journal -- --nocapture
pnpm exec vitest run src/features/activity src/features/works/workStore.test.ts
```

Expected: PASS。

- [ ] **Step 4: 在真实 Tauri 窗口做单 Run 视觉验收**

Run:

```powershell
pnpm tauri dev
```

使用一个只读 Work 提示词触发 thought、read、bash test 和最终回答，确认：

1. 主时间线的同一个 tool 只有一行，progress 更新不追加重复行。
2. Thought/plan 默认折叠。
3. Raw activity tab 含所有 Journal event 和关联字段。
4. 关闭并重新打开 Work 后，Feed 与 Raw Rail 由 SQLite 恢复且内容一致。
5. 未知 Pi 事件只进入 Raw Rail，不污染主时间线。
6. 主时间线不泄露 RunFailed 原始内部错误；现有 diagnostics 入口仍可用。

结束 `tauri dev` 后再继续，不提交运行产生的本地数据库、截图或 runtime 文件。

- [ ] **Step 5: 审计 Buzz 派生范围和文档**

逐项对照 `docs/architecture/buzz-upstream-map.md`：

- 每个派生文件头都有 commit、original path 和 modifications。
- Projector 测试能映射到 Buzz 的 tool merge、delta upsert、plan、permission 用例。
- Grouping 测试能映射到 Buzz 的 session boundary stable key 和 tool burst 用例。
- 没有复制 Relay、Nostr、Community、Channel 或 ACP transport 代码。
- `THIRD_PARTY_NOTICES.md` 包含 Apache-2.0 来源。

把实际本地测试文件和最终差异更新进 map，不写未实现能力。

- [ ] **Step 6: 检查 diff、工作树和意外文件**

Run:

```powershell
git diff --check
git status --short
git diff --stat HEAD~11..HEAD
```

Expected: 没有 whitespace error；`research_buzz_piwork_20260809.md` 仍可保持未跟踪且不进入任何 commit；没有 `target/`、数据库、runtime 或构建产物。

- [ ] **Step 7: 提交最终审计修正**

只有当 Step 1-6 需要更新上游映射时，提交该文档：

```powershell
git add -- docs/architecture/buzz-upstream-map.md
git commit -m "docs: audit Buzz activity port"
```

若发现代码问题，返回拥有该文件的 Task，使用该 Task 已列出的明确路径提交并重新运行验证；若上游映射没有变化，不创建空 commit。

## 子项目 A 完成定义

只有以下条件全部满足，才能开始为子项目 B 编写独立实施计划：

- 新事件使用 Activity Protocol v2；旧 v1 Event 可读取。
- thought、plan、tool pending/start/progress/finish、permission、waiting、liveness、session transition、artifact、validation、usage 和 raw event 都能被协议表达。
- Pi 实际输出的 thought、tool progress、usage 被翻译；不支持的 plan/permission 不被伪造。
- 所有新 Event 有稳定 event ID、Turn/Session/causation/correlation；Agent/Assignment 字段为可选扩展位。
- SQLite 是唯一事实源，journal 成功发生在 Observer/Tauri publish 之前。
- Buzz 派生 Projector 将 delta 和工具生命周期聚合为稳定 ActivityItem。
- 主 Timeline 使用投影后的高信噪比 Feed；Inspector Raw Rail 展示同一 Journal 的全部事件。
- live + hydration 不重复，restart 后投影一致。
- Buzz commit、来源、测试映射和本地差异均有记录。
- Rust tests、Clippy、frontend tests、typecheck 和 build 全部通过，真实 Tauri 窗口完成视觉验收。
