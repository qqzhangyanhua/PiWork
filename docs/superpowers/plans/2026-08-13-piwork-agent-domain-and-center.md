# PiWork Agent Domain 与智能体中心 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现版本化的角色模板、Agent Definition/Instance、能力包、Work 成员与唯一主理人，并把现有智能体中心升级为“我的团队 + 能力库”，同时保持现有单 Agent Work 执行链路可运行。

**Architecture:** 新建独立 `agent` 领域模块与 SQLite Repository；migration 原子播种四个内置角色/Definition/Instance、四个内置可执行基础能力包，把 96 项现有业务目录能力登记为 `catalog_only`，并给全部现有 Work 回填唯一主理人。`AgentService` 负责装配校验、内置定义只读、本地副本和 Work membership；React 通过新增 typed commands 读取真实团队数据。此子项目不创建 Assignment、Scheduler、Agent Session 或 Memory；这些分别由后续 C、D 计划实现。

**Tech Stack:** Rust 2024、SQLx/SQLite、Serde、ts-rs、Tauri 2、React 19、TypeScript、Zustand、Vitest、Testing Library。

**Design source:** `docs/superpowers/specs/2026-08-09-piwork-agent-assembly-design.md` §§5.3–6.7、11、13、15、18、19.B、20。

**Prerequisite:** 子项目 A（`docs/superpowers/plans/2026-08-09-piwork-activity-protocol.md`）已经完成。执行本计划时不要实现子项目 C/D。

**Plan index:** `docs/superpowers/plans/2026-08-13-piwork-agent-assembly-plan-index.md`。本文件只拥有 B；C/D 的交接合同与 §20 覆盖在索引中审计。

---

## 范围、不变量与明确延期

本计划完成后：

1. 每个 Work 恰好有一个 `WorkLead`，且 Lead 同时存在于 `work_agents`。
2. 内置 `lead / researcher / engineer / reviewer` RoleTemplate、Definition、Instance 是稳定、版本化、幂等播种的数据。
3. 内置 Definition/Instance 不可原地编辑；“自定义副本”创建新的本地 Definition/Instance。
4. 四个内置 Definition 分别绑定一个系统基础能力包：`lead_coordination / source_research / engineering_execution / independent_review`；它们是预置成员运行合同，不冒充 96 项业务目录能力。
5. 现有 96 项能力均登记为 `catalog_only`，不虚报为可执行 Agent 或可装载能力包。
6. 装配校验是服务端权威：角色兼容、工具、权限、Engine capability、冲突、上下文预算任一失败都阻止保存。
7. 研究员和审阅者默认只读；工程师继承 Work permission mode；主理身份不会扩大文件权限。
8. `WorkDetail` 不膨胀为 Agent 聚合 DTO；团队和目录通过独立查询加载。
9. 现有 `start_work`、单 `EngineSupervisor`、Pi `sessions_root/<work_id>` 行为保持不变。

明确延期到后续计划：

- C：`agent_sessions`、`assignments`、`assignment_dependencies`、queue/scheduler、EngineCapabilities、Agent × Work session 隔离、queue/steer/interrupt。
- D：主理宿主工具、Result Envelope、Context Builder、Work Ledger、Agent/Work Memory、完整委派闭环。
- E：非 Pi Engine、任意用户创作 executable pack、同 Work 只读并行、RemoteRunner。

---

## 文件职责图

### Rust / SQLite

- Create `src-tauri/migrations/0005_agent_domain.sql`：领域表、约束、索引、四个内置实体、96 项目录能力、legacy Work Lead 回填。
- Create `src-tauri/src/domain/agent.rs`：wire-safe Agent/Capability/Work membership DTO、输入和校验诊断。
- Modify `src-tauri/src/domain/mod.rs`：导出 agent 类型与 TypeScript bindings。
- Create `src-tauri/src/agent/mod.rs`：模块边界。
- Create `src-tauri/src/agent/repository.rs`：SQLx 查询、复制 Definition/Instance、membership 与 Lead 事务。
- Create `src-tauri/src/agent/assembly.rs`：纯装配解析和 fail-closed 校验。
- Create `src-tauri/src/agent/service.rs`：业务授权、内置只读、本地副本和查询组合。
- Create `src-tauri/src/agent/commands.rs`：Tauri command 边界。
- Modify `src-tauri/src/app_state.rs`：持有 `AgentService`。
- Modify `src-tauri/src/lib.rs`：启动组装、命令注册、模块导出。
- Modify `src-tauri/src/work/repository.rs`：创建 Work 的同一事务内绑定主理人。
- Modify `src-tauri/tests/storage_contract.rs`：migration 结构、约束、seed 与回填合同。
- Create `src-tauri/tests/agent_domain.rs`：Repository、Service、复制、校验与 membership 集成测试。

### React / TypeScript

