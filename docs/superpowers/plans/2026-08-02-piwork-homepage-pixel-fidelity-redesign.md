# PiWork Homepage Pixel Fidelity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the PiWork homepage so its 1536 × 1024 rendering matches the approved reference screenshot while preserving all existing project, runtime, and composer behavior.

**Architecture:** Keep the existing React component boundaries and real data flow, but give the homepage a dedicated visual shell that overrides the older Linear-style layout. Use the supplied reference screenshot as a clipped decorative background only for the 3D orbit art; every interface region remains real DOM. Centralize page geometry and responsive behavior in `dashboard.css`, which is already imported after the legacy workspace styles.

**Tech Stack:** React 19, TypeScript, CSS, Vitest, Testing Library, Vite, Tauri 2, Lucide React.

---

## File Structure

- Create `src/assets/piwork-home-reference.png`: exact user-supplied reference used as a clipped source for the 3D orbit artwork.
- Modify `src/features/dashboard/DashboardGreeting.tsx`: render the greeting text and decorative art viewport.
- Modify `src/features/dashboard/DashboardHeader.tsx`: match the compact target header and identity treatment.
- Modify `src/features/works/WorkSidebar.tsx`: add the visible shortcut hint and target-specific sidebar structure details.
- Modify `src/features/workspace/WorkHome.tsx`: add stable layout hooks for the main dashboard grid.
- Modify `src/features/workspace/NewWorkStart.tsx`: add stable text/icon wrappers needed by the five target task cards.
- Modify `src/i18n/locales/en.json`: add the concise English task-card descriptions shown on the homepage.
- Modify `src/i18n/locales/zh-CN.json`: add the matching Chinese task-card descriptions.
- Modify `src/styles/tokens.css`: add the approved blue-white dashboard surface and shadow tokens.
- Replace `src/styles/dashboard.css`: make this file the sole owner of homepage, homepage-sidebar, panel, rail, and dashboard-composer geometry.
- Modify `src/features/workspace/WorkHome.test.tsx`: protect target structure and preserved interactions.
- Modify `src/app/App.test.tsx`: protect the sidebar shortcut and default homepage shell.

### Task 1: Lock the target DOM contract with failing tests

**Files:**
- Modify: `src/features/workspace/WorkHome.test.tsx`
- Modify: `src/app/App.test.tsx`

- [ ] **Step 1: Add a failing homepage structure test**

Add this assertion block to the existing dashboard render test in `WorkHome.test.tsx`:

```tsx
const home = await screen.findByRole("region", { name: "首页" });
expect(within(home).getByTestId("dashboard-orbit-art")).toHaveAttribute("aria-hidden", "true");
expect(within(home).getByRole("heading", {
  name: "今天想让 Pi 帮你完成什么？",
})).toBeInTheDocument();
expect(within(home).getByLabelText("任务起点").children).toHaveLength(5);
expect(screen.getByRole("complementary", { name: "智能体活动" })).toBeInTheDocument();
```

- [ ] **Step 2: Add a failing sidebar shortcut test**

Extend the product-shell test in `App.test.tsx`:

```tsx
const newConversation = await screen.findByRole("button", { name: "New conversation" });
expect(newConversation).toHaveTextContent("Ctrl N");
expect(screen.getByRole("button", { name: "Home" })).toHaveAttribute("aria-current", "page");
```

- [ ] **Step 3: Run the focused tests and confirm the new expectations fail**

Run:

```powershell
pnpm vitest run src/features/workspace/WorkHome.test.tsx src/app/App.test.tsx
```

Expected: failure because `dashboard-orbit-art` and the visible `Ctrl N` hint do not exist yet.

- [ ] **Step 4: Commit the red tests**

```powershell
git add -- src/features/workspace/WorkHome.test.tsx src/app/App.test.tsx
git commit -m "test: lock homepage fidelity structure"
```

### Task 2: Add the exact orbit asset and hero structure

**Files:**
- Create: `src/assets/piwork-home-reference.png`
- Modify: `src/features/dashboard/DashboardGreeting.tsx`
- Modify: `src/features/dashboard/DashboardHeader.tsx`
- Modify: `src/features/workspace/WorkHome.tsx`

