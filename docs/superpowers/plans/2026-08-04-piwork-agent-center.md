# PiWork Agent Center Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Magic Factory agent center with all 96 capabilities, guided P0 discovery, filtering, accessible details, and editable new-conversation handoff.

**Architecture:** Keep the catalog as typed, versioned frontend data and expose pure filtering and prompt-building functions. Add an `agents` workspace view that renders a focused feature page; the detail drawer hands an editable prompt back through `WorkSurface` to the existing `WorkHome`/`NewWorkStart` creation path.

**Tech Stack:** React 19, TypeScript 5.7, i18next, lucide-react, handwritten CSS, Vitest, Testing Library, user-event.

---

## File structure

- Create `src/features/agent-center/agentCapabilities.ts`: domain metadata, build paths, all 96 typed capability records.
- Create `src/features/agent-center/agentCapabilities.test.ts`: source-count, ID, domain, priority, filtering, and prompt tests.
- Create `src/features/agent-center/CapabilityCard.tsx`: keyboard-accessible capability summary button.
- Create `src/features/agent-center/CapabilityDetailDrawer.tsx`: dialog semantics, focus lifecycle, close and start actions.
- Create `src/features/agent-center/AgentCenterPage.tsx`: guided/all views, local filter state, result grid and empty state.
- Create `src/features/agent-center/AgentCenterPage.test.tsx`: page behavior, filters, drawer, accessibility, and start callback.
- Create `src/styles/agent-center.css`: page-local responsive styling using existing dashboard tokens.
- Modify `src/features/works/WorkSidebar.tsx`: enable the agent-center navigation item and callback.
- Modify `src/features/workspace/WorkSurface.tsx`: add the agents view and prompt handoff state.
- Modify `src/features/workspace/WorkHome.tsx`: accept and forward an initial prompt.
- Modify `src/features/workspace/NewWorkStart.tsx`: initialize/synchronize the editable prompt from a caller-provided draft.
- Modify `src/features/workspace/WorkSurface.test.tsx`: navigation and end-to-end prompt-handoff coverage.
- Modify `src/features/workspace/WorkHome.test.tsx`: forward-prop coverage and updated live agent-center expectations.
- Modify `src/i18n/locales/zh-CN.json`: Chinese agent-center interface strings.
- Modify `src/i18n/locales/en.json`: English agent-center interface strings while capability source content stays Chinese.

### Task 1: Capability catalog and pure behavior

**Files:**
- Create: `src/features/agent-center/agentCapabilities.ts`
- Create: `src/features/agent-center/agentCapabilities.test.ts`

- [ ] **Step 1: Write failing data-completeness tests**

```ts
import { describe, expect, it } from "vitest";
import {
  AGENT_CAPABILITIES,
  AGENT_CAPABILITY_DOMAINS,
  buildCapabilityPrompt,
  filterCapabilities,
} from "./agentCapabilities";

describe("agent capabilities", () => {
  it("contains the complete Magic Factory V2 catalog", () => {
    expect(AGENT_CAPABILITIES).toHaveLength(96);
    expect(new Set(AGENT_CAPABILITIES.map(({ id }) => id)).size).toBe(96);
    expect(AGENT_CAPABILITIES.map(({ id }) => id)).toEqual(
      Array.from({ length: 96 }, (_, index) => index + 1),
    );
    expect(AGENT_CAPABILITY_DOMAINS).toHaveLength(9);
    expect(AGENT_CAPABILITIES.filter(({ priority }) => priority === "P0")).toHaveLength(23);
    expect(AGENT_CAPABILITY_DOMAINS.map(({ id }) =>
      AGENT_CAPABILITIES.filter(({ domainId }) => domainId === id).length
    )).toEqual([8, 7, 9, 12, 10, 14, 16, 8, 12]);
  });

  it("combines search, domain and priority filters", () => {
    expect(filterCapabilities(AGENT_CAPABILITIES, {
      query: "报价",
      domainId: "quote-finance",
      priority: "P0",
    }).map(({ name }) => name)).toEqual(["参考报价智能体", "报价预审与版本智能体"]);
  });

  it("builds an editable task draft from source fields", () => {
    const capability = AGENT_CAPABILITIES.find(({ id }) => id === 16)!;
    const prompt = buildCapabilityPrompt(capability);
    expect(prompt).toContain("需求澄清智能体");
    expect(prompt).toContain(capability.coreCapability);
    expect(prompt).toContain(capability.outputs[0]);
    expect(prompt).toContain("业务目标：");
  });
});
```

- [ ] **Step 2: Run the test and verify the missing module failure**

Run: `pnpm test -- src/features/agent-center/agentCapabilities.test.ts`