- Generate `src/bindings/Agent*.ts`、`RoleTemplateSummary.ts`、`CapabilityPackSummary.ts`、`WorkTeamSummary.ts` 等 DTO；更新 `src/bindings/index.ts`。
- Modify `src/app/tauriClient.ts`：Agent commands 的 typed client。
- Modify `src/test/mockTauriClient.ts`：稳定默认 mock。
- Modify `src/features/agent-center/agentCapabilities.ts`：保留 96 项展示内容，增加稳定 catalog id 与服务端状态合并 helper。
- Create `src/features/agent-center/agentCenterModel.ts`：团队/能力库纯 view model。
- Create `src/features/agent-center/TeamMemberCard.tsx`：成员卡。
- Create `src/features/agent-center/MemberDetailDrawer.tsx`：职责、边界、能力、Engine/Model、权限、记忆策略（B 阶段固定显示 `confirmed_only`，并说明 D 上线后才产生长期记忆）、版本状态。
- Create `src/features/agent-center/MemberAssembler.tsx`：复制内置成员、安装/卸载 executable pack、显示校验诊断、保存。
- Modify `src/features/agent-center/AgentCenterPage.tsx`：一级视图改为“我的团队 / 能力库”。
- Modify `src/features/agent-center/CapabilityCard.tsx` 与 `CapabilityDetailDrawer.tsx`：显式区分 `catalog_only / executable / deprecated`。
- Modify `src/features/workspace/WorkSurface.tsx`：传入 client 和当前 Work；目录能力继续生成 Prompt，executable pack 才走成员装配入口。
- Modify `src/styles/agent-center.css`、`src/i18n/locales/en.json`、`src/i18n/locales/zh-CN.json`。
- Modify existing Agent Center tests；create focused assembler/model tests。

---

### Task 1: 冻结 Agent Domain 的 wire contract

**Files:**
- Create: `src-tauri/src/domain/agent.rs`
- Modify: `src-tauri/src/domain/mod.rs`
- Test: `src-tauri/src/domain/agent.rs`
- Generate: `src/bindings/RoleTemplateSummary.ts`
- Generate: `src/bindings/AgentDefinitionSummary.ts`
- Generate: `src/bindings/AgentInstanceSummary.ts`
- Generate: `src/bindings/CapabilityPackSummary.ts`
- Generate: `src/bindings/WorkAgentSummary.ts`
- Generate: `src/bindings/WorkTeamSummary.ts`
- Generate: `src/bindings/AssemblyDiagnostic.ts`
- Generate: `src/bindings/SaveAgentAssemblyInput.ts`
- Modify: `src/bindings/index.ts`

- [ ] **Step 1: 写失败的领域序列化测试**

在 `domain/agent.rs` 先写测试，锁定这些枚举：

```rust
#[test]
fn agent_wire_enums_are_stable() {
    assert_eq!(serde_json::to_value(RoleKind::Lead).unwrap(), "lead");
    assert_eq!(serde_json::to_value(AgentStatus::Active).unwrap(), "active");
    assert_eq!(serde_json::to_value(CapabilityPackStatus::CatalogOnly).unwrap(), "catalog_only");
    assert_eq!(serde_json::to_value(WorkAgentStatus::Joined).unwrap(), "joined");
    assert_eq!(serde_json::to_value(PermissionPolicy::ReadOnly).unwrap(), "read_only");
}
```

再锁定 DTO 使用 camelCase，并且所有 ID 均为 `String`：

```rust
#[test]
fn work_team_summary_serializes_one_lead_and_members() {
    let value = serde_json::to_value(team_fixture()).unwrap();
    assert_eq!(value["workId"], "work-1");
    assert_eq!(value["lead"]["instance"]["definition"]["roleKind"], "lead");
    assert_eq!(value["members"].as_array().unwrap().len(), 1);
}
```

- [ ] **Step 2: 运行测试确认因模块不存在而失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml domain::agent -- --nocapture`

Expected: FAIL，提示 `domain::agent` 或类型尚未定义。

- [ ] **Step 3: 实现最小领域类型**

定义：

```rust
pub enum RoleKind { Lead, Researcher, Engineer, Reviewer }
pub enum AgentStatus { Active, Inactive }
pub enum CapabilityPackStatus { CatalogOnly, Executable, Deprecated }
pub enum WorkAgentStatus { Joined, Inactive }
pub enum PermissionPolicy { InheritWork, ReadOnly, WorkWrite }
pub enum MemoryPolicy { ConfirmedOnly }
pub enum AssemblyDiagnosticCode {
    NotExecutable,
    IncompatibleRole,
    MissingTool,
    MissingEngineCapability,
    PermissionEscalation,
    CapabilityConflict,
    ContextBudgetExceeded,
}
```

定义以下 DTO 并统一使用 `#[serde(rename_all = "camelCase")]`、`#[ts(rename_all = "camelCase")]`。字段合同必须完整采用：