- [ ] **Step 1: Copy the approved reference into the app asset tree**

Run:

```powershell
New-Item -ItemType Directory -Force -Path 'src/assets' | Out-Null
Copy-Item -LiteralPath 'C:\Users\vayneluo\AppData\Local\Temp\codex-clipboard-7918575c-78a9-41bb-8214-614d8db863b4.png' -Destination 'src/assets/piwork-home-reference.png'
```

Expected: a 1536 × 1024 PNG exists at `src/assets/piwork-home-reference.png`.

- [ ] **Step 2: Replace the placeholder round logo with a clipped art viewport**

Change `DashboardGreeting.tsx` to import the reference and expose it as a CSS custom property:

```tsx
import type { CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import referenceUrl from "../../assets/piwork-home-reference.png";

export function DashboardGreeting() {
  const { t } = useTranslation();
  return (
    <section className="dashboard-greeting">
      <div className="dashboard-greeting__text">
        <p className="dashboard-greeting__hello">{t("dashboard.greeting.hello")}</p>
        <h1>{t("dashboard.greeting.title")}</h1>
        <p className="dashboard-greeting__body">{t("dashboard.greeting.body")}</p>
      </div>
      <div
        aria-hidden="true"
        className="dashboard-greeting__art"
        data-testid="dashboard-orbit-art"
        style={{ "--dashboard-reference": `url(${referenceUrl})` } as CSSProperties}
      />
    </section>
  );
}
```

- [ ] **Step 3: Give the homepage stable grid landmarks**

Change the outer `WorkHome` markup to:

```tsx
<div aria-label={t("workspace.home")} className="work-home" role="region">
  <div className="work-home__main">
    <DashboardHeader onImportProject={() => void handleImportProject()} />
    <div className="work-home__scroll">
      <div className="work-home__dashboard-content">
        <DashboardGreeting />
        <NewWorkStart
          dashboardExtras={
            <div className="dashboard-panels-row">
              <RecentProjectsPanel groups={groups} onSelectProject={handleSelectProject} onViewAll={onAllWorks} />
              <AgentSkillsPanel />
              <EnvironmentStatusPanel client={client} />
            </div>
          }
          initialRootPath={dashboardRootPath}
          modelLabel={modelLabel}
          onStarted={onStarted}
          pickAttachments={pickAttachments}
          pickProjectDirectory={pickProjectDirectory}
          variant="dashboard"
          works={works}
        />
      </div>
    </div>
  </div>
  <aside aria-label={t("dashboard.activity.title")} className="work-home__activity-rail">
    <AgentActivityPanel />
    <DailySummaryPanel />
  </aside>
</div>
```

Keep the current `NewWorkStart` props and `dashboardExtras` contents unchanged; only add `work-home__dashboard-content` and `role="region"`.

- [ ] **Step 4: Match the compact dashboard identity**

Update `DashboardHeader.tsx` so the left identity includes the brand mark and label:

```tsx
import { ContinuousLoopLogo } from "../../components/brand/ContinuousLoopLogo";

<span className="dashboard-header__identity">
  <ContinuousLoopLogo size={13} />
  <span>PiWork</span>
</span>
```

Keep all three existing action callbacks and disabled states unchanged.

- [ ] **Step 5: Run the homepage test**

```powershell
pnpm vitest run src/features/workspace/WorkHome.test.tsx
```

Expected: the orbit-art and homepage landmark assertions pass; any snapshot-independent behavior tests remain green.

- [ ] **Step 6: Commit the hero structure**

```powershell
git add -- src/assets/piwork-home-reference.png src/features/dashboard/DashboardGreeting.tsx src/features/dashboard/DashboardHeader.tsx src/features/workspace/WorkHome.tsx src/features/workspace/WorkHome.test.tsx
git commit -m "feat: add screenshot-faithful dashboard hero"
```

### Task 3: Rebuild the shell and sidebar proportions

**Files:**
- Modify: `src/features/works/WorkSidebar.tsx`
- Modify: `src/styles/tokens.css`
- Modify: `src/styles/dashboard.css`

- [ ] **Step 1: Add the visible keyboard hint without changing the button name**

Inside `work-sidebar__new-conversation`, after the translated label, add:

```tsx
<kbd aria-hidden="true" className="work-sidebar__shortcut">Ctrl N</kbd>
```

- [ ] **Step 2: Add dashboard-specific visual tokens**

Add to the light `:root` block in `tokens.css`:

```css
--pw-dashboard-canvas: #f3f7fd;
--pw-dashboard-panel: rgb(255 255 255 / 84%);
--pw-dashboard-border: #dfebf8;
--pw-dashboard-shadow: 0 8px 24px rgb(54 99 154 / 7%);
--pw-dashboard-glow: 0 10px 30px rgb(59 125 250 / 18%);
```

Add these dark-mode values inside the existing dark `:root` block:

```css
--pw-dashboard-canvas: #111827;
--pw-dashboard-panel: rgb(24 32 47 / 88%);
--pw-dashboard-border: #29364a;
--pw-dashboard-shadow: 0 8px 24px rgb(0 0 0 / 24%);
--pw-dashboard-glow: 0 10px 30px rgb(65 125 255 / 20%);
```

- [ ] **Step 3: Replace the dashboard shell and sidebar section of `dashboard.css`**

Use these exact primary dimensions at the reference viewport:

```css
.work-surface {
  grid-template-columns: clamp(220px, 16vw, 245px) minmax(0, 1fr);
  padding: 0;
  background: var(--pw-dashboard-canvas);
}

.work-sidebar {
  padding: 28px 18px 18px;
  border-right: 1px solid var(--pw-dashboard-border);
  background: rgb(255 255 255 / 70%);
}

.work-sidebar__topbar { min-height: 52px; margin-bottom: 12px; }
.work-sidebar__brand { gap: 10px; padding: 0 4px; font-size: 19px; }
.work-sidebar__brand-mark { width: 40px; height: 40px; flex-basis: 40px; }

.work-sidebar__new-conversation {
  display: flex;
  min-height: 42px;
  align-items: center;
  gap: 10px;
  padding: 0 12px;
  border: 0;
  border-radius: 10px;
  background: linear-gradient(135deg, #337df7, #4f87ff);
  color: #fff;
  box-shadow: var(--pw-dashboard-glow);
}

.work-sidebar__shortcut {
  margin-left: auto;
  padding: 2px 7px;
  border-radius: 10px;
  background: rgb(255 255 255 / 15%);
  color: inherit;
  font: inherit;
  font-size: 11px;
}
```

Set nav rows to 40px, the selected Home row to the target pale-blue background, project rows to 34px, and keep `AccountMenu` pinned by the existing `margin-top: auto` behavior.

- [ ] **Step 4: Run shell tests**

```powershell
pnpm vitest run src/app/App.test.tsx src/features/workspace/WorkHome.test.tsx
```

Expected: both suites pass, including the visible `Ctrl N` assertion.

- [ ] **Step 5: Commit the shell rebuild**

```powershell
git add -- src/features/works/WorkSidebar.tsx src/styles/tokens.css src/styles/dashboard.css src/app/App.test.tsx
git commit -m "feat: match dashboard shell and sidebar proportions"
```

### Task 4: Match the homepage cards, panels, activity rail, and composer

**Files:**
- Modify: `src/features/workspace/NewWorkStart.tsx`
- Modify: `src/i18n/locales/en.json`
- Modify: `src/i18n/locales/zh-CN.json`
- Modify: `src/styles/dashboard.css`
- Modify: `src/features/workspace/WorkHome.test.tsx`

- [ ] **Step 1: Expose task-card descriptions and arrows**

Inside each suggestion button, keep the existing icon and title, then add:

```tsx
<small>{t(`newWork.suggestions.${key}.body`)}</small>
<ArrowUpRight aria-hidden="true" className="new-work-suggestion__arrow" size={14} />
```

Import `ArrowUpRight` from `lucide-react`. Preserve the existing click behavior that fills and focuses the prompt.

Add matching `body` strings to each locale. The Chinese entries are “理解项目结构与依赖”“从想法到可用代码”“发现潜在问题与优化建议”“定位并解决 Bug”“自动生成项目文档”; the English entries are “Understand structure and dependencies”, “Turn an idea into working code”, “Find issues and optimization opportunities”, “Locate and resolve a bug”, and “Generate project documentation”.

