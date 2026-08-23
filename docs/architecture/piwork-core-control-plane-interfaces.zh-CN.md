# PiWork 核心控制面稳定 Interface

> 状态：阶段 0–5 已实现；本文是后续能力平台、Adapter 与 UI read model 的依赖边界。

## 不变量

1. SQLite 是 Work、Run、Assignment、Capability、Delivery 与 Workspace 的事实源。
2. 生产执行入口只有 `ExecutionCoordinator`；Adapter 不直接控制 Work。
3. 能力授权以不可变 `RunCapabilitySnapshot` 为权威，所有未知或失效输入默认拒绝。
4. Work 非归档执行状态由 `project_work_status(WorkExecutionFacts)` 计算。
5. Work Completed 只来自当前 Lead 的有效 Work Delivery；Member Completed 必须有 valid Result。
6. 新跨领域关联使用 `workspace_id`，`works.root_path` 仅是一个发布周期内的兼容列。

## Module interface

| Module | 调用入口 | 隐藏职责 | 禁止绕过 |
|---|---|---|---|
| `execution` | `submit(work_id, WorkInput)`、`control(work_id, ExecutionCommand)` | Assignment 提交、Scheduler 控制、幂等 Receipt | Tauri command 不得直接调用 Engine/Scheduler Repository |
| `capability` | `snapshot(request)`、`authorize_and_record(snapshot, operation)`、`inspect/revoke` | 快照编译、决策/审批/执行审计持久化、默认拒绝策略 | Pi/Extension/Connector 不得独立扩权 |
| `work::projector` | `project_work_status(facts)` | 纯状态表；Delivery、Run、Lead/Member、恢复、死信与归档优先级 | Scheduler outcome 不得直接写 Work 终态 |
| `delivery` | `submit_member_result`、`complete_work`、`inspect` | Result 校验、Artifact admission、Validation 证据、Delivery 有效性 | 普通 `agent_end` 不得完成 Assignment/Work |
| `workspace` | `resolve_or_create(root, policy)`、`get`、`reconcile_legacy_paths` | 规范路径 identity、稳定 ID、旧路径合并 | 新 module 不得把 `root_path` 当稳定主键 |

## 执行与授权顺序

`ExecutionCoordinator` 持久化 Lead Assignment 并唤醒 Scheduler；Scheduler claim Assignment、创建 Run、读取 Agent/Workspace/Extension grants；Harness 通过 `CapabilityBroker` 为该 Run 写入唯一快照，再按快照签发 Host Tool lease；Pi 启动参数只包含快照允许且当前仍可用的工具；每次 Host Tool 调用重新验证 token、快照状态和精确工具 ID，先持久化 Allow/Deny/Ask 决策，Allow 再建立 started 执行审计，调用结束后写入 succeeded/failed；Harness 最终只提交 Run/Assignment 事实，由 DeliveryModule 与 WorkStatusProjector 决定产品状态。

## Delivery 语义

- Lead 调用 `complete_work_delivery` 时，所有直接必需子 Assignment 必须 Completed。
- Artifact 路径必须存在、是普通文件、位于规范 Workspace 内且不超过 100 MiB。
- 没有 `source_event_id` 的 Validation 保存为 `unverified`，不能满足强制验收项。
- Delivery 先以 `pending` 持久化；Assignment 完成与 Delivery Event 成功后才变为 `valid`。
- Projector 只读取 `status = 'valid'` 的当前 Lead Delivery，因此部分失败不会误报 Completed。

## Workspace 迁移与恢复

- `0018` 创建 Workspace 并回填 `works.workspace_id`；兼容触发器修复仍只写 `root_path` 的旧调用方。
- `0019` 增加 Memory、Extension、Connector 的 Workspace policy links；冲突配置默认禁用或空权限。
- `0020` 加入逻辑非空和 identity 不变量；`0021` 仅兼容历史空路径测试/旧数据。
- `0022` 持久化 Capability 决策与待审批请求；`0023` 把实际 Host Tool 执行结果关联到 Allow 决策，未确认执行保留为 `started`。
- 启动时 `reconcile_legacy_paths` 使用操作系统真实路径解析合并大小写、符号链接和 Windows 短路径别名；不可访问路径标记 `unavailable`，不删除 Work。
- 数据库升级或重建后运行 `WorkRepository::rebuild_work_statuses`，从持久事实修复旧状态缓存。

## Adapter 接入检查单

新 LSP、MCP、Browser、Sandbox、Documents 或 Worktree Adapter 必须：从 Workspace 获得路径；在执行前取得 Capability Snapshot；把危险操作提交给 Broker；把运行事件先持久化再发布；把产物与验证提交给 Delivery；不得直接更新 `works.status`、`assignments.status`、Delivery 或 Workspace policy。
