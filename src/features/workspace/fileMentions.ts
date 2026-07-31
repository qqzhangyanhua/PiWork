import type { ProjectFileSummary } from "../../bindings";

export type MentionTrigger = {
  start: number;
  query: string;
};

const tokenPattern = /@\{([^{}\r\n]+)\}/g;

export const findMentionTrigger = (
  prompt: string,
  caret: number,
): MentionTrigger | null => {
  const beforeCaret = prompt.slice(0, caret);
  const start = beforeCaret.lastIndexOf("@");
  if (start < 0) return null;
  if (start > 0 && !/\s/u.test(beforeCaret[start - 1] ?? "")) return null;
  const query = beforeCaret.slice(start + 1);
  if (/[\r\n{}]/u.test(query)) return null;
  return { start, query };
};

export const insertFileMention = (
  prompt: string,
  trigger: MentionTrigger,
  relativePath: string,
) => {
  const token = `@{${relativePath}}`;
  const replacedUntil = trigger.start + trigger.query.length + 1;
  return {
    prompt: `${prompt.slice(0, trigger.start)}${token}${prompt.slice(replacedUntil)}`,
    caret: trigger.start + token.length,
  };
};

export const extractReferencedFiles = (prompt: string): string[] => {
  const references: string[] = [];
  const seen = new Set<string>();
  for (const match of prompt.matchAll(tokenPattern)) {
    const path = match[1]?.trim();
    if (path && !seen.has(path)) {
      seen.add(path);
      references.push(path);
    }
  }
  return references;
};

export const stripFileMentions = (prompt: string) =>
  prompt.replace(tokenPattern, "").replace(/ {2,}/gu, " ").trimEnd();

export const filterProjectFiles = (
  files: ProjectFileSummary[],
  query: string,
  limit = 50,
) => {
  const normalized = query.trim().toLocaleLowerCase();
  return files
    .filter(({ relativePath }) =>
      normalized ? relativePath.toLocaleLowerCase().includes(normalized) : true,
    )
    .slice(0, limit);
};

const pathParts = (path: string) => path.replaceAll("\\", "/").split("/");

export const mentionLabels = (paths: string[]): Record<string, string> => {
  const groups = new Map<string, string[]>();
  for (const path of paths) {
    const parts = pathParts(path);
    const filename = parts.at(-1) ?? path;
    groups.set(filename, [...(groups.get(filename) ?? []), path]);
  }

  const labels: Record<string, string> = {};
  for (const [filename, group] of groups) {
    if (group.length === 1) {
      const onlyPath = group[0];
      if (onlyPath) labels[onlyPath] = filename;
      continue;
    }
    const partsByPath = new Map(group.map((path) => [path, pathParts(path)]));
    for (const path of group) {
      const parts = partsByPath.get(path) ?? [filename];
      let depth = Math.min(2, parts.length);
      let label = parts.slice(-depth).join("/");
      while (
        depth < parts.length &&
        group.some(
          (candidate) =>
            candidate !== path &&
            (partsByPath.get(candidate) ?? []).slice(-depth).join("/") === label,
        )
      ) {
        depth += 1;
        label = parts.slice(-depth).join("/");
      }
      labels[path] = label;
    }
  }
  return labels;
};