- [ ] **Step 2: Implement the reference homepage grid**

Replace the remaining homepage layout rules in `dashboard.css` with:

```css
.work-home {
  display: grid;
  height: 100%;
  grid-template-columns: minmax(0, 1fr) clamp(270px, 20vw, 300px);
  gap: 14px;
  padding: 16px 14px 16px 16px;
  overflow: hidden;
  border: 0;
  border-radius: 0;
  background: var(--pw-dashboard-canvas);
}

.work-home__main {
  display: grid;
  min-width: 0;
  min-height: 0;
  grid-template-rows: 58px minmax(0, 1fr);
  overflow: hidden;
  border: 1px solid var(--pw-dashboard-border);
  border-radius: 18px;
  background: var(--pw-dashboard-panel);
  box-shadow: var(--pw-dashboard-shadow);
}

.work-home__scroll { min-height: 0; overflow: hidden; }
.work-home__dashboard-content {
  display: grid;
  height: 100%;
  grid-template-rows: 238px minmax(0, 1fr);
  padding: 0 18px 14px;
}

.dashboard-greeting { position: relative; min-height: 0; padding: 54px 24px 20px; overflow: hidden; }
.dashboard-greeting__art {
  position: absolute;
  top: 6px;
  right: -12px;
  width: min(47%, 430px);
  height: 224px;
  background-image: linear-gradient(90deg, #fff0 0%, transparent 7%), var(--dashboard-reference);
  background-position: center, 66.5% 20%;
  background-size: 100% 100%, 1536px 1024px;
  background-repeat: no-repeat;
  mask-image: linear-gradient(90deg, transparent 0, #000 12%, #000 90%, transparent 100%);
}

.new-work-start--dashboard {
  display: grid;
  height: 100%;
  grid-template-rows: 122px minmax(250px, 1fr) 172px;
  gap: 14px;
}

.new-work-start--dashboard .new-work-suggestions { grid-template-columns: repeat(5, minmax(0, 1fr)); gap: 12px; }
.new-work-suggestion { position: relative; min-height: 122px; padding: 16px 14px; border-radius: 14px; }
.dashboard-panels-row { display: grid; grid-template-columns: 1.05fr 1.05fr .9fr; gap: 12px; margin: 0; }
.dashboard-panel { min-height: 0; padding: 16px; border-radius: 14px; overflow: hidden; }
.new-work-start--dashboard .new-work-start__composer { min-height: 0; padding: 10px 12px; border-radius: 16px; }
```

Complete the density and rail rules with:

```css
.dashboard-panel__header h2 { font-size: 14px; line-height: 20px; }
.dashboard-project-item { min-height: 42px; padding: 5px 0; }
.dashboard-project-item__body strong,
.dashboard-skill-item__body strong { font-size: 12px; line-height: 17px; }
.dashboard-project-item__body small,
.dashboard-skill-item__body small,
.dashboard-project-item__time { font-size: 10px; line-height: 15px; }
.dashboard-skill-list li { min-height: 42px; padding: 4px 0; }
.dashboard-skill-item__icon { width: 30px; height: 30px; border-radius: 9px; }
.dashboard-environment__list { gap: 0; }
.dashboard-environment__list li { min-height: 36px; font-size: 11px; }
.work-home__activity-rail {
  display: grid;
  min-width: 0;
  min-height: 0;
  grid-template-rows: minmax(0, 1.65fr) minmax(0, 1fr);
  gap: 14px;
}
.dashboard-activity,
.dashboard-summary { min-height: 0; padding: 18px; overflow: hidden; border-radius: 16px; }
.dashboard-activity__list { gap: 14px; }
.dashboard-summary__chart { height: 90px; margin-top: auto; }
```

- [ ] **Step 3: Protect suggestion behavior after the markup change**

Add this test to `WorkHome.test.tsx`:

```tsx
it("fills and focuses the real editor from a task starter", async () => {
  const user = userEvent.setup();
  renderHome();

  await user.click(screen.getByRole("button", { name: "探索并理解项目" }));

  const editor = screen.getByLabelText("首个任务");
  expect(editor).toHaveTextContent("浏览当前项目，说明它的结构、核心模块和运行方式。");
  expect(editor).toHaveFocus();
});
```

