# PiWork Home and Conversation V2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复首页环境探测终端闪烁与导航/Composer/对齐问题，并新增默认启用、可切回经典版的主页延展型项目对话流。

**Architecture:** 保持 Work Store、Tauri client、时间线、Composer 和检查器逻辑共享。经典详情页继续使用原作用域；新版通过独立 `WorkDetail` 外壳与 `.workspace-main--dashboard` 样式作用域呈现。运行环境检测在 Windows 进程层隐藏控制台，并由 Rust 会话缓存去重。

**Tech Stack:** React 19、TypeScript、react-i18next、Vitest、Testing Library、Tauri 2、Rust、Tokio、CSS。

---

## 文件结构

- Create `src/features/workspace/detailExperience.ts`：详情体验类型、读取和持久化。
- Create `src/features/workspace/detailExperience.test.ts`：偏好默认值与持久化测试。
- Create `src/features/workspace/WorkDetail.tsx`：经典/新版共享详情外壳。
- Create `src/components/brand/OrbLogo.tsx`：3D 蓝色 π 球体品牌组件。
- Create `src-tauri/src/environment/cache.rs`：运行环境会话缓存。
- Modify `src-tauri/src/environment/detection.rs`：Windows 无窗口子进程。
- Modify `src-tauri/src/environment/commands.rs`、`mod.rs`：使用缓存。
- Modify `src/features/works/WorkSidebar.tsx`：移除首页与项目标题栏加号，调整新对话入口。
- Modify `src/features/workspace/WorkSurface.tsx`：首页草稿修订、Ctrl+N、详情体验选择。
- Modify `src/features/workspace/WorkHome.tsx`、`NewWorkStart.tsx`：受控首页项目与草稿重置。
- Modify `src/features/dashboard/DashboardGreeting.tsx`：品牌化 `Pi` 文本。
- Modify `src/features/workspace/WorkHeader.tsx`：经典/新版切换。
- Modify `src/styles/dashboard.css`：Logo、等宽信息面板、首页 Composer、新版详情页。
- Modify `src/i18n/locales/en.json`、`zh-CN.json`：带标记的问候和体验切换文案。
- Modify `src/features/workspace/WorkHome.test.tsx`、`WorkSurface.test.tsx`、`src/app/App.test.tsx`：行为回归测试。

当前工作树包含其他未提交修改。所有实现步骤只编辑上述范围，并且不自动提交实现文件。

### Task 1: 隐藏并缓存运行环境探测

**Files:**
- Create: `src-tauri/src/environment/cache.rs`
- Modify: `src-tauri/src/environment/detection.rs`
- Modify: `src-tauri/src/environment/commands.rs`
- Modify: `src-tauri/src/environment/mod.rs`

- [ ] **Step 1: 写缓存只加载一次的失败测试**

在 `cache.rs` 中建立可测试缓存：

```rust
use std::future::Future;
use tokio::sync::OnceCell;

use crate::domain::environment::RuntimeStatus;

pub struct RuntimeStatusCache {
    value: OnceCell<RuntimeStatus>,
}

impl RuntimeStatusCache {
    pub const fn new() -> Self {
        Self { value: OnceCell::const_new() }
    }

    pub async fn get_or_init_with<F, Fut>(&self, load: F) -> RuntimeStatus
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = RuntimeStatus>,
    {
        self.value.get_or_init(load).await.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
    use crate::domain::environment::{RuntimeCheck, RuntimeStatus};
    use super::RuntimeStatusCache;

    fn status() -> RuntimeStatus {
        let ready = RuntimeCheck { available: true, version: Some("1.0".into()) };
        RuntimeStatus { python: ready.clone(), node: ready.clone(), git: ready }
    }

    #[tokio::test]
    async fn detects_only_once_per_cache() {
        let cache = RuntimeStatusCache::new();
        let calls = Arc::new(AtomicUsize::new(0));
        for _ in 0..2 {
            let calls = Arc::clone(&calls);
            let _ = cache.get_or_init_with(|| async move {
                calls.fetch_add(1, Ordering::SeqCst);
                status()
            }).await;
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
```

- [ ] **Step 2: 运行测试确认当前模块尚未接入**

Run: `cargo test --manifest-path src-tauri/Cargo.toml environment::cache`

