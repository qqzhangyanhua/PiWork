# TencentDB Agent Memory 与 CoDo 工作区记忆层接入研究

日期：2026-08-21  
范围：只使用腾讯官方 GitHub 仓库、仓库内文档和源码；以用户已部署的云端实例为前提。

## 结论

用户所说的“腾讯 Agent Memory”可明确对应为 **[TencentCloud/TencentDB-Agent-Memory](https://github.com/TencentCloud/TencentDB-Agent-Memory)**。仓库将产品定义为团队级记忆中枢，统一管理四类资产：Chat Memory、Skill、LLM-Wiki 和 Code-Graph；其中 CoDo 第一阶段应只接 **Memory Core v3 HTTP API** 的 Chat Memory（L0-L3），不要把 Memory Proxy、Memory Hub 或整套 Node 运行时嵌入 Tauri 应用。[官方总览](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/README.md) [Memory Core 说明](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/MemoryCore/README.md)

推荐的产品边界是：

- CoDo 本地 SQLite 继续保存 Work、Ledger、Assignment、审计和确定性状态。
- 腾讯 Memory Core 作为可配置、可关闭、允许暂时不可用的跨会话语义记忆服务。
- 一个 CoDo 用户或部署映射为一个腾讯 Team；工作区映射为 Team 下的 Task；跨 Work 持续存在的 CoDo AgentInstance 映射为腾讯 Agent；一次 Work 映射为 Session。
- 在新一轮构造提示词前跨 Session 召回；在一轮完整对话成功落地后异步写入 L0。远端失败不得阻断用户继续工作。

## 项目、许可与成熟度

- 精确项目：`TencentCloud/TencentDB-Agent-Memory`。官方仓库 README 使用 `TencentDB Agent Memory` 名称。[仓库](https://github.com/TencentCloud/TencentDB-Agent-Memory)
- 许可：仓库 `LICENSE` 明确为 **MIT License**，允许使用、修改、分发和商业化，但需保留版权与许可文本。[LICENSE](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/LICENSE)
- 截至本次核查，稳定版本为 `v2.0.0`，另有 `v2.0.1-beta.2` 预发布版本；README 也明确称 Team Memory Beta 正在快速演进。应固定服务端版本并在升级前做契约测试，不能按“成熟稳定基础设施”对待。[Releases](https://github.com/TencentCloud/TencentDB-Agent-Memory/releases) [Roadmap](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/ROADMAP.md)

## Memory Core 的模型与 API

Memory Core 把聊天记忆分成四层：[Memory Core README](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/MemoryCore/README.md) [技术说明](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/README.md#technical-implementation)

| 层 | 内容 | CoDo 用途 |
| --- | --- | --- |
| L0 Conversation | 原始会话与来源 | 可追溯的完整轮次 |
| L1 Atom | 提取后的事实、偏好、约束、事件 | 日常跨会话召回的主入口 |
| L2 Scenario | 按项目或场景整理的知识块 | 恢复工作区/项目上下文 |
| L3 Core / Persona | 长期画像和稳定模式 | Agent 快速理解用户与团队 |

新接入应使用 v3 严格隔离数据面。每个业务请求都必须带 `team_id`、`agent_id`、`user_id`；L0 写入还必须带非空 `session_id`。读取 L0/L1 时省略 `session_id` 才会在同一 `team + agent + user` 下跨会话聚合；L2/L3 的作用域是 `team + agent`，不使用 Session。[TypeScript SDK README](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/sdk/memory-core/typescript/README.md) [v3 client 源码](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/sdk/memory-core/typescript/src/v3/client.ts)

关键接口如下：[SDK API 表](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/sdk/memory-core/typescript/README.md#api-methods)

| 操作 | API |
| --- | --- |
| 写入原始轮次 | `POST /v3/conversation/add` |
| 查询/搜索 L0 | `POST /v3/conversation/query`, `/v3/conversation/search` |
| 删除 L0 | `POST /v3/conversation/delete`，显式传 `message_ids` 或 `session_ids` |
| 查询/搜索 L1 | `POST /v3/atomic/query`, `/v3/atomic/search` |
| 修改/删除 L1 | `POST /v3/atomic/update`, `/v3/atomic/delete` |
| 读写/删除 L2 | `/v3/scenario/ls`, `/read`, `/write`, `/rm` |
| 读写 L3 | `/v3/core/read`, `/v3/core/write` |
| 清空整项 Chat Memory | `POST /v3/chat-memory/clear` |

`conversation/add` 先保存 L0；L1/L2/L3 由 Memory Core 内部异步流水线抽取和聚合，因此刚写入后立即搜索不保证能看到抽取结果。官方默认配置按会话量、空闲时间和层级间隔触发，并包含重试与后台 Worker；这些 Worker 属于远端 Memory Core，CoDo 客户端不需要另起 Worker，但需要接受最终一致性。[standalone 配置](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/MemoryCore/tdai-gateway.standalone.yaml) [Pipeline Worker](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/MemoryCore/src/services/pipeline-worker.ts)

## 云端连接配置

建议在 CoDo 设置中新增一个 `TencentDB Agent Memory` 连接器，非敏感设置放 SQLite，密钥进入现有 Secret 存储：

```text
enabled
hub_url                  # 可选；用于发现实例、endpoint、service_id
endpoint                 # Memory Core gateway_endpoint；生产必须 https
service_id               # 实例 instance_id；发送为 x-tdai-service-id
gateway_api_key          # Gateway 服务凭据；发送为 Authorization: Bearer ...
user_key                 # Hub 创建的用户密钥；发送为 x-tdai-user-key
team_id                  # 从 Hub 选择或通过 meta/team/list 获取
user_id                  # 由 user_key 调 auth/verify 得到，建议只读展示
request_timeout_ms
recall_timeout_ms
max_recall_items
max_recall_chars
capture_enabled
recall_enabled
```

官方 v3 transport 使用 `Authorization: Bearer <apiKey>`、`x-tdai-service-id` 和可选 `x-tdai-user-key`；JSON 响应以 `code === 0` 表示成功，并返回 request/trace ID 供排障。[v3 transport](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/sdk/memory-core/typescript/src/v3/http.ts) 管理面通常还要求用户 Key，并区分普通用户与 system admin。[管理面鉴权](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/MemoryCore/src/metadata/router/auth.ts)

### 字段如何获得：官方实现核对

| CoDo 设置字段 | 官方来源/获得方式 | 精确语义 |
| --- | --- | --- |
| `endpoint` | Memory Hub Control 的公开 `GET /api/v1/meta/instances` 返回 `gateway_endpoint`；自托管也可由部署者直接提供 | 直连 Memory Core 时使用 `gateway_endpoint`。Hub API Key 页展示给 Claude/Codex 等客户端的地址优先取 `proxy_endpoint`，那是 Memory Proxy 地址，不是 Core 数据面地址 |
| `service_id` | 上述实例列表的 `instance_id`；Hub API Key 页也显示当前实例名和 `(instance_id)` | 等于实例注册表的 `id`，发送为 `x-tdai-service-id`。一键部署用 `REMOTE_INSTANCE_ID` 配置，默认是 `default`；Proxy 地址尾部的 `<spaceId>` 也是同一个值 |
| `gateway_api_key` | 只能由部署/运维方提供 | Memory Core 的 `TDAI_GATEWAY_API_KEY` 或 YAML `server.apiKey`；一键部署变量是 `MEMORY_CORE_GATEWAY_API_KEY`，Hub 对应 `REMOTE_INSTANCE_KEY`，源码版 Hub 注册表字段为 `api_key`。该字段是服务端私密值，`GET /api/v1/meta/instances` 明确不会返回 |
| `user_key` | Memory Hub 左侧“API Key”页创建；明文只在创建响应展示一次 | 形如 `sk-mem-...`。直连 Core 管理面时发送为 `x-tdai-user-key`；通过 Memory Proxy 使用 Claude/Codex 等客户端时，同一密钥放在 `Authorization: Bearer <user_key>` 中，由 Proxy 调 `auth/verify` 解析用户 |
| `user_id` | 用 `user_key` 调 `POST /v3/meta/auth/verify`，读取 `data.user.user_id`；Hub 登录流程就是这样做 | 不应让用户手填，也不应从 key 文本推导。Hub Control 入口为 `POST /api/v1/meta/auth/verify`，body 传 `user_key`、header 只传实例 ID；直连 Core 时还需 Gateway Bearer |
| `team_id` | Hub 团队页面选择；或用当前 `user_id` 调 `POST /v3/meta/team/list` | 列表项返回真实 `team_id`。管理 API 调用需要 Gateway Bearer、`x-tdai-service-id` 和 `x-tdai-user-key`；不要用团队名称代替 ID |

官方证据链：

- Hub 实例注册表规定 `id = instance_id = x-tdai-service-id`，`gateway_endpoint` 指 Core，`proxy_endpoint` 仅供客户端地址卡片，`api_key` 只供服务端转发且不公开：[实例注册表说明](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/MemoryPanel/config/metadata-instances.README.md) [注册表源码](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/MemoryPanel/src/panel/config/instance-registry.ts) [公开实例路由](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/MemoryPanel/src/panel/http/routes/meta/instances.ts)
- 容器化 Hub 的对应部署键为 `REMOTE_INSTANCE_ID`、`REMOTE_INSTANCE_URL`、`REMOTE_INSTANCE_KEY`、`REMOTE_INSTANCE_PROXY_URL`；Core 的键为 `TDAI_GATEWAY_API_KEY`，外层示例变量为 `MEMORY_CORE_GATEWAY_API_KEY`：[Hub 启动脚本](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/deploy/global-images/start-memory-hub.sh) [Core 启动脚本](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/deploy/global-images/start-memory-core.sh) [.env 模板](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/deploy/global-images/.env.example)
- Hub API Key 页面调用 `user-key/create`，把返回的 `key_value` 只显示一次；页面注释明确它是 `User_Key`，登录后元数据请求把它注入 `X-Tdai-User-Key`：[API Key 页面](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/MemoryPanel/web/src/pages/team/ApiKeysPage/components/ApiKeyPanel.tsx) [用户与 User Key API](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/MemoryPanel/web/src/lib/api/users.ts) [Hub 请求头注入](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/MemoryPanel/web/src/lib/api/base.ts)
- Memory Proxy 的官方客户端示例则把 `sk-mem-...` 配成模型客户端 API Key；Proxy 从来访 `Authorization` 提取它，再调用 Core `auth/verify`，且从 URL 的 `<spaceId>` 得到 Service ID：[Memory Proxy README](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/MemoryProxy/README.md#client-configuration) [Proxy 鉴权源码](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/MemoryProxy/src/auth.ts)
- `user_id` 的官方发现链为 `auth/verify`，Team 列表 body 需要该 ID：[Hub 登录源码](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/MemoryPanel/web/src/components/LoginGate.tsx) [Team API](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/MemoryPanel/web/src/lib/api/teams.ts)

### 两把 Key 不能合并

Memory Hub“API Key”页创建的不是 `gateway_api_key`。两者应在 CoDo UI 中使用不同名称：

```text
Gateway 服务密钥  -> Authorization: Bearer ...  -> Core 第一层共享密钥
用户 API Key      -> x-tdai-user-key: sk-mem... -> 用户身份、权限与管理面
```

对 Core 原生 L0-L3 数据面，官方 `V3MemoryClientConfig.userKey` 明确是可选的，源码说明内核数据面不做用户级鉴权；`team_id/agent_id/user_id` 是调用方提交的隔离字段。`/v3/meta/*` 管理面则除 `auth/verify` 外都强制 `x-tdai-user-key`。[v3 配置类型](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/sdk/memory-core/typescript/src/v3/types.ts) [Meta 鉴权路由](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/MemoryCore/src/metadata/router/v3-meta-router.ts)

因此，CoDo 即使第一阶段只读写记忆，也建议携带 `user_key`，但不能把它误当成 Core Bearer。若部署方只给普通用户 `sk-mem-...`，没有提供 Gateway Bearer，则按官方开源架构 **无法安全直连启用了 Bearer gate 的 Memory Core**；可选方案是由部署方发放受限服务凭据、在已有前置网关中完成用户认证，或改走 Memory Proxy。官方 OSS 没有从 Hub 浏览器读取 Gateway Bearer 的用户流程。

本地一键部署还有一个特殊情况：`MEMORY_CORE_GATEWAY_API_KEY` 默认留空，Core 的共享密钥校验关闭，但 v3 数据面路由仍要求存在一个非空 `Authorization: Bearer ...` 头；此时 token 不会与配置值比对。生产环境不能依赖这种默认开放行为。[Core 认证门](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/MemoryCore/src/gateway/server.ts) [数据面 header 解析](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/MemoryCore/src/gateway/v2-router.ts)

应从 Rust/Tauri 后端用 `reqwest` 直接调用 HTTP API，而不是从 WebView 发请求，也不必引入官方 TypeScript SDK。这样密钥不会暴露给前端，超时、重试、TLS、审计和脱敏可以集中处理。连接测试至少验证 `/health`、认证、`service_id` 与一组只读 v3 调用；不要用真实写入作为普通“测试连接”。

## CoDo 标识映射

| CoDo | TencentDB Agent Memory | 规则 |
| --- | --- | --- |
| CoDo 用户或部署 | `team_id` | 在全局连接设置中配置一个稳定 Team；所有工作区共享该 Team |
| 工作区/项目根目录 | `task_id` | 首次绑定时通过 Hub 或 `/v3/meta/task/create` 创建 Task，持久化服务端返回的 `task-*` ID；每个工作区一个 Task |
| 本地用户 | `user_id` | 首次绑定时选择/创建；单机用户也使用稳定 ID |
| AgentInstance（长期成员身份） | `agent_id` | 使用跨 Work 稳定的实例 ID；不要使用 Definition 版本或每次运行新建的 Session ID |
| Work | `session_id` | 同一 Work 的多个 Run 共享一个稳定 Session；L0 写入必填 |

这一区分决定了跨会话是否成立：若把临时 Agent instance 映射成 `agent_id`，新会话就会落到新的记忆隔离桶中。工作区共享也不等于把所有 Agent 混成一个 Agent；每个角色仍应有独立 `agent_id`，共享内容通过 Team 资产、ACL 或绑定治理。

当前 CoDo 并非“只有单个 Pi session 的记忆”：`agent_memory` 已持久化到 SQLite，并按 `agent_instance_id` 读取；`work_memory` 则按 `work_id` 保存 Ledger 投影。不过它们分别面向 Agent 实例和单个 Work，还没有工作区级语义检索与跨 Agent 共享层。[当前 MemoryService](../../src-tauri/src/collaboration/memory.rs) [当前迁移](../../src-tauri/migrations/0008_work_memory_and_results.sql)

## 推荐接入流程

1. **绑定**：用户为 CoDo 连接配置一个远端 Team 和 User；工作区首次启用时调用 `/v3/meta/task/create` 创建 Task，保存返回的 `task_id`，并为 CoDo AgentInstance 建立远端 Agent 映射。映射必须持久化，禁止每次启动重新生成。
2. **召回**：新一轮开始前，以当前用户请求调用 `atomic/search`，带上工作区 `task_id` 但省略 `session_id`，从而只在当前工作区内跨 Work 召回；按需读取 L2/L3。限制条目、字符数和总超时，把结果标为“不可信参考记忆”，不得当作系统指令。
3. **写入**：一轮用户消息和最终助手回复完成并写入本地事件库后，再调用 `conversation/add`。默认不上传思维链、原始工具输出、环境变量、临时文件内容或密钥。
4. **可靠性**：用本地 outbox 记录待同步轮次，以幂等键去重并退避重试。远端不可用时继续执行 Work，只显示“记忆暂不可用/待同步”。
5. **治理**：在 CoDo 中至少提供查看来源、停用工作区记忆、删除单次 Session、删除 L1 条目和打开腾讯管理面。整项 `chat-memory/clear` 属于高风险操作，必须二次确认；官方 SDK 还特别指出 Core 的清空接口本身不提供用户级 owner 授权，owner-only 语义应经过 Panel 后端。[清空语义与权限说明](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/sdk/memory-core/typescript/README.md#batch-delete-and-clear)

### `task_id` 是否可以由 CoDo 任意生成

结论分两层：

- **仅对 L0/L1 数据面而言，技术上可以传任意非空字符串**：`task_id` 是可选业务过滤维度，v3 严格隔离真正强制的是 `team_id + agent_id + user_id`；数据面存取使用 `task_id` 过滤，没有查询 Meta Task 是否存在。[数据面隔离类型](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/MemoryCore/src/core/store/isolation.ts) [数据面路由](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/MemoryCore/src/gateway/v2-router.ts)
- **若 CoDo 将工作区建模为 Memory Hub 中可见、可关联 Agent/资产、可记录参与关系的正式 Task，就不能自行生成**：公开 `/v3/meta/task/create` schema 不接受 `task_id`，服务端存储层生成 `task-*` ID；参与日志还会验证 Team、Task、Agent、User 关系。[Task schema](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/MemoryCore/src/metadata/router/v3-meta-schemas.ts) [Task 服务](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/MemoryCore/src/metadata/service/metadata-service.ts) [ID 生成器](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/MemoryCore/src/metadata/utils/id-generator.ts)

对 CoDo 的明确建议是采用第二种：调用管理 API 或让用户在 Hub 先创建 Task，再保存返回 ID。若暂时没有管理面权限，宁可省略 `task_id` 并只按 Team/Agent/User/Session 接入，也不要制造一个 Hub 不认识的“伪 Task”。

第一阶段不要让腾讯 Memory Core 替换 CoDo 现有 confirmed-only `memory_candidates` 流程。后者负责“哪些 Agent 结论可以成为受控事实”，远端层负责语义检索和跨会话延续；两者职责不同。后续可把用户确认的候选同步为显式 L1/L2 内容，但需要保留来源和撤销能力。

## 依赖与部署事实

虽然本方案只连接已部署云实例，仍需理解服务端约束：Memory Core 要求 Node.js `>=22.16`；单机模式使用 SQLite、本地文件和进程内状态，除 OpenAI-compatible LLM API 外可不依赖其他服务。只读查询不一定调用 LLM，但记忆抽取与聚合需要有效 LLM 凭据；远程 Embedding 默认关闭时仍可使用 BM25，向量/混合检索可选 OpenAI-compatible Embedding 或腾讯云向量数据库。[Memory Core Runtime](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/MemoryCore/README.md#runtime) [默认配置](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/MemoryCore/tdai-gateway.standalone.yaml)

官方也支持本地 Node 或 Docker 运行，默认监听 `127.0.0.1:8420`；非 loopback 绑定必须配置 Gateway API Key。Windows 理论上可运行 Node 服务，但官方一键部署入口主要是 shell/Docker 流程，不应作为 CoDo Windows 安装包的内置依赖。已部署云实例因此是当前更合适的产品形态。[安装与安全](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/MemoryCore/README.md#docker)

## 关键风险与保护措施

- **数据出境与隐私**：完整对话会离开本机。必须按工作区显式启用，展示目标域名，默认排除附件原文、工具输出、密钥和敏感路径，并提供可执行的删除/导出策略。
- **认证混淆**：Gateway Bearer token、实例 `service_id`、用户 Key 是不同维度。配置和错误提示要分开，不把 system-admin Key 当普通运行密钥。
- **TLS**：生产只接受 HTTPS 且必须校验证书。官方旧 transport 曾为自签名环境提供跳过校验选项；CoDo 不应暴露默认关闭校验的生产配置。[HTTP transport](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/sdk/memory-core/typescript/src/v3/http.ts)
- **跨租户泄漏**：每次调用都从持久化映射组装 `team_id/task_id/agent_id/user_id`，不接受模型生成这些 ID；服务端升级后用隔离契约测试验证 A 工作区永远搜不到 B 工作区内容。
- **记忆污染/提示注入**：召回内容是用户数据，不是指令。必须用固定边界包装、携带来源和时间，并限制其优先级；不能把 Memory 内容拼进 system prompt 的不可区分区域。
- **最终一致性**：L0 写入成功不代表 L1/L2/L3 已生成。UI 不应承诺“立即记住”，也不能把暂时搜不到判定为数据丢失。
- **版本风险**：当前稳定版发布不久且 beta 分支持续变化。固定服务版本和 API 合同，记录 `x-trace-id`/request ID，升级前覆盖写入、跨 Session 召回、删除和权限隔离测试。
- **可用性**：Memory 是增强层而非 Work 的事务依赖。读取短超时、失败降级为空上下文；写入本地排队、有限重试，并让用户看见同步状态。

## 建议决策

可以引入，但应以 **“远程可选连接器 + Memory Core v3 适配层”** 实现，而不是作为 Pi 插件，也不是替换本地 Work Memory。第一阶段只交付：连接配置、稳定 ID 映射、L0 完整轮次写入、跨 Session L1 召回、L2/L3 只读、同步状态和删除入口。Skill、Wiki、CodeGraph 以及自动资产分配应在这一条数据链路经过隐私、隔离和可靠性验证后再逐步打开。