```text
RoleTemplateSummary:
  id, slug, role_kind, name, description, base_instructions,
  responsibilities, non_responsibilities, base_result_contract,
  compatible_capability_kinds, builtin, version, created_at, updated_at

AgentDefinitionSummary:
  id, role_template_id, role_kind, slug, name, description, instructions,
  responsibilities, non_responsibilities, input_contract, result_contract,
  quality_rubric, default_engine_kind, default_model_configuration_id,
  default_permission_policy, default_parallelism, memory_policy,
  capability_packs, builtin, active, version, created_at, updated_at

AgentInstanceSummary:
  id, definition, display_name, engine_override,
  model_configuration_override, permission_policy_override,
  parallelism_override, builtin, status, created_at, updated_at

CapabilityPackSummary:
  id, catalog_capability_id, name, description, instructions,
  input_schema, output_schema, procedure, validation_rubric,
  required_tools, default_permission_scope, compatible_role_template_ids,
  required_engine_capabilities, conflicts_with_capability_pack_ids,
  version, status

WorkAgentSummary:
  work_id, instance, role_kind, status, permission_policy,
  joined_at, updated_at

WorkTeamSummary:
  work_id, lead, members

AssemblyDiagnostic:
  code, capability_pack_id, message

SaveAgentAssemblyInput:
  source_instance_id, display_name, capability_pack_ids,
  engine_override, model_configuration_override,
  permission_policy_override, parallelism_override
```

其中 `responsibilities`、`non_responsibilities`、`compatible_capability_kinds`、`required_tools`、`compatible_role_template_ids`、`required_engine_capabilities`、`conflicts_with_capability_pack_ids`、`capability_pack_ids` 是 `Vec<String>`；schema/contract/rubric 是 `serde_json::Value`；可空 override/catalog/model 字段是 `Option<T>`；时间是 `DateTime<Utc>`。

JSON 数组/对象字段在 Rust 中使用明确类型，数据库层才编码成 JSON；不要在 domain DTO 暴露裸 JSON 字符串。

- [ ] **Step 4: 导出 bindings 并锁定生成文件**

在 `domain/mod.rs::export_bindings` 添加新类型，运行：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml domain::tests::export_bindings -- --nocapture
pnpm typecheck
```

Expected: PASS，`src/bindings` 中生成类型，并由 `index.ts` 导出。

- [ ] **Step 5: 提交领域合同**

```powershell
git add src-tauri/src/domain/agent.rs src-tauri/src/domain/mod.rs src/bindings
git commit -m "feat: define agent assembly domain contracts"
```

---

### Task 2: 创建 Agent Domain schema、内置成员与 legacy 回填

**Files:**
- Create: `src-tauri/migrations/0005_agent_domain.sql`
- Modify: `src-tauri/tests/storage_contract.rs`

- [ ] **Step 1: 写 migration 失败测试**

新增测试断言表和约束：

```rust
#[tokio::test]
async fn agent_domain_migration_seeds_builtin_team_and_backfills_leads() {
    let database = Database::open_in_memory().await.unwrap();
    let names = database.table_names().await.unwrap();
    for table in [
        "role_templates", "agent_definitions", "agent_instances",
        "capability_packs", "agent_capability_bindings",
        "work_agents", "work_leads",
    ] {
        assert!(names.contains(&table.to_owned()), "missing {table}");
    }

    let roles: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM role_templates WHERE builtin = 1")
        .fetch_one(database.pool()).await.unwrap();
    let instances: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_instances WHERE builtin = 1")
        .fetch_one(database.pool()).await.unwrap();
    let catalog: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM capability_packs WHERE status = 'catalog_only'")
        .fetch_one(database.pool()).await.unwrap();
    let executable: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM capability_packs WHERE status = 'executable' AND catalog_capability_id IS NULL")
        .fetch_one(database.pool()).await.unwrap();
    assert_eq!((roles, instances, catalog, executable), (4, 4, 96, 4));
}
```

增加迁移文件 LF 检查、每个 Work 最多一个 Lead 的 UNIQUE 检查、Lead 必须同时是 Work member 的外键检查、内置 slug/version 唯一性检查。另写一个真实 legacy upgrade 测试：在单一 SQLite connection 上只应用 0001–0004，插入旧 Work，再应用 0005 并断言 Lead membership 与 `work_leads` 回填；`Database::open_in_memory()` 直接跑完全部 migration，不能单独证明回填。

- [ ] **Step 2: 运行测试确认缺表失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test storage_contract agent_domain -- --nocapture`

Expected: FAIL，缺少 `role_templates`。

- [ ] **Step 3: 写完整 migration**

表和关键约束采用以下完整列合同；所有 ID/时间为 `TEXT NOT NULL`，外键按下述 owner 删除策略设置：

