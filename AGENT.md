# AGENT.md

本文件为 AI 编码助手（Agent）提供在 PiWork 仓库中工作所需的上下文，帮助其快速理解项目结构、开发流程与约定。

## 项目概述

**PiWork (代号 CoDo)** 是一个 local-first 的桌面 AI 助手应用，用于指挥持久化的 Agent 完成跨专业领域的耐久工作（Work）。每个 Work 在本地 SQLite 中保存目标、工作区、多次 Run 与事件历史，支持跨会话继续协作。执行引擎（engine）被隔藏在 CoDo 自有的引擎接口之后，可替换。

- 当前仓库是基础垂直切片（foundation vertical slice），使用确定性的 fake engine 演练流式输出、工具事件、完成、多 Run 历史、持久化与中断恢复。真实的 pi 二进制、模型 provider、凭据流程、权限桥接尚未在此阶段集成/打包。
- 首个支持的桌面目标平台是 **Windows 10/11 x64**。

核心术语（Work / Assignment / Run / Agent Definition / Agent Instance / Result Envelope 等）的完整定义见 [`CONTEXT.md`](CONTEXT.md)，修改相关代码前建议先读一遍，避免概念混用。

## 技术栈

- **前端**：React 19 + TypeScript（严格模式）+ Vite 7 + Tailwind CSS 4 + Zustand 5 + i18next
- **桌面壳层**：Tauri 2（Rust）
- **后端/本地服务**：Rust（edition 2024）+ Tokio + sqlx（SQLite）+ tauri-plugin-*
- **测试**：Vitest（前端，jsdom）+ Testing Library；`cargo test`（Rust）
- **包管理**：pnpm（`packageManager: pnpm@9.12.0`），**禁止使用 npm/yarn**

## 目录结构

```
src/                    前端源码（React + TS）
├─ app/                 应用入口、Tauri client、App 组件
├─ domain/               前端领域模型（如 work.ts）
├─ features/             按功能域拆分（activity, agent-center, connectors,
│                        dashboard, extensions, model-setup, notifications,
│                        settings, works, workspace）
├─ components/           可复用 UI 组件（brand, motion）
├─ bindings/             与 Tauri/Rust 侧的类型绑定
├─ i18n/                 国际化资源
├─ motion/ styles/       动画与样式
└─ test/                 测试工具

src-tauri/               Rust 后端（Tauri 2）
└─ src/
   ├─ agent/ assignment/ capability/ collaboration/
   ├─ connectors/ delivery/ document_runtime/ domain/
   ├─ engine/ (含 engine/pi)  environment/ execution/
   ├─ extensions/ memory/ model/ resource/ storage/
   ├─ work/ workspace/

docs/
├─ adr/                  架构决策记录（ADR）
├─ architecture/         架构说明文档（中文为主）
└─ superpowers/specs/    产品与架构设计文档（含中英文）

scripts/                 构建/打包脚本（PowerShell 为主，Windows 优先）
```

## 常用命令

```bash
pnpm install                # 安装依赖（务必用 pnpm，不要用 npm/yarn）
pnpm dev                    # 仅启动 Vite 前端开发服务器
pnpm tauri dev               # 启动完整桌面应用（含热重载）
pnpm build                   # tsc -b && vite build
pnpm typecheck                # tsc -b --pretty false
pnpm test                    # vitest run（前端单测）
pnpm test:watch              # vitest watch 模式
pnpm cargo:test               # cargo test（Rust 侧单测）
```

Rust 侧静态检查（提交前建议执行）：

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

打包（调试版 Windows NSIS 安装包）：

```bash
pnpm build
pnpm tauri build --debug --bundles nsis
```

## 编码约定

- **TypeScript 严格类型安全**：禁止使用 `any`，所有类型都要明确定义；`tsconfig.json` 已开启 `strict` 与 `noUncheckedIndexedAccess`。
- **组件拆分**：单个组件不超过 400 行，复杂组件要拆分。
- **样式统一**：优先使用 Tailwind CSS，避免自定义 CSS。
- **按功能域组织前端代码**：新功能优先放入 `src/features/<feature>/`，跨功能复用的组件放 `src/components/`。
- **Rust 侧按领域模块组织**：`agent` `assignment` `work` `workspace` `capability` 等模块与前端 `CONTEXT.md` 术语一一对应，新增能力时先确认应归属哪个既有领域模块。
- 测试文件与实现文件同目录、同名加 `.test.ts(x)` 后缀（如 `workStore.ts` / `workStore.test.ts`）。
- 依赖安装统一用 **pnpm**，不要用 npm。

## 关键文档索引

- 术语与产品边界：[`CONTEXT.md`](CONTEXT.md)
- 项目说明与前置要求：[`README.md`](README.md)
- 架构设计（中/英）：`docs/superpowers/specs/2026-07-28-piwork-design*.md`
- 当前 Agent 架构：`docs/architecture/piwork-current-agent-architecture.zh-CN.md`
- 核心控制面接口：`docs/architecture/piwork-core-control-plane-interfaces.zh-CN.md`
- 能力平台/市场实施计划：`docs/architecture/piwork-capability-platform-market-implementation-plan.zh-CN.md`
- ADR（架构决策记录）：`docs/adr/000*.md`
- 第三方归属与许可：[`NOTICE`](NOTICE)、[`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md)

## 注意事项

- 修改涉及 Work / Assignment / Run / Agent Definition / Agent Instance 等核心概念的代码前，务必先对照 `CONTEXT.md` 确认术语用法是否一致。
- 引擎（engine）当前是可替换的 fake engine，真实 pi 引擎集成、模型 provider、凭据与权限桥接尚未完成，涉及这些模块时注意不要假设生产级实现已存在。
- `src-tauri/binaries/pi-sidecar` 下包含打包的第三方 sidecar，一般无需手动编辑。内置扩展 `pi-web-access` 的 `node_modules` **不入库**，由 `scripts/bundle-pi-web-access.sh`（macOS）或 `scripts/bundle-pi-web-access.ps1`（Windows）在 `tauri` dev/build 前生成；也可手动执行 `pnpm bundle:pi-web-access`（Windows）或 `bash scripts/bundle-pi-web-access.sh`。
- Windows 是首要目标平台，构建脚本以 PowerShell 为主；在其他平台开发时注意跨平台差异。