Expected: FAIL because `agentCapabilities.ts` does not exist.

- [ ] **Step 3: Implement types, filters, prompt builder, domains, paths, and the exact 96 records**

```ts
export type CapabilityPriority = "P0" | "P1" | "P2";
export type AgentCapabilityDomainId =
  | "innovation-service"
  | "sales-operations"
  | "requirements-assessment"
  | "quote-finance"
  | "project-delivery"
  | "engineering-rd"
  | "manufacturing-quality"
  | "delivery-knowledge"
  | "ai-governance";

export type AgentCapability = {
  id: number;
  name: string;
  domainId: AgentCapabilityDomainId;
  priority: CapabilityPriority;
  audiences: string[];
  coreCapability: string;
  outputs: string[];
  implementation: string;
  suggestedInputs: string[];
};

export type CapabilityFilters = {
  query: string;
  domainId: AgentCapabilityDomainId | "all";
  priority: CapabilityPriority | "all";
};

export const AGENT_CAPABILITY_DOMAINS = [
  { id: "innovation-service", name: "研创用户与创新服务", count: 8 },
  { id: "sales-operations", name: "客户、线索与销售运营", count: 7 },
  { id: "requirements-assessment", name: "需求、评估与承接决策", count: 9 },
  { id: "quote-finance", name: "报价、合同、支付与财务", count: 12 },
  { id: "project-delivery", name: "项目、协作与履约管理", count: 10 },
  { id: "engineering-rd", name: "专业工程研发", count: 14 },
  { id: "manufacturing-quality", name: "采购、试制、生产与质量", count: 16 },
  { id: "delivery-knowledge", name: "交付、售后与知识资产", count: 8 },
  { id: "ai-governance", name: "AI中台运营、治理与安全", count: 12 },
] as const;

export function filterCapabilities(items: AgentCapability[], filters: CapabilityFilters) {
  const query = filters.query.trim().toLocaleLowerCase();
  return items.filter((item) => {
    const searchable = [item.name, ...item.audiences, item.coreCapability, ...item.outputs]
      .join(" ").toLocaleLowerCase();
    return (!query || searchable.includes(query))
      && (filters.domainId === "all" || item.domainId === filters.domainId)
      && (filters.priority === "all" || item.priority === filters.priority);
  });
}

export function buildCapabilityPrompt(capability: AgentCapability) {
  return [
    `我想使用「${capability.name}」完成一项任务。`,
    "", "业务目标：", "", "已有资料：",
    ...capability.suggestedInputs.map((item) => `- ${item}`),
    "", "需要协助：", capability.coreCapability,
    "", "期望输出:", ...capability.outputs.map((item) => `- ${item}`),
    "", "补充约束或人工确认点：",
  ].join("\n");
}
```

Populate `AGENT_CAPABILITIES` in ascending ID order from the supplied Magic Factory V2 Markdown table. Split comma-like source fields into arrays without rewriting their meaning. Give each record 1–3 conservative `suggestedInputs` based only on its described audiences, capability, and outputs; do not claim an unavailable integration.

- [ ] **Step 4: Run catalog tests**

Run: `pnpm test -- src/features/agent-center/agentCapabilities.test.ts`

Expected: PASS with three tests and exact 96/9/23 counts.

- [ ] **Step 5: Commit the catalog slice**

```text
git add src/features/agent-center/agentCapabilities.ts src/features/agent-center/agentCapabilities.test.ts
git commit -m "feat: add Magic Factory capability catalog"
```

### Task 2: Agent-center page and accessible detail drawer

**Files:**
- Create: `src/features/agent-center/CapabilityCard.tsx`
- Create: `src/features/agent-center/CapabilityDetailDrawer.tsx`
- Create: `src/features/agent-center/AgentCenterPage.tsx`
- Create: `src/features/agent-center/AgentCenterPage.test.tsx`

- [ ] **Step 1: Write failing page tests**

```tsx
render(<AgentCenterPage onStartCapability={onStartCapability} />);
expect(screen.getByRole("heading", { name: "智能体中心" })).toBeInTheDocument();
expect(screen.getByText("96")).toBeInTheDocument();
expect(screen.getByText("23")).toBeInTheDocument();
expect(screen.getByRole("tab", { name: "首期推荐" })).toHaveAttribute("aria-selected", "true");

await user.click(screen.getByRole("tab", { name: "全部能力" }));
expect(screen.getAllByRole("button", { name: /查看.*详情/u })).toHaveLength(96);
await user.type(screen.getByRole("searchbox", { name: "搜索能力" }), "报价");
await user.selectOptions(screen.getByLabelText("优先级"), "P0");
expect(screen.getByText(/项能力/u)).toBeInTheDocument();

await user.click(screen.getByRole("button", { name: "查看参考报价智能体详情" }));
const dialog = screen.getByRole("dialog", { name: "参考报价智能体" });
expect(within(dialog).getByText("实现方式")).toBeInTheDocument();
await user.click(within(dialog).getByRole("button", { name: "开始使用" }));
expect(onStartCapability).toHaveBeenCalledWith(expect.objectContaining({ name: "参考报价智能体" }));
```

