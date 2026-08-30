# Pi 权限握手可行性（#20）

日期：2026-08-30
范围：捆绑 sidecar `@earendil-works/pi-coding-agent` 0.80.2（`src-tauri/binaries/pi-sidecar/dist/piwork-pi.js`），对照当前 `PiEngineAdapter` / `PiRunArguments` 生产启动参数。
方法：在 Linux 上直接调用 JS sidecar（生产 Rust spawn 在 Linux 上 fail-closed）；用独立 spike 脚本复现，不改生产权限语义。Parent：#16。
复现：`node scripts/spikes/pi-permission-handshake/run.mjs`

## 结论

**否。Pi 原生 RPC 不会在 `read` / `edit` / `write` / `bash` 执行前发出权限请求，外部也无法用一条 Allow/Deny RPC 命令拦截这些内置工具。**

更具体：

1. 捆绑 sidecar 没有 `--permission*` CLI 开关，也没有 `permission_request` / `permission_response` / `approve` / `deny` / `allow` / `ask` 命令。这些 stdin 命令一律返回 `Unknown command`。
2. `--approve` / `--no-approve` **不是**工具自动放行开关。Help 原文是 “Trust project-local files for this run” / “Ignore project-local files for this run”。它只影响是否信任项目本地文件（例如 `.pi` 下的 extension / skill），与 CapabilityBroker、Permission Mode、Run Capability Snapshot 无关。
3. 无拦截 extension 时，内置工具的 RPC 序列是 `tool_execution_start` → 立即执行 → `tool_execution_end`。Host 不能通过回复 `tool_execution_start` 来暂停或拒绝。本次 spike 中 `read` / `edit` / `write` / `bash` 都在无任何 UI/权限请求的情况下真正改了 Workspace 文件或跑了命令。
4. 生产 `PiEngineAdapter` 仍声明 `permission_requests: false`，启动仍带 `--approve` 且不带权限握手开关。这与 sidecar 能力一致，不是文档误读。Linux 上 Rust 适配器继续 fail-closed，本 spike 因此直接调用 JS sidecar。

因此 #21 不能假设“去掉 `--approve`、打开 `permission_requests`”就能接通 Balanced / AutoExecute。那条开关在捆绑 sidecar 里不存在。

## 可复现调用

生产 `PiRunArguments` 在 Windows/macOS 上等价于：

```text
<node> src-tauri/binaries/pi-sidecar/dist/piwork-pi.js
  --mode rpc
  --provider piwork
  --model <model_id>
  --session-dir <sessions>
  --session-id <id>
  --tools read,grep,find,ls,edit,write,bash   # AskEveryStep 只有前四个
  --no-extensions
  --no-skills
  --no-prompt-templates
  --no-themes
  --approve
```

环境变量：`PI_CODING_AGENT_DIR`（写入 `models.json`）、`PIWORK_MODEL_API_KEY`。

本 spike 为了不依赖 live credentials，把 `--provider/--model` 换成 spike 扩展注册的 mock provider，并显式 `--extension` 加载 spike 文件。其余开关与生产一致，包括 `--approve` 与 `--no-extensions`。完整 argv 见 `scripts/spikes/pi-permission-handshake/recorded/latest.json`。

从仓库根目录：

```bash
node scripts/spikes/pi-permission-handshake/run.mjs
```

要求：Node >= 22；捆绑 `piwork-pi.js` 在树内。不要求模型 API Key，不启动 Tauri，不跑 `cargo test`。

## 观察到的原生消息

权限类命令（stdin JSONL）全部失败，形态如下：

```json
{"id":"inventory-1","type":"response","command":"permission_request","success":false,"error":"Unknown command: permission_request"}
```

无 gate 时，`write` 在执行前没有请求，Host 无需回复：

```json
{"type":"tool_execution_start","toolCallId":"spike-write-2","toolName":"write","args":{"path":"created.txt","content":"SPIKE_WRITE_OK\n"}}
{"type":"tool_execution_end","toolCallId":"spike-write-2","toolName":"write","isError":false,"result":{"content":[{"type":"text","text":"Successfully wrote 15 bytes to created.txt"}]}}
```

`read` / `edit` / `bash` 同构：`toolCallId`、`toolName`、`args`，然后直接 `tool_execution_end`。Workspace 结果：`probe.txt` 被改成 `PROBE_EDITED`，`created.txt` 被写成 `SPIKE_WRITE_OK`，bash 输出 `SPIKE_BASH_OK`。

