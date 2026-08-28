# OpenConnector 接入 PiWork 的边界与推荐方案

日期：2026-08-28
范围：OpenConnector `main` commit `0fa2c728dfbf957735da2843ec2b8a4f3425b105`（2026-08-27）、官方 `v1.4.0` release，以及 PiWork 当前 MCP、Capability Broker、Secret Store 和 Connector 实现。
方法：只使用 OOMOL/OpenConnector 官方 README、源码、配置、发布与 PiWork 本地源码；未修改产品代码。

## 结论

**不要把 1.5 万个 Action 当成 1.5 万个 PiWork 工具，也不要默认开放全部执行权限。**

对 PiWork 最合适的形态是：

1. **完整 catalog 可搜索**：保留上游完整 provider/action metadata，不做一份长期难以跟进上游的裁剪 fork。
2. **只启用少量 Action**：按 Workspace/Agent 签发独立 runtime token，用 `allowedActions` 和 `allowedConnections` 形成精确交集；上线时从只读 Action 开始。
3. **Agent 接入首选远程 HTTP MCP**：OpenConnector 在 `/mcp` 只暴露 5 个发现型工具，不会把整个 catalog 塞进模型上下文；PiWork 现有 `pi-mcp-adapter` 已支持 HTTP MCP。
4. **本地且不用 Docker 时，把 OpenConnector 作为受管 Node sidecar**：使用 PiWork 已绑定的 Node `v24.13.0`，按 OpenConnector 官方 Node runtime 启动，只监听 loopback，独立 SQLite 与 data directory，由 PiWork 管理进程生命周期。
5. **有副作用的写操作后续宜走 `/v1/actions/:actionId`**：HTTP Action 支持 `Idempotency-Key`，MCP `execute_action` 不支持这个幂等机制。MCP 可以先用来验证发现和只读调用，写入再收口到 PiWork 原生 HTTP adapter。