Expected: FAIL，`environment::cache` 尚未由 `mod.rs` 声明或命令尚未使用缓存。

- [ ] **Step 3: 为 Windows 探测进程设置无窗口标志**

将 `check_version` 改为显式构造命令：

```rust
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

async fn check_version(program: &str, args: &[&str]) -> RuntimeCheck {
    let mut command = Command::new(program);
    command.args(args);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command.output().await;
    match output {
        Ok(output) if output.status.success() => {
            let raw: &[u8] = if !output.stdout.is_empty() {
                &output.stdout
            } else {
                &output.stderr
            };
            let text = String::from_utf8_lossy(raw).trim().to_string();
            RuntimeCheck {
                available: true,
                version: extract_version(&text),
            }
        }
        _ => RuntimeCheck {
            available: false,
            version: None,
        },
    }
}
```

仅在 Windows 编译常量，避免其他平台产生未使用警告：

```rust
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
```

- [ ] **Step 4: 接入进程级缓存**

`mod.rs`：

```rust
mod cache;
mod commands;
mod detection;

pub use cache::RuntimeStatusCache;
pub use detection::detect_runtime_status;
```

`commands.rs`：

```rust
use std::sync::OnceLock;
use crate::domain::environment::RuntimeStatus;
use super::{detect_runtime_status, RuntimeStatusCache};

fn runtime_status_cache() -> &'static RuntimeStatusCache {
    static CACHE: OnceLock<RuntimeStatusCache> = OnceLock::new();
    CACHE.get_or_init(RuntimeStatusCache::new)
}

#[tauri::command]
pub async fn get_runtime_status() -> RuntimeStatus {
    runtime_status_cache().get_or_init_with(detect_runtime_status).await
}
```

- [ ] **Step 5: 验证 Rust 环境模块**

Run: `cargo test --manifest-path src-tauri/Cargo.toml environment`

Expected: 缓存与版本解析测试全部 PASS。

### Task 2: 让“新对话”成为唯一首页入口

**Files:**
- Modify: `src/features/works/WorkSidebar.tsx`
- Modify: `src/features/workspace/WorkSurface.tsx`
- Modify: `src/features/workspace/WorkHome.tsx`
- Modify: `src/app/App.test.tsx`
- Modify: `src/features/workspace/WorkSurface.test.tsx`

- [ ] **Step 1: 写导航失败测试**

在 `App.test.tsx` 添加：

```tsx
it("uses new conversation as the home entry and removes the duplicate home button", async () => {
  const user = userEvent.setup();
  render(<App client={createMockTauriClient()} />);
  const newConversation = await screen.findByRole("button", { name: "New conversation" });
  expect(screen.queryByRole("button", { name: "Home" })).not.toBeInTheDocument();
  await user.click(newConversation);
  expect(await screen.findByRole("region", { name: "Conversation home" })).toBeInTheDocument();
  expect(newConversation).toHaveAttribute("aria-current", "page");
});
```

在 `WorkSurface.test.tsx` 添加具体项目入口测试：点击项目行的“在 PiTest 中新建对话”，断言首页出现且项目选择按钮包含 `PiTest`；同时断言名为“新建项目”的标题栏按钮不存在。

- [ ] **Step 2: 运行测试确认失败**

Run: `pnpm vitest run src/app/App.test.tsx src/features/workspace/WorkSurface.test.tsx`

Expected: FAIL，因为首页按钮仍存在，`onCreateRequest` 仍进入独立 `new` 视图。

- [ ] **Step 3: 重构侧栏入口**

删除 `Home` 图标导入和首页按钮；删除 `.work-sidebar__new-project` 按钮。保留 `project-group__new`：

```tsx
<button
  aria-current={activeView === "home" ? "page" : undefined}
  aria-label={t("conversation.new")}
  className="work-sidebar__new-conversation"
  onClick={() => onCreateRequest(defaultRootPath)}
  type="button"
>
```

- [ ] **Step 4: 使用首页草稿修订替代 `new` 页面**

在 `WorkSurface` 中引入：

```tsx
const [homeDraft, setHomeDraft] = useState({ revision: 0, rootPath: undefined as string | undefined });

const openHomeDraft = (rootPath?: string) => {
  setHomeDraft(({ revision }) => ({ revision: revision + 1, rootPath }));
  setActiveView("home");
};
```