`tool_execution_start` 是通知，不是握手。没有 request id 需要 Host 回答，也没有 Allow/Deny 载荷。

## 若后续仍要做执行前拦截：相关但非一等契约

sidecar **可以**在加载自定义 extension 后，于真正 `execute` 前停下并让外部 Allow/Deny。这不是 Pi 发出的权限协议，而是两条已有机制的组合：

1. 进程内 `tool_call` 钩子（在 `tool_execution_start` **之后**、`execute` **之前**）。返回 `{ "block": true, "reason": "..." }` 会阻止执行，工具结果变为 error。
2. RPC 通用对话框：`ctx.ui.confirm()` 向 stdout 发 `extension_ui_request`，并阻塞到 stdin 的 `extension_ui_response`。超时/中止时 confirm 的默认值是 `false`（拒绝）。

本次用 spike-only `permission-gate.ts` 观察到的最小往返：

请求（stdout）：

```json
{
  "type": "extension_ui_request",
  "id": "<uuid>",
  "method": "confirm",
  "title": "Allow write?",
  "message": "{\"toolCallId\":\"spike-write-1\",\"toolName\":\"write\",\"input\":{\"path\":\"created.txt\",\"content\":\"SPIKE_WRITE_OK\\n\"}}"
}
```

响应（stdin）：

```json
{"type":"extension_ui_response","id":"<uuid>","confirmed":true}
```

或 `{"type":"extension_ui_response","id":"...","confirmed":false}` / `{"cancelled":true}`。

Allow 后 `write` 落到磁盘；Deny 后 `tool_execution_end.isError === true`，文案为 `spike-denied:write:spike-write-1`，`created.txt` 不存在。

字段对应：

| 需要的握手字段 | 原生 RPC | extension UI + `tool_call` |
| --- | --- | --- |
| 请求标识 | 无 | `extension_ui_request.id`（UUID） |
| 操作/工具 | 仅出现在事后的 `tool_execution_start.toolName` | 钩子里的 `toolName`；默认不在 UI 请求顶层，需 extension 自行写入 `title`/`message` |
| 参数 | 仅 `tool_execution_start.args` | 钩子里的 `input`；同样要 extension 带出去 |
| Allow/Deny | 无 | `confirmed: true/false`；Deny 再 `{ block: true, reason }` |

注意：`tool_execution_start` 会在 UI 请求之前发出。Host 若只订阅该事件，会误以为已经执行。

这条路径要求 Run 显式 `--extension` 加载拦截器。生产当前 `--no-extensions`，且不会加载本 spike 的 gate。实现拦截属于后续 spec，不在 #20。

## 给 #21 的替代路径

因为原生握手不存在，后续只能在下列路线里选，而不能“打开 permission_requests”：

1. **前置围栏（已部分存在）**：继续用 `--tools` 收窄 allowlist。`AskEveryStep` 只给 `read,grep,find,ls`；`Balanced` 与 `AutoExecute` 仍给全部 7 个内置工具。这限制“能不能看见工具”，不提供逐次 Ask。
2. **Host Tool 中介高风险工具**：把 `edit` / `write` / `bash` 从 Pi 内置 allowlist 拿掉，改成 Host Tool，走现有 CapabilityBroker + 不可变 Run Capability Snapshot。`read` 等只读工具可留在 sidecar。
3. **自写拦截 extension**：在 `tool_call` 里询问 Host（`extension_ui_request` 或像 Host Tool 那样打回环 HTTP），再 `{ block }`。这才能让 Permission Mode 的 Allow/Deny/Ask 碰到内置工具。消息契约是通用 UI confirm，或我们自己的 Host 协议，不是 Pi 一等 permission schema。
4. **组合**：只读工具前置围栏 + 写/进程走 Host Tool 或拦截 extension。

本笔记不推荐其中哪一条；#21 负责路线建议。置信度：对“原生有没有握手”为高——已对捆绑 sidecar 实跑，并写进可重复脚本。

## 词汇与边界

本结论使用项目词汇：Work、Assignment、Run、Agent Instance、Permission Mode、Capability Pack、Run Capability Snapshot、Host Tool。

未改：生产 `--approve`、`permission_requests`、Balanced / AutoExecute 语义、policy、schema、审批 UI、MCP / Browser / LSP。普通 `cargo test` / `pnpm test` 仍不启动 sidecar，也不需要 live credentials。
