import { PanelHeader, Metric, EmptyState } from "./components";
import { sourceNames } from "./types";
import type { SearchResult } from "./utils";

interface ReportViewProps {
  results: SearchResult[];
  logs: string[];
  packageCount: number;
  uniqueFoundCount: number;
  searching: boolean;
  onClearLogs: () => void;
}

export function ReportView({
  results, logs, packageCount, uniqueFoundCount, searching, onClearLogs,
}: ReportViewProps) {
  return (
    <div className="report-layout">
      <div className="metric-row">
        <Metric label="输入包" value={packageCount} />
        <Metric label="已验证包" value={uniqueFoundCount} tone="success" />
        <Metric
          label="未找到"
          value={new Set(results.filter((item) => !item.found).map((item) => item.package)).size}
          tone="danger"
        />
        <Metric label="来源记录" value={results.length} />
      </div>
      <section className="panel report-panel">
        <PanelHeader step="结果" title="来源验证" meta={searching ? "实时更新" : "已完成"} />
        {results.length === 0 ? (
          <EmptyState text={searching ? "正在等待首条检索结果" : "尚未执行检索"} />
        ) : (
          <div className="result-table" role="table" aria-label="包来源验证结果">
            <div className="result-row result-head" role="row">
              <span role="columnheader">包名</span>
              <span role="columnheader">来源</span>
              <span role="columnheader">版本</span>
              <span role="columnheader">仓库</span>
              <span role="columnheader">状态</span>
            </div>
            {results.map((result, index) => (
              <div className="result-row" role="row" key={`${result.package}-${result.source}-${index}`}>
                <strong role="cell">{result.package}</strong>
                <span role="cell" className={`source-tag ${result.source}`}>{sourceNames[result.source] ?? result.source}</span>
                <code role="cell">{result.latestVersion || "—"}</code>
                <span role="cell" className="repo-cell">{result.repository || "—"}</span>
                <span role="cell" className={result.found ? "found" : result.status === "timeout" ? "timeout" : result.status === "rateLimited" ? "rate-limited" : "missing"}>
                  {result.status === "timeout" ? "超时" : result.status === "rateLimited" ? "频率限制" : result.found ? "已验证" : "未找到"}
                </span>
              </div>
            ))}
          </div>
        )}
      </section>
      <section className="panel log-panel">
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <PanelHeader step="日志" title="检索过程" meta={`${logs.length} 行`} />
          <button className="button ghost" style={{ marginRight: "16px", padding: "4px 8px", fontSize: "12px", height: "auto" }} onClick={onClearLogs} disabled={searching || logs.length === 0}>
            清除日志
          </button>
        </div>
        <div className="log-console">
          {logs.length ? logs.map((line, index) => <div key={`${line}-${index}`}><span>{String(index + 1).padStart(2, "0")}</span>{line}</div>) : <EmptyState text="日志将在检索开始后显示" />}
        </div>
      </section>
    </div>
  );
}