`onCreateRequest={openHomeDraft}`；删除 `activeView === "new"` 的独立页面分支。给 `WorkHome` 传递 `draftRevision` 与 `initialRootPath`，并以修订号重置 `NewWorkStart`。`WorkSurface` 的首页分支完整调用为：

```tsx
<WorkHome
  draftRevision={homeDraft.revision}
  initialRootPath={homeDraft.rootPath}
  modelLabel={modelLabel}
  onAllWorks={() => setActiveView("all")}
  onStarted={() => setActiveView("detail")}
  onWorkSelected={(work) => openWork(work.id)}
  pickAttachments={pickAttachments}
  pickProjectDirectory={pickProjectDirectory}
  works={Object.values(works)}
/>
```

`WorkHome` 内部将对应值传给 Composer，并保留其余现有属性：

```tsx
<NewWorkStart
  dashboardExtras={dashboardPanels}
  initialRootPath={initialRootPath}
  key={draftRevision}
  modelLabel={modelLabel}
  onStarted={onStarted}
  pickAttachments={pickAttachments}
  pickProjectDirectory={pickProjectDirectory}
  variant="dashboard"
  works={works}
/>
```

注册 `Ctrl+N`：忽略输入法组合状态，阻止默认行为并调用 `openHomeDraft(defaultRootPath)`；保留 `Ctrl+,` 设置快捷键。

- [ ] **Step 5: 验证导航测试**

Run: `pnpm vitest run src/app/App.test.tsx src/features/workspace/WorkSurface.test.tsx`

Expected: 新对话、项目预选、首页按钮移除测试 PASS，既有 Work 导航测试保持 PASS。

### Task 3: 更新 3D 品牌、问候和首页网格

**Files:**
- Create: `src/components/brand/OrbLogo.tsx`
- Modify: `src/features/works/WorkSidebar.tsx`
- Modify: `src/features/dashboard/DashboardGreeting.tsx`
- Modify: `src/i18n/locales/en.json`
- Modify: `src/i18n/locales/zh-CN.json`
- Modify: `src/styles/dashboard.css`
- Modify: `src/features/workspace/WorkHome.test.tsx`

- [ ] **Step 1: 写品牌与面板结构失败测试**

```tsx
expect(within(home).getAllByTestId("brand-pi")).not.toHaveLength(0);
expect(within(home).getByText("Pi", { selector: ".dashboard-greeting__pi" })).toBeInTheDocument();
expect(within(home).getByTestId("dashboard-panels-row")).toHaveClass("dashboard-panels-row");
```

侧栏测试断言 `getByTestId("orb-logo")` 存在。

- [ ] **Step 2: 运行结构测试确认失败**

Run: `pnpm vitest run src/features/workspace/WorkHome.test.tsx src/app/App.test.tsx`

Expected: FAIL，品牌球组件与独立 `Pi` 标记尚不存在。

- [ ] **Step 3: 创建可复用球体 Logo**

```tsx
import type { CSSProperties } from "react";
import referenceUrl from "../../assets/piwork-home-reference.png";

export function OrbLogo({ size = 32 }: { size?: number }) {
  return (
    <span
      aria-label="PiWork"
      className="orb-logo"
      data-testid="orb-logo"
      role="img"
      style={{ "--orb-size": `${size}px`, "--orb-reference": `url(${referenceUrl})` } as CSSProperties}
    ><span aria-hidden="true" className="orb-logo__image" /></span>
  );
}
```

`dashboard.css` 使用精确背景裁切，外层负责浮动/旋转，伪元素负责光晕呼吸；`prefers-reduced-motion` 禁止动画。

- [ ] **Step 4: 使用 `Trans` 标记问候中的 Pi**

翻译文本改为：

```json
"hello": "你好，<pi>Pi</pi> 👋",
"title": "今天想让 <pi>Pi</pi> 帮你完成什么？"
```

组件：

```tsx
<Trans i18nKey="dashboard.greeting.hello" components={{ pi: <em className="dashboard-greeting__pi" /> }} />
<Trans i18nKey="dashboard.greeting.title" components={{ pi: <em className="dashboard-greeting__pi" /> }} />
```

- [ ] **Step 5: 对齐三个等宽信息面板**

