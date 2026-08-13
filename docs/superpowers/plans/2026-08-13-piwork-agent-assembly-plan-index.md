# PiWork Agent Assembly A–D 实施计划索引与设计覆盖矩阵

本文是计划索引和交接合同，不是把 B/C/D 合并成一份计划。实施仍严格拆成独立可验收的 B、C、D 三份，并按设计规定的 `A → B → C → D` 顺序执行。

## 计划清单

| 子项目 | 独立实施计划 | 入口条件 | 独立交付物 |
|---|---|---|---|
| A | [`2026-08-09-piwork-activity-protocol.md`](./2026-08-09-piwork-activity-protocol.md) | 基线 | Activity Protocol v2、Buzz projector、Timeline/Raw Rail |
| B | [`2026-08-13-piwork-agent-domain-and-center.md`](./2026-08-13-piwork-agent-domain-and-center.md) | A 完成 | 长期 Agent Domain、四个预置成员、Work Lead、智能体中心 |
| C | [`2026-08-13-piwork-assignment-queue-and-session-harness.md`](./2026-08-13-piwork-assignment-queue-and-session-harness.md) | A+B 完成 | 持久 Assignment、Buzz Queue、Scheduler、Engine Harness、Agent × Work Session |
| D | [`2026-08-13-piwork-lead-delegation-and-context.md`](./2026-08-13-piwork-lead-delegation-and-context.md) | A+B+C 完成 | Lead 宿主工具、单层委派、Result、Context、Ledger、Memory、完整闭环 |

E 不混入这三份计划：多 Engine、用户从空白创作 executable pack、经验证的同 Work 只读并行、RemoteRunner 都在 A–D 验收后另行设计。

## 设计章节归属

设计源为 [`2026-08-09-piwork-agent-assembly-design.md`](../specs/2026-08-09-piwork-agent-assembly-design.md)。下表中的“主责”表示该计划必须完成实现和验收；“消费”表示只使用上游合同，不重复拥有同一实现。

| 设计章节 | 主责计划/任务 | 下游消费或边界 |
|---|---|---|
| §5.1–5.2 一个 Work、一个主理、少量成员 | D T9–T16 | B 提供成员；C 提供运行底座 |
| §5.3–5.4 积木与四个预置成员 | B T1–T4、T7 | D 只委派已装配的成员 |
| §6.2–6.7 Agent Domain、能力包、WorkLead/WorkAgent | B T1–T5 | C/D 只通过稳定 ID 和 Repository API 消费 |
| §6.8 Assignment | C T1–T3 | D 扩展 parent/dependency/result 行为，不另建第二套 Assignment |
| §6.9 AgentSession | C T1–T3、T6 | D 复用 Lead/Member 的 Agent × Work Session |
| §6.10 Memory | D T1–T2、T12 | B 仅冻结 `confirmed_only` policy |
| §7.1–7.4 宿主工具、单层委派、Lead 恢复、Result | D T1–T3、T6–T11 | C 不向模型暴露 delegate tool |
| §7.5 用户介入 | C T9–T10 | D 复用同一 queue/steer/interrupt 语义 |
| §8 Context、Ledger、Memory 写入 | D T4–T5、T12 | C Harness 提供 ContextBuilder 注入点 |
| §9 Queue、持久化与并发 | C T2–T4、T8 | D 只能 wake Scheduler，不能绕过 Queue |
| §10.1–10.4 Engine capability 与 Session | C T5–T7 | D T8 仅增加 Pi 的显式内置 extension 适配 |
| §10.5 运行身体/工具接入 | D T5–T8 | HostToolBridge 是 Rust 权威边界 |
| §11 权限交集 | B T1、T4；C T7、T9；D T6–T10 | B 存 policy，C 在运行时携带 scope，D 做最终工具授权 |
| §12 Activity/Inspector | A 主协议；C T11；D T13–T14 | C/D 只增加领域事件和投影语义，不改双视图原则 |
| §13 智能体中心 | B T6–T7；D T13 | D 追加 Assignments/Memory Inspector，不重做 Team/Library |
| §14.1 新 Work | B T3；C T9；D T9–T11 | 三段顺序是绑定 Lead → 建 Lead Assignment → 可选委派/恢复 |
| §14.2 重启恢复 | C T8、T12；D T11、T15–T16 | B 只保证 legacy Work 可回填 |
| §15 SQLite 演进 | B migration 0005；C 0006；D 0007 | 每个 migration 只归一份计划，且逐级可打开旧库 |
| §16 Buzz 移植边界 | A projector；C T4/T12 queue | B/D 不移植 Buzz Agent/Channel/Relay |
| §17 错误与恢复 | C T3、T7–T12；D T3、T7、T11、T15 | 权限/上下文/结果失败均 fail closed |
| §18 测试策略 | B T1–T8；C T1–T12；D T1–T16 | 每份计划各自质量门，不用 D 的最终门替代 B/C |
| §19 分解与顺序 | 本索引 + 三份独立计划 | 固定 A→B→C→D |
| §20 验收标准 | 见下一节 | A–D 合并验收只在 D T16 执行 |