```text
role_templates:
  id PK, slug, role_kind CHECK lead|researcher|engineer|reviewer,
  name, description, base_instructions, responsibilities_json,
  non_responsibilities_json, base_result_contract_json,
  compatible_capability_kinds_json, builtin CHECK 0|1,
  version CHECK > 0, created_at, updated_at, UNIQUE(slug, version)

agent_definitions:
  id PK, role_template_id FK RESTRICT, slug, name, description,
  instructions, responsibilities_json, non_responsibilities_json,
  input_contract_json, result_contract_json, quality_rubric_json,
  default_engine_kind, default_model_configuration_id nullable,
  default_permission_policy CHECK inherit_work|read_only|work_write,
  default_parallelism CHECK 1..8, memory_policy CHECK confirmed_only,
  builtin CHECK 0|1, active CHECK 0|1, version CHECK > 0,
  created_at, updated_at, UNIQUE(slug, version)

agent_instances:
  id PK, definition_id FK RESTRICT, display_name,
  engine_override nullable, model_configuration_override nullable,
  permission_policy_override nullable,
  parallelism_override nullable CHECK 1..8,
  builtin CHECK 0|1, status CHECK active|inactive,
  created_at, updated_at

capability_packs:
  id PK, catalog_capability_id nullable UNIQUE, name, description,
  instructions, input_schema_json, output_schema_json, procedure_json,
  validation_rubric_json, required_tools_json,
  default_permission_scope CHECK inherit_work|read_only|work_write,
  compatible_role_template_ids_json, required_engine_capabilities_json,
  conflicts_with_capability_pack_ids_json, version CHECK > 0,
  status CHECK catalog_only|executable|deprecated,
  created_at, updated_at

agent_capability_bindings:
  agent_definition_id FK CASCADE, capability_pack_id FK RESTRICT,
  installed_at, PRIMARY KEY(agent_definition_id, capability_pack_id)

work_agents:
  work_id FK CASCADE, agent_instance_id FK RESTRICT,
  role_kind CHECK lead|researcher|engineer|reviewer,
  status CHECK joined|inactive, permission_policy,
  joined_at, updated_at, PRIMARY KEY(work_id, agent_instance_id)

work_leads:
  work_id PK FK CASCADE, agent_instance_id,
  created_at, FOREIGN KEY(work_id, agent_instance_id)
  REFERENCES work_agents(work_id, agent_instance_id)
```

每个 `_json` 列使用 `CHECK(json_valid(column_name))`；schema/contract/rubric 存 JSON object，集合存 JSON array。播种稳定 ID：

```text
role-template:lead:v1       agent-definition:piwork-lead:v1       agent-instance:piwork-lead
role-template:researcher:v1 agent-definition:piwork-researcher:v1 agent-instance:piwork-researcher
role-template:engineer:v1   agent-definition:piwork-engineer:v1   agent-instance:piwork-engineer
role-template:reviewer:v1   agent-definition:piwork-reviewer:v1   agent-instance:piwork-reviewer
catalog-capability:001 through catalog-capability:096
capability-pack:lead-coordination:v1
capability-pack:source-research:v1
capability-pack:engineering-execution:v1
capability-pack:independent-review:v1
```

四个系统基础能力包状态为 `executable`、`catalog_capability_id = NULL`，分别绑定对应内置 Definition；它们只提供角色运行所需的方法、输入/结果合同、工具和权限边界，不对应业务目录卡片。四个内置 Definition 的职责、非职责、结果合同和默认权限逐项编码设计 §5.4；96 项 seed 的稳定 catalog id 和名称与 `agentCapabilities.ts` 一致，状态全部为 `catalog_only`。领域、优先级、受众与长文展示继续由 Task 6 明确保留的静态 TypeScript 目录拥有，不向未定义这些字段的数据库合同中塞入第二份数据。

目录行使用同一个稳定值作为 `id` 与 `catalog_capability_id`：`catalog-capability:001` 至 `catalog-capability:096`；不要发明第二套 catalog pack id。目录行的 `description/instructions` 使用空字符串，对象合同使用 `{}`，集合合同使用 `[]`，默认权限为 `read_only`，从而明确表示它们尚未达到 executable 合同。migration 末尾：

migration 末尾用 `INSERT OR IGNORE ... SELECT FROM works` 回填：`work_agents(work_id, agent_instance_id, role_kind, status, permission_policy, joined_at, updated_at)` 绑定内置 Lead，随后 `work_leads(work_id, agent_instance_id, created_at)` 指向同一 membership。两条语句必须显式列出这些列，不能依赖表列顺序。

`work_leads.work_id` 主键和复合外键只能在 schema 层保证“最多一个 Lead 且 Lead 是 member”；migration 回填与 Task 3 的 Work 创建事务共同保证产品层“每个 Work 恰好一个 Lead”。删除当前 Lead membership 必须被复合外键拒绝，删除 Work 则级联删除 membership 与 Lead。