```css
.new-work-start--dashboard .new-work-suggestions,
.new-work-start--dashboard .dashboard-panels-row {
  margin-inline: 15px;
}

.dashboard-panels-row {
  grid-template-columns: repeat(3, minmax(0, 1fr));
}
```

保持现有响应式断点在窄屏切换到两列/一列。

- [ ] **Step 6: 验证首页结构**

Run: `pnpm vitest run src/features/workspace/WorkHome.test.tsx src/app/App.test.tsx`

Expected: 品牌、问候、信息面板与导航测试 PASS。

### Task 4: 重构首页 Composer 为统一工作台

**Files:**
- Modify: `src/features/workspace/NewWorkStart.tsx`
- Modify: `src/styles/dashboard.css`
- Modify: `src/features/workspace/WorkHome.test.tsx`
- Modify: `src/features/workspace/ProjectPromptEditor.test.tsx`

- [ ] **Step 1: 写 Composer 分组失败测试**

为 dashboard Composer 增加稳定分组：

```tsx
const composer = within(home).getByTestId("dashboard-composer");
expect(within(composer).getByTestId("dashboard-composer-context")).toBeInTheDocument();
expect(within(composer).getByTestId("dashboard-composer-tools")).toBeInTheDocument();
expect(within(composer).getByTestId("dashboard-composer-submit")).toBeInTheDocument();
```

- [ ] **Step 2: 运行测试确认失败**

Run: `pnpm vitest run src/features/workspace/WorkHome.test.tsx src/features/workspace/ProjectPromptEditor.test.tsx`

Expected: FAIL，因为工具与提交动作仍在同一个无语义分组中。

- [ ] **Step 3: 拆分底部动作组**

Dashboard 变体渲染：

```tsx
<div className="new-work-start__actions">
  <div className="new-work-start__tool-group" data-testid="dashboard-composer-tools">
    <AttachmentButton
      available={attachmentResults}
      disabled={submitting}
      draftId={draftId}
      pickAttachments={pickAttachments}
      selectedIds={selectedResourceIds}
      workId={null}
      onImported={(imported) =>
        setAttachmentResults((current) => {
          const merged = new Map(current.map((resource) => [resource.id, resource]));
          for (const resource of imported) merged.set(resource.id, resource);
          return [...merged.values()];
        })
      }
      onSelectedIdsChange={setSelectedResourceIds}
    />
    <button aria-label={t("dashboard.comingSoon", { feature: t("dashboard.composer.agentButton") })} className="new-work-start__extra-action" disabled type="button">
      <Bot aria-hidden="true" size={16} />
    </button>
    <button aria-label={t("dashboard.comingSoon", { feature: t("dashboard.composer.knowledgeButton") })} className="new-work-start__extra-action" disabled type="button">
      <BookOpen aria-hidden="true" size={16} />
    </button>
    <button aria-label={t("dashboard.comingSoon", { feature: t("dashboard.composer.webSearchButton") })} className="new-work-start__extra-action" disabled type="button">
      <Globe aria-hidden="true" size={16} />
    </button>
  </div>
  <div className="new-work-start__submit-group" data-testid="dashboard-composer-submit">
    <ComposerModelIndicator modelLabel={modelLabel} />
    <button
      aria-label={t("newWork.start")}
      className="new-work-start__send"
      disabled={
        (!prompt.trim() && !attachmentResults.some((resource) => resource.status === "ready" && selectedResourceIds.includes(resource.id))) ||
        !effectiveRootPath ||
        submitting
      }
      onClick={() => void submit()}
      type="button"
    >
      <ArrowUp aria-hidden="true" size={17} />
      <span className="new-work-start__send-hint" aria-hidden="true">{t("dashboard.composer.sendHint")}</span>
    </button>
  </div>
</div>
```

上下文栏添加 `data-testid="dashboard-composer-context"`，Composer 容器添加 `data-testid="dashboard-composer"`。Standalone 变体保持现有布局。

- [ ] **Step 4: 建立稳定三行网格与响应式规则**

