# PiWork 桌面基础纵向切片实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 构建一个可在 Windows 运行的 Tauri 2 + React PiWork 基础应用，具备 Continuous Loop 品牌、双语界面、SQLite 持久化、类型化 IPC、持久 Work/Run 状态机，以及可驱动完整 UI 时间线的 fake engine 纵向闭环。

**Architecture:** React 只通过类型化 Tauri command/event 与 Rust 通信。Rust 的 WorkService、EventJournal、SQLite repository 和 EngineAdapter 组成最小产品核心；FakeEngineAdapter 先替代 pi，验证“创建 Work → 执行 Run → 流式事件 → 结构化完成 → 重启恢复 → 继续产生新 Run”的完整链路。后续 pi 接入只替换 Adapter，不改产品领域模型。

**Tech Stack:** Tauri 2、Rust 2024、Tokio、SQLx/SQLite、ts-rs、React 19、TypeScript 5、Vite 7、Tailwind CSS 4、Zustand、i18next、Vitest、Testing Library、pnpm。

---

## 计划边界与后续计划

本规格覆盖四个独立子系统，不能在空仓库中用一份巨型计划可靠实施。本计划是四份执行计划中的第一份：

1. **当前计划：桌面基础 + fake engine 纵向闭环。**
2. 模型引导、Windows Credential Manager 与真实 pi RPC sidecar。
3. 权限桥、持续协作、diff/产物与中断恢复。
4. 并发调度、托盘通知、诊断、NSIS 与干净机器验收。

当前计划不打包真实 pi、不访问真实模型 Provider，也不实现系统级权限拦截。它必须产出可运行软件和稳定 Adapter 契约，为第二份计划提供真实代码基础。

## 文件结构

### 根目录与前端

- `package.json`：pnpm 脚本、前端依赖和 Tauri CLI 入口。
- `pnpm-lock.yaml`：由 pnpm 生成并提交的依赖锁。
- `index.html`：Vite HTML 入口。
- `tsconfig.json`：浏览器 TypeScript 配置。
- `tsconfig.node.json`：Vite/Vitest 配置文件的 TypeScript 配置。
- `vite.config.ts`：Vite、React、Tailwind 与 Vitest 设置。
- `vitest.setup.ts`：Testing Library DOM matcher 与全局清理。
- `assets/piwork-icon.svg`：Continuous Loop 源图标。
- `src/main.tsx`：React 启动入口。
- `src/app/App.tsx`：应用顶层壳和首次数据加载。
- `src/app/App.test.tsx`：应用壳行为测试。
- `src/app/tauriClient.ts`：唯一允许直接导入 `@tauri-apps/api` 的前端模块。
- `src/bindings/*.ts`：由 Rust `ts-rs` 导出的 DTO。
- `src/bindings/index.ts`：手工维护的 binding 类型出口；业务代码不逐个导入生成文件。
- `src/domain/work.ts`：前端 Work/Run 派生类型与显示规则。
- `src/i18n/index.ts`：i18next 初始化与系统语言解析。
- `src/i18n/locales/en.json`：英文文案。
- `src/i18n/locales/zh-CN.json`：简体中文文案。
- `src/styles/tokens.css`：Violet Loop 设计 token。
- `src/styles/globals.css`：全局与 Tailwind 样式入口。
- `src/components/brand/ContinuousLoopLogo.tsx`：品牌 Logo 组件。
- `src/features/works/workStore.ts`：Zustand Work/Run/event 状态。
- `src/features/works/workStore.test.ts`：store reducer 与 event replay 测试。
- `src/features/works/WorkStoreProvider.tsx`：把可注入 client 创建的 Zustand vanilla store 提供给 React。
- `src/features/works/useWorkEvents.ts`：Tauri event 订阅生命周期。
- `src/features/works/WorkSidebar.tsx`：左侧 Work 列表。
- `src/features/workspace/WorkHeader.tsx`：目标、路径与模型占位展示。
- `src/features/workspace/WorkTimeline.tsx`：消息和 engine event 时间线。
- `src/features/workspace/WorkComposer.tsx`：创建/继续 Run 的输入区。
- `src/features/workspace/WorkInspector.tsx`：计划、变更、产物与日志占位标签。
- `src/features/workspace/WorkSurface.tsx`：三栏主工作台。
- `src/features/workspace/WorkSurface.test.tsx`：纵向交互组件测试。
- `src/test/mockTauriClient.ts`：前端组件测试使用的确定性 IPC/event fake。

### Tauri / Rust

- `src-tauri/Cargo.toml`：Rust 依赖和 crate 配置。
- `src-tauri/build.rs`：Tauri build hook。
- `src-tauri/tauri.conf.json`：Windows 窗口、bundle 与安全配置。
- `src-tauri/capabilities/default.json`：最小 Tauri capability。
- `src-tauri/src/main.rs`：桌面二进制入口。
- `src-tauri/src/lib.rs`：应用装配、plugin 与 command 注册。
- `src-tauri/src/error.rs`：可序列化 AppError。
- `src-tauri/src/paths.rs`：唯一的 PiWork 路径解析服务。
- `src-tauri/src/app_state.rs`：Tauri managed state。
- `src-tauri/src/domain/mod.rs`：领域类型模块出口。
- `src-tauri/src/domain/work.rs`：Work、Run、状态和 DTO。
- `src-tauri/src/domain/event.rs`：版本化 WorkEvent envelope。
- `src-tauri/src/storage/mod.rs`：存储模块出口。
- `src-tauri/src/storage/sqlite.rs`：SQLite pool、migration 与事务入口。
- `src-tauri/migrations/0001_foundation.sql`：第一版 schema。
- `src-tauri/src/work/mod.rs`：Work 子系统出口。
- `src-tauri/src/work/repository.rs`：Work/Run/event SQL repository。
- `src-tauri/src/work/state_machine.rs`：纯状态转换规则。
- `src-tauri/src/work/service.rs`：创建、启动、完成与继续 Work。
- `src-tauri/src/work/commands.rs`：Tauri commands。
- `src-tauri/src/engine/mod.rs`：EngineAdapter 契约与 engine 类型。
- `src-tauri/src/engine/fake.rs`：确定性 fake engine。
- `src-tauri/src/engine/publisher.rs`：可替换的产品事件发布端口；生产 emit Tauri event，测试使用 channel。
- `src-tauri/src/engine/supervisor.rs`：Run task、event journal 与 UI emit。
- `src-tauri/tests/storage_contract.rs`：迁移和重启持久化测试。
- `src-tauri/tests/work_lifecycle.rs`：Work/Run 纵向集成测试。

