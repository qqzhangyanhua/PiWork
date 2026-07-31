import { describe, expect, it } from "vitest";

import {
  extractReferencedFiles,
  filterProjectFiles,
  findMentionTrigger,
  insertFileMention,
  mentionLabels,
  stripFileMentions,
} from "./fileMentions";

describe("file mention model", () => {
  it("finds an @ query only at the start or after whitespace", () => {
    expect(findMentionTrigger("参考 @work", 8)).toEqual({ start: 3, query: "work" });
    expect(findMentionTrigger("@src/", 5)).toEqual({ start: 0, query: "src/" });
    expect(findMentionTrigger("email@example", 13)).toBeNull();
  });

  it("inserts a stable path token and places the caret after it", () => {
    expect(
      insertFileMention("参考 @work 继续", { start: 3, query: "work" }, "src/work file.ts"),
    ).toEqual({
      prompt: "参考 @{src/work file.ts} 继续",
      caret: 22,
    });
  });

  it("extracts unique references and drops references whose token was edited away", () => {
    expect(
      extractReferencedFiles(
        "比较 @{src/a.ts} 和 @{src/b.ts}，再看一次 @{src/a.ts}",
      ),
    ).toEqual(["src/a.ts", "src/b.ts"]);
    expect(extractReferencedFiles("比较 @src/a.ts")).toEqual([]);
  });

  it("filters by filename or relative path case-insensitively", () => {
    const files = [
      { relativePath: "README.md" },
      { relativePath: "src/features/WorkComposer.tsx" },
      { relativePath: "src/styles/workspace.css" },
    ];

    expect(filterProjectFiles(files, "composer")).toEqual([files[1]]);
    expect(filterProjectFiles(files, "SRC/STYLES")).toEqual([files[2]]);
  });

  it("uses the shortest unique parent suffix for duplicate filenames", () => {
    expect(
      mentionLabels([
        "src/app/index.ts",
        "src/features/index.ts",
        "README.md",
      ]),
    ).toEqual({
      "src/app/index.ts": "app/index.ts",
      "src/features/index.ts": "features/index.ts",
      "README.md": "README.md",
    });
  });

  it("removes old-project mentions without removing the surrounding draft", () => {
    expect(stripFileMentions("比较 @{src/a.ts} 和当前实现")).toBe("比较 和当前实现");
  });
});