```css
.new-work-start--dashboard .new-work-start__composer {
  grid-template-rows:auto minmax(52px, 1fr) auto;
  padding:10px 12px;
  background:#fff;
}

.new-work-start--dashboard .new-work-start__contextbar {
  min-height:28px;
  margin:0;
  padding:0;
  border:0;
  background:transparent;
}

.new-work-start--dashboard .new-work-start__actions {
  display:flex;
  align-items:center;
  justify-content:space-between;
}

.new-work-start__tool-group,
.new-work-start__submit-group {
  display:flex;
  align-items:center;
  gap:6px;
}
```

覆盖旧样式的负 margin、固定宽度和居中规则，所有覆盖必须位于 dashboard 作用域。

- [ ] **Step 5: 验证 Composer 行为**

Run: `pnpm vitest run src/features/workspace/WorkHome.test.tsx src/features/workspace/ProjectPromptEditor.test.tsx src/features/workspace/AttachmentChips.test.tsx`

Expected: 分组结构、输入、附件和文件引用测试全部 PASS。

### Task 5: 新增全局经典/新版体验切换

**Files:**
- Create: `src/features/workspace/detailExperience.ts`
- Create: `src/features/workspace/detailExperience.test.ts`
- Create: `src/features/workspace/WorkDetail.tsx`
- Modify: `src/features/workspace/WorkSurface.tsx`
- Modify: `src/features/workspace/WorkHeader.tsx`
- Modify: `src/i18n/locales/en.json`
- Modify: `src/i18n/locales/zh-CN.json`
- Modify: `src/features/workspace/WorkSurface.test.tsx`

- [ ] **Step 1: 写偏好持久化失败测试**

```ts
import { describe, expect, it, beforeEach } from "vitest";
import { readDetailExperience, persistDetailExperience } from "./detailExperience";

describe("detailExperience", () => {
  beforeEach(() => localStorage.clear());
  it("defaults to dashboard and persists classic", () => {
    expect(readDetailExperience()).toBe("dashboard");
    expect(persistDetailExperience("classic")).toBe("classic");
    expect(readDetailExperience()).toBe("classic");
  });
  it("ignores invalid persisted values", () => {
    localStorage.setItem("piwork.detailExperience", "broken");
    expect(readDetailExperience()).toBe("dashboard");
  });
});
```

- [ ] **Step 2: 运行测试确认失败**

Run: `pnpm vitest run src/features/workspace/detailExperience.test.ts`

Expected: FAIL，模块尚不存在。

- [ ] **Step 3: 实现偏好模块**

```ts
export type DetailExperience = "classic" | "dashboard";
export const DETAIL_EXPERIENCE_STORAGE_KEY = "piwork.detailExperience";

export const readDetailExperience = (): DetailExperience =>
  typeof localStorage !== "undefined" && localStorage.getItem(DETAIL_EXPERIENCE_STORAGE_KEY) === "classic"
    ? "classic"
    : "dashboard";

export const persistDetailExperience = (value: DetailExperience) => {
  if (typeof localStorage !== "undefined") localStorage.setItem(DETAIL_EXPERIENCE_STORAGE_KEY, value);
  return value;
};
```

- [ ] **Step 4: 创建共享详情外壳**

`WorkDetail.tsx` 接收 `experience`、Work、timeline、resources、inspector 状态和动作回调，继续渲染同一组 `WorkHeader`、`WorkTimeline`、`WorkComposer`、`WorkInspector`。根 class：

```tsx
<section
  className={`workspace-main${experience === "dashboard" ? " workspace-main--dashboard" : ""}`}
  data-detail-experience={experience}
  data-inspector-open={inspectorOpen}
>
```

`WorkHeader` 增加分段控件：

```tsx
<fieldset className="work-header__experience">
  <legend className="sr-only">{t("workspace.detailExperience")}</legend>
  {(["classic", "dashboard"] as const).map((value) => (
    <button aria-pressed={experience === value} onClick={() => onExperienceChange(value)} type="button">
      {t(`workspace.detailExperienceOptions.${value}`)}
    </button>
  ))}
</fieldset>
```

- [ ] **Step 5: 在 WorkSurface 持有全局偏好**

```tsx
const [detailExperience, setDetailExperience] = useState(readDetailExperience);
const changeDetailExperience = (value: DetailExperience) => {
  setDetailExperience(persistDetailExperience(value));
};
```

以 `WorkDetail` 替换现有内联详情 JSX，传入所有既有状态和回调。

- [ ] **Step 6: 验证切换与共享数据**

