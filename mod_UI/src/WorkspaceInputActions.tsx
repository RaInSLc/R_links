import { useState } from "react";
import { dedupePackageInput } from "./utils";
import { isCommentLine } from "./utils-sanitize";
import type { InputRules } from "./types";

interface WorkspaceInputActionsProps {
  input: string;
  inputTooLarge: boolean;
  inputRules: InputRules;
  duplicateCount: number;
  searching: boolean;
  paused?: boolean;
  openingSearchTabs: boolean;
  onInputChange: (value: string, source: "manual" | "clipboard") => string;
  onPaste: () => void;
  onClear: () => void;
  onImportFile: () => void;
  onOpenSearchTabs: () => void;
  onStartSearch: () => void;
  onStopSearch: () => void;
  onTogglePause: () => void;
  onTempFilter: (text: string, mode: "chars" | "lines") => void;
  onCleanInput: () => void;
}

export function WorkspaceInputActions({
  input,
  inputTooLarge,
  inputRules,
  duplicateCount,
  searching,
  paused,
  openingSearchTabs,
  onInputChange,
  onPaste,
  onClear,
  onImportFile,
  onOpenSearchTabs,
  onStartSearch,
  onStopSearch,
  onTogglePause,
  onTempFilter,
  onCleanInput,
}: WorkspaceInputActionsProps) {
  const [filterText, setFilterText] = useState("");

  function sortInputAlphabetical() {
    const active: string[] = [];
    const comments: { idx: number; line: string }[] = [];
    input.split(/\r?\n/).forEach((line) => {
      const trimmed = line.trim();
      if (!trimmed || isCommentLine(trimmed, inputRules.commentChars)) comments.push({ idx: active.length, line });
      else active.push(line);
    });
    active.sort((a, b) => a.trim().toLowerCase().localeCompare(b.trim().toLowerCase()));
    comments.forEach((comment) => active.splice(comment.idx, 0, comment.line));
    onInputChange(active.join("\n"), "manual");
  }

  return (
    <>
      <div className="temp-filter-bar">
        <input type="text" className="temp-filter-input" value={filterText} onChange={(event) => setFilterText(event.target.value)} placeholder="临时过滤：输入字符/正则..." disabled={searching} />
        <button type="button" className="button ghost" onClick={() => onTempFilter(filterText, "chars")} disabled={searching || !filterText.trim()}>剔除字符</button>
        <button type="button" className="button ghost" onClick={() => onTempFilter(filterText, "lines")} disabled={searching || !filterText.trim()}>剔除整行</button>
      </div>
      <div className="input-actions">
        <button className="button ghost" onClick={onPaste} disabled={searching}>粘贴</button>
        <button className="button ghost" onClick={onClear} disabled={searching}>清空</button>
        <button className="button ghost" onClick={sortInputAlphabetical} disabled={searching || !input.trim()} title="按字母排序">排序</button>
        <button className="button ghost" onClick={onCleanInput} disabled={searching || !input.trim()} title="去除行首尾空白与空行、统一全角分隔符（保留注释行）">清理</button>
        <button className="button ghost" onClick={sortInputAlphabetical} disabled={searching || !input.trim()} title="按字母 A-Z 排序（保留注释行位置）">A-Z</button>
        <button
          className="button ghost"
          onClick={() => onInputChange(dedupePackageInput(input, inputRules), "manual")}
          disabled={searching || duplicateCount === 0}
          title="大小写不敏感去重"
        >
          去重{duplicateCount > 0 ? `(${duplicateCount})` : ""}
        </button>
        <button className="button ghost" onClick={onImportFile} disabled={searching} title="导入 .txt / .csv / .r 文件">
          导入文件
        </button>
        <button
          className="button ghost"
          onClick={() =>
            onInputChange(
              "Seurat\nggplot2\ndplyr\nDESeq2\nClusterProfiler\nbuenrostrolab/FigR\nGSVA\nSingleCellExperiment\nlimma\ntidyverse",
              "manual",
            )
          }
          disabled={searching}
          title="填充常用生物信息学 R 包示例"
        >
          示例
        </button>
        <button className="button ghost wide" onClick={onOpenSearchTabs} disabled={searching || openingSearchTabs || inputTooLarge}>{openingSearchTabs ? "正在打开..." : "浏览器搜索"}</button>
        {searching ? (
          <>
            <button className="button ghost" onClick={onTogglePause}>{paused ? "继续" : "暂停"}</button>
            <button className="button danger" onClick={onStopSearch}>停止</button>
          </>
        ) : <button className="button primary" onClick={onStartSearch} disabled={!input.trim() || inputTooLarge} title="Ctrl+Enter">开始检索<span className="kbd-hint">Ctrl+↵</span></button>}
      </div>
    </>
  );
}