## §20 验收标准逐条落点

| 设计验收标准 | 实现主责 | 证据入口 |
|---|---|---|
| 新 Work 自动绑定唯一 Lead | B | T2 storage seed/backfill、T3 transaction、T8 legacy/new Work 验收 |
| Lead 自行完成或委派预置成员 | D | T9、T11、T15–T16 |
| 角色、装配、能力、权限、Result 长期版本化 | B + D | B T1–T4；D T1–T3、T6 |
| 同 Work 不出现两个 in-flight Assignment | C | T4、T8、T12 race tests |
| 跨 Work 公平且有界并行 | C | T4、T8、T12 |
| Assignment 执行前持久化，重启不丢 | C | T2–T3、T8、T12 |
| Lead waiting 不占持续 Engine turn | D | T11、T15–T16 |
| Result 进入 Ledger，Lead 获得压缩上下文 | D | T3–T5、T10–T11 |
| 主 Timeline 可懂、Raw Rail 可追溯 | A + C + D | A projector；C T11；D T13–T14 |
| 用户可 queue/steer/interrupt 且看到降级 | C | T9–T10、T12 |
| Session/Engine 丢失不丢产品身份和历史 | C + D | C T6–T8/T12；D T11/T15 |
| 96 项复用但不伪装成可运行 Agent | B | T2、T4、T6–T8 |
| Buzz 派生代码/测试有固定来源和差异记录 | A + C | A 现有上游映射；C T4、T12 |
| 旧 Work 与 legacy Run 无损打开 | B + C | B T2–T3/T8；C T1–T2/T6/T12 |

## 跨计划交接合同

### B → C

- C 只依赖 B 暴露的 `AgentInstance`、`WorkAgent`、`WorkLead`、`PermissionPolicy` 和 assembly result，不直接更新 B 的 builtin 表。
- 四个预置 Definition 都有一个内部 executable 系统基础能力包；96 个业务目录项保持 `catalog_only`。这样预置成员可执行，同时目录不会被虚假升级。
- 新 Work 创建和 legacy 回填已经保证唯一 Lead；C 的 `start_work` 必须把首个 Assignment 分配给该 Lead。

### C → D

- `AssignmentRepository` 是唯一 Assignment 事实源；D 的委派必须调用 Service/Repository 事务并在 commit 后 wake C Scheduler。
- C 冻结 `StartWorkOutput { assignment, run: Option<RunSummary>, user_message }`，因为 accepted 不等于已启动。
- C 的 `EngineRunContext` 必须包含 work/assignment/agent/session/run/permission/context identities；D 通过既定 ContextBuilder hook 和 extension args 扩展，不旁路 Harness。
- D 复用 C 的 dependency、waiting、retry、dead-letter、resume/session contracts，不创建第二套队列、supervisor 或 session store。

## 执行门禁

1. 每份计划从失败测试开始、逐 Task 提交，不能先一次性写完实现再补测试。
2. B 完成定义未满足时不得开始 C；C 完成定义未满足时不得开始 D。
3. 每个子项目都运行自己的 Rust、前端、构建和真实窗口质量门；D 的最终 A–D 验收不能替代上游质量门。
4. 任何实现需要改变本索引中的所有权、公开 DTO 或 migration 边界时，先更新设计与对应独立计划，再修改代码。
5. 完成 D 后，以本矩阵逐行附上测试名、事件样例或真实窗口证据；存在空行即不能宣称 A–D 完成。