## Task 1：建立可测试的 Tauri + React 壳

**Files:**
- Create: `package.json`
- Create: `index.html`
- Create: `tsconfig.json`
- Create: `tsconfig.node.json`
- Create: `vite.config.ts`
- Create: `vitest.setup.ts`
- Create: `src/main.tsx`
- Create: `src/app/App.test.tsx`
- Create: `src/app/App.tsx`
- Create: `src/styles/globals.css`
- Create: `src-tauri/Cargo.toml`
- Create: `src-tauri/build.rs`
- Create: `src-tauri/tauri.conf.json`
- Create: `src-tauri/capabilities/default.json`
- Create: `src-tauri/src/main.rs`
- Create: `src-tauri/src/lib.rs`

- [ ] **Step 1：写前端失败测试**

创建 `src/app/App.test.tsx`：

```tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { App } from "./App";

describe("App", () => {
  it("renders the PiWork product shell", () => {
    render(<App />);
    expect(screen.getByRole("heading", { name: "PiWork" })).toBeInTheDocument();
  });
});
```

- [ ] **Step 2：创建包清单与测试配置**

创建 `package.json`：

```json
{
  "name": "piwork",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc -b && vite build",
    "typecheck": "tsc -b --pretty false",
    "test": "vitest run",
    "test:watch": "vitest",
    "tauri": "tauri",
    "cargo:test": "cargo test --manifest-path src-tauri/Cargo.toml"
  },
  "dependencies": {
    "@tauri-apps/api": "^2.8.0",
    "i18next": "^25.0.0",
    "lucide-react": "^0.468.0",
    "react": "^19.0.0",
    "react-dom": "^19.0.0",
    "react-i18next": "^16.0.0",
    "zustand": "^5.0.0"
  },
  "devDependencies": {
    "@tailwindcss/vite": "^4.0.0",
    "@tauri-apps/cli": "^2.8.0",
    "@testing-library/jest-dom": "^6.6.0",
    "@testing-library/react": "^16.1.0",
    "@testing-library/user-event": "^14.6.0",
    "@types/react": "^19.0.0",
    "@types/react-dom": "^19.0.0",
    "@vitejs/plugin-react": "^4.3.0",
    "jsdom": "^26.0.0",
    "tailwindcss": "^4.0.0",
    "typescript": "^5.7.0",
    "vite": "^7.0.0",
    "vitest": "^3.0.0"
  },
  "packageManager": "pnpm@9.12.0"
}
```

创建 `vite.config.ts`：

```ts
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  envPrefix: ["VITE_", "TAURI_"],
  test: {
    environment: "jsdom",
    setupFiles: ["./vitest.setup.ts"],
    restoreMocks: true
  }
});
```

创建 `vitest.setup.ts`：

```ts
import "@testing-library/jest-dom/vitest";
```

创建 `tsconfig.json`：

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "useDefineForClassFields": true,
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "allowJs": false,
    "skipLibCheck": true,
    "esModuleInterop": true,
    "allowSyntheticDefaultImports": true,
    "strict": true,
    "forceConsistentCasingInFileNames": true,
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx",
    "noUncheckedIndexedAccess": true,
    "types": ["vitest/globals"]
  },
  "include": ["src", "vitest.setup.ts"],
  "references": [{ "path": "./tsconfig.node.json" }]
}
```

创建 `tsconfig.node.json`：

```json
{
  "compilerOptions": {
    "composite": true,
    "skipLibCheck": true,
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "noEmit": true
  },
  "include": ["vite.config.ts"]
}
```

- [ ] **Step 3：安装依赖并验证测试失败**

Run:

```powershell
pnpm install
pnpm test -- src/app/App.test.tsx
```

Expected: FAIL，错误指向 `./App` 不存在。

- [ ] **Step 4：实现最小 React 壳**

创建 `src/app/App.tsx`：

```tsx
export function App() {
  return (
    <main className="min-h-screen bg-neutral-50 text-neutral-950">
      <h1>PiWork</h1>
    </main>
  );
}
```

创建 `src/main.tsx`：

```tsx
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./app/App";
import "./styles/globals.css";

const root = document.getElementById("root");
if (!root) throw new Error("Missing #root mount point");

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>
);
```

创建 `src/styles/globals.css`：

```css
@import "tailwindcss";

html,
body,
#root {
  min-height: 100%;
  margin: 0;
}

body {
  font-family: Inter, "Segoe UI Variable", "Segoe UI", sans-serif;
}
```

创建 `index.html`：

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <meta name="theme-color" content="#6b50ef" />
    <title>PiWork</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

- [ ] **Step 5：实现最小 Tauri 壳**

创建 `src-tauri/Cargo.toml`：

```toml
[package]
name = "piwork"
version = "0.1.0"
description = "Local-first AI work desktop application"
edition = "2024"
publish = false

