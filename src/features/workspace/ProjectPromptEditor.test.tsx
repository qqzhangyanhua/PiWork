import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18n } from "../../i18n";
import { createMockTauriClient } from "../../test/mockTauriClient";
import { WorkStoreProvider } from "../works/WorkStoreProvider";
import { ProjectPromptEditor } from "./ProjectPromptEditor";

function Harness({
  client = createMockTauriClient(),
  onDraftChange = vi.fn(),
}: {
  client?: ReturnType<typeof createMockTauriClient>;
  onDraftChange?: (prompt: string, referencedFiles: string[]) => void;
}) {
  const [value, setValue] = useState("");
  return (
    <WorkStoreProvider client={client}>
      <ProjectPromptEditor
        id="test-editor"
        label="测试任务"
        placeholder="描述任务"
        rootPath={"D:\\workspace\\project"}
        value={value}
        onDraftChange={(prompt, referencedFiles) => {
          setValue(prompt);
          onDraftChange(prompt, referencedFiles);
        }}
        onSubmit={() => undefined}
      />
    </WorkStoreProvider>
  );
}

describe("ProjectPromptEditor", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("zh-CN");
  });

  it("loads project files lazily and selects a filtered result with Enter", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.listProjectFiles.mockResolvedValue([
      { relativePath: "src/features/WorkComposer.tsx" },
      { relativePath: "src/styles/workspace.css" },
    ]);
    const onDraftChange = vi.fn();
    render(<Harness client={client} onDraftChange={onDraftChange} />);

    const editor = screen.getByRole("textbox", { name: "测试任务" });
    await user.click(editor);
    await user.type(editor, "参考 @composer");

    expect(client.listProjectFiles).toHaveBeenCalledWith("D:\\workspace\\project");
    expect(
      await screen.findByRole("option", { name: /WorkComposer\.tsx/ }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: /workspace\.css/ })).not.toBeInTheDocument();

    await user.keyboard("{Enter}");

    expect(onDraftChange).toHaveBeenLastCalledWith(
      "参考 @{src/features/WorkComposer.tsx}",
      ["src/features/WorkComposer.tsx"],
    );
    expect(editor).toHaveTextContent("参考 WorkComposer.tsx");
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
  });

  it("supports Arrow navigation, Tab selection, Escape, and IME suppression", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.listProjectFiles.mockResolvedValue([
      { relativePath: "a.ts" },
      { relativePath: "b.ts" },
    ]);
    const onDraftChange = vi.fn();
    render(<Harness client={client} onDraftChange={onDraftChange} />);
    const editor = screen.getByRole("textbox", { name: "测试任务" });

    await user.click(editor);
    await user.type(editor, "@");
    await screen.findByRole("option", { name: "a.ts" });
    await user.keyboard("{ArrowDown}{Tab}");
    expect(onDraftChange).toHaveBeenLastCalledWith("@{b.ts}", ["b.ts"]);

    await user.keyboard(" {Escape}");
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
  });

  it("keeps indexing failures retryable without clearing the draft", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.listProjectFiles
      .mockRejectedValueOnce(new Error("disk unavailable"))
      .mockResolvedValueOnce([{ relativePath: "README.md" }]);
    render(<Harness client={client} />);

    const editor = screen.getByRole("textbox", { name: "测试任务" });
    await user.click(editor);
    await user.type(editor, "参考 @");
    expect(await screen.findByText("无法读取项目文件")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "重试" }));
    expect(await screen.findByRole("option", { name: "README.md" })).toBeInTheDocument();
    expect(editor).toHaveTextContent("参考 @");
  });

  it("reindexes the project when a new mention session opens", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.listProjectFiles
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([{ relativePath: "README.md" }]);
    render(<Harness client={client} />);

    const editor = screen.getByRole("textbox", { name: "测试任务" });
    await user.click(editor);
    await user.type(editor, "@");
    expect(await screen.findByText("没有匹配的文件")).toBeInTheDocument();

    await user.keyboard("{Escape}");
    editor.replaceChildren();
    fireEvent.input(editor);
    await user.type(editor, "@");

    await waitFor(() => expect(client.listProjectFiles).toHaveBeenCalledTimes(2));
    expect(await screen.findByRole("option", { name: "README.md" })).toBeInTheDocument();
  });

  it("does not open the mention menu during composition", async () => {
    const user = userEvent.setup();
    const client = createMockTauriClient();
    client.listProjectFiles.mockResolvedValue([{ relativePath: "中文.md" }]);
    render(<Harness client={client} />);
    const editor = screen.getByRole("textbox", { name: "测试任务" });

    await user.click(editor);
    editor.dispatchEvent(new CompositionEvent("compositionstart", { bubbles: true }));
    await user.type(editor, "@");
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    editor.dispatchEvent(new CompositionEvent("compositionend", { bubbles: true }));

    await waitFor(() => expect(client.listProjectFiles).toHaveBeenCalledTimes(1));
  });
});