- [ ] **Step 4: 验证 migration 和旧 Work 回填**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test storage_contract -- --nocapture
```

Expected: PASS；四个 builtin、四个系统基础 executable pack、96 个 catalog-only、已有 Work 的唯一 Lead 全部成立。

- [ ] **Step 5: 提交 schema**

```powershell
git add src-tauri/migrations/0005_agent_domain.sql src-tauri/tests/storage_contract.rs
git commit -m "feat: persist builtin agent domain"
```

---

### Task 3: 实现 Agent Repository 和原子 Work Lead 绑定

**Files:**
- Create: `src-tauri/src/agent/mod.rs`
- Create: `src-tauri/src/agent/repository.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/work/repository.rs`
- Test: `src-tauri/tests/agent_domain.rs`
- Test: `src-tauri/tests/work_lifecycle.rs`

- [ ] **Step 1: 写失败的 Repository 集成测试**

实现四个具名测试并写出明确断言：

```text
repository_lists_builtin_instances_and_capabilities:
  精确比较四个 instance stable id、四个 system executable pack id、
  catalog id 1..96，且所有 catalog status 为 catalog_only。

creating_a_work_atomically_assigns_the_builtin_lead:
  创建后 work_agents 恰有内置 Lead membership，work_leads 指向它。

failed_lead_binding_rolls_back_the_new_work:
  临时删除内置 Lead instance 后 create 返回 FK error，works 不出现新 id。

replacing_a_work_lead_keeps_exactly_one_lead_and_both_memberships_auditable:
  新 Lead 成为 joined member，work_leads 恰一行，旧 Lead 仍为 joined member。
```

- [ ] **Step 2: 运行测试确认 Repository 不存在**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test agent_domain repository -- --nocapture`

Expected: FAIL，`agent::repository` 未定义。

- [ ] **Step 3: 实现专用 Repository**

提供明确方法：

```rust
pub async fn list_role_templates(&self) -> Result<Vec<RoleTemplateSummary>, AppError>;
pub async fn list_agent_instances(&self) -> Result<Vec<AgentInstanceSummary>, AppError>;
pub async fn get_agent_instance(&self, id: &str) -> Result<Option<AgentInstanceSummary>, AppError>;
pub async fn list_capability_packs(&self) -> Result<Vec<CapabilityPackSummary>, AppError>;
pub async fn get_work_team(&self, work_id: &str) -> Result<Option<WorkTeamSummary>, AppError>;
pub async fn add_work_member(&self, work_id: &str, instance_id: &str) -> Result<WorkTeamSummary, AppError>;
pub async fn set_work_lead(&self, work_id: &str, instance_id: &str) -> Result<WorkTeamSummary, AppError>;
pub async fn copy_agent_assembly(&self, input: ResolvedAgentAssembly) -> Result<AgentInstanceSummary, AppError>;
```

Repository 只持久化已解析、已验证的 `ResolvedAgentAssembly`；不在 SQL 层做业务降级。所有 membership/Lead 改动使用 `BEGIN IMMEDIATE`。

- [ ] **Step 4: 把新 Work 和 Lead 写入同一事务**

不要在 `WorkService::create_work` 成功后另调一次 Agent Repository。改造 `WorkRepository::create` 的现有事务：插入 `works` 和 resource links 后，直接插入内置 lead 的 `work_agents`、`work_leads`，最后统一 commit。失败时整个 Work 创建回滚。

- [ ] **Step 5: 运行 Repository 与 Work 生命周期测试**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test agent_domain repository -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test work_lifecycle -- --nocapture
```

Expected: PASS；现有 Work 创建行为不回归且每个新 Work 有 Lead。

- [ ] **Step 6: 提交 Repository**

```powershell
git add src-tauri/src/agent src-tauri/src/lib.rs src-tauri/src/work/repository.rs src-tauri/tests/agent_domain.rs src-tauri/tests/work_lifecycle.rs
git commit -m "feat: manage persistent agent membership"
```

---

### Task 4: 实现 fail-closed 装配校验与内置副本语义

**Files:**
- Create: `src-tauri/src/agent/assembly.rs`
- Create: `src-tauri/src/agent/service.rs`
- Modify: `src-tauri/src/agent/mod.rs`
- Test: `src-tauri/tests/agent_domain.rs`

- [ ] **Step 1: 写装配矩阵失败测试**

逐项证明设计 §6.6，测试名和断言固定为：

```text
assembly_rejects_catalog_only_pack -> NotExecutable
assembly_rejects_incompatible_role -> IncompatibleRole
assembly_rejects_missing_tool_and_engine_capability -> MissingTool + MissingEngineCapability
assembly_rejects_permission_escalation_and_explicit_conflicts -> PermissionEscalation + CapabilityConflict
assembly_rejects_instruction_budget_over_approved_limit -> ContextBudgetExceeded
valid_assembly_resolves_least_privilege_and_parallelism -> 无 diagnostics，权限取交集，builtin parallelism=1
```

定义产品上限常量 `MAX_ASSEMBLY_INSTRUCTION_CHARS = 32_000`，第一版可确定测试，不依赖 tokenizer。

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test agent_domain assembly -- --nocapture`

Expected: FAIL，校验器不存在。

- [ ] **Step 3: 实现纯校验器**

入口：

```rust
pub fn validate_assembly(
    role: &RoleTemplateSummary,
    source: &AgentDefinitionSummary,
    packs: &[CapabilityPackSummary],
    available_tools: &BTreeSet<String>,
    engine_capabilities: &BTreeSet<String>,
    requested_permission: PermissionPolicy,
) -> Result<ResolvedAgentAssembly, Vec<AssemblyDiagnostic>>;
```

