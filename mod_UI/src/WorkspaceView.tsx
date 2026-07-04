import { useState, useRef, useEffect } from "react";
import { PanelHeader, Toggle } from "./components";
import { MAX_INPUT_CHARS, MAX_INPUT_LINE_BYTES, MAX_PACKAGE_LINES, MAX_SCRIPT_CHARS, dedupePackageInput, type SmartSuggestion } from "./utils";
import type { Method, Settings } from "./types";
import { methods, defaultPinnedMethods } from "./types";

interface WorkspaceViewProps {
  input: string;
  inputTooLarge: boolean;
  inputProfile: { total: number; archiveUrls: number; repositories: number };
  method: Method;
  conditional: boolean;
  installDependencies: boolean;
  showRemoteVersion: boolean;
  verifyInstall: boolean;
  settings: Settings;
  smartSuggestions: SmartSuggestion[];
  script: string;
  scriptTooLarge: boolean;
  scriptCommandCount: number;
  duplicateCount: number;
  searching: boolean;
  openingSearchTabs: boolean;
  onInputChange: (value: string, source: "manual" | "clipboard") => string;
  onPaste: () => void;
  onClear: () => void;
  onOpenSearchTabs: () => void;
  onStartSearch: () => void;
  onStopSearch: () => void;
  onMethodChange: (method: Method) => void;
  pinnedMethods: Method[];
  onPinnedMethodsChange: (methods: Method[]) => void;
  onApplySmartSuggestion: (suggestion: SmartSuggestion) => void;
  onConditionalChange: (v: boolean) => void;
  onInstallDependenciesChange: (v: boolean) => void;
  onShowRemoteVersionChange: (v: boolean) => void;
  onVerifyInstallChange: (v: boolean) => void;
  onFullSearchChange: (v: boolean) => void;
  onUseCacheChange: (v: boolean) => void;
  onTempFilter: (text: string, mode: "chars" | "lines") => void;
  onCopyScript: () => void;
  onCleanComments: () => void;
  onDownloadScript: () => void;
  isMethodDisabled: (candidate: Method) => boolean;
}