- [ ] **Step 2: Run the page test and verify missing components**

Run: `pnpm test -- src/features/agent-center/AgentCenterPage.test.tsx`

Expected: FAIL because the page modules do not exist.

- [ ] **Step 3: Implement the card and drawer boundaries**

`CapabilityCard` renders one `<button>` with `aria-label={t("agentCenter.capability.open", { name })}`. `CapabilityDetailDrawer` renders a portal-free in-tree `role="dialog" aria-modal="true"`, focuses its close button on mount, keeps `Tab` and `Shift+Tab` within the drawer, closes on `Escape`, restores the invoking card through a passed ref, and exposes only `onClose` and `onStart` actions.

- [ ] **Step 4: Implement the guided and catalog views**

`AgentCenterPage` owns:

```ts
const [activeView, setActiveView] = useState<"recommended" | "all">("recommended");
const [filters, setFilters] = useState<CapabilityFilters>({
  query: "", domainId: "all", priority: "all",
});
const [selectedCapability, setSelectedCapability] = useState<AgentCapability | null>(null);
```

Render real summary counts, the four build-path cards, all 23 P0 cards in the default view, all filtered records in the catalog view, a result count, and a clearable no-results state. Preserve `filters` while switching tabs.

- [ ] **Step 5: Run page tests**

Run: `pnpm test -- src/features/agent-center/AgentCenterPage.test.tsx`

Expected: PASS for default content, filtering, drawer behavior, Escape, focus restoration, and start callback.

- [ ] **Step 6: Commit the page slice**

```text
git add src/features/agent-center
git commit -m "feat: add guided agent center"
```

### Task 3: Workspace navigation and prompt handoff

**Files:**
- Modify: `src/features/works/WorkSidebar.tsx`
- Modify: `src/features/workspace/WorkSurface.tsx`
- Modify: `src/features/workspace/WorkHome.tsx`
- Modify: `src/features/workspace/NewWorkStart.tsx`
- Modify: `src/features/workspace/WorkSurface.test.tsx`
- Modify: `src/features/workspace/WorkHome.test.tsx`

- [ ] **Step 1: Replace the disabled-navigation test with a failing live-navigation test**

```tsx
const agents = within(sidebar).getByRole("button", { name: "智能体中心" });
expect(agents).toBeEnabled();
await user.click(agents);
expect(await screen.findByRole("heading", { name: "智能体中心" })).toBeInTheDocument();
expect(agents).toHaveAttribute("aria-current", "page");
```

Add an integration test that opens capability 16, clicks “开始使用”, and asserts the `first-work-prompt` editor contains “需求澄清智能体” and “业务目标：” without calling `client.createWork`.

- [ ] **Step 2: Run the focused integration tests**

Run: `pnpm test -- src/features/workspace/WorkSurface.test.tsx src/features/workspace/WorkHome.test.tsx`

Expected: FAIL because the sidebar button is disabled and there is no prompt handoff.

- [ ] **Step 3: Enable sidebar navigation**

Extend `WorkspaceView` with `"agents"`. Add `onAgentsRequest()` to `WorkSidebarProps`, remove `disabled` and the coming-soon label from the button, set `aria-current` for the active agents view, and call the prop on click.

- [ ] **Step 4: Add the workspace view and draft state**

In `WorkSurface`, add:

```ts
const [homeDraft, setHomeDraft] = useState<{
  revision: number;
  rootPath?: string;
  prompt?: string;
}>({ revision: 0 });

const startCapability = (capability: AgentCapability) => {
  setHomeDraft(({ revision, rootPath }) => ({
    revision: revision + 1,
    rootPath,
    prompt: buildCapabilityPrompt(capability),
  }));
  setActiveView("home");
};
```

Render `AgentCenterPage` for the agents view and forward `homeDraft.prompt` into `WorkHome`.

- [ ] **Step 5: Initialize the existing composer from the handed-off draft**

Add `initialPrompt?: string` to `WorkHome` and `NewWorkStart`. Initialize `prompt` from `initialPrompt ?? ""`, and synchronize it only when the keyed dashboard draft changes. Existing suggestion clicks and user editing remain unchanged.

- [ ] **Step 6: Run focused integration tests**

Run: `pnpm test -- src/features/workspace/WorkSurface.test.tsx src/features/workspace/WorkHome.test.tsx`

