import { FileText, RotateCw } from "lucide-react";
import {
  type FormEvent,
  type KeyboardEvent,
  type Ref,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";

import type { ProjectFileSummary } from "../../bindings";
import { useWorkStoreContext } from "../works/WorkStoreProvider";
import {
  extractReferencedFiles,
  filterProjectFiles,
  findMentionTrigger,
  insertFileMention,
  mentionLabels,
  type MentionTrigger,
} from "./fileMentions";

type ProjectPromptEditorProps = {
  id: string;
  label: string;
  placeholder: string;
  rootPath: string;
  value: string;
  disabled?: boolean;
  autoFocus?: boolean;
  editorRef?: Ref<HTMLDivElement>;
  onDraftChange(prompt: string, referencedFiles: string[]): void;
  onSubmit(): void;
};

const tokenPattern = /@\{([^{}\r\n]+)\}/g;

const setRef = (ref: Ref<HTMLDivElement> | undefined, value: HTMLDivElement | null) => {
  if (typeof ref === "function") ref(value);
  else if (ref) ref.current = value;
};

const serializeNode = (node: Node): string => {
  if (node.nodeType === Node.TEXT_NODE) return node.textContent ?? "";
  if (!(node instanceof HTMLElement)) return "";
  const reference = node.dataset.fileReference;
  if (reference) return `@{${reference}}`;
  if (node.tagName === "BR") return "\n";
  const content = [...node.childNodes].map(serializeNode).join("");
  return node.tagName === "DIV" || node.tagName === "P" ? `${content}\n` : content;
};

const serializeEditor = (editor: HTMLElement) =>
  [...editor.childNodes].map(serializeNode).join("").replace(/\n$/u, "");

const serializedBeforeCaret = (editor: HTMLElement): number | null => {
  const selection = window.getSelection();
  if (!selection?.rangeCount) return null;
  const range = selection.getRangeAt(0);
  if (!editor.contains(range.endContainer) && editor !== range.endContainer) return null;
  const prefix = range.cloneRange();
  prefix.selectNodeContents(editor);
  prefix.setEnd(range.endContainer, range.endOffset);
  const fragment = prefix.cloneContents();
  const container = document.createElement("div");
  container.append(fragment);
  return serializeEditor(container).length;
};

const placeCaret = (editor: HTMLElement, target: number) => {
  const selection = window.getSelection();
  if (!selection) return;
  let consumed = 0;
  let placed = false;
  const range = document.createRange();
  for (const node of [...editor.childNodes]) {
    const serialized = serializeNode(node);
    const next = consumed + serialized.length;
    if (node instanceof HTMLElement && node.dataset.fileReference) {
      if (target <= next) {
        range.setStartAfter(node);
        placed = true;
        break;
      }
    } else if (node.nodeType === Node.TEXT_NODE && target <= next) {
      range.setStart(node, Math.max(0, target - consumed));
      placed = true;
      break;
    }
    consumed = next;
  }
  if (!placed) range.selectNodeContents(editor), range.collapse(false);
  else range.collapse(true);
  selection.removeAllRanges();
  selection.addRange(range);
};

const renderPrompt = (
  editor: HTMLElement,
  prompt: string,
  labels: Record<string, string>,
  removeLabel: (name: string) => string,
  onRemove: (path: string) => void,
) => {
  const fragment = document.createDocumentFragment();
  let cursor = 0;
  for (const match of prompt.matchAll(tokenPattern)) {
    const index = match.index ?? 0;
    if (index > cursor) fragment.append(document.createTextNode(prompt.slice(cursor, index)));
    const path = match[1];
    if (!path) continue;
    const mention = document.createElement("span");
    mention.className = "file-mention";
    mention.contentEditable = "false";
    mention.dataset.fileReference = path;
    mention.title = path;
    mention.append(document.createTextNode(labels[path] ?? path.split("/").at(-1) ?? path));
    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "file-mention__remove";
    remove.tabIndex = -1;
    remove.setAttribute("aria-label", removeLabel(labels[path] ?? path));
    remove.textContent = "×";
    remove.addEventListener("mousedown", (event) => event.preventDefault());
    remove.addEventListener("click", () => onRemove(path));
    mention.append(remove);
    fragment.append(mention);
    cursor = index + match[0].length;
  }
  if (cursor < prompt.length) fragment.append(document.createTextNode(prompt.slice(cursor)));
  editor.replaceChildren(fragment);
};

export function ProjectPromptEditor({
  id,
  label,
  placeholder,
  rootPath,
  value,
  disabled = false,
  autoFocus = false,
  editorRef,
  onDraftChange,
  onSubmit,
}: ProjectPromptEditorProps) {
  const { t } = useTranslation();
  const { client } = useWorkStoreContext();
  const elementRef = useRef<HTMLDivElement | null>(null);
  const valueRef = useRef(value);
  valueRef.current = value;
  const composingRef = useRef(false);
  const triggerOpenRef = useRef(false);
  const [files, setFiles] = useState<ProjectFileSummary[]>([]);
  const [loadState, setLoadState] = useState<"idle" | "loading" | "loaded" | "error">("idle");
  const [trigger, setTrigger] = useState<MentionTrigger | null>(null);
  const [activeIndex, setActiveIndex] = useState(0);
  const labels = useMemo(
    () => mentionLabels(files.map(({ relativePath }) => relativePath)),
    [files],
  );
  const matches = useMemo(
    () => (trigger ? filterProjectFiles(files, trigger.query, 12) : []),
    [files, trigger],
  );

  const publish = (prompt: string) => {
    onDraftChange(prompt, extractReferencedFiles(prompt));
  };
  const removeReference = (path: string) => {
    const next = valueRef.current.replaceAll(`@{${path}}`, "").replace(/ {2,}/gu, " ");
    publish(next);
  };
  const paint = (prompt: string, caret?: number) => {
    const editor = elementRef.current;
    if (!editor) return;
    renderPrompt(
      editor,
      prompt,
      labels,
      (name) => t("mentions.remove", { name }),
      removeReference,
    );
    if (caret !== undefined) placeCaret(editor, caret);
  };

  useLayoutEffect(() => {
    const editor = elementRef.current;
    if (editor && serializeEditor(editor) !== value) paint(value);
  });
  useEffect(() => {
    setFiles([]);
    setLoadState("idle");
    setTrigger(null);
    triggerOpenRef.current = false;
  }, [rootPath]);
  useEffect(() => {
    if (autoFocus && !disabled) elementRef.current?.focus();
  }, [autoFocus, disabled]);

  const loadFiles = async () => {
    if (!rootPath || loadState === "loading") return;
    setFiles([]);
    setLoadState("loading");
    try {
      const next = await client.listProjectFiles(rootPath);
      setFiles(next);
      setLoadState("loaded");
    } catch {
      setLoadState("error");
    }
  };

  const updateTrigger = (prompt: string, caret: number | null) => {
    if (caret === null || !rootPath) {
      triggerOpenRef.current = false;
      setTrigger(null);
      return;
    }
    const next = findMentionTrigger(prompt, caret);
    const isOpening = next !== null && !triggerOpenRef.current;
    triggerOpenRef.current = next !== null;
    setTrigger(next);
    setActiveIndex(0);
    if (isOpening) void loadFiles();
  };

  const handleInput = (_event: FormEvent<HTMLDivElement>) => {
    const editor = elementRef.current;
    if (!editor) return;
    const prompt = serializeEditor(editor);
    const caret = serializedBeforeCaret(editor);
    publish(prompt);
    if (!composingRef.current) updateTrigger(prompt, caret);
  };

  const selectFile = (file: ProjectFileSummary) => {
    if (!trigger) return;
    const current = elementRef.current ? serializeEditor(elementRef.current) : valueRef.current;
    const next = insertFileMention(current, trigger, file.relativePath);
    triggerOpenRef.current = false;
    setTrigger(null);
    publish(next.prompt);
    paint(next.prompt, next.caret);
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (trigger) {
      if (event.key === "Escape") {
        event.preventDefault();
        triggerOpenRef.current = false;
        setTrigger(null);
        return;
      }
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        if (matches.length) {
          const direction = event.key === "ArrowDown" ? 1 : -1;
          setActiveIndex((current) => (current + direction + matches.length) % matches.length);
        }
        return;
      }
      if ((event.key === "Enter" || event.key === "Tab") && matches[activeIndex]) {
        event.preventDefault();
        selectFile(matches[activeIndex]);
        return;
      }
    }
    if (event.key === "Enter" && !event.shiftKey && !composingRef.current) {
      event.preventDefault();
      onSubmit();
    }
  };

  return (
    <div className="project-prompt-editor">
      <label className="sr-only" htmlFor={id}>{label}</label>
      <div
        aria-disabled={disabled}
        aria-label={label}
        aria-multiline="true"
        className="project-prompt-editor__surface"
        contentEditable={!disabled}
        data-placeholder={placeholder}
        id={id}
        onCompositionEnd={() => {
          composingRef.current = false;
          const editor = elementRef.current;
          if (editor) updateTrigger(serializeEditor(editor), serializedBeforeCaret(editor));
        }}
        onCompositionStart={() => {
          composingRef.current = true;
          triggerOpenRef.current = false;
          setTrigger(null);
        }}
        onInput={handleInput}
        onKeyDown={handleKeyDown}
        ref={(element) => {
          elementRef.current = element;
          setRef(editorRef, element);
        }}
        role="textbox"
        suppressContentEditableWarning
      />
      {trigger && (
        <div className="file-mention-menu" role="listbox" aria-label={t("mentions.files")}>
          <div className="file-mention-menu__label">{t("mentions.files")}</div>
          {loadState === "loading" && <p>{t("mentions.loading")}</p>}
          {loadState === "error" && (
            <div className="file-mention-menu__error">
              <span>{t("mentions.loadError")}</span>
              <button type="button" onClick={() => void loadFiles()}>
                <RotateCw aria-hidden="true" size={13} />
                {t("common.retry")}
              </button>
            </div>
          )}
          {loadState === "loaded" && matches.length === 0 && <p>{t("mentions.empty")}</p>}
          {matches.map((file, index) => (
            <button
              aria-selected={index === activeIndex}
              className="file-mention-menu__option"
              key={file.relativePath}
              onClick={() => selectFile(file)}
              onMouseDown={(event) => event.preventDefault()}
              role="option"
              title={file.relativePath}
              type="button"
            >
              <FileText aria-hidden="true" size={14} />
              <span>{file.relativePath}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