[lib]
name = "piwork_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
async-trait = "0.1"
chrono = { version = "0.4", features = ["serde"] }
dunce = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite", "migrate", "chrono", "uuid"] }
tauri = { version = "2", features = [] }
thiserror = "2"
tokio = { version = "1", features = ["macros", "rt-multi-thread", "sync", "time"] }
ts-rs = { version = "11", features = ["chrono-impl", "uuid-impl"] }
uuid = { version = "1", features = ["v4", "serde"] }

[dev-dependencies]
tempfile = "3"
```

创建 `src-tauri/build.rs`：

```rust
fn main() {
    tauri_build::build();
}
```

创建 `src-tauri/src/lib.rs`：

```rust
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("failed to run PiWork");
}
```

创建 `src-tauri/src/main.rs`：

```rust
fn main() {
    piwork_lib::run();
}
```

创建 `src-tauri/tauri.conf.json`：

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "PiWork",
  "version": "0.1.0",
  "identifier": "dev.piwork.desktop",
  "build": {
    "beforeDevCommand": "pnpm dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "pnpm build",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [
      {
        "label": "main",
        "title": "PiWork",
        "width": 1232,
        "height": 800,
        "minWidth": 900,
        "minHeight": 640,
        "resizable": true,
        "decorations": true
      }
    ],
    "security": {
      "csp": "default-src 'self'; img-src 'self' asset: data:; style-src 'self' 'unsafe-inline'; connect-src 'self' ipc: http://ipc.localhost http://localhost:1420 ws://localhost:1420"
    }
  },
  "bundle": {
    "active": true,
    "targets": ["nsis"],
    "category": "Productivity",
    "shortDescription": "Local-first AI work desktop application"
  }
}
```

创建 `src-tauri/capabilities/default.json`：

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "PiWork main window capability",
  "windows": ["main"],
  "permissions": ["core:default"]
}
```

- [ ] **Step 6：运行基础验证**

Run:

```powershell
pnpm test -- src/app/App.test.tsx
pnpm typecheck
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: 三条命令均退出 0；测试 1 passed。

- [ ] **Step 7：提交**

```powershell
git add package.json pnpm-lock.yaml index.html tsconfig*.json vite.config.ts vitest.setup.ts src src-tauri
git commit -m "build: bootstrap PiWork Tauri application"
```

## Task 2：实现 Continuous Loop 品牌与双语壳

**Files:**
- Create: `assets/piwork-icon.svg`
- Create: `src/styles/tokens.css`
- Modify: `src/styles/globals.css`
- Create: `src/components/brand/ContinuousLoopLogo.tsx`
- Create: `src/i18n/index.ts`
- Create: `src/i18n/locales/en.json`
- Create: `src/i18n/locales/zh-CN.json`
- Modify: `src/main.tsx`
- Modify: `src/app/App.tsx`
- Modify: `src/app/App.test.tsx`

- [ ] **Step 1：写语言与品牌失败测试**

在 `App.test.tsx` 增加：

```tsx
import { i18n } from "../i18n";

it("renders the localized product shell", async () => {
  await i18n.changeLanguage("zh-CN");
  render(<App />);
  expect(await screen.findByRole("button", { name: "新建 Work" })).toBeInTheDocument();
  expect(screen.getByTestId("continuous-loop-logo")).toBeInTheDocument();
});
```

- [ ] **Step 2：运行测试确认失败**

Run: `pnpm test -- src/app/App.test.tsx`  
Expected: FAIL，找不到“新建 Work”与 Logo test id。

- [ ] **Step 3：实现 i18n**

`src/i18n/index.ts` 以 named export `i18n` 导出已初始化的 i18next instance，并按 `zh-* → zh-CN`、其他语言 → `en` 解析。测试环境必须允许重复初始化而不抛错。测试文件从 `../i18n` 导入该 instance 后再切换语言。

两个 locale JSON 至少包含：

```json
{
  "app": { "name": "PiWork" },
  "work": { "new": "新建 Work", "empty": "创建第一个 Work" },
  "status": { "draft": "草稿", "running": "运行中", "completed": "已完成" }
}
```

英文文件使用相同 key，值分别为 `New Work`、`Create your first Work`、`Draft`、`Running`、`Completed`。

- [ ] **Step 4：实现品牌 token 与 Logo**

`tokens.css` 定义 `--pw-accent-500: #6b50ef`、`--pw-accent-600: #5940dc`、中性表面、边框、文本、成功/等待/失败色、圆角和阴影。

`ContinuousLoopLogo.tsx` 使用内联 SVG，固定 `viewBox="0 0 70 70"`，包含已批准的三条连续 π 曲线路径，并允许 `size` 与 `showWordmark` props。根元素带 `data-testid="continuous-loop-logo"` 和可访问名称 `PiWork`。

- [ ] **Step 5：接入应用入口并验证**

`main.tsx` 在渲染前导入 `../src/i18n`；`App.tsx` 使用 `useTranslation()` 渲染 Logo、PiWork 和“新建 Work”按钮。

Run:

```powershell
pnpm test -- src/app/App.test.tsx
pnpm typecheck
```

Expected: 全部 PASS。

- [ ] **Step 6：提交**

```powershell
git add assets src
git commit -m "feat: add PiWork branding and localization"
```

## Task 3：建立 SQLite 与路径基础