Expected: PASS; knowledge base and data source buttons remain disabled.

- [ ] **Step 7: Commit navigation and handoff**

```text
git add src/features/works/WorkSidebar.tsx src/features/workspace/WorkSurface.tsx src/features/workspace/WorkHome.tsx src/features/workspace/NewWorkStart.tsx src/features/workspace/WorkSurface.test.tsx src/features/workspace/WorkHome.test.tsx
git commit -m "feat: launch capability tasks from agent center"
```

### Task 4: Product-consistent styling and responsive layout

**Files:**
- Create: `src/styles/agent-center.css`
- Modify: `src/features/workspace/WorkSurface.tsx`

- [ ] **Step 1: Add structural test assertions**

Assert the page exposes `agent-center`, `agent-center__catalog`, `capability-grid`, and `capability-drawer` classes so responsive styling attaches to stable component boundaries.

- [ ] **Step 2: Run the page test and verify class assertions fail**

Run: `pnpm test -- src/features/agent-center/AgentCenterPage.test.tsx`

Expected: FAIL on missing layout classes.

- [ ] **Step 3: Add page-local CSS using existing tokens**

Import `agent-center.css` after `dashboard.css`. Implement a full-height scrollable surface, dashboard-token hero, 3-column capability grid, 2-column layout below 1120px, 1-column layout below 760px, and a right drawer that becomes full-page below 760px. Use transform/opacity only for drawer motion and disable transitions under `prefers-reduced-motion`.

- [ ] **Step 4: Run page tests and build**

Run: `pnpm test -- src/features/agent-center/AgentCenterPage.test.tsx && pnpm build`

Expected: PASS and a successful Vite production build without horizontal overflow warnings.

- [ ] **Step 5: Commit styling**

```text
git add src/styles/agent-center.css src/features/workspace/WorkSurface.tsx src/features/agent-center
git commit -m "style: align agent center with PiWork workspace"
```

### Task 5: Interface localization and homepage entry points

**Files:**
- Modify: `src/i18n/locales/zh-CN.json`
- Modify: `src/i18n/locales/en.json`
- Modify: `src/features/dashboard/DashboardHeader.tsx`
- Modify: `src/features/dashboard/AgentSkillsPanel.tsx`
- Modify: `src/features/workspace/WorkHome.tsx`
- Modify: `src/features/workspace/WorkHome.test.tsx`

- [ ] **Step 1: Write failing live-entry tests**

Assert “探索智能体”和“更多技能” are enabled and invoke `onAgentsRequest`, while individual placeholder skill buttons and unreleased knowledge/search controls remain disabled.

- [ ] **Step 2: Run the homepage test**

Run: `pnpm test -- src/features/workspace/WorkHome.test.tsx`

Expected: FAIL because the two catalog entry points are still disabled.

- [ ] **Step 3: Add bilingual interface strings**

Add `agentCenter` locale trees covering page label, tabs, statistics, paths, search, filters, priorities, result count, empty state, card action, drawer sections, close, and start. Keep the capability data itself in Chinese in both locales.

- [ ] **Step 4: Wire homepage catalog entry points**

Add `onAgentsRequest` to `WorkHome`, `DashboardHeader`, and `AgentSkillsPanel`. Enable “探索智能体” and “更多技能” only; both switch `WorkSurface` to `agents`. Keep the four sample skill “使用” buttons disabled because they do not map one-to-one to the approved 96-item source catalog.

- [ ] **Step 5: Run localization and homepage tests**

Run: `pnpm test -- src/i18n/locales/locales.test.ts src/features/workspace/WorkHome.test.tsx`

Expected: PASS with matching zh-CN/en key structures and live catalog navigation.

- [ ] **Step 6: Commit localization and entry points**

```text
git add src/i18n/locales src/features/dashboard src/features/workspace/WorkHome.tsx src/features/workspace/WorkHome.test.tsx src/features/workspace/WorkSurface.tsx
git commit -m "feat: connect agent center discovery entry points"
```

### Task 6: Full verification and regression cleanup

**Files:**
- Modify only files implicated by failures.

- [ ] **Step 1: Run type checking**

Run: `pnpm typecheck`

Expected: exit code 0.

- [ ] **Step 2: Run all tests**

Run: `pnpm test`

Expected: all Vitest suites pass.

- [ ] **Step 3: Run the production build**

Run: `pnpm build`

Expected: TypeScript and Vite complete successfully.

- [ ] **Step 4: Check the final diff and protected user change**

Run: `git diff --check && git status --short`

Expected: no whitespace errors; `src-tauri/Cargo.toml` remains an unstaged user modification and is absent from feature commits.
