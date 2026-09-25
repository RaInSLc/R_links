import { forwardRef, useEffect, useImperativeHandle, useRef, useState } from "react";
import {
  MAX_INPUT_CHARS,
  extractSystemRequirements,
  normalizePackageInputDisplay,
  parseProjectDependencyFile,
  trimTrailingBlankLines,
} from "./utils";

export interface PackageInputEditorHandle {
  openFilePicker: () => void;
}

interface PackageInputEditorProps {
  input: string;
  inputTooLarge: boolean;
  searching: boolean;
  onInputChange: (value: string, source: "manual" | "clipboard") => string;
  onStartSearch: () => void;
  onPasteIssues: () => void;
}

export const PackageInputEditor = forwardRef<PackageInputEditorHandle, PackageInputEditorProps>(function PackageInputEditor({
  input,
  inputTooLarge,
  searching,
  onInputChange,
  onStartSearch,
  onPasteIssues,
}, ref) {
  const [dragOver, setDragOver] = useState(false);
  const [rScriptHint, setRScriptHint] = useState<string | null>(null);
  const [fileLoadHint, setFileLoadHint] = useState<string | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const lineGutterRef = useRef<HTMLDivElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const importSequence = useRef(0);

  useImperativeHandle(ref, () => ({ openFilePicker: () => fileInputRef.current?.click() }), []);

  useEffect(() => {
    textareaRef.current?.focus();
  }, []);

  async function loadFile(file: File) {
    const sequence = ++importSequence.current;
    if (file.size > MAX_INPUT_CHARS) {
      setFileLoadHint("文件超过输入大小限制，未读取");
      return;
    }
    try {
      const text = await file.text();
      if (sequence !== importSequence.current) return;
      if (!text) { setFileLoadHint("文件为空，未导入"); return; }
      const parsed = parseProjectDependencyFile(file.name, text) ?? text;
      if (onInputChange(parsed, "clipboard") === "rejected") {
        setFileLoadHint("文件输入未通过校验，未导入");
        return;
      }
      const requirements = extractSystemRequirements(file.name, text);
      setFileLoadHint(`已加载文件: ${file.name}${requirements ? `；系统依赖：${requirements}` : ""}`);
    } catch (error) {
      if (sequence === importSequence.current) setFileLoadHint(`文件导入失败: ${String(error)}`);
    }
  }

  async function handleFileDrop(event: React.DragEvent) {
    event.preventDefault();
    setDragOver(false);
    const file = event.dataTransfer.files?.[0];
    if (!file) return;
    const name = file.name.toLowerCase();
    if (!name.endsWith(".txt") && !name.endsWith(".csv") && !name.endsWith(".r") && !name.endsWith("renv.lock") && !name.endsWith("description")) return;
    await loadFile(file);
  }

  async function handleFilePick(event: React.ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (file) await loadFile(file);
  }

  function handlePaste(event: React.ClipboardEvent<HTMLTextAreaElement>) {
    const text = event.clipboardData.getData("text");
    const normalized = normalizePackageInputDisplay(text);
    if (normalized !== text) {
      event.preventDefault();
      const element = event.currentTarget;
      const nextValue = trimTrailingBlankLines(input.slice(0, element.selectionStart) + normalized + input.slice(element.selectionEnd));
      onInputChange(nextValue, "clipboard");
      return;
    }
    const lines = text.split("\n").filter((line) => line.trim());
    if (
      lines.length > 1 &&
      (lines.some((line) => line !== line.trim()) ||
        lines.some((line) => line.includes(",")) ||
        lines.some((line) => line.includes("\t")) ||
        text.includes("\n\n"))
    )
      onPasteIssues();
    const installPkgs = text.match(/install\.packages\s*\(\s*["'`]([^"'`]+)["'`]/g);
    const biocPkgs = text.match(/BiocManager::install\s*\(\s*["'`]([^"'`]+)["'`]/g);
    const githubPkgs = text.match(/(?:remotes|devtools)::install_github\s*\(\s*["'`]([^"'`]+)["'`]/g);
    if ((installPkgs?.length ?? 0) + (biocPkgs?.length ?? 0) + (githubPkgs?.length ?? 0) === 0) return;
    event.preventDefault();
    const names = [...(installPkgs ?? []), ...(biocPkgs ?? []), ...(githubPkgs ?? [])]
      .map((match) => match.match(/["'`]([^"'`]+)["'`]/)?.[1] ?? "");
    const unique = [...new Set(names.filter(Boolean))];
    if (unique.length > 0) {
      onInputChange(unique.join("\n"), "clipboard");
      setRScriptHint(`已从 R 脚本中提取 ${unique.length} 个包名`);
      setTimeout(() => setRScriptHint(null), 5000);
    }
  }

  function handleKeyDown(event: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (event.key === "Enter" && (event.ctrlKey || event.metaKey) && !event.altKey && !event.nativeEvent.isComposing) {
      event.preventDefault();
      if (!searching && input.trim() && !inputTooLarge) onStartSearch();
      return;
    }
    if (event.key === "Enter" && !event.ctrlKey && !event.metaKey && !event.altKey && !event.nativeEvent.isComposing) {
      event.preventDefault();
      const start = event.currentTarget.selectionStart;
      const end = event.currentTarget.selectionEnd;
      // 被拒绝时输入未变更，此时不得移动光标，否则光标会与文本错位。
      if (onInputChange(input.slice(0, start) + "\n" + input.slice(end), "manual") === "rejected") return;
      requestAnimationFrame(() => {
        if (textareaRef.current) textareaRef.current.selectionStart = textareaRef.current.selectionEnd = start + 1;
      });
      return;
    }
    if (event.key === "Tab") {
      event.preventDefault();
      const element = event.currentTarget;
      const start = element.selectionStart;
      const nextValue = input.slice(0, start) + "  " + input.slice(element.selectionEnd);
      if (onInputChange(nextValue, "manual") !== "rejected") requestAnimationFrame(() => { element.selectionStart = element.selectionEnd = start + 2; });
    }
  }

  return (
    <>
      <div className="textarea-with-gutter">
        <div className="line-gutter" ref={lineGutterRef} aria-hidden="true">
          {input.split("\n").map((_, index) => <div key={index}>{index + 1}</div>)}
        </div>
        <textarea
          ref={textareaRef}
          value={input}
          onChange={(event) => onInputChange(event.currentTarget.value, "manual")}
          onPaste={handlePaste}
          onScroll={() => { if (lineGutterRef.current && textareaRef.current) lineGutterRef.current.scrollTop = textareaRef.current.scrollTop; }}
          onKeyDown={handleKeyDown}
          onDragOver={(event) => { event.preventDefault(); if (!searching) setDragOver(true); }}
          onDragLeave={() => setDragOver(false)}
          onDrop={handleFileDrop}
          className={dragOver ? "drag-over" : ""}
          placeholder={"每行一个包，例如：\nSeurat 5.2.1\nGSVA 1.50\nbuenrostrolab/FigR\nhttps://example.org/pkg_1.0.tar.gz\n\n可拖放 .txt / .csv / .r / renv.lock / DESCRIPTION / requirements.txt"}
          aria-label="R 包输入列表"
          aria-describedby={inputTooLarge ? "input-limit-warning" : undefined}
          aria-invalid={inputTooLarge}
          spellCheck={false}
          maxLength={MAX_INPUT_CHARS + 1}
          disabled={searching}
        />
      </div>
      {rScriptHint && <div className="r-script-hint-bar"><span>{rScriptHint}</span></div>}
      {fileLoadHint && <div className="r-script-hint-bar"><span>{fileLoadHint}</span></div>}
      <input ref={fileInputRef} type="file" accept=".txt,.csv,.r,.lock,DESCRIPTION" onChange={handleFilePick} style={{ display: "none" }} />
    </>
  );
});
