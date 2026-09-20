import { useState } from "react";
import type { Ecosystem } from "./types";
import { MAX_SCRIPT_CHARS } from "./utils";

interface ScriptPreviewProps {
  ecosystem: Ecosystem;
  script: string;
  scriptTooLarge: boolean;
  scriptCommandCount: number;
  copyWithLineNumbers: boolean;
  onCopyWithLineNumbersChange: (value: boolean) => void;
  onCleanComments: () => void;
  onDownloadScript: () => void;
  onDownloadPowerShellScript: () => void;
  onDownloadBashScript: () => void;
  onDownloadSystemRequirements: (kind: "bash" | "powershell") => void;
  onCopyScript: () => void;
}

export function ScriptPreview({
  ecosystem,
  script,
  scriptTooLarge,
  scriptCommandCount,
  copyWithLineNumbers,
  onCopyWithLineNumbersChange,
  onCleanComments,
  onDownloadScript,
  onDownloadPowerShellScript,
  onDownloadBashScript,
  onDownloadSystemRequirements,
  onCopyScript,
}: ScriptPreviewProps) {
  const [collapsed, setCollapsed] = useState(false);
  const isR = ecosystem === "r" || ecosystem === "r-binary";
  const fileLabel = isR ? ".R" : ecosystem === "pip" ? ".sh" : "Conda .sh";
  const commandSummary = scriptCommandCount > 0
    ? `${scriptCommandCount} 条 · CRAN ${(script.match(/install\.packages/g) || []).length} · Bioc ${(script.match(/BiocManager::install/g) || []).length} · GitHub ${(script.match(/(?:remotes|devtools)::install_github/g) || []).length}`
    : "R Script";

  return (
    <section className="panel script-panel">
      <header className="panel-header" style={{ gridTemplateColumns: "auto auto 1fr auto" }}>
        <span>03</span><h2>脚本预览</h2>
        <div className="script-toolbar">
          <label className="line-num-toggle" title="复制时在每行前添加行号"><input type="checkbox" checked={copyWithLineNumbers} onChange={(event) => onCopyWithLineNumbersChange(event.target.checked)} /><span>行号</span></label>
          <button className="button ghost script-toolbar-btn" onClick={onCleanComments} disabled={scriptTooLarge}>移除注释</button>
          <button className="button ghost script-toolbar-btn" onClick={onDownloadScript} disabled={!script || script === "等待输入..." || scriptTooLarge} title="Ctrl+S">下载 {fileLabel}<span className="kbd-hint">Ctrl+S</span></button>
          <button className="button ghost script-toolbar-btn" onClick={onDownloadPowerShellScript} disabled={!script || script === "等待输入..." || scriptTooLarge}>下载 .ps1</button>
          <button className="button ghost script-toolbar-btn" onClick={onDownloadBashScript} disabled={!script || script === "等待输入..." || scriptTooLarge}>下载 .sh</button>
          {isR && <><button className="button ghost script-toolbar-btn" onClick={() => onDownloadSystemRequirements("bash")} disabled={!script}>系统依赖 .sh</button><button className="button ghost script-toolbar-btn" onClick={() => onDownloadSystemRequirements("powershell")} disabled={!script}>系统依赖 .ps1</button></>}
          <button className="button primary script-toolbar-btn" onClick={onCopyScript} disabled={!script || script === "等待输入..." || scriptTooLarge} title="Ctrl+Shift+C">复制脚本<span className="kbd-hint">Ctrl+⇧C</span></button>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: "6px" }}><small>{commandSummary}</small><button type="button" className="button ghost script-toolbar-btn" onClick={() => setCollapsed((value) => !value)} title={collapsed ? "展开脚本" : "折叠脚本"}>{collapsed ? "▴" : "▾"}</button></div>
      </header>
      {!collapsed && <pre aria-label="生成的 R 脚本" tabIndex={0}>{script === "等待输入..." || !script ? script : script.split("\n").map((line, index) => <div className="script-line" key={index}><span className="line-no" aria-hidden="true">{index + 1}</span><span className="line-text">{highlightRLine(line)}</span></div>)}</pre>}
      {scriptTooLarge && <div className="inline-warning">脚本内容超出限制：最多 {MAX_SCRIPT_CHARS} 字节。</div>}
    </section>
  );
}

function highlightRLine(line: string) {
  if (line.trimStart().startsWith("#")) return <span className="r-comment">{line}</span>;
  const stringAlt = /"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'/.source;
  const keywordAlt = /\b(?:if|else|for|while|function|return|TRUE|FALSE|NULL|NA|library|require|cat|message|warning|stop|invisible)\b/.source;
  const callAlt = /[A-Za-z_][A-Za-z0-9_.]*(?=\s*\()/.source;
  const numAlt = /\b\d+\.?\d*\b/.source;
  const regex = new RegExp(
    `(${stringAlt})|(${keywordAlt})|(${callAlt})|(${numAlt})`,
    "g",
  );
  const tokens: Array<{ text: string; cls: string }> = [];
  let last = 0;
  let match: RegExpExecArray | null;
  while ((match = regex.exec(line)) !== null) {
    if (match.index > last) tokens.push({ text: line.slice(last, match.index), cls: "" });
    if (match[1]) tokens.push({ text: match[1], cls: "r-string" });
    else if (match[2]) tokens.push({ text: match[2], cls: "r-keyword" });
    else if (match[3]) tokens.push({ text: match[3], cls: "r-func" });
    else if (match[4]) tokens.push({ text: match[4], cls: "r-number" });
    last = regex.lastIndex;
  }
  if (last < line.length) tokens.push({ text: line.slice(last), cls: "" });
  return tokens.map((token, index) => token.cls ? <span key={index} className={token.cls}>{token.text}</span> : token.text);
}