export function WorkspaceView({
  input, inputTooLarge, inputProfile, method,
  conditional, installDependencies, showRemoteVersion, verifyInstall, settings,
  smartSuggestions,
  script, scriptTooLarge,
  scriptCommandCount, duplicateCount,
  searching, openingSearchTabs,
  onInputChange, onPaste, onClear, onOpenSearchTabs, onStartSearch, onStopSearch,
  onMethodChange, pinnedMethods, onPinnedMethodsChange, onApplySmartSuggestion, onConditionalChange, onInstallDependenciesChange,
  onShowRemoteVersionChange, onVerifyInstallChange, onFullSearchChange,
  onUseCacheChange, onTempFilter,
  onCopyScript, onCleanComments, onDownloadScript, isMethodDisabled,
}: WorkspaceViewProps) {
  const [filterText, setFilterText] = useState("");
  const [strategyExpanded, setStrategyExpanded] = useState(false);
  const [dragOver, setDragOver] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const lineGutterRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    textareaRef.current?.focus();
  }, []);

  async function handleFileDrop(e: React.DragEvent) {
    e.preventDefault();
    setDragOver(false);
    const file = e.dataTransfer.files?.[0];
    if (!file) return;
    const name = file.name.toLowerCase();
    if (!name.endsWith(".txt") && !name.endsWith(".csv") && !name.endsWith(".r")) return;
    const text = await file.text();
    if (text) onInputChange(text, "clipboard");
  }

  const fileInputRef = useRef<HTMLInputElement>(null);

  async function handleFilePick(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    e.target.value = "";
    if (!file) return;
    const text = await file.text();
    if (text) onInputChange(text, "clipboard");
  }

  function sortInputAlphabetical() {
    const lines = input.split(/\r?\n/);
    const active: string[] = [];
    const comments: { idx: number; line: string }[] = [];
    lines.forEach((line) => {
      const t = line.trim();
      if (!t || t.startsWith("#")) comments.push({ idx: active.length, line });
      else active.push(line);
    });
    active.sort((a, b) => a.trim().toLowerCase().localeCompare(b.trim().toLowerCase()));
    comments.forEach((c) => active.splice(c.idx, 0, c.line));
    onInputChange(active.join("\n"), "manual");
  }

  return (
    <div className="workspace-grid">
      <section className="panel input-panel">
        <PanelHeader step="01" title="输入包列表" meta={`${inputProfile.total}/${MAX_PACKAGE_LINES} 项${duplicateCount > 0 ? ` · ${duplicateCount} 重复` : ""} · ${new Blob([input]).size}/${MAX_INPUT_CHARS}B`} />
        <div className="textarea-with-gutter">
          <div className="line-gutter" ref={lineGutterRef} aria-hidden="true">
            {input.split("\n").map((_, i) => (
              <div key={i}>{i + 1}</div>
            ))}
          </div>
          <textarea
            ref={textareaRef}
            value={input}
            onChange={(event) => onInputChange(event.currentTarget.value, "manual")}
            onScroll={() => {
              if (lineGutterRef.current && textareaRef.current) {
                lineGutterRef.current.scrollTop = textareaRef.current.scrollTop;
              }
            }}
            onKeyDown={(e) => {
              if (e.key === "Tab") {
                e.preventDefault();
                const el = e.currentTarget;
                const s = el.selectionStart;
                const en = el.selectionEnd;
                const newVal = input.slice(0, s) + "  " + input.slice(en);
                const accepted = onInputChange(newVal, "manual");
                if (accepted !== "rejected") {
                  requestAnimationFrame(() => { el.selectionStart = el.selectionEnd = s + 2; });
                }
              }
            }}
            onDragOver={(e) => { e.preventDefault(); if (!searching) setDragOver(true); }}
            onDragLeave={() => setDragOver(false)}
            onDrop={handleFileDrop}
            className={dragOver ? "drag-over" : ""}
            placeholder={"每行一个包，例如：\nSeurat 5.2.1\nGSVA 1.50\nbuenrostrolab/FigR\nhttps://example.org/pkg_1.0.tar.gz\n\n可拖放 .txt / .csv / .r 文件"}
            aria-label="R 包输入列表"
            aria-describedby={inputTooLarge ? "input-limit-warning" : undefined}
            aria-invalid={inputTooLarge}
            spellCheck={false}
            maxLength={MAX_INPUT_CHARS + 1}
            disabled={searching}
          />
        </div>
        {inputTooLarge && (
          <div className="inline-warning" id="input-limit-warning" role="alert">
            输入超出限制或包含非法字符：最多 {MAX_PACKAGE_LINES} 行、总计 {MAX_INPUT_CHARS} 字节、单行 {MAX_INPUT_LINE_BYTES} 字节。
          </div>
        )}
        {smartSuggestions.length > 0 && (
          <div className="smart-suggestion-list" aria-label="智能建议">
            {smartSuggestions.map((suggestion) => (
              <div className="smart-suggestion" key={suggestion.id}>
                <div>
                  <strong>{suggestion.title}</strong>
                  <span>{suggestion.detail}</span>
                </div>
                {suggestion.actionLabel && (
                  <button type="button" className="text-button" onClick={() => onApplySmartSuggestion(suggestion)} disabled={searching}>
                    {suggestion.actionLabel}
                  </button>
                )}
              </div>
            ))}
          </div>
        )}
        <div className="temp-filter-bar">
          <input
            type="text"
            className="temp-filter-input"
            value={filterText}
            onChange={(e) => setFilterText(e.target.value)}
            placeholder="临时过滤：输入字符/正则..."
            disabled={searching}
          />
          <button
            type="button"
            className="button ghost"
            onClick={() => onTempFilter(filterText, "chars")}
            disabled={searching || !filterText.trim()}
          >
            剔除字符
          </button>
          <button
            type="button"
            className="button ghost"
            onClick={() => onTempFilter(filterText, "lines")}
            disabled={searching || !filterText.trim()}
          >
            剔除整行
          </button>
        </div>
        <div className="input-actions">
          <button className="button ghost" onClick={onPaste} disabled={searching}>粘贴</button>
          <button className="button ghost" onClick={onClear} disabled={searching}>清空</button>
          <button className="button ghost" onClick={sortInputAlphabetical} disabled={searching || !input.trim()} title="按字母排序">排序</button>
          <button
            className="button ghost"
            onClick={() => onInputChange(dedupePackageInput(input), "manual")}
            disabled={searching || duplicateCount === 0}
            title="大小写不敏感去重"
          >
            去重{duplicateCount > 0 ? `(${duplicateCount})` : ""}
          </button>
          <button className="button ghost" onClick={() => fileInputRef.current?.click()} disabled={searching} title="导入 .txt / .csv / .r 文件">导入文件</button>
          <input
            ref={fileInputRef}
            type="file"
            accept=".txt,.csv,.r"
            onChange={handleFilePick}
            style={{ display: "none" }}
          />
          <button
            className="button ghost"
            onClick={() => onInputChange(`Seurat\nggplot2\ndplyr\nDESeq2\nClusterProfiler\nbuenrostrolab/FigR\nGSVA\nSingleCellExperiment\nlimma\ntidyverse`, "manual")}
            disabled={searching}
            title="填充常用生物信息学 R 包示例"
          >
            示例
          </button>
          <button className="button ghost wide" onClick={onOpenSearchTabs} disabled={searching || openingSearchTabs || inputTooLarge}>
            {openingSearchTabs ? "正在打开..." : "浏览器搜索"}
          </button>
          {searching ? (
            <button className="button danger" onClick={onStopSearch}>停止</button>
          ) : (
            <button className="button primary" onClick={onStartSearch} disabled={!input.trim() || inputTooLarge} title="Ctrl+Enter">
              开始检索<span className="kbd-hint">Ctrl+↵</span>
            </button>
          )}
        </div>
      </section>

      <section className="panel method-panel compact-method-panel">
        <PanelHeader step="02" title="安装策略" meta={settings.fullSearch ? "全量检索" : "快速检索"} />
        <div className="method-grid pinned-method-grid" aria-label="常用安装策略">
          {pinnedMethods.map((id) => {
            const item = methods.find((m) => m.id === id);
            if (!item) return null;
            return (
              <button
                key={item.id}
                className={`method-card ${method === item.id ? "selected" : ""}`}
                disabled={isMethodDisabled(item.id)}
                aria-pressed={method === item.id}
                onClick={() => onMethodChange(item.id)}
              >
                <span>{item.title}</span>
                <small>{item.description}</small>
              </button>
            );
          })}
        </div>
        <div className="strategy-footer">
          <div className="strategy-chips" aria-label="当前策略选项">
            {conditional && <span>条件安装</span>}
            {installDependencies && <span>安装依赖</span>}
            {showRemoteVersion && <span>同步版本</span>}
            {settings.fullSearch && <span>全量检索</span>}
            {settings.useCache && <span>使用缓存</span>}
            {verifyInstall && <span>安装后验证</span>}
          </div>
          <button type="button" className="button ghost compact-btn" onClick={() => setStrategyExpanded(true)}>
            配置策略
          </button>
        </div>
      </section>

      {strategyExpanded && (
        <div className="strategy-overlay" role="presentation" onClick={() => setStrategyExpanded(false)}>
          <section className="panel strategy-drawer" role="dialog" aria-modal="true" aria-label="安装策略配置" onClick={(event) => event.stopPropagation()}>
            <PanelHeader step="02" title="安装策略" meta={settings.fullSearch ? "全量检索" : "快速检索"} />
            <div className="method-grid">
              {methods.map((item) => (
                <button
                  key={item.id}
                  className={`method-card ${method === item.id ? "selected" : ""}`}
                  disabled={isMethodDisabled(item.id)}
                  aria-pressed={method === item.id}
                  onClick={() => onMethodChange(item.id)}
                >
                  <span>{item.title}</span>
                  <small>{item.description}</small>
                </button>
              ))}
            </div>
            <div className="pin-section">
              <p className="pin-section-title">面板常用策略</p>
              <div className="pin-chips">
                {methods.map((item) => {
                  const pinned = pinnedMethods.includes(item.id);
                  return (
                    <button
                      key={item.id}
                      type="button"
                      className={`pin-chip ${pinned ? "active" : ""}`}
                      onClick={() => {
                        if (pinned) {
                          if (pinnedMethods.length <= 1) return;
                          onPinnedMethodsChange(pinnedMethods.filter((m) => m !== item.id));
                        } else {
                          onPinnedMethodsChange([...pinnedMethods, item.id]);
                        }
                      }}
                      aria-pressed={pinned}
                      title={pinned ? "从面板移除" : "添加到面板"}
                    >
                      {item.title}
                    </button>
                  );
                })}
              </div>
              {pinnedMethods.length < defaultPinnedMethods.length && (
                <button
                  type="button"
                  className="text-button pin-reset"
                  onClick={() => onPinnedMethodsChange([...defaultPinnedMethods])}
                >
                  恢复默认常用
                </button>
              )}
            </div>
            <div className="toggle-row">
              <Toggle checked={conditional} label="条件安装" description="已安装时自动跳过" onChange={onConditionalChange} />
              <Toggle checked={installDependencies} label="安装依赖" description="dependencies = TRUE" onChange={onInstallDependenciesChange} />
              <Toggle checked={showRemoteVersion} label="同步远程版本" description="显示版本并生成精确版本安装" onChange={onShowRemoteVersionChange} />
              <Toggle checked={settings.fullSearch} label="全量检索" description="命中后仍继续查询 GitHub" onChange={onFullSearchChange} />
              <Toggle checked={settings.useCache} label="使用缓存" description="使用包结果缓存" onChange={onUseCacheChange} />
              <Toggle checked={verifyInstall} label="安装后验证" description="脚本末尾追加安装结果验证代码" onChange={onVerifyInstallChange} />
            </div>
            <div className="strategy-drawer-actions">
              <button type="button" className="button primary" onClick={() => setStrategyExpanded(false)}>
                完成
              </button>
            </div>
          </section>
        </div>
      )}

      <section className="panel script-panel">
        <header className="panel-header" style={{ gridTemplateColumns: "auto auto 1fr auto" }}>
          <span>03</span>
          <h2>脚本预览</h2>
          <div style={{ display: "flex", gap: "8px", justifyContent: "flex-end", marginRight: "10px" }}>
            <button className="button ghost" style={{ padding: "4px 10px", fontSize: "11px", height: "30px", minHeight: "auto" }} onClick={onCleanComments} disabled={scriptTooLarge}>
              移除注释
            </button>
            <button className="button ghost" style={{ padding: "4px 10px", fontSize: "11px", height: "30px", minHeight: "auto" }} onClick={onDownloadScript} disabled={!script || script === "等待输入..." || scriptTooLarge} title="Ctrl+S">
              下载 .R<span className="kbd-hint">Ctrl+S</span>
            </button>
            <button className="button primary" style={{ padding: "4px 12px", fontSize: "11px", height: "30px", minHeight: "auto" }} onClick={onCopyScript} disabled={!script || script === "等待输入..." || scriptTooLarge} title="Ctrl+Shift+C">
              复制脚本<span className="kbd-hint">Ctrl+⇧C</span>
            </button>
          </div>
          <small>{scriptCommandCount > 0 ? `${scriptCommandCount} 条命令` : "R Script"}</small>
        </header>
        <pre aria-label="生成的 R 脚本" tabIndex={0}>
          {script === "等待输入..." || !script ? (
            script
          ) : (
            script.split("\n").map((line, i) => (
              <div className="script-line" key={i}>
                <span className="line-no" aria-hidden="true">{i + 1}</span>
                <span className="line-text">{line}</span>
              </div>
            ))
          )}
        </pre>
        {scriptTooLarge && (
          <div className="inline-warning">
            脚本内容超出限制：最多 {MAX_SCRIPT_CHARS} 字节。
          </div>
        )}
      </section>
    </div>
  );
}