规则顺序固定：status → role compatibility → tools → engine capabilities → permission → conflicts → budget。收集全部可修复诊断后一次返回；不跳过不兼容 pack。

- [ ] **Step 4: 实现 Service 的授权语义**

`AgentService` 提供：

```rust
pub async fn list_team_members(&self) -> Result<Vec<AgentInstanceSummary>, AppError>;
pub async fn list_capability_catalog(&self) -> Result<Vec<CapabilityPackSummary>, AppError>;
pub async fn get_work_team(&self, work_id: &str) -> Result<WorkTeamSummary, AppError>;
pub async fn validate_agent_assembly(&self, input: SaveAgentAssemblyInput) -> Result<Vec<AssemblyDiagnostic>, AppError>;
pub async fn save_agent_copy(&self, input: SaveAgentAssemblyInput) -> Result<AgentInstanceSummary, AppError>;
pub async fn add_member_to_work(&self, work_id: &str, instance_id: &str) -> Result<WorkTeamSummary, AppError>;
```

服务端拒绝修改 builtin 行；保存永远创建本地 Definition version 1 + Instance。复制内置成员可保留或卸载其对应的系统基础 executable pack，并可调整显示名、model/engine/permission/parallelism override；四个系统基础 pack 不可编辑，安装任何 `catalog_only` id 必须失败并返回明确诊断。

B 阶段的 `AgentService` 通过构造参数接收当前 Pi 执行身体的工具与 capability allowlist，测试注入确定集合；这只用于保存时 fail-closed 装配校验，不新增 C 所拥有的 `EngineCapabilities` wire DTO、运行时协商或 Harness。C 上线后由运行时 capability 协商替换该静态生产 allowlist。

- [ ] **Step 5: 运行全部 Agent Domain 测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test agent_domain -- --nocapture`

Expected: PASS，包含内置只读、复制不改源记录、catalog-only 拒绝和权限不扩大。

- [ ] **Step 6: 提交校验与服务**

```powershell
git add src-tauri/src/agent src-tauri/tests/agent_domain.rs
git commit -m "feat: validate agent assemblies fail closed"
```

---

### Task 5: 暴露 typed Tauri commands 并组装生产服务

**Files:**
- Create: `src-tauri/src/agent/commands.rs`
- Modify: `src-tauri/src/agent/mod.rs`
- Modify: `src-tauri/src/app_state.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/app/tauriClient.ts`
- Modify: `src/test/mockTauriClient.ts`
- Test: `src-tauri/src/agent/commands.rs`
- Test: `src/app/App.test.tsx`

- [ ] **Step 1: 写 command/client 合同测试**

锁定命令名和参数：

```text
list_agent_instances
list_capability_packs
get_work_team(workId)
validate_agent_assembly(input)
save_agent_copy(input)
add_work_member(workId, agentInstanceId)
```

在 `mockTauriClient.ts` 提供四个内置 instance、96 个 catalog-only pack 和按 Work 返回的 lead，避免无关 UI 测试因新增必需方法崩溃。

- [ ] **Step 2: 运行 Rust 与前端测试确认失败**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml agent::commands -- --nocapture
pnpm test -- src/app/App.test.tsx
```

Expected: FAIL，commands/client 方法不存在。

- [ ] **Step 3: 实现 commands 和 AppState**

`AppState` 新增非可选 `Arc<AgentService>`。startup prepare 阶段用同一 pool 构造 `AgentRepository`；assemble 阶段把 `AgentService` 与现有 Work/Model/Resource services 一起注入。所有 command 只调用 Service，不直接写 SQL。

- [ ] **Step 4: 更新 typed client**

从生成 bindings 引入 DTO；`PiWorkClient` 和 `tauriClient` 实现全部六个方法。不要在 TypeScript 手写重复的 Rust DTO。

- [ ] **Step 5: 运行 bindings、命令与客户端回归**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml domain::tests::export_bindings -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml agent::commands -- --nocapture
pnpm typecheck
pnpm test -- src/app/App.test.tsx
```

Expected: PASS。

- [ ] **Step 6: 提交 API 边界**

```powershell
git add src-tauri/src/agent src-tauri/src/app_state.rs src-tauri/src/lib.rs src/app/tauriClient.ts src/test/mockTauriClient.ts src/app/App.test.tsx src/bindings
git commit -m "feat: expose agent team commands"
```

---

### Task 6: 建立 Agent Center 的纯数据模型并诚实标注能力状态

**Files:**
- Create: `src/features/agent-center/agentCenterModel.ts`
- Create: `src/features/agent-center/agentCenterModel.test.ts`
- Modify: `src/features/agent-center/agentCapabilities.ts`
- Modify: `src/features/agent-center/agentCapabilities.test.ts`

- [ ] **Step 1: 写失败的 view-model 测试**

覆盖：

```typescript
it("merges all 96 static descriptions with authoritative catalog status", () => {
  const { catalog } = splitCapabilityPacks(capabilityPacksFixture());
  const model = buildCapabilityLibrary(AGENT_CAPABILITIES, catalog);
  expect(model).toHaveLength(96);
  expect(model.every(({ status }) => status === "catalog_only")).toBe(true);
});