- [ ] **Step 4: Run the dashboard and composer suites**

```powershell
pnpm vitest run src/features/workspace/WorkHome.test.tsx src/features/workspace/ProjectPromptEditor.test.tsx src/features/workspace/AttachmentChips.test.tsx
```

Expected: all suites pass and suggestion clicks still populate the real editor.

- [ ] **Step 5: Commit the dashboard content rebuild**

```powershell
git add -- src/features/workspace/NewWorkStart.tsx src/i18n/locales/en.json src/i18n/locales/zh-CN.json src/styles/dashboard.css src/features/workspace/WorkHome.test.tsx
git commit -m "feat: match homepage cards panels and composer"
```

### Task 5: Add responsive fallbacks and perform visual QA

**Files:**
- Modify: `src/styles/dashboard.css`

- [ ] **Step 1: Add ordered responsive breakpoints**

Append:

```css
@media (max-width: 1240px) {
  .work-home { grid-template-columns: minmax(0, 1fr); }
  .work-home__activity-rail { display: none; }
}

@media (max-width: 980px) {
  .work-surface { grid-template-columns: 72px minmax(0, 1fr); }
  .work-sidebar__brand strong,
  .work-sidebar__new-conversation span,
  .work-sidebar__shortcut,
  .work-sidebar__nav-item span,
  .work-sidebar__section-label,
  .project-group__name,
  .project-conversation span,
  .account-menu__copy { display: none; }
  .dashboard-panels-row { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  .dashboard-panel--environment { grid-column: span 2; }
}

@media (max-width: 760px), (max-height: 760px) {
  .work-home__scroll { overflow-y: auto; }
  .work-home__dashboard-content { height: auto; grid-template-rows: 190px auto; }
  .dashboard-greeting__art { opacity: .58; }
  .new-work-start--dashboard { height: auto; grid-template-rows: auto auto auto; }
  .new-work-start--dashboard .new-work-suggestions { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  .dashboard-panels-row { grid-template-columns: minmax(0, 1fr); }
  .dashboard-panel--environment { grid-column: auto; }
}
```

- [ ] **Step 2: Run automated verification**

```powershell
pnpm typecheck
pnpm test
pnpm build
```

Expected: all commands exit 0.

- [ ] **Step 3: Launch the desktop app at the reference size**

```powershell
pnpm tauri dev
```

Resize the PiWork window to 1536 × 1024. Capture a screenshot and compare it side by side with the approved reference. Verify sidebar width, hero height, orbit art crop, five task cards, three equal-height panels, right rail, and bottom composer.

- [ ] **Step 4: Correct visual deltas and repeat the screenshot**

Only tune values in `dashboard.css` and dashboard tokens unless a missing DOM hook is proven necessary. Repeat until the primary geometry is visibly aligned and no section is clipped.

- [ ] **Step 5: Check one compact viewport**

Resize to 1100 × 760 and verify the activity rail hides, the central content scrolls, and no horizontal scrollbar or overlapping control appears.

- [ ] **Step 6: Commit the verified responsive polish**

```powershell
git add -- src/styles/dashboard.css src/styles/tokens.css
git commit -m "fix: polish homepage screenshot fidelity"
```

### Task 6: Final regression and scope audit

**Files:**
- Review only: all files changed by Tasks 1–5

- [ ] **Step 1: Confirm only intended files are staged or committed**

```powershell
git status --short
git diff --stat HEAD~5..HEAD
```

Expected: unrelated pre-existing working-tree changes remain untouched; homepage commits contain only the listed frontend files and the reference asset.

- [ ] **Step 2: Re-run the complete verification set**

```powershell
pnpm typecheck
pnpm test
pnpm build
```

Expected: every command exits 0 with no new warnings attributable to the homepage work.

- [ ] **Step 3: Record the final visual evidence**

Save the final 1536 × 1024 screenshot under `docs/screenshots/piwork-current/08-home-dashboard.png` and compare it against the user reference before claiming completion.

- [ ] **Step 4: Commit the visual evidence only if it was intentionally added**

```powershell
git add -- docs/screenshots/piwork-current/08-home-dashboard.png
git commit -m "docs: capture rebuilt PiWork homepage"
```