**Files:**
- Create: `src-tauri/src/error.rs`
- Create: `src-tauri/src/paths.rs`
- Create: `src-tauri/src/storage/mod.rs`
- Create: `src-tauri/src/storage/sqlite.rs`
- Create: `src-tauri/migrations/0001_foundation.sql`
- Create: `src-tauri/tests/storage_contract.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1：写迁移失败测试**

`storage_contract.rs`：

```rust
use piwork_lib::storage::sqlite::Database;

#[tokio::test]
async fn migration_creates_foundation_tables() {
    let database = Database::open_in_memory().await.unwrap();
    let names = database.table_names().await.unwrap();

    for expected in ["works", "runs", "messages", "events", "settings"] {
        assert!(names.contains(&expected.to_string()), "missing {expected}");
    }
}
```

- [ ] **Step 2：运行测试确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test storage_contract`  
Expected: FAIL，`storage` module 不存在。

- [ ] **Step 3：实现第一版 migration**

`0001_foundation.sql` 必须：

- 开启 foreign key；
- 创建 `works`、`runs`、`messages`、`events`、`settings`；
- 使用 TEXT UUID 主键；
- 对 `runs.work_id`、`messages.work_id`、`events.run_id + sequence` 建索引；
- 用 CHECK 约束状态字符串；
- 为 events 建立 `(run_id, sequence)` UNIQUE 约束。

Work 状态允许：`draft|queued|running|waiting|idle|completed|failed|stopped|interrupted|archived`。Run 状态允许：`queued|running|waiting|completed|failed|stopped|interrupted`。

- [ ] **Step 4：实现 Database 与 AppPaths**

`Database` 包装 `sqlx::SqlitePool`，提供 `open(path)`、`open_in_memory()`、`pool()` 和仅供测试的 `table_names()`。连接设置使用 WAL、foreign keys 和 busy timeout；in-memory pool 的 `max_connections` 必须为 1。

`AppPaths` 只通过构造函数接收 roaming/local 根目录，生产构造使用 Tauri `PathResolver`。它提供 `database_path()`、`engine_sessions_dir()`、`logs_dir()`、`runtime_dir()`、`backups_dir()`，所有返回值都必须位于传入根目录之下。

- [ ] **Step 5：运行存储测试**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test storage_contract
cargo test --manifest-path src-tauri/Cargo.toml paths
```

Expected: PASS。

- [ ] **Step 6：提交**

```powershell
git add src-tauri
git commit -m "feat: add versioned SQLite foundation"
```

## Task 4：实现 Work/Run 状态机

**Files:**
- Create: `src-tauri/src/domain/mod.rs`
- Create: `src-tauri/src/domain/work.rs`
- Create: `src-tauri/src/domain/event.rs`
- Create: `src-tauri/src/work/mod.rs`
- Create: `src-tauri/src/work/state_machine.rs`
- Create: `src/bindings/index.ts`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1：写状态机失败测试**

在 `state_machine.rs` 的 test module 中写：

```rust
#[test]
fn completed_work_can_start_a_new_run() {
    assert_eq!(transition(WorkStatus::Completed, WorkAction::Queue).unwrap(), WorkStatus::Queued);
}

#[test]
fn idle_is_not_completion() {
    assert!(transition(WorkStatus::Idle, WorkAction::Complete).is_ok());
    assert_ne!(WorkStatus::Idle, WorkStatus::Completed);
}

#[test]
fn archived_work_cannot_run() {
    assert!(transition(WorkStatus::Archived, WorkAction::Queue).is_err());
}
```

- [ ] **Step 2：运行测试确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml state_machine`  
Expected: FAIL，状态类型和 `transition` 不存在。

- [ ] **Step 3：实现领域类型**

在 `domain/work.rs` 定义并派生 `Serialize`、`Deserialize`、`sqlx::Type`、`ts_rs::TS`：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, TS)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
pub enum WorkStatus {
    Draft, Queued, Running, Waiting, Idle,
    Completed, Failed, Stopped, Interrupted, Archived,
}
```

同文件定义 `RunStatus`、`PermissionMode`、`WorkSummary`、`WorkDetail`、`RunSummary` 与 `CreateWorkInput`。所有面向前端的 DTO 使用 camelCase JSON 字段，并导出到 `../../src/bindings/`。

`WorkDetail` 必须包含 `summary: WorkSummary`、按创建时间排序的 `runs: Vec<RunSummary>` 和按 Run/sequence 排序的 `events: Vec<WorkEventEnvelope>`，保证重启后前端无需读取 pi session 就能恢复时间线。

创建 `src/bindings/index.ts`，显式 re-export 上述生成类型以及 `WorkEventEnvelope`；该文件由开发者维护，不由 `ts-rs` 覆盖。

`domain/event.rs` 定义 `WorkEventEnvelope { version, work_id, run_id, sequence, occurred_at, payload }`，payload 是 tagged enum，第一阶段包含 `RunStarted`、`AssistantDelta`、`ToolStarted`、`ToolFinished`、`RunCompleted`、`RunFailed`。

- [ ] **Step 4：实现纯状态转换**

`transition(current, action)` 必须显式列出允许转换；任何未列出的组合返回 `InvalidTransition { from, action }`。不允许使用 `_ => Ok(...)` 兜底。

- [ ] **Step 5：运行测试与导出绑定**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml state_machine
cargo test --manifest-path src-tauri/Cargo.toml export_bindings
pnpm typecheck
```

Expected: Rust 测试 PASS，`src/bindings` 生成且 TypeScript 可解析。

- [ ] **Step 6：提交**

```powershell
git add src-tauri src/bindings
git commit -m "feat: define Work and Run lifecycle"
```

## Task 5：实现 Work repository、service 与类型化 commands