Run: `pnpm vitest run src/features/workspace/detailExperience.test.ts src/features/workspace/WorkSurface.test.tsx`

Expected: 默认新版、切换经典版、localStorage 持久化和既有时间线/Composer 测试全部 PASS。

### Task 6: 实现主页延展型新版详情视觉

**Files:**
- Modify: `src/styles/dashboard.css`
- Modify: `src/styles/tokens.css`
- Modify: `src/features/workspace/WorkSurface.test.tsx`

- [ ] **Step 1: 添加新版作用域结构断言**

```tsx
expect(screen.getByRole("main").querySelector('[data-detail-experience="dashboard"]')).not.toBeNull();
await user.click(screen.getByRole("button", { name: "经典版" }));
expect(screen.getByRole("main").querySelector('[data-detail-experience="classic"]')).not.toBeNull();
```

- [ ] **Step 2: 建立 dashboard 详情外壳**

所有新增规则限定在 `.workspace-main--dashboard`：

```css
.workspace-main--dashboard {
  grid-template-columns:minmax(0, 1fr) var(--inspector-width);
  gap:11px;
  padding:13px 11px 13px 13px;
  border:0;
  background:var(--pw-dashboard-canvas);
}

.workspace-main--dashboard .work-header-region,
.workspace-main--dashboard .workspace-center,
.workspace-main--dashboard .work-inspector {
  border:1px solid var(--pw-dashboard-border);
  background:var(--pw-dashboard-panel);
  box-shadow:var(--pw-dashboard-shadow);
}
```

主卡内部继续使用 header + timeline + composer 三行结构。检查器关闭时主卡占满；小于 1150px 时检查器保持现有覆盖式行为。

- [ ] **Step 3: 统一时间线语义组件**

在 dashboard 作用域内定义：

- 用户消息：浅蓝背景、右对齐、11–12px 圆角。
- 助手消息：白色细边框卡、左对齐。
- 执行进度：浅蓝灰背景、成功/失败颜色复用 token。
- 交付物与失败状态：与首页信息卡相同边框和阴影层级。
- 新输出按钮：品牌蓝圆角胶囊。

不得改写未带 `.workspace-main--dashboard` 前缀的经典规则。

- [ ] **Step 4: 统一详情 Composer**

在 dashboard 作用域内让 `work-composer__box` 复用首页 Composer 的背景、边框、圆角、动作基线与按钮尺寸；不改变发送、停止、附件或文件引用逻辑。

- [ ] **Step 5: 验证双模式结构**

Run: `pnpm vitest run src/features/workspace/WorkSurface.test.tsx src/features/workspace/WorkTimeline.test.tsx src/features/workspace/ExecutionProgressCard.test.tsx`

Expected: 两种模式结构可切换，所有消息、执行进度和 Composer 行为测试 PASS。

### Task 7: 完整验证与真实窗口视觉验收

**Files:**
- Verify all files above

- [ ] **Step 1: 类型检查**

Run: `pnpm typecheck`

Expected: exit 0。

- [ ] **Step 2: 完整前端测试**

Run: `pnpm test`

Expected: 所有 Vitest 文件与测试通过。

- [ ] **Step 3: Rust 测试**

Run: `pnpm cargo:test`

Expected: 环境缓存与全部 Rust 测试通过。

- [ ] **Step 4: 生产构建**

Run: `pnpm build`

Expected: exit 0；允许现有 chunk size warning，不允许新增编译错误。

- [ ] **Step 5: 真实桌面窗口验收**

在 1234×830 逻辑视口（Windows 125% DPI，对应 1536×1024 参考图）检查：

1. 连续在详情与首页间切换五次，没有终端闪烁。
2. 新对话是唯一首页入口；具体项目 `+` 正确预选目录。
3. 3D π Logo 动效克制，两个 `Pi` 为蓝色斜体。
4. 三个信息面板等宽，左右边界与五卡区域一致。
5. Composer 标签、输入、工具和提交区无错位。
6. 新版详情与首页协调；切回经典版后原布局保持不变。

- [ ] **Step 6: 改动审计**

Run: `git diff --check`

Run: `git status --short`

Expected: 无空白错误；只报告任务范围文件及用户原有未提交文件，不执行自动提交、合并或清理。
