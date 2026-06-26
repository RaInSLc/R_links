import { useState } from "react";
import { PanelHeader, Toggle } from "./components";
import { MAX_INPUT_CHARS, MAX_INPUT_LINE_BYTES, MAX_PACKAGE_LINES, MAX_SCRIPT_CHARS } from "./utils";
import type { Method, Settings } from "./types";
import { methods } from "./types";

interface WorkspaceViewProps {
  input: string;
  inputTooLarge: boolean;
  inputProfile: { total: number; archiveUrls: number; repositories: number };
  method: Method;
  conditional: boolean;
  installDependencies: boolean;
  showRemoteVersion: boolean;
  settings: Settings;
  script: string;
  scriptTooLarge: boolean;
  searching: boolean;
  openingSearchTabs: boolean;
  onInputChange: (value: string, source: "manual" | "clipboard") => string;
  onPaste: () => void;
  onClear: () => void;
  onOpenSearchTabs: () => void;
  onStartSearch: () => void;
  onStopSearch: () => void;
  onMethodChange: (method: Method) => void;
  onConditionalChange: (v: boolean) => void;
  onInstallDependenciesChange: (v: boolean) => void;
  onShowRemoteVersionChange: (v: boolean) => void;
  onFullSearchChange: (v: boolean) => void;
  onUseCacheChange: (v: boolean) => void;
  onTempFilter: (text: string, mode: "chars" | "lines") => void;
  onCopyScript: () => void;
  onCleanComments: () => void;
  isMethodDisabled: (candidate: Method) => boolean;
}

export function WorkspaceView({
  input, inputTooLarge, inputProfile, method,
  conditional, installDependencies, showRemoteVersion, settings,
  script, scriptTooLarge,
  searching, openingSearchTabs,
  onInputChange, onPaste, onClear, onOpenSearchTabs, onStartSearch, onStopSearch,
  onMethodChange, onConditionalChange, onInstallDependenciesChange,
  onShowRemoteVersionChange, onFullSearchChange,
  onUseCacheChange, onTempFilter,
  onCopyScript, onCleanComments, isMethodDisabled,
}: WorkspaceViewProps) {
  const [filterText, setFilterText] = useState("");

  return (
    <div className="workspace-grid">
      <section className="panel input-panel">
        <PanelHeader step="01" title="输入包列表" meta={`${inputProfile.total}/${MAX_PACKAGE_LINES} 项`} />
        <textarea
          value={input}
          onChange={(event) => onInputChange(event.currentTarget.value, "manual")}
          placeholder={"每行一个包，例如：\nSeurat 5.2.1\nGSVA 1.50\nbuenrostrolab/FigR\nhttps://example.org/pkg_1.0.tar.gz"}
          aria-label="R 包输入列表"
          aria-describedby={inputTooLarge ? "input-limit-warning" : undefined}
          aria-invalid={inputTooLarge}
          spellCheck={false}
          maxLength={MAX_INPUT_CHARS + 1}
          disabled={searching}
        />
        {inputTooLarge && (
          <div className="inline-warning" id="input-limit-warning" role="alert">
            输入超出限制或包含非法字符：最多 {MAX_PACKAGE_LINES} 行、总计 {MAX_INPUT_CHARS} 字节、单行 {MAX_INPUT_LINE_BYTES} 字节。
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
          <button className="button ghost wide" onClick={onOpenSearchTabs} disabled={searching || openingSearchTabs || inputTooLarge}>
            {openingSearchTabs ? "正在打开..." : "浏览器搜索"}
          </button>
          {searching ? (
            <button className="button danger" onClick={onStopSearch}>停止</button>
          ) : (
            <button className="button primary" onClick={onStartSearch} disabled={!input.trim() || inputTooLarge}>
              开始检索
            </button>
          )}
        </div>
      </section>

      <section className="panel method-panel">
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
        <div className="toggle-row">
          <Toggle checked={conditional} label="条件安装" description="已安装时自动跳过" onChange={onConditionalChange} />
          <Toggle checked={installDependencies} label="安装依赖" description="dependencies = TRUE" onChange={onInstallDependenciesChange} />
          <Toggle checked={showRemoteVersion} label="同步远程版本" description="显示版本并生成精确版本安装" onChange={onShowRemoteVersionChange} />
          <Toggle checked={settings.fullSearch} label="全量检索" description="命中后仍继续查询 GitHub" onChange={onFullSearchChange} />
          <Toggle checked={settings.useCache} label="使用缓存" description="使用包结果缓存" onChange={onUseCacheChange} />
        </div>
      </section>

      <section className="panel script-panel">
        <header className="panel-header" style={{ gridTemplateColumns: "auto auto 1fr auto" }}>
          <span>03</span>
          <h2>脚本预览</h2>
          <div style={{ display: "flex", gap: "8px", justifyContent: "flex-end", marginRight: "10px" }}>
            <button className="button ghost" style={{ padding: "4px 10px", fontSize: "11px", height: "30px", minHeight: "auto" }} onClick={onCleanComments} disabled={scriptTooLarge}>
              移除注释
            </button>
            <button className="button primary" style={{ padding: "4px 12px", fontSize: "11px", height: "30px", minHeight: "auto" }} onClick={onCopyScript} disabled={!script || script === "等待输入..." || scriptTooLarge}>
              复制脚本
            </button>
          </div>
          <small>R Script</small>
        </header>
        <pre aria-label="生成的 R 脚本" tabIndex={0}>{script}</pre>
        {scriptTooLarge && (
          <div className="inline-warning">
            脚本内容超出限制：最多 {MAX_SCRIPT_CHARS} 字节。
          </div>
        )}
      </section>
    </div>
  );
}