**Files:**
- Create: `src-tauri/src/work/repository.rs`
- Create: `src-tauri/src/work/service.rs`
- Create: `src-tauri/src/work/commands.rs`
- Create: `src-tauri/src/app_state.rs`
- Create: `src-tauri/tests/work_lifecycle.rs`
- Modify: `src-tauri/src/work/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Create: `src/app/tauriClient.ts`

- [ ] **Step 1：写 repository 失败测试**

在 `work_lifecycle.rs` 先写创建与重启读取测试：

```rust
#[tokio::test]
async fn created_work_survives_database_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("piwork.db");
    let workspace = temp.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();

    let database = Database::open(&path).await.unwrap();
    let repository = WorkRepository::new(database.pool().clone());
    let created = repository
        .create(CreateWorkInput {
            title: "Revenue dashboard".into(),
            goal: "Build the dashboard".into(),
            root_path: workspace.to_string_lossy().into_owned(),
            permission_mode: PermissionMode::Balanced,
        })
        .await
        .unwrap();
    drop(repository);
    drop(database);

    let reopened = Database::open(&path).await.unwrap();
    let found = WorkRepository::new(reopened.pool().clone())
        .get(&created.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.title, "Revenue dashboard");
    assert_eq!(found.status, WorkStatus::Draft);
}
```

- [ ] **Step 2：运行测试确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test work_lifecycle created_work_survives_database_reopen`  
Expected: FAIL，`WorkRepository` 不存在。

- [ ] **Step 3：实现 WorkRepository**

Repository 提供：

```rust
pub async fn create(&self, input: CreateWorkInput) -> Result<WorkDetail, AppError>;
pub async fn get(&self, id: &str) -> Result<Option<WorkDetail>, AppError>;
pub async fn list(&self) -> Result<Vec<WorkSummary>, AppError>;
pub async fn insert_run(&self, work_id: &str, model_label: &str) -> Result<RunSummary, AppError>;
pub async fn set_work_status(&self, work_id: &str, status: WorkStatus) -> Result<(), AppError>;
pub async fn set_run_status(&self, run_id: &str, status: RunStatus) -> Result<(), AppError>;
```

所有多表变更必须使用 SQLx transaction。`list` 按 `updated_at DESC` 返回。路径写入数据库前通过 `dunce::canonicalize`；测试使用存在的 temp directory，不能伪造不存在路径。

- [ ] **Step 4：实现 WorkService 与 commands**

`WorkService` 依赖 repository，不直接持有 Tauri `AppHandle`。第一阶段提供：

```rust
pub async fn create_work(&self, input: CreateWorkInput) -> Result<WorkDetail, AppError>;
pub async fn list_works(&self) -> Result<Vec<WorkSummary>, AppError>;
pub async fn get_work(&self, work_id: &str) -> Result<WorkDetail, AppError>;
```

`commands.rs` 暴露同名 `#[tauri::command] async fn`，只负责从 `State<AppState>` 取 service 并转发。`AppError` 实现 `Serialize`，JSON 结构固定为 `{ code, message, details? }`。

- [ ] **Step 5：实现唯一前端 IPC 包装层**

`src/app/tauriClient.ts` 先定义可注入接口，再提供生产实现：

```ts
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { CreateWorkInput, RunSummary, WorkDetail, WorkEventEnvelope, WorkSummary } from "../bindings";

export type PiWorkClient = {
  createWork(input: CreateWorkInput): Promise<WorkDetail>;
  listWorks(): Promise<WorkSummary[]>;
  getWork(workId: string): Promise<WorkDetail>;
  startWork(workId: string, prompt: string): Promise<RunSummary>;
  listenToWorkEvents(handler: (event: WorkEventEnvelope) => void): Promise<UnlistenFn>;
};

export const tauriClient: PiWorkClient = {
  createWork: (input: CreateWorkInput) => invoke<WorkDetail>("create_work", { input }),
  listWorks: () => invoke<WorkSummary[]>("list_works"),
  getWork: (workId: string) => invoke<WorkDetail>("get_work", { workId }),
  startWork: (workId: string, prompt: string) =>
    invoke<RunSummary>("start_work", { workId, prompt }),
  listenToWorkEvents: (handler: (event: WorkEventEnvelope) => void): Promise<UnlistenFn> =>
    listen<WorkEventEnvelope>("piwork://work-event", ({ payload }) => handler(payload))
};
```

其他前端文件不得直接导入 Tauri API。

- [ ] **Step 6：验证 repository、commands 编译与绑定一致**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test work_lifecycle
cargo test --manifest-path src-tauri/Cargo.toml
pnpm typecheck
```

Expected: PASS，且 TypeScript 没有未解析 binding。

- [ ] **Step 7：提交**

```powershell
git add src-tauri src/app src/bindings
git commit -m "feat: persist Works behind typed commands"
```

## Task 6：实现 FakeEngineAdapter 与事务事件日志

**Files:**
- Create: `src-tauri/src/engine/mod.rs`
- Create: `src-tauri/src/engine/fake.rs`
- Create: `src-tauri/src/engine/publisher.rs`
- Create: `src-tauri/src/engine/supervisor.rs`
- Modify: `src-tauri/src/work/repository.rs`
- Modify: `src-tauri/src/work/service.rs`
- Modify: `src-tauri/src/work/commands.rs`
- Modify: `src-tauri/src/app_state.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/tests/work_lifecycle.rs`

- [ ] **Step 1：写 fake engine 顺序失败测试**

```rust
#[tokio::test]
async fn fake_engine_emits_a_complete_ordered_run() {
    let (sender, mut receiver) = tokio::sync::mpsc::channel(16);
    let engine = FakeEngineAdapter::new(Duration::ZERO);
    let context = EngineRunContext::test("work-1", "run-1");

    engine.start(context, "Build it".into(), sender).await.unwrap();

    let mut kinds = Vec::new();
    while let Some(event) = receiver.recv().await {
        let terminal = event.is_terminal();
        kinds.push(event.kind());
        if terminal { break; }
    }

    assert_eq!(kinds, [
        "run_started",
        "assistant_delta",
        "tool_started",
        "tool_finished",
        "assistant_delta",
        "run_completed"
    ]);
}
```

- [ ] **Step 2：运行测试确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml fake_engine_emits_a_complete_ordered_run`  
Expected: FAIL，engine types 不存在。