it("places the unique lead first and never duplicates it in members", () => {
  expect(buildTeamModel(teamFixture()).map(({ roleKind }) => roleKind))
    .toEqual(["lead", "researcher"]);
});
```

增加 `catalog_capability_id` 缺失、重复或未知时 fail loudly 的测试；不要静默把未知状态当 executable。

- [ ] **Step 2: 运行测试确认 helper 不存在**

Run: `pnpm test -- src/features/agent-center/agentCenterModel.test.ts src/features/agent-center/agentCapabilities.test.ts`

Expected: FAIL。

- [ ] **Step 3: 实现纯模型**

`AgentCapability.id` 继续为 1–96；新增稳定映射字段：

```typescript
catalogId: `catalog-capability:${string}`;
status: CapabilityPackStatus;
capabilityPackId: string;
```

服务端状态为权威；`splitCapabilityPacks()` 按 `catalogCapabilityId` 把 96 项业务目录与 4 个系统基础包分开，静态 TS 只合并前者并保留现有长文、受众、输入提示和领域展示。`catalog_only` 的 CTA 仍是“创建 Prompt 草稿”；系统基础包只显示在对应成员详情，不作为业务能力卡片，也不能从 UI 卸载。

- [ ] **Step 4: 运行纯模型测试**

Run: `pnpm test -- src/features/agent-center/agentCenterModel.test.ts src/features/agent-center/agentCapabilities.test.ts`

Expected: PASS，9 域、96 项、23 个 P0 的现有断言仍通过。

- [ ] **Step 5: 提交前端模型**

```powershell
git add src/features/agent-center/agentCenterModel.ts src/features/agent-center/agentCenterModel.test.ts src/features/agent-center/agentCapabilities.ts src/features/agent-center/agentCapabilities.test.ts
git commit -m "feat: model persistent agents in the center"
```

---

### Task 7: 把智能体中心升级为“我的团队 + 能力库”

**Files:**
- Create: `src/features/agent-center/TeamMemberCard.tsx`
- Create: `src/features/agent-center/MemberDetailDrawer.tsx`
- Create: `src/features/agent-center/MemberAssembler.tsx`
- Create: `src/features/agent-center/MemberAssembler.test.tsx`
- Modify: `src/features/agent-center/AgentCenterPage.tsx`
- Modify: `src/features/agent-center/AgentCenterPage.test.tsx`
- Modify: `src/features/agent-center/CapabilityCard.tsx`
- Modify: `src/features/agent-center/CapabilityDetailDrawer.tsx`
- Modify: `src/features/workspace/WorkSurface.tsx`
- Modify: `src/features/workspace/WorkSurface.test.tsx`
- Modify: `src/styles/agent-center.css`
- Modify: `src/i18n/locales/en.json`
- Modify: `src/i18n/locales/zh-CN.json`
- Modify: `src/i18n/locales/locales.test.ts`

- [ ] **Step 1: 写页面行为失败测试**

必须实现以下具名测试与断言：

```text
opens_on_my_team_with_four_builtin_members:
  四个 display name 可见，Lead badge 唯一，四个 system pack 可见。
shows_catalog_badges_and_keeps_prompt_drafting:
  catalog badge=目录能力，点击创建任务草稿回调一次。
does_not_offer_install_for_catalog_only_capabilities:
  详情无安装按钮，显示不可安装原因。
customizes_a_builtin_member_as_a_copy:
  builtin 表单只读；点击自定义后 sourceInstanceId 正确且保存调用 saveAgentCopy。
blocks_save_for_every_server_diagnostic:
  返回的每个 diagnostic message 可见，保存按钮 disabled。
adds_a_member_to_the_selected_work:
  调 addWorkMember 后重新 getWorkTeam，新成员卡可见。
preserves_tabs_drawer_focus_and_error_states:
  ArrowLeft/Right、Escape、关闭后焦点返回、加载失败 alert 均成立。