如果当前只想最快验证产品，不要本地 runtime，则用 **OOMOL 托管 runtime + MCP/Connector SDK** 工作量最小；代价是 provider 授权和调用边界位于第三方服务。[OpenConnector 中文 README](https://github.com/oomol-lab/open-connector/blob/0fa2c728dfbf957735da2843ec2b8a4f3425b105/docs/README.zh-CN.md) [SDK/CLI 说明](https://github.com/oomol-lab/open-connector/blob/0fa2c728dfbf957735da2843ec2b8a4f3425b105/docs/sdk-cli.md)

## 它是什么，不是什么

OpenConnector 是一个带状态的 connector gateway，不是一组可以直接嵌入 Rust 的无状态函数库。它负责：

- provider catalog 和 Action JSON Schema；
- credential/OAuth 的存储、验证和刷新；
- action executor 的调度、策略、运行日志和临时文件；
- HTTP、OpenAPI、MCP 和 Web Console 等多个入口。

`@oomol-lab/connector` 是调用该 gateway 的 TypeScript client，官方明确说它“不在本地运行 provider integration，也不管理 OAuth setup”。上游根 `package.json` 还标记为 `private: true`，因此当前没有一个“安装 npm 包后在 PiWork Rust 进程内直接调用 runtime”的稳定嵌入面。[SDK/CLI 说明](https://github.com/oomol-lab/open-connector/blob/0fa2c728dfbf957735da2843ec2b8a4f3425b105/docs/sdk-cli.md) [package.json](https://github.com/oomol-lab/open-connector/blob/0fa2c728dfbf957735da2843ec2b8a4f3425b105/package.json)

因此，“集成到 PiWork”应该理解为 **PiWork 管理一个 OpenConnector endpoint 与它的授权**，而不是把 1,444 个 provider executor 拆进 PiWork 主进程。

## 架构与加载模型

Node runtime 启动时读取生成的 `catalog/apps/*.json`，建立不可变的 provider/action 索引；Action schema 是 catalog 数据，executor 通过生成的 dynamic import registry 在首次执行某 provider 时才加载。完整 catalog 因此不等于启动时就 import 1,444 套 provider 代码。[catalog-store.ts](https://github.com/oomol-lab/open-connector/blob/0fa2c728dfbf957735da2843ec2b8a4f3425b105/src/catalog-store.ts) [provider-loader.ts](https://github.com/oomol-lab/open-connector/blob/0fa2c728dfbf957735da2843ec2b8a4f3425b105/src/providers/provider-loader.ts) [registry generator](https://github.com/oomol-lab/open-connector/blob/0fa2c728dfbf957735da2843ec2b8a4f3425b105/scripts/generate-provider-registry.ts) [Catalog 格式](https://github.com/oomol-lab/open-connector/blob/0fa2c728dfbf957735da2843ec2b8a4f3425b105/docs/catalog-format.md)

Node server 默认组装的核心边界是：

```text
PiWork / Browser / SDK
        │  MCP or HTTP + runtime token
        ▼
OpenConnector Node gateway (127.0.0.1)
        ├─ catalog/search/schema
        ├─ runtime/action/connection policy
        ├─ credential + OAuth boundary
        ├─ SQLite + run audit + idempotency
        └─ lazy provider executor → provider API
```

启动入口把所有 Node registry service 标记为 executable，默认 SQLite 文件是 `<data-dir>/connect.sqlite`，也可换 PostgreSQL。[Node server](https://github.com/oomol-lab/open-connector/blob/0fa2c728dfbf957735da2843ec2b8a4f3425b105/src/server/index.ts) [Runtime 配置](https://github.com/oomol-lab/open-connector/blob/0fa2c728dfbf957735da2843ec2b8a4f3425b105/docs/configuration.md)

## 当前 catalog 到底有多大

对上述 `main` commit 运行官方 `generate:catalog` 后得到：

| 项目 | 结果 |
| --- | ---: |
| Provider | 1,444 |
| Action | 15,151 |
| Node runtime 标记为 executable 的 Action | 15,151 |
| Cloudflare registry provider | 1,441 |
| Cloudflare 排除的 Node-only provider | `generic_imap`、`netease_mail`、`qq_mail` |

官方托管 catalog 的实时统计在本次查询时是 1,448 providers / 14,807 actions，说明托管目录与某个开源 commit 不应被假定为永久同步。产品中应保存 runtime/catalog version 和 action schema digest，不应把数量写死。[托管 catalog 统计](https://connector.oomol.com/v1/catalog)

上游提供了一层面向任务的 scenario，该 commit 的分布为：

| Scenario | Provider 数 |
| --- | ---: |
| Productivity | 330 |
| Data & Storage | 277 |
| Developer | 257 |
| Marketing | 179 |
| Communication | 165 |
| AI | 159 |
| Other | 34 |
| Cross-border ecommerce | 32 |
| Docs | 11 |

这层 scenario 是从 provider id、category 和关键字解析的发现分组，不是安全或可用性评级。[scenario resolver](https://github.com/oomol-lab/open-connector/blob/0fa2c728dfbf957735da2843ec2b8a4f3425b105/src/core/provider-scenarios.ts)

认证类型统计为：`api_key` 1,301、`oauth2` 103、`custom_credential` 75、`no_auth` 20。同一 provider 可声明多种 auth，因此数量不可相加为 provider 总数。Auth 枚举和字段定义见 [core/types.ts](https://github.com/oomol-lab/open-connector/blob/0fa2c728dfbf957735da2843ec2b8a4f3425b105/src/core/types.ts)。

## 怎样判定“我可以用的 connector”

一个 Action 只有同时满足下面条件才是对当前 PiWork Run **真正可用**：

```text
catalog 中存在
∩ locallyExecutable = true
∩ 已有可用的 provider connection（no_auth 除外）
∩ deployment/runtime/token 三层 Action policy 都允许
∩ runtime token 允许所选 connection id
∩ PiWork Run Capability Snapshot/Broker 允许本次 actionId + arguments
```

因此 UI 不应只有“已收录/未收录”，至少要分开显示：

- **Available in catalog**：可搜索，不代表可执行。
- **Executable by this runtime**：当前 runtime 有 executor。
- **Connected**：有已验证的账号/凭据。
- **Enabled for this Workspace/Agent**：在当前 token 和 PiWork binding 的交集中。
- **Approval required**：涉及发送、创建、修改、删除、发布或付款。

OpenConnector `ActionDefinition` 有 description、scope、provider permission 和 JSON Schema，**但没有统一的 `readOnly` / `destructive` 字段**。因此不能把 15,151 个 Action 自动全部分类后直接放行；PiWork 需要对首批 Action 做精确审核，并把有副作用的 Action 标为 Ask。[ActionDefinition](https://github.com/oomol-lab/open-connector/blob/0fa2c728dfbf957735da2843ec2b8a4f3425b105/src/core/types.ts)

## 是否应该“全部接入”

需要区分三种“全部”：

| 层次 | 建议 | 原因 |
| --- | --- | --- |
| 随 runtime 携带完整 catalog/executor 代码 | **可以** | 上游就是整体发布；provider executor 延迟加载；另做一个裁剪 fork 会提高升级和安全修复成本 |
| 让 Agent 发现完整 catalog | **可以，但默认优先“已连接 + 已授权”** | MCP 只有 5 个工具，搜索按需返回，不会把所有 schema 塞进 prompt |
| 让 Agent 执行全部 Action | **不可以** | 默认空 `allowedActions` 在上游语义中是不收窄；Action 包含大量写入/删除/发布/消息/财务操作，且无统一风险标签 |

选择性启用是上游的正式能力：

- deployment：`OOMOL_CONNECT_ALLOWED_ACTIONS` / `BLOCKED_ACTIONS`；
- runtime：持久化 `/api/runtime-policy`；
- caller：持久 runtime token 自己的 `allowedActions`、`blockedActions`、`allowedConnections`；
- proxy：独立 `ALLOWED_PROXIES` / `BLOCKED_PROXIES`，Action policy 不能限制 `/v1/proxy/:service`。

三层 Action policy 取交集，deny 优先。初期建议设置 `OOMOL_CONNECT_BLOCKED_PROXIES="*"`，不开放可绕过预置 Action 契约的 provider proxy。[Credential 与 Action Policy](https://github.com/oomol-lab/open-connector/blob/0fa2c728dfbf957735da2843ec2b8a4f3425b105/docs/credentials.md) [action-policy.ts](https://github.com/oomol-lab/open-connector/blob/0fa2c728dfbf957735da2843ec2b8a4f3425b105/src/core/action-policy.ts) [Runtime API](https://github.com/oomol-lab/open-connector/blob/0fa2c728dfbf957735da2843ec2b8a4f3425b105/docs/runtime-api.md)

## 候选 provider 如何收敛

下表不是默认安装清单，而是从 PiWork 当前的“本地 Windows Agent + 工作区 + 能力中心”形态出发的候选组；Action 数是上述 `main` commit 的生成 catalog 结果。

| 组 | 优先候选 | Auth / 当前 Action 数 | 建议 |
| --- | --- | --- | --- |
| 无凭据 smoke | Hacker News | no-auth / 14 | 先验证 catalog、MCP、policy、audit 和升级回滚，不涉及真实账号 |
| 代码与协作 | GitHub、GitLab、Gitea | OAuth/API key / 145、15、93 | GitHub API key 是最容易的有凭据试点；先开 list/get/search，create/update/merge/delete 收到 Ask |
| 项目与知识 | Notion、Linear、ClickUp、Asana、Airtable | OAuth/API key / 25、34、68、101、14 | 优先选能用 API key 的 provider，避免首轮同时解 OAuth app 发布问题 |
| 中国团队消息 | Feishu App Bot、WeCom Bot、DingTalk Bot | custom/API key / 330、54、5 | 连接较直接，但“发消息”是外部副作，必须预览 + Ask |
| Google / Slack / Feishu 个人账号 | Gmail、Google Calendar、Google Drive、Slack、Feishu | OAuth / 46、38、43、23、396 | 开源自托管需自己申请 OAuth client；要快速验证可先用 OOMOL 托管，或推迟到第二批 |
| 邮件协议 | IMAP Mailbox、QQ Mail、NetEase Mail | custom credential / 各 12 | 这三个是 Node-only；PiWork 已有原生 IMAP/SMTP connector，首期不应制造第二套邮件权限和审计路径 |

选择原则应是“用户当前有账号 + 当前 Work 真有需求 + 能用最小 scope/action 完成”，而不是按 catalog 热度全量开启。

## 部署选项：不用 Docker 也有三条路

| 方式 | 本地是否有 OpenConnector 进程 | OAuth app / 存储责任 | PiWork 工作量 | 判定 |
| --- | --- | --- | --- | --- |
| OOMOL 托管 | 无 | OOMOL 托管 | 最小；配远程 endpoint/token | **最快产品验证**，但非 local-first |
| Cloudflare Workers + D1/R2 | 无 | 自己管 OAuth app、Cloudflare 资源 | 中等 | 适合共享远程 runtime，不适合零部署桌面体验；三个 IMAP provider 不在 CF registry |
| 直接 Node 自托管 | **有，但无 Docker** | 自己管 OAuth app、SQLite/Postgres、加密 key | 中等 | **PiWork local-first 的推荐最终形态** |

官方 Node 方式是 `npm install && npm run dev`；构建 Web Console 后可 `npm run build:web && npm run start`。Node server 默认只监听 `127.0.0.1`。官方 Dockerfile 的 production stage 也只是 Node 24 + `src/catalog/dist/migrations/scripts` + production `node_modules`，因此 PiWork 可以在自己 CI 中按同一边界制作 Windows sidecar，而不需要在用户机器上安装 npm 或 Docker。[Quickstart](https://github.com/oomol-lab/open-connector/blob/0fa2c728dfbf957735da2843ec2b8a4f3425b105/docs/quickstart.md) [Dockerfile](https://github.com/oomol-lab/open-connector/blob/0fa2c728dfbf957735da2843ec2b8a4f3425b105/docker/Dockerfile) [Cloudflare 部署](https://github.com/oomol-lab/open-connector/blob/0fa2c728dfbf957735da2843ec2b8a4f3425b105/docs/cloudflare.md)

需要注意两个供应链/发布事实：

- 官方当前只发布 Linux amd64/arm64 Docker image，没有 Windows 二进制包；根 npm package 也是 `private` [Docker publish workflow](https://github.com/oomol-lab/open-connector/blob/0fa2c728dfbf957735da2843ec2b8a4f3425b105/.github/workflows/publish-docker.yml) [package.json](https://github.com/oomol-lab/open-connector/blob/0fa2c728dfbf957735da2843ec2b8a4f3425b105/package.json)。
- 上游 CI 只在 Ubuntu + Node 24 运行，没有 Windows matrix；PiWork 必须为自己打包的 sidecar 增加 Windows 10/11 contract 和进程树回收测试。[CI workflow](https://github.com/oomol-lab/open-connector/blob/0fa2c728dfbf957735da2843ec2b8a4f3425b105/.github/workflows/ci.yml)

官方当前最新稳定 release 是 [`v1.4.0`](https://github.com/oomol-lab/open-connector/releases/tag/v1.4.0)（2026-08-20）。正式集成应锁 tag/commit、npm lockfile、生成 catalog digest 和运行时文件清单，不跟随 `main` 或 `latest`。

## 暴露协议和适用场景

| 表面 | 上游 endpoint / 特性 | PiWork 用法 |
| --- | --- | --- |
| MCP | `POST /mcp`，无长连接 GET SSE；5 个工具：`list_apps`、`list_connections`、`search_actions`、`get_action_guide`、`execute_action` | 最快完成 Agent 发现/调用；使用 proxy tool，不将 Action 展开为直接工具 |
| HTTP | `/v1/providers`、`/v1/actions/*`、`/v1/apps/*`、`/v1/proxy/*` | 产品化 Rust adapter、幂等写操作、精确 audit 和错误映射 |
| OpenAPI | `/openapi.json`，支持 `?actionId=<id>` 生成单 Action 精确 schema | 可用于生成/校验选定 Action contract，不要一次导入整份大规模 spec |
| Action guide | `/api/actions/:actionId/agent.md` | 在执行前临时取得输入、scope、permission 和 connection 说明 |
| Web Console | `/` | 作为 PiWork 的过渡期账号连接/调试 UI；正式产品后再投影到 Capability Center |

来源：[Runtime API and MCP](https://github.com/oomol-lab/open-connector/blob/0fa2c728dfbf957735da2843ec2b8a4f3425b105/docs/runtime-api.md) [MCP 源码](https://github.com/oomol-lab/open-connector/blob/0fa2c728dfbf957735da2843ec2b8a4f3425b105/src/mcp.ts) [OpenAPI 源码](https://github.com/oomol-lab/open-connector/blob/0fa2c728dfbf957735da2843ec2b8a4f3425b105/src/server/api/openapi.ts)

## 凭据、OAuth 和 runtime 认证

OpenConnector 的两类 secret 不能混在一起：

1. **Provider credential**：API key、custom credential、OAuth access/refresh token，保存在 OpenConnector 选定的 SQLite/Postgres/D1。
2. **PiWork 调用 OpenConnector 的 runtime token**：用于 `/v1` 和 `/mcp`；持久 token 只存 hash，可带 Action、proxy 和 connection grant。

管理表面 `/api`、`/docs` 和 Web Console 另由 `OOMOL_CONNECT_ADMIN_TOKEN` 保护。Node runtime 还可做 JWT resource server，但它不映射 JWT claims 到 Action policy，首期没必要为本地桌面引入。[Runtime API 认证](https://github.com/oomol-lab/open-connector/blob/0fa2c728dfbf957735da2843ec2b8a4f3425b105/docs/runtime-api.md) [Configuration](https://github.com/oomol-lab/open-connector/blob/0fa2c728dfbf957735da2843ec2b8a4f3425b105/docs/configuration.md)

自托管 OAuth provider 必须由部署者自己申请 provider OAuth app，配置 client ID/secret 和精确 callback URL；这是 Gmail/Slack/Feishu 等 provider 无法“代码一打包就全部可用”的核心原因。OOMOL 托管路径的价值主要也在于代管这一层。OAuth 可把默认 scope 缩小为 provider 定义的子集，不认识的 scope 会被拒绝。[Credentials and OAuth](https://github.com/oomol-lab/open-connector/blob/0fa2c728dfbf957735da2843ec2b8a4f3425b105/docs/credentials.md)

如果未设 `OOMOL_CONNECT_ENCRYPTION_KEY`，上述 provider credential、OAuth client config/state 和幂等 Action 的完成响应会以明文存储。设置后使用 AES-256-GCM，key 本身不存在 OpenConnector 数据库。对 PiWork sidecar，正确做法是把这个 encryption key、admin token 和 runtime token 保存在 PiWork 现有 OS Secret Store，启动时短期注入；不把明文写入 PiWork SQLite、manifest 或日志。[Credential encryption](https://github.com/oomol-lab/open-connector/blob/0fa2c728dfbf957735da2843ec2b8a4f3425b105/docs/credentials.md) [PiWork 当前 Secret 边界](../architecture/piwork-current-agent-architecture.zh-CN.md)

## 与 PiWork 当前架构的接缝

PiWork 已有三个直接可复用的 seam：

- `pi-mcp-adapter@2.27.0` 已被绑定到 Pi sidecar，并且 adapter 本身支持 `url` HTTP MCP server；当前内置配置只注册 `piwork-verified` 和 `piwork-browser`，所以增加 OpenConnector 是扩展现有路径，不是新建第二个 MCP host。[当前 Pi MCP extension](../../src-tauri/binaries/pi-sidecar/builtin-extensions/pi-mcp/index.ts)
- `CapabilityBroker` / immutable Run Capability Snapshot 已经在 server/tool/arguments 层裁决 MCP call。OpenConnector 的 `execute_action` 是一个通用工具，所以 Broker 不能只允许工具名；必须再校验 `args.actionId`、`args.connectionName` 和 action input。[当前架构](../architecture/piwork-current-agent-architecture.zh-CN.md) [Pi MCP approval bridge](../../src-tauri/binaries/pi-sidecar/builtin-extensions/pi-mcp/index.ts)
- PiWork 已有 Capability Center Connections 与原生 email connector，包含 OS Secret Store、Workspace grant、发送预览/审批和审计。OpenConnector connection 应投影进这个 read model，不要长期保留一个完全不受 PiWork Broker 控制的平行连接中心。[当前 connector 实现](../../src-tauri/src/connectors/mod.rs)

最小 MCP server 配置的概念形态如下（不是本轮已实施代码）：

```ts
openconnector: {
  url: "http://127.0.0.1:<managed-port>/mcp",
  auth: "bearer",
  bearerTokenEnv: "PIWORK_OPEN_CONNECTOR_RUNTIME_TOKEN",
  lifecycle: "eager",
  protocolVersion: "auto",
  directTools: false,
  exposeResources: false,
  includeTools: [
    "list_apps",
    "list_connections",
    "search_actions",
    "get_action_guide",
    "execute_action",
  ],
}
```

PiWork 当前 MCP adapter 的 `config` 模式是隔离 snapshot，不会合并用户全局 `.mcp.json`；这与 PiWork 的 immutable Run snapshot 方向一致。但是现有 approval handler 对未识别 server 会 deny，正式实施时必须增加 `openconnector` 的 Broker 映射，不能只把 URL 加进 config。

## Windows 无 Docker 可行性实测

本次在 Windows 上使用 PiWork 已绑定的 `node.exe v24.13.0`、OpenConnector `main` commit `0fa2c72`进行了只读 smoke：

1. `npm ci --ignore-scripts` 和官方 `npm run generate:catalog` 成功；
2. 以 loopback、独立临时 data dir、admin/runtime/encryption token、`hackernews.*` allowlist 启动 `node src/server/index.ts` 成功；
3. 12 个 SQLite migration 自动完成；
4. `/v1/health` 和 `/mcp/tools` 成功；
5. `hackernews.get_top_stories` 经 HTTP 执行成功并产生 `executionId`与 audit；
6. 官方 MCP client 以 `auto` 协商为 modern protocol，列出 5 个工具并成功调用 `search_actions`。

这证明“PiWork 自带 Node + 受管 sidecar + SQLite + HTTP MCP”在当前开发机上是真实可运行的，但不替代 Windows 10/11 发布矩阵、安装包、升级/回滚、进程树回收、OAuth browser callback 和真实 provider 的 contract 测试。

## 建议的分阶段决策

### P0：一天内的可行性验证

- 使用当前 main 或更好的固定 `v1.4.0` 源码在开发机上启一个 Node runtime，不用 Docker。
- 只允许 `hackernews.*`，proxy 全部禁用。
- 用 PiWork 现有 MCP adapter 连接 `/mcp`，打通 discovery 和 Broker deny/allow audit。
- 不改 PiWork 连接 UI，先用 OpenConnector Web Console 调试。

### P1：第一个真实账号

- 选 GitHub PAT，只开经审核的 read action；不要直接 `github.*`。
- 由 PiWork OS Secret Store 保存 OpenConnector encryption/admin/runtime token；provider PAT 留在 OpenConnector 的加密 SQLite 边界内。
- 为测试 Workspace 发一个只允许这些 Action 和某一 connection id 的 persistent runtime token。
- 验证未授权 action、未授权 connection、缺失 credential、runtime 停止和 token revoke 都 fail closed。

### P2：PiWork 产品化

- 在 CI 中生成固定、可复现的 Windows OpenConnector sidecar bundle，不在用户机器上跑 `npm install`。
- 增加 `OpenConnectorRuntimeManager`：端口租约、health、start/stop/recover、完整进程树回收、data dir 版本和升级前备份。
- 把 provider/action/connection/health 投影到 Capability Center，不复制 provider credential 明文。
- MCP 继续用于发现和低风险调用；为需要重试的写 Action 增加 Rust HTTP adapter 和 `Idempotency-Key`。
- 建立 action review registry：`actionId -> read/write/publish/delete/financial + argument schema + approval mode`。未审核 Action 一律 deny。

## 不建议的做法

- **直接把 15,151 个 Action 生成 Pi 直接工具**：上下文、发现和权限都会失控，而上游已经用 5 个 MCP 工具解决这个问题。
- **只设 runtime token，不设 Action allowlist**：空 allowlist 是不收窄，不是全拒绝。
- **开放 `/v1/proxy/:service` 却以为 Action policy 会保护它**：两套 policy 独立。
- **把 OpenConnector SDK 当成 runtime**：SDK 只是 client，不保存 provider credential 或运行 executor。
- **用 `oo CLI` 做正式 PiWork 中间层**：它适合人工 CLI 工作流；PiWork 已有 MCP 和 HTTP seam，再加一个 CLI 进程、配置和认证层会降低可审计性。
- **首期用 OpenConnector 替换 PiWork 原生邮件 connector**：现有 IMAP/SMTP 路径已有 Workspace grant、预览、审批和审计；重复实现会形成两套权限权威。

## 最终建议

如果目标是“PiWork 用户不安装 Docker，但仍然 local-first”，选择：

> **完整 OpenConnector 作为一个固定版本的受管 Node sidecar，通过 loopback HTTP MCP 接入 PiWork；保留完整 catalog，但仅对经审核的 Action 和 connection 签发 runtime token，同时由 PiWork Broker 在 `execute_action.args.actionId` 与 arguments 层再裁决。**

这条路既不要求 Docker，也不需要为 1,444 个 provider 维护 PiWork fork，还能复用 PiWork 已经落地的 MCP、Capability Snapshot、Broker、Secret Store、Connections 和 audit 边界。