- [ ] **Step 3：定义第一版 EngineAdapter**

`engine/mod.rs`：

```rust
#[async_trait::async_trait]
pub trait EngineAdapter: Send + Sync {
    fn kind(&self) -> &'static str;
    async fn start(
        &self,
        context: EngineRunContext,
        prompt: String,
        sink: tokio::sync::mpsc::Sender<EngineEvent>,
    ) -> Result<EngineSessionRef, EngineError>;
    async fn abort(&self, run_id: &str) -> Result<(), EngineError>;
}
```

同模块定义：

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineSessionRef {
    pub engine_kind: String,
    pub session_id: String,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum EngineError {
    #[error("engine start failed: {0}")]
    Start(String),
    #[error("engine event channel closed")]
    ChannelClosed,
    #[error("engine run was aborted")]
    Aborted,
}
```

`EngineRunContext::test(work_id, run_id)` 只在 `#[cfg(test)]` 下提供，生产构造函数要求规范工作目录和 permission mode。

`EngineEvent` 使用内部 enum；只有 supervisor 能把它映射为产品 `WorkEventPayload`。Fake engine 在 `start` 中使用 `tokio::spawn` 启动事件生产 task，并立即返回 `EngineSessionRef`；不能等所有事件发送结束后才返回。它以确定顺序发送事件，文本内容固定且不依赖时钟。延迟由构造参数注入，测试用 `Duration::ZERO`，开发 UI 用 120ms。

- [ ] **Step 4：先写事件持久化顺序测试**

在 `work_lifecycle.rs` 增加：

```rust
#[tokio::test]
async fn event_is_persisted_before_it_is_published() {
    let harness = TestHarness::new().await;
    let work = harness.create_work().await;
    let mut events = harness.subscribe();

    harness.start_work(&work.id, "Build it").await.unwrap();
    let published = events.recv().await.unwrap();
    let persisted = harness.repository.events_for_run(&published.run_id).await.unwrap();

    assert!(persisted.iter().any(|event| event.sequence == published.sequence));
}
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test work_lifecycle event_is_persisted_before_it_is_published`  
Expected: FAIL，start/supervisor/event repository 不存在。

- [ ] **Step 5：实现可测试 publisher、EngineSupervisor 和 EventJournal 路径**

`engine/publisher.rs` 定义：

```rust
#[async_trait::async_trait]
pub trait EventPublisher: Send + Sync {
    async fn publish(&self, event: WorkEventEnvelope) -> Result<(), AppError>;
}
```

生产 `TauriEventPublisher` 包装 `AppHandle` 并 emit `piwork://work-event`；测试 `ChannelEventPublisher` 把 envelope 发送到 mpsc channel。Supervisor 只依赖 trait，不在测试中构造 Tauri runtime。

Supervisor 每次 start：

1. 在一个 transaction 中创建 Run、保存本次用户 prompt message，并把 Work 转为 running。
2. 启动 Adapter，并读取其 mpsc event。
3. 为每条 event 分配从 1 开始的 sequence。
4. 在 transaction 中写入 event，并同步更新 Run/Work 状态。
5. transaction commit 后调用 `app.emit("piwork://work-event", envelope)`。
6. 收到 terminal event 后停止 task 并清理 active-run map。

`RunCompleted` payload 必须包含 `summary`、`artifacts: []`、`validation: []`、`limitations: []`，使后续真实引擎不需要改变完成结构。

- [ ] **Step 6：暴露 start_work command**

command 输入 `{ workId, prompt }`，返回创建的 `RunSummary`。当 Work archived 或已有活动 Run 时返回稳定错误 code：`invalid_work_state` 或 `work_already_running`。

把 `start_work` 注册到 Tauri invoke handler，并用 Task 5 已定义的 `tauriClient.startWork(workId, prompt)` 调用它；不得新增第二个 IPC wrapper。

- [ ] **Step 7：运行 Rust 全套测试**

Run:

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: 全部退出 0。

- [ ] **Step 8：提交**

```powershell
git add src-tauri src/app src/bindings
git commit -m "feat: add fake engine event pipeline"
```

## Task 7：实现前端 Work store 与事件 replay

**Files:**
- Create: `src/domain/work.ts`
- Create: `src/features/works/workStore.ts`
- Create: `src/features/works/workStore.test.ts`
- Create: `src/features/works/WorkStoreProvider.tsx`
- Create: `src/features/works/useWorkEvents.ts`
- Modify: `src/app/App.tsx`

- [ ] **Step 1：写 store 失败测试**

```ts
it("replays ordered events and ignores duplicate sequences", () => {
  const store = createWorkStore();
  store.getState().upsertWork(workSummary({ id: "w1", status: "running" }));

  store.getState().applyEvent(event({ workId: "w1", runId: "r1", sequence: 1, type: "runStarted" }));
  store.getState().applyEvent(event({ workId: "w1", runId: "r1", sequence: 2, type: "assistantDelta", text: "Hello" }));
  store.getState().applyEvent(event({ workId: "w1", runId: "r1", sequence: 2, type: "assistantDelta", text: "Hello" }));

  expect(store.getState().timelines.w1).toHaveLength(2);
  expect(store.getState().lastSequenceByRun.r1).toBe(2);
});
```

- [ ] **Step 2：运行测试确认失败**

Run: `pnpm test -- src/features/works/workStore.test.ts`  
Expected: FAIL，store 不存在。

- [ ] **Step 3：实现 store**

Store state 必须包含：

```ts
type WorkState = {
  works: Record<string, WorkSummary>;
  selectedWorkId: string | null;
  timelines: Record<string, TimelineItem[]>;
  lastSequenceByRun: Record<string, number>;
  loading: boolean;
  error: AppError | null;
  hydrate(): Promise<void>;
  createWork(input: CreateWorkInput): Promise<WorkDetail>;
  startWork(workId: string, prompt: string): Promise<RunSummary>;
  selectWork(workId: string): void;
  upsertWork(work: WorkSummary): void;
  applyEvent(event: WorkEventEnvelope): void;
};
```

`createWorkStore(client: PiWorkClient = tauriClient)` 创建 Zustand vanilla store。`applyEvent` 必须忽略 `sequence <= lastSequence` 的 event，并按 payload 更新状态和 timeline。它不能执行 IPC。

`hydrate()` 先加载 WorkSummary 列表，选择最近更新 Work，再调用 `getWork` 获取 WorkDetail，并按 Run/sequence replay 持久事件；切换 Work 时重复 detail 加载。不能把仅存在于内存的 timeline 当成恢复来源。

`WorkStoreProvider` 使用 React context 保存 vanilla store；props 接收 `client?: PiWorkClient`，只在首次 mount 创建 store。组件通过 `useWorkStore(selector)` hook 访问它。这样生产使用真实 client，测试可注入 mock。

- [ ] **Step 4：实现 event hook**

`useWorkEvents` 从 provider 取得同一个 client，在 mount 时调用 `client.listenToWorkEvents`，把 event 交给 store；unmount 时处理尚未 resolve 的 Promise，并在取得 unlisten 后立即执行。测试使用注入 client，避免依赖真实 Tauri window。

- [ ] **Step 5：运行 store 测试**

Run:

```powershell
pnpm test -- src/features/works/workStore.test.ts
pnpm typecheck
```

Expected: PASS。

- [ ] **Step 6：提交**

```powershell
git add src
git commit -m "feat: add Work event store and replay"
```

## Task 8：实现三栏 Work 工作台与持续协作入口

**Files:**
- Create: `src/features/works/WorkSidebar.tsx`
- Create: `src/features/workspace/WorkHeader.tsx`
- Create: `src/features/workspace/WorkTimeline.tsx`
- Create: `src/features/workspace/WorkComposer.tsx`
- Create: `src/features/workspace/WorkInspector.tsx`
- Create: `src/features/workspace/WorkSurface.tsx`
- Create: `src/features/workspace/WorkSurface.test.tsx`
- Create: `src/test/mockTauriClient.ts`
- Modify: `src/app/App.tsx`
- Modify: `src/i18n/locales/en.json`
- Modify: `src/i18n/locales/zh-CN.json`

- [ ] **Step 1：写完整交互失败测试**

```tsx
it("creates a Work, starts it, renders completion, and continues with a new Run", async () => {
  const client = createMockTauriClient();
  render(<WorkSurface client={client} />);

  await userEvent.click(screen.getByRole("button", { name: "新建 Work" }));
  await userEvent.type(screen.getByLabelText("目标"), "构建营收看板");
  await userEvent.type(screen.getByLabelText("工作目录"), "D:\\workspace\\revenue");
  await userEvent.click(screen.getByRole("button", { name: "创建" }));

  await userEvent.type(screen.getByRole("textbox", { name: "给 PiWork 指令" }), "开始执行");
  await userEvent.click(screen.getByRole("button", { name: "发送" }));
  client.emit(runCompletedEvent({ runId: "run-1" }));

  expect(await screen.findByText("已完成")).toBeInTheDocument();
  await userEvent.type(screen.getByRole("textbox", { name: "给 PiWork 指令" }), "再优化一次");
  await userEvent.click(screen.getByRole("button", { name: "继续 Work" }));

  expect(client.startWork).toHaveBeenLastCalledWith(expect.any(String), "再优化一次");
});
```

`createMockTauriClient()` 实现完整 `PiWorkClient`，保存 command 调用，并提供 `emit(event)` 同步触发已注册 handler；测试不得 mock `@tauri-apps/api` 模块。

- [ ] **Step 2：运行测试确认失败**

Run: `pnpm test -- src/features/workspace/WorkSurface.test.tsx`  
Expected: FAIL，WorkSurface 不存在。

- [ ] **Step 3：实现左栏与中栏**

- Sidebar 显示 Logo、新建 Work、状态点、标题和相对更新时间。
- Header 显示 Work 标题、规范目录与 `Fake model` 开发标签。
- Timeline 为每种第一阶段 payload 提供独立 renderer；raw payload 不直接 JSON dump。
- Composer 在 completed/failed/stopped/interrupted/idle 状态使用“继续 Work”，其他可执行状态使用“发送”。
- 运行时 composer 仍可输入，但第一阶段只把额外输入排队到 store；真实 steering 在后续 pi 计划实现。

- [ ] **Step 4：实现右栏与响应式布局**

Inspector 包含“进度 / 变更 / 产物 / 日志”四个 tab。第一阶段只有进度来自 fake event，其他 tab 显示明确空状态，不伪造数据。

CSS 断点：

- `>= 1100px`：220px / minmax(420px, 1fr) / 300px 三栏。
- `900px–1099px`：左栏 200px，中栏填充，右栏通过按钮打开 drawer。
- `< 900px` 不作为 1.0 支持窗口宽度；Tauri minWidth 阻止进入。

- [ ] **Step 5：实现 App 装配和错误状态**

App mount 时 hydrate，失败时显示带“重试”和“打开诊断”的产品错误页。没有 Work 时展示空状态和新建按钮；有 Work 时选择最后更新时间最新的 Work。

- [ ] **Step 6：运行前端验证**

Run:

```powershell
pnpm test
pnpm typecheck
pnpm build
```

Expected: 全部退出 0。

- [ ] **Step 7：提交**

```powershell
git add src
git commit -m "feat: build the Work collaboration surface"
```

## Task 9：完成持久恢复、图标、文档与发布前验证

**Files:**
- Modify: `src-tauri/tests/work_lifecycle.rs`
- Modify: `src-tauri/src/work/service.rs`
- Modify: `src-tauri/tauri.conf.json`
- Generate: `src-tauri/icons/*`
- Create: `README.md`
- Create: `NOTICE`
- Modify: `.gitignore`

- [ ] **Step 1：写中断恢复失败测试**

```rust
#[tokio::test]
async fn startup_marks_unfinished_runs_interrupted_without_resuming() {
    let harness = TestHarness::new().await;
    let work = harness.create_work().await;
    let run = harness.repository.insert_run(&work.id, "Fake model").await.unwrap();
    harness.repository.set_work_status(&work.id, WorkStatus::Running).await.unwrap();
    harness.repository.set_run_status(&run.id, RunStatus::Running).await.unwrap();

    let recovered = harness.service.recover_interrupted_runs().await.unwrap();

    assert_eq!(recovered, 1);
    assert_eq!(harness.repository.get(&work.id).await.unwrap().unwrap().status, WorkStatus::Interrupted);
    assert_eq!(harness.engine.start_count(), 0);
}
```

- [ ] **Step 2：运行测试确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test work_lifecycle startup_marks_unfinished_runs_interrupted_without_resuming`  
Expected: FAIL，recover method 不存在。

- [ ] **Step 3：实现启动恢复**

`recover_interrupted_runs()` 在单一 transaction 中把所有 `running|waiting|queued` Run 设为 `interrupted`，对应 Work 设为 `interrupted`，返回受影响数量。它不能调用 EngineAdapter。`lib.rs` 在完成 database migration、注册 state、启动 UI 前调用此方法。

- [ ] **Step 4：生成品牌图标**

`assets/piwork-icon.svg` 使用 1024×1024 紫色圆角方形背景和白色 Continuous Loop 路径。Run:

```powershell
pnpm tauri icon assets/piwork-icon.svg
```

Expected: `src-tauri/icons/icon.ico`、PNG 尺寸和 Windows Store 图标全部生成。

- [ ] **Step 5：添加 README 与 NOTICE**

README 必须包含：产品定位、当前 fake-engine 阶段说明、开发前置条件、`pnpm install`、`pnpm tauri dev`、测试命令、构建命令、英文/中文架构文档链接。

NOTICE 必须列出：

- PiWork copyright 占位使用仓库所有者，不伪造公司实体。
- pi coding agent MIT 与仓库 URL；注明本阶段尚未打包二进制。
- Onyx MIT 与仓库 URL；注明未来只允许复用非 `ee` 内容。
- 完整许可证将在真实依赖或源代码进入产品时随包提供。

- [ ] **Step 6：执行完整验证**

Run:

```powershell
pnpm test
pnpm typecheck
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
pnpm tauri build --debug --bundles nsis
```

Expected:

- 所有测试 PASS；
- TypeScript 和 Rust 无 warning/error；
- `src-tauri/target/debug/bundle/nsis/` 下生成可安装的 debug NSIS 包。

- [ ] **Step 7：手工 Windows 冒烟验证**

1. 启动 `pnpm tauri dev`。
2. 验证界面默认跟随 Windows 语言。
3. 创建一个真实存在的临时目录和 Work。
4. 运行 fake engine，观察 streaming、tool 和 completion。
5. 在同一个 Work 输入新指令，验证创建第二个 Run。
6. 执行中强制结束开发应用，重新启动，验证 Work 显示“已中断”且没有自动续跑。
7. 把窗口缩到最小宽度，验证右栏变成 drawer 且中栏仍可输入。

- [ ] **Step 8：提交阶段完成结果**

```powershell
git add .gitignore README.md NOTICE assets src src-tauri package.json pnpm-lock.yaml
git commit -m "feat: complete PiWork foundation vertical slice"
```

## 阶段完成定义

只有同时满足以下条件，本计划才算完成：

- `pnpm test`、`pnpm typecheck`、`pnpm build` 全部通过。
- `cargo fmt --check`、`cargo clippy -D warnings`、`cargo test` 全部通过。
- debug NSIS 能构建并安装。
- 一个 Work 能通过 fake engine 完成完整 Run，事件先持久化后显示。
- 应用重启后 Work、Run 和 timeline 可恢复。
- 已完成 Work 可以继续交互并创建新 Run。
- 异常退出后的活动 Run 变为 interrupted，绝不自动执行。
- UI 具备已确认的 Continuous Loop 品牌、双语和三栏 Work 结构。

完成后再针对真实 pi RPC 与模型引导编写第二份详细计划；不得在本计划中临时塞入真实 Provider、凭据或权限桥实现。