```

- [ ] **Step 2: 运行 focused UI tests 确认失败**

```powershell
pnpm test -- src/features/agent-center/AgentCenterPage.test.tsx src/features/agent-center/MemberAssembler.test.tsx src/features/workspace/WorkSurface.test.tsx
```

Expected: FAIL，团队视图与组件不存在。

- [ ] **Step 3: 实现两级页面与成员详情**

`AgentCenterPage` props 改为：

```typescript
type Props = {
  client: PiWorkClient;
  currentWorkId?: string;
  onStartCatalogCapability(capability: AgentCapability): void;
};
```

页面加载 `listAgentInstances()` 和 `listCapabilityPacks()`；存在 current Work 时并行加载 `getWorkTeam()`。成员卡显示角色、职责、权限、Engine/Model、绑定的系统基础能力、版本/启用状态；没有 Assignment 前不伪造“运行中”。

- [ ] **Step 4: 实现装配器的复制与校验流**

内置详情只有“自定义副本”“加入当前 Work”“查看已参与 Work（无数据时诚实为空）”。每次选择改变后调用 `validateAgentAssembly`；有 diagnostics 时禁用保存并按 code 显示具体修复建议。对应角色的系统基础能力包固定继承且不可卸载；B 阶段 96 项业务能力均 catalog-only，因此业务 pack selector 显示不可安装原因，但复制仍可保存基础 overrides。

- [ ] **Step 5: 保持能力库旧行为但修正文案**

目录能力“开始使用”继续调用 `buildCapabilityPrompt`，文案明确为“创建任务草稿”，不暗示启动独立 Agent。未来 `executable` 分支只进入选择/装配成员，不在 B 阶段启动 Assignment。

- [ ] **Step 6: 完成本地化、响应式和无障碍**

新增 team/library tabs、角色、状态、权限、诊断、customize/add-to-work 等中英文 key。更新 locale parity 测试；保留 roving tab、Escape、focus return、reduced-motion 和窄屏 drawer 行为。

- [ ] **Step 7: 运行 UI 与类型验证**

```powershell
pnpm test -- src/features/agent-center src/features/workspace/WorkSurface.test.tsx src/i18n/locales/locales.test.ts
pnpm typecheck
```

Expected: PASS。

- [ ] **Step 8: 提交智能体中心**

```powershell
git add src/features/agent-center src/features/workspace/WorkSurface.tsx src/features/workspace/WorkSurface.test.tsx src/styles/agent-center.css src/i18n/locales/en.json src/i18n/locales/zh-CN.json src/i18n/locales/locales.test.ts
git commit -m "feat: turn agent center into a persistent team"
```

---

### Task 8: 完整验证、真实窗口验收与 B→C 交接审计

**Files:**
- Review: all files changed in Tasks 1–7
- Review: `docs/architecture/buzz-upstream-map.md`（B 不移植 Buzz managed-agent 代码，审计结果应为无需修改）
- Review: `THIRD_PARTY_NOTICES.md`（B 不新增第三方派生代码，审计结果应为无需修改）

- [ ] **Step 1: 运行后端完整质量门**

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: 全部 exit 0；没有 ignored failure。

- [ ] **Step 2: 运行前端完整质量门**

```powershell
pnpm test
pnpm typecheck
pnpm build
```

Expected: 全部 exit 0。

- [ ] **Step 3: 启动真实 Tauri 窗口验收**

Run: `pnpm tauri dev`

验证：

1. 旧 Work 可打开且 Team 显示唯一 PiWork 主理人。
2. 新 Work 创建后立即有唯一 Lead。
3. 我的团队显示四个预置成员及各自系统基础能力；内置记录不可原地编辑。
4. 自定义副本不会改变内置成员；可以加入当前 Work。
5. 能力库仍为 9 域、96 项，全部显示“目录能力”；点击只创建 Prompt 草稿。
6. Agent Center 没有 Assignment、Session、伪运行状态或独立成员私聊。
7. 现有单 Agent Work 启动、停止、Activity Feed、附件与模型选择仍工作。

- [ ] **Step 4: 审计设计覆盖和后续接口**

逐项确认设计 §§5.3–6.7、11、13、15 migration 原则、18.2/18.5、19.B。确认 C 可以直接依赖：

```text
AgentRepository.get_work_team
AgentRepository.get_agent_instance
AgentDefinition.default_parallelism
AgentInstance overrides
WorkLead / WorkAgent foreign keys
PermissionPolicy
```

确认本计划没有新增 `assignments`、`agent_sessions`、Memory 或主理 tool bridge。

- [ ] **Step 5: 检查范围和工作树**

```powershell
git diff --check
git status --short
```

Expected: 无 whitespace error；只包含 B 计划范围文件和用户原有改动。

- [ ] **Step 6: 记录最终验收结果**

本任务默认不产生新代码。如果 Step 1–5 暴露 B 范围缺陷，返回对应 Task 修复并重新运行完整门禁；不要创建只含验收文案的空提交。

---

## 子项目 B 完成定义

- [ ] 七张 Agent Domain 表、约束、seed 和 legacy Lead 回填已验证。
- [ ] 每个新旧 Work 恰好一个 Lead，Lead 也是 member。
- [ ] 四个预置长期成员及四个系统基础可执行能力包可查询、正确绑定；内置只读，自定义产生副本。
- [ ] 装配校验逐项覆盖设计 §6.6，并 fail closed。
- [ ] 96 项能力全部诚实标为 `catalog_only`，现有 Prompt 草稿能力保留。
- [ ] 智能体中心显示“我的团队 + 能力库”，并支持加入当前 Work。
- [ ] 现有单 Agent 执行链路、旧 Work 和 Activity Protocol 无回归。
- [ ] 没有提前实现 C/D/E 范围。
